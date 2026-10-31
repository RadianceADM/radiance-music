//! No vocals / Vocals only: a song split in two by Demucs (an open-source model
//! that separates the voice from the music), run on a split server over SSH so
//! the laptop stays free (a song takes about 3 minutes there).
//!
//! The song goes up as it is (FLAC, or Opus remuxed into Ogg: no re-encoding,
//! small), and comes back as `<song>.vocals.*` and `<song>.music.*` at 44.1 kHz:
//! FLAC for a lossless song, high-quality Ogg Vorbis for a compressed one. They're
//! kept next to a liked song in the Music folder, else in the cache.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use gst::prelude::*;
use gtk::glib;

use crate::net;
use crate::settings::{LocalSplitter, SplitServer};
use crate::track::{file_name, Track};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Voice {
    /// The song as it is.
    Song,
    /// Karaoke: the music without the voice.
    NoVocals,
    VocalsOnly,
}

impl Voice {
    pub fn stem(self) -> Option<&'static str> {
        match self {
            Voice::Song => None,
            Voice::NoVocals => Some("music"),
            Voice::VocalsOnly => Some("vocals"),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Voice::Song => "Song",
            Voice::NoVocals => "No vocals",
            Voice::VocalsOnly => "Vocals only",
        }
    }
}

/// The cache's place for a song's halves (any song, by its key).
fn cache_base(t: &Track) -> PathBuf {
    glib::user_cache_dir().join("radiance-music/stems").join(file_name(&t.key().replace(['/', ':'], "_")))
}

/// Where one of a song's part files ("drums.ogg"…) is kept in the cache (a
/// song not saved: a server's parts, fetched for the Mixer).
pub fn cache_part(t: &Track, file: &str) -> Option<PathBuf> {
    let (part, ext) = file.split_once('.')?;
    let base = cache_base(t);
    Some(base.with_file_name(format!("{}.{part}.{ext}", base.file_name()?.to_string_lossy())))
}

/// Where a song's two halves are kept: next to a saved (liked) song in the
/// Music folder, else in the cache.
pub fn stems_base(t: &Track) -> PathBuf {
    match &t.file {
        Some(f) => Path::new(f).with_extension(""),
        None => cache_base(t),
    }
}

fn at(base: &Path, voice: Voice, ext: &str) -> Option<PathBuf> {
    let stem = voice.stem()?;
    let name = format!("{}.{stem}.{ext}", base.file_name()?.to_string_lossy());
    Some(base.with_file_name(name))
}

fn stem_path(t: &Track, voice: Voice, ext: &str) -> Option<PathBuf> {
    at(&stems_base(t), voice, ext)
}

/// The half wanted, if the song has been split already: found wherever it was
/// kept (a liked song's halves split before it was saved sit in the cache; they
/// move next to the song then).
pub fn ready(t: &Track, voice: Voice) -> Option<PathBuf> {
    for ext in ["flac", "ogg"] {
        if let Some(p) = stem_path(t, voice, ext).filter(|p| p.exists()) {
            return Some(p);
        }
    }
    for ext in ["flac", "ogg"] {
        let Some(cached) = at(&cache_base(t), voice, ext).filter(|p| p.exists()) else { continue };
        if t.file.is_some() {
            if let Some(home) = stem_path(t, voice, ext) {
                if fs::rename(&cached, &home).is_ok() || fs::copy(&cached, &home).is_ok() {
                    let _ = fs::remove_file(&cached);
                    return Some(home);
                }
            }
        }
        return Some(cached);
    }
    None
}

/// Both halves are ready (No vocals and Vocals only play at once).
pub fn is_split(t: &Track) -> bool {
    ready(t, Voice::NoVocals).is_some() && ready(t, Voice::VocalsOnly).is_some()
}

/// The four parts the mixer plays (Demucs' own), in order.
pub const PARTS: [&str; 4] = ["drums", "bass", "other", "vocals"];

/// One of a song's parts (drums, bass, other, vocals, music), wherever kept.
pub fn ready_part(t: &Track, name: &str) -> Option<PathBuf> {
    let file = |base: &Path, ext: &str| Some(base.with_file_name(format!("{}.{name}.{ext}", base.file_name()?.to_string_lossy())));
    for base in [stems_base(t), cache_base(t)] {
        for ext in ["flac", "ogg", "m4a"] {
            if let Some(p) = file(&base, ext).filter(|p| p.exists()) {
                return Some(p);
            }
        }
    }
    None
}

/// Split in six (ticked song by song): the four, and the beat's piano and guitar apart.
pub const SIX: [&str; 6] = ["drums", "bass", "other", "vocals", "guitar", "piano"];

/// Six parts (Piano and Guitar) are put away for a later release (owner,
/// 2026-10-30): no tick, songs split in four. Songs already split in six still
/// play whole, their Piano and Guitar moving with Other; their files and ticks
/// are kept for when it's back.
pub const SIX_PARTS_ON: bool = false;

/// The song is to be split in six (ticked, with six parts on).
pub fn wants_six(t: &Track) -> bool {
    SIX_PARTS_ON && crate::db::wants_six(&t.key())
}

/// All four parts are there: the mixer works for this song.
pub fn has_parts(t: &Track) -> bool {
    PARTS.iter().all(|p| ready_part(t, p).is_some())
}

/// All six are there (it was split in six).
pub fn has_six(t: &Track) -> bool {
    SIX.iter().all(|p| ready_part(t, p).is_some())
}

/// Split as it should be: in six when it was ticked for six, else in four.
pub fn done(t: &Track) -> bool {
    if wants_six(t) { has_six(t) } else { has_parts(t) }
}

/// The parts a split of the song should have that `files` ("drums.ogg"…) lacks.
fn lacks(t: &Track, files: &[&str]) -> Vec<&'static str> {
    let want: &[&'static str] = if model_for(t) == "htdemucs_6s" { &SIX } else { &PARTS };
    want.iter().copied().filter(|p| !files.iter().any(|f| f.split('.').next() == Some(*p))).collect()
}

/// The model a song is split with.
fn model_for(t: &Track) -> &'static str {
    if wants_six(t) { "htdemucs_6s" } else { "htdemucs" }
}

/// The parts the mixer plays for a song: its six (split in six), its four; or,
/// split before there were four, its music and vocals; else none.
pub fn mix_parts(t: &Track) -> Vec<&'static str> {
    if has_six(t) {
        SIX.to_vec()
    } else if has_parts(t) {
        PARTS.to_vec()
    } else if ready_part(t, "music").is_some() && ready_part(t, "vocals").is_some() {
        vec!["music", "vocals"]
    } else {
        vec![]
    }
}

/// Runs a GStreamer pipeline to its end (worker thread).
fn run(desc: &str) -> Result<(), String> {
    gst::init().map_err(|e| e.to_string())?;
    let pipeline = gst::parse::launch(desc).map_err(|e| e.to_string())?;
    let bus = pipeline.bus().ok_or("no bus")?;
    pipeline.set_state(gst::State::Playing).map_err(|e| e.to_string())?;
    let msg = bus.timed_pop_filtered(gst::ClockTime::from_seconds(600), &[gst::MessageType::Eos, gst::MessageType::Error]);
    let _ = pipeline.set_state(gst::State::Null);
    match msg.as_ref().map(|m| m.view()) {
        Some(gst::MessageView::Eos(_)) => Ok(()),
        Some(gst::MessageView::Error(e)) => Err(e.error().to_string()),
        _ => Err("converting the song took too long".into()),
    }
}

fn ssh(server: &SplitServer, command: &str, stdin: Option<&Path>) -> Result<Vec<u8>, String> {
    let mut cmd = Command::new("ssh");
    cmd.args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=10", &server.host, command]);
    cmd.stdin(match stdin {
        Some(p) => Stdio::from(fs::File::open(p).map_err(|e| e.to_string())?),
        None => Stdio::null(),
    });
    let out = cmd.output().map_err(|e| format!("ssh: {e}"))?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        let last = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("the split server didn't answer");
        Err(last.trim().to_string())
    }
}

/// The song, ready to go up: FLAC or Ogg Opus as it is, else remuxed (WebM
/// Opus -> Ogg Opus, no re-encoding), else decoded to FLAC.
fn upload_file(uri: &str, work: &Path, job: &str) -> Result<PathBuf, String> {
    let lower = uri.to_lowercase();
    let local_path = uri.strip_prefix("file://").map(|_| glib::filename_from_uri(uri).map(|(p, _)| p)).transpose().map_err(|e| e.to_string())?;
    if let Some(p) = &local_path {
        if lower.ends_with(".flac") || lower.ends_with(".opus") || lower.ends_with(".ogg") {
            return Ok(p.clone());
        }
    }
    if uri.starts_with("http") && lower.contains(".flac") {
        // a lossless file on the web (the Internet Archive): fetched as is
        let to = work.join(format!("{job}.flac"));
        let mut resp = net::agent().get(uri).call().map_err(|e| e.to_string())?;
        let mut bytes = vec![];
        resp.body_mut().as_reader().read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        fs::write(&to, bytes).map_err(|e| e.to_string())?;
        return Ok(to);
    }
    let ogg = work.join(format!("{job}.opus"));
    let src = if uri.starts_with("http") { format!("souphttpsrc location=\"{uri}\"") } else { format!("filesrc location=\"{}\"", local_path.clone().unwrap_or_default().display()) };
    if run(&format!("{src} ! matroskademux ! opusparse ! oggmux ! filesink location=\"{}\"", ogg.display())).is_ok() {
        return Ok(ogg);
    }
    let flac = work.join(format!("{job}.flac"));
    run(&format!("uridecodebin uri=\"{uri}\" ! audioconvert ! flacenc ! filesink location=\"{}\"", flac.display()))?;
    Ok(flac)
}

/// Progress, 0 to 1, as the work goes (called from worker threads).
pub type Progress = std::sync::Arc<dyn Fn(f64) + Send + Sync>;

/// Runs `cmd`, passing its "progress N" lines (0-100) to `report` as they come;
/// returns its last other line (the "files ..." line).
fn run_reporting(mut cmd: Command, stdin: Option<&Path>, report: impl Fn(f64)) -> Result<String, String> {
    use std::io::{BufRead, BufReader};
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.stdin(match stdin {
        Some(p) => Stdio::from(fs::File::open(p).map_err(|e| e.to_string())?),
        None => Stdio::null(),
    });
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let mut err = child.stderr.take().ok_or("no stderr")?;
    let errs = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = err.read_to_string(&mut text);
        text
    });
    let mut last = String::new();
    for line in BufReader::new(child.stdout.take().ok_or("no stdout")?).lines().map_while(Result::ok) {
        match line.strip_prefix("progress ").and_then(|n| n.trim().parse::<f64>().ok()) {
            Some(n) => report((n / 100.0).clamp(0.0, 1.0)),
            None if !line.trim().is_empty() => last = line.trim().to_string(),
            None => {}
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let errs = errs.join().unwrap_or_default();
    if status.success() {
        Ok(last)
    } else {
        Err(errs.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("the splitter stopped").trim().to_string())
    }
}

fn ssh_command(server: &SplitServer, command: &str) -> Command {
    let mut cmd = Command::new("ssh");
    cmd.args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=10", &server.host, command]);
    cmd
}

/// The server's half (or whole) of a split: up, split (reporting), and back into `into`.
fn remote_part(server: &SplitServer, remote: &str, file: &Path, ext: &str, until: Option<f64>, model: &str, into: &Path, report: impl Fn(f64)) -> Result<Vec<String>, String> {
    let range = until.map(|e| format!(" --to {e:.2}")).unwrap_or_default() + &format!(" --model {model}");
    let command = format!(
        "set -e; mkdir -p {remote}; cat > {remote}/in.{ext}; chown -R {user}: {remote}; \
         sudo -H -u {user} nice -n 10 {python} {script} {remote}/in.{ext} {remote}/out{range}",
        user = server.user,
        python = server.python,
        script = server.script,
    );
    let said = run_reporting(ssh_command(server, &command), Some(file), report)?;
    let files: Vec<String> = said.strip_prefix("files ").ok_or_else(|| format!("the split server said: {said}"))?.split_whitespace().map(String::from).collect();
    fs::create_dir_all(into).map_err(|e| e.to_string())?;
    for name in &files {
        let bytes = ssh(server, &format!("cat {remote}/out/{name}"), None)?;
        if bytes.len() < 1024 {
            return Err(format!("{name} came back empty"));
        }
        fs::write(into.join(name), bytes).map_err(|e| e.to_string())?;
    }
    let _ = ssh(server, &format!("rm -rf {remote}"), None); // the job's own temporary files
    Ok(files)
}

/// Moves finished halves (vocals.*, music.*) from `dir` to where the song keeps them.
/// Where a part file ("drums.ogg", "vocals.flac"…) of a song is kept.
fn part_home(t: &Track, file: &str) -> Option<PathBuf> {
    let (part, ext) = file.split_once('.')?;
    let base = stems_base(t);
    Some(base.with_file_name(format!("{}.{part}.{ext}", base.file_name()?.to_string_lossy())))
}

fn keep(t: &Track, dir: &Path, files: &[String]) -> Result<(), String> {
    for name in files {
        let to = part_home(t, name).ok_or("no place to keep it")?;
        if let Some(d) = to.parent() {
            fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        let part = to.with_extension("part");
        fs::copy(dir.join(name), &part).map_err(|e| e.to_string())?;
        fs::rename(&part, &to).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// One song on two machines: the server does the first 60 % (it's quicker),
/// this computer the rest, overlapping by 4 s; then the parts are joined here,
/// crossfaded across that overlap (both split the same sound there: no seam).
#[allow(clippy::too_many_arguments)]
fn shared(t: &Track, server: &SplitServer, local: &LocalSplitter, file: &Path, ext: &str, remote: &str, work: &Path, report: &Progress) -> Result<(), String> {
    const SHARE: f64 = 0.6;
    const OVERLAP: f64 = 2.0;
    let cut = (t.seconds as f64 * SHARE).round();
    let (a_dir, b_dir, out_dir) = (work.join("a"), work.join("b"), work.join("out"));
    for d in [&a_dir, &b_dir, &out_dir] {
        let _ = fs::remove_dir_all(d);
    }
    let done = std::sync::Arc::new(std::sync::Mutex::new((0.0f64, 0.0f64)));
    let tell = |d: &std::sync::Arc<std::sync::Mutex<(f64, f64)>>| {
        let (a, b) = *d.lock().unwrap();
        report(0.05 + 0.88 * (SHARE * a + (1.0 - SHARE) * b));
    };
    // the server's part, alongside
    let (srv, rem, f, e, ad, d2, rep) = (server.clone(), format!("{remote}-a"), file.to_path_buf(), ext.to_string(), a_dir.clone(), done.clone(), report.clone());
    let model = model_for(t);
    let server_part = std::thread::spawn(move || {
        remote_part(&srv, &rem, &f, &e, Some(cut + OVERLAP), model, &ad, |p| {
            d2.lock().unwrap().0 = p;
            let (a, b) = *d2.lock().unwrap();
            rep(0.05 + 0.88 * (SHARE * a + (1.0 - SHARE) * b));
        })
    });
    // this computer's part, at idle priority (see `here`)
    let mut cmd = here(local);
    cmd.arg(&local.script).arg(file).arg(&b_dir).args(["--from", &format!("{:.2}", cut - OVERLAP)]).args(["--model", model]);
    let mine = run_reporting(cmd, None, |p| {
        done.lock().unwrap().1 = p;
        tell(&done);
    });
    let theirs = server_part.join().map_err(|_| "the server's part stopped")?;
    mine?;
    theirs?;
    // joined
    let mut join = Command::new(&local.python);
    join.arg(&local.script).arg("--join").arg(&out_dir).arg(&a_dir).arg(&b_dir).arg(format!("{cut:.2}")).arg(format!("{OVERLAP:.2}"));
    let said = run_reporting(join, None, |_| {})?;
    let files: Vec<String> = said.strip_prefix("files ").ok_or("the join didn't finish")?.split_whitespace().map(String::from).collect();
    // every part the model makes, or it isn't done (the server then splits it alone)
    let missing = lacks(t, &files.iter().map(String::as_str).collect::<Vec<_>>());
    if !missing.is_empty() {
        return Err(format!("the joined song lacks its {}", missing.join(", ")));
    }
    keep(t, &out_dir, &files)?;
    for d in [&a_dir, &b_dir, &out_dir] {
        let _ = fs::remove_dir_all(d);
    }
    report(1.0);
    Ok(())
}

/// A command for this computer's splitter: at idle priority (whatever you're
/// doing always goes first), on at most the cores you allow it (gauged
/// 2026-10-27 on a 4-core laptop: 2 cores this way slow other apps by ~60 %).
fn here(local: &LocalSplitter) -> Command {
    let threads = local.threads.to_string();
    if let Some(models) = crate::setup::bundled_models() {
        // Windows: the installer's Python, at idle priority with no console window, its models
        // in the app's folder and never fetched
        let mut cmd = Command::new(&local.python);
        crate::setup::quiet_window(&mut cmd, true);
        cmd.env("HF_HOME", models.join("huggingface")).env("HF_HUB_OFFLINE", "1").env("TORCH_HOME", models.join("torch")).env("PYTHONUTF8", "1");
        cmd.env("SPLIT_THREADS", &threads).env("ALIGN_THREADS", &threads).env("OMP_NUM_THREADS", &threads).env("MKL_NUM_THREADS", &threads);
        return cmd;
    }
    let idle = Path::new("/usr/bin/chrt").exists();
    let mut cmd = Command::new(if idle { "/usr/bin/chrt" } else { local.python.as_str() });
    if idle {
        cmd.args(["--idle", "0", "nice", "-n", "19"]).arg(&local.python);
    }
    cmd.env("SPLIT_THREADS", &threads).env("ALIGN_THREADS", &threads).env("OMP_NUM_THREADS", &threads).env("MKL_NUM_THREADS", &threads);
    cmd
}

/// The whole song split on this computer, no server (worker thread).
fn split_here(t: &Track, local: &LocalSplitter, file: &Path, work: &Path, report: &Progress) -> Result<(), String> {
    let out = work.join("here");
    let _ = fs::remove_dir_all(&out);
    let mut cmd = here(local);
    cmd.arg(&local.script).arg(file).arg(&out).arg("--model").arg(model_for(t));
    let said = run_reporting(cmd, None, |p| report(0.05 + 0.9 * p))?;
    let files: Vec<String> = said.strip_prefix("files ").ok_or_else(|| format!("the splitter said: {said}"))?.split_whitespace().map(String::from).collect();
    keep(t, &out, &files)?;
    let _ = fs::remove_dir_all(&out);
    Ok(())
}

/// Every word's time from MMS on this computer (worker thread; a minute or
/// two): `given` as align_on_server. The Lyricsfile it makes.
pub fn align_here(t: &Track, local: &LocalSplitter, given: &serde_json::Value) -> Result<String, String> {
    let vocals = ready_part(t, "vocals").ok_or("no Vocals part")?;
    let script = Path::new(&local.script).with_file_name("align.py");
    let work = glib::user_cache_dir().join("radiance-music/split-work").join(format!("{}-align", job_id(t)));
    fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let (given_file, out) = (work.join("lyrics.json"), work.join("lyrics.lyricsfile.yaml"));
    fs::write(&given_file, given.to_string()).map_err(|e| e.to_string())?;
    let mut cmd = here(local);
    cmd.arg(&script).arg(&vocals).arg(&out);
    let done = run_reporting(cmd, Some(&given_file), |_| {}).and_then(|_| fs::read_to_string(&out).map_err(|e| e.to_string()));
    let _ = fs::remove_dir_all(&work);
    done
}

/// Splits a song (worker thread). `uri`: where it plays from now (a file, the
/// Internet Archive, or the local YouTube relay). With a `server`, it splits
/// there (with `local`, this computer takes part of the song); without one,
/// on this computer alone.
pub fn split(t: &Track, uri: &str, server: Option<&SplitServer>, local: Option<&LocalSplitter>, report: Progress) -> Result<(), String> {
    let base = stems_base(t);
    let dir = base.parent().ok_or("no folder")?.to_path_buf();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let job = job_id(t);
    let work = glib::user_cache_dir().join("radiance-music/split-work").join(&job);
    fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let Some(server) = server else {
        let local = local.ok_or("splitting isn't set up on this computer")?;
        report(0.0);
        let file = upload_file(uri, &work, &job)?;
        report(0.05);
        let result = split_here(t, local, &file, &work, &report);
        let _ = fs::remove_dir_all(&work);
        report(1.0);
        return result;
    };
    let remote = format!("{}/{job}", server.work);
    report(0.0);
    // 0. never split a song twice: its halves may be on the server already (a
    //    split finished while the app was closed, or its download failed), or
    //    a split of it may be running there right now (wait for that one)
    let mut waited = 0;
    loop {
        let probe = format!(
            "cd {remote}/out 2>/dev/null && v=$(ls vocals.flac vocals.ogg 2>/dev/null | head -n 1) && m=$(ls music.flac music.ogg 2>/dev/null | head -n 1) \
             && [ -n \"$v\" ] && [ -n \"$m\" ] && echo \"files $(ls *.flac *.ogg 2>/dev/null | tr '\\n' ' ')\" && exit 0; \
             pgrep -f \"[s]plit.py {remote}/\" >/dev/null && echo running && exit 0; echo none"
        );
        let state = String::from_utf8_lossy(&ssh(server, &probe, None)?).trim().to_string();
        if let Some(files) = state.strip_prefix("files ") {
            if lacks(t, &files.split_whitespace().collect::<Vec<_>>()).is_empty() {
                return fetch(t, server, &remote, files);
            }
            break; // an older split of it, with fewer parts: split it again
        }
        // ([s]plit.py: so the search can't find this very command and wait forever)
        if state != "running" || waited >= 600 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_secs(15));
        waited += 15;
    }
    // 1. the song, small
    let file = upload_file(uri, &work, &job)?;
    let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("flac").to_string();
    let ours = file.starts_with(&work);
    report(0.05);
    // 2. two machines when this one can help, else the server alone
    let mut result = Err(String::new());
    if let Some(local) = local.filter(|_| t.seconds >= 60) {
        result = shared(t, server, local, &file, &ext, &remote, &work, &report);
        if let Err(e) = &result {
            if std::env::var_os("RADIANCE_DEBUG").is_some() {
                eprintln!("[split] shared split failed, the server does it all: {e}");
            }
        }
    }
    if result.is_err() {
        let out = work.join("server");
        result = remote_part(server, &remote, &file, &ext, None, model_for(t), &out, |p| report(0.05 + 0.88 * p)).and_then(|files| keep(t, &out, &files));
        let _ = fs::remove_dir_all(&out);
    }
    if ours {
        let _ = fs::remove_file(&file);
    }
    let _ = fs::remove_dir_all(&work);
    report(1.0);
    result
}

/// Brings the two halves home; only once both are safely here does the
/// server's copy go (so a failed download is fetched again, never re-split).
fn fetch(t: &Track, server: &SplitServer, remote: &str, files: &str) -> Result<(), String> {
    for name in files.split_whitespace() {
        let to = part_home(t, name).ok_or("no place to keep it")?;
        if let Some(dir) = to.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let bytes = ssh(server, &format!("cat {remote}/out/{name}"), None)?;
        if bytes.len() < 1024 {
            return Err(format!("{name} came back empty"));
        }
        let part = to.with_extension("part");
        fs::write(&part, bytes).map_err(|e| e.to_string())?;
        fs::rename(&part, &to).map_err(|e| e.to_string())?;
    }
    let _ = ssh(server, &format!("rm -rf {remote}"), None); // the job's own temporary files
    Ok(())
}

/// A song's own short name on the server (its jobs and its library folder).
fn job_id(t: &Track) -> String {
    format!("{:x}", t.key().bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3)))
}

/// Keeps a split song on the server for hosting (worker thread): in its own
/// folder of the library, its parts, its lyrics (`lyrics`: a Lyricsfile, or
/// plain text), its cover and info.json with what it is. Files already there
/// at the same size aren't sent again. All over the LAN; nothing is deleted.
pub fn host_copy(t: &Track, server: &SplitServer, lyrics: Option<(&str, String)>) -> Result<(), String> {
    let dir = format!("{}/{}", server.library, job_id(t));
    let listed = ssh(server, &format!("mkdir -p {dir} && cd {dir} && stat -c '%s %n' -- * 2>/dev/null || true"), None)?;
    let there: std::collections::HashMap<String, u64> = String::from_utf8_lossy(&listed)
        .lines()
        .filter_map(|l| l.split_once(' ').and_then(|(size, name)| Some((name.to_string(), size.parse().ok()?))))
        .collect();
    let send = |name: &str, path: &Path| -> Result<(), String> {
        let size = fs::metadata(path).map(|m| m.len()).map_err(|e| e.to_string())?;
        if there.get(name) == Some(&size) {
            return Ok(());
        }
        ssh(server, &format!("cat > {dir}/.{name}.part && mv -f {dir}/.{name}.part {dir}/{name}"), Some(path)).map(|_| ())
    };
    for part in PARTS.iter().copied().chain(["music"]) {
        if let Some(p) = ready_part(t, part) {
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("ogg").to_string();
            send(&format!("{part}.{ext}"), &p)?;
        }
    }
    if let Some(cover) = t.file.as_deref().map(|f| crate::library::beside(f, "jpg")).filter(|p| p.exists()) {
        send("cover.jpg", &cover)?;
    }
    let tmp = glib::user_cache_dir().join("radiance-music/host").join(job_id(t));
    fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    if let Some((name, text)) = lyrics {
        let p = tmp.join(name);
        fs::write(&p, text).map_err(|e| e.to_string())?;
        send(name, &p)?;
    }
    let info = serde_json::json!({
        "key": t.key(), "title": t.title, "artists": t.artists(), "credit": t.credit(), "album": t.album,
        "seconds": t.seconds, "source": t.to_json()["source"], "id": t.id, "format": t.format,
    });
    let p = tmp.join("info.json");
    fs::write(&p, serde_json::to_string_pretty(&info).unwrap_or_default()).map_err(|e| e.to_string())?;
    send("info.json", &p)?;
    let _ = fs::remove_dir_all(&tmp);
    ssh(server, &format!("chown -R {}: {dir}", server.user), None).map(|_| ())
}

/// The songs (keys) whose copy the server's library already has.
pub fn hosted() -> std::collections::HashSet<String> {
    crate::db::hosted()
}

pub fn mark_hosted(key: &str) {
    crate::db::add_hosted(key);
}

/// Each word's time for a split song's lyrics, worked out on the server from its
/// Vocals part in the library (MMS forced alignment: server/align.py, next to
/// split.py). `given`: {"lines": [{start, end, text}...]} or {"plain": "..."}.
/// The Lyricsfile comes back; the library keeps it too (worker thread; a few
/// minutes for a song).
pub fn align_on_server(t: &Track, server: &SplitServer, given: &serde_json::Value) -> Result<String, String> {
    let ext = ready_part(t, "vocals").and_then(|p| p.extension().and_then(|e| e.to_str()).map(String::from)).ok_or("no Vocals part")?;
    let dir = format!("{}/{}", server.library, job_id(t));
    let script = Path::new(&server.script).with_file_name("align.py");
    let tmp = glib::user_cache_dir().join("radiance-music/host").join(format!("{}-align.json", job_id(t)));
    if let Some(d) = tmp.parent() {
        fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    fs::write(&tmp, given.to_string()).map_err(|e| e.to_string())?;
    let said = ssh(
        server,
        &format!(
            "cd {dir} && sudo -H -u {user} env ALIGN_THREADS=4 nice -n 10 {python} {script} vocals.{ext} lyrics.lyricsfile.yaml",
            user = server.user,
            python = server.python,
            script = script.display()
        ),
        Some(&tmp),
    );
    let _ = fs::remove_file(&tmp);
    String::from_utf8(said?).map_err(|e| e.to_string())
}
