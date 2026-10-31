//! The split queue: one song at a time, at the lowest priority (nice 19), with
//! the apps' own scripts. split.py makes the parts (drums, bass, other,
//! vocals, and music: everything but the voice); then align.py times each
//! word of the song's lyrics from its Vocals part (MMS). The parts land beside
//! the song in library/, named as the apps name them, for everyone.
//!
//! The models download at the server's first start into the data folder
//! (owner, 2026-10-31: "download on first open"; never baked into the image),
//! with their progress on the web page. Songs wait in the queue until then.

use std::ffi::OsString;
use std::io::Write;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::Json;
use rusqlite::{params, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Notify;

use crate::auth::Who;
use crate::library::{self, put};
use crate::{conflict, forbidden, lyrics, not_found, now_ms, ApiResult, Shared};

/// The models' download: Demucs (about 80 MB) and MMS (about 1.2 GB), as the app's setup.rs.
const MODELS_BYTES: f64 = 1.3e9;
/// (its "progress 6": the first is in, the second starts)
const MODELS_PY: &str = "
from demucs.pretrained import get_model
get_model('htdemucs')
print('progress 6', flush=True)
import torchaudio
torchaudio.pipelines.MMS_FA.get_model(with_star=False)
print('ok', flush=True)
";
/// What the splitter needs to be there (the app's own check).
const CHECK_PY: &str = "import torch, torchaudio, demucs, soundfile, av; print('ok', torch.__version__)";
/// The share of a job's bar that's the split (the rest: timing its words).
const SPLIT_SHARE: f64 = 0.85;
const REMOVED: &str = "removed";

#[derive(Clone, Default)]
struct Status {
    checked_ms: i64,
    ready: bool,
    message: String,
}

#[derive(Clone)]
struct Models {
    /// waiting | downloading | ready | failed
    state: &'static str,
    progress: f64,
    message: String,
    failed_ms: i64,
}

impl Default for Models {
    fn default() -> Self {
        Models { state: "waiting", progress: 0.0, message: "Waiting for the splitter to be checked.".into(), failed_ms: 0 }
    }
}

/// The queue's helper: whether the splitter is installed, the models' download, and the job at work.
#[derive(Default)]
pub struct Splitter {
    /// A song was queued or removed: the worker looks again.
    pub wake: Notify,
    status: Mutex<Status>,
    models: Mutex<Models>,
    running: Mutex<Option<(i64, Arc<Notify>)>>,
}

impl Splitter {
    fn status(&self) -> Status {
        self.status.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn set_status(&self, ready: bool, message: String) {
        *self.status.lock().unwrap_or_else(|e| e.into_inner()) = Status { checked_ms: now_ms(), ready, message };
    }

    fn models(&self) -> Models {
        self.models.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn set_models(&self, f: impl FnOnce(&mut Models)) {
        f(&mut self.models.lock().unwrap_or_else(|e| e.into_inner()));
    }

    pub fn status_json(&self) -> Value {
        let s = self.status();
        let message = if s.checked_ms == 0 { "Checking the splitter…".to_string() } else { s.message };
        json!({ "ready": s.ready, "checked": s.checked_ms > 0, "message": message })
    }

    pub fn models_json(&self) -> Value {
        let m = self.models();
        json!({ "state": m.state, "progress": (m.progress * 1000.0).round() / 1000.0, "message": m.message })
    }

    /// Why a waiting song waits, in plain words (None: it's just in line).
    fn holdup(&self) -> Option<String> {
        let s = self.status();
        if s.checked_ms == 0 {
            return Some("Waiting: the splitter is being checked.".into());
        }
        if !s.ready {
            return Some("Waiting: the splitter isn't installed on this server yet.".into());
        }
        let m = self.models();
        match m.state {
            "ready" => None,
            "downloading" => Some(format!("Waiting for the splitting models to download ({:.0} %).", m.progress * 100.0)),
            "failed" => Some("Waiting: the splitting models' download stopped; it tries again in a minute.".into()),
            _ => Some("Waiting for the splitting models.".into()),
        }
    }
}

/// The splitter's Python, at the lowest priority, its models in the data folder.
fn python(app: &Shared, args: &[OsString]) -> Command {
    let nice = ["/usr/bin/nice", "/bin/nice"].into_iter().find(|p| Path::new(p).exists());
    let mut cmd = match nice {
        Some(n) => {
            let mut c = Command::new(n);
            c.args(["-n", "19"]).arg(&app.cfg.python);
            c
        }
        None => Command::new(&app.cfg.python),
    };
    cmd.args(args);
    let models = app.cfg.models();
    let threads = app.cfg.threads.to_string();
    cmd.env("TORCH_HOME", models.join("torch"))
        .env("HF_HOME", models.join("huggingface"))
        .env("XDG_CACHE_HOME", models.join("cache"))
        .env("SPLIT_THREADS", &threads)
        .env("ALIGN_THREADS", &threads)
        .env("OMP_NUM_THREADS", &threads)
        .env("MKL_NUM_THREADS", &threads)
        .env("PYTHONUNBUFFERED", "1")
        .env("PYTHONUTF8", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    cmd
}

/// The splitter's log (data/logs/split.log), started over past 5 MB.
fn log_file(app: &Shared) -> Option<std::fs::File> {
    let p = app.cfg.data.join("logs").join("split.log");
    if std::fs::metadata(&p).is_ok_and(|m| m.len() > 5 << 20) {
        let _ = std::fs::remove_file(&p);
    }
    std::fs::OpenOptions::new().create(true).append(true).open(p).ok()
}

/// Runs one Python step to its end. Its "progress N" lines (0-100; on stdout,
/// or stderr for align.py) go to `report`; its last other stdout line comes
/// back (split.py's "files …"). `stop`: the job was removed.
async fn run_step(mut cmd: Command, stdin: Option<&Path>, progress_on_stderr: bool, report: &(dyn Fn(f64) + Send + Sync), stop: Option<&Notify>, log: &mut Option<std::fs::File>) -> Result<String, String> {
    if let Some(p) = stdin {
        cmd.stdin(std::fs::File::open(p).map_err(|e| e.to_string())?);
    }
    let mut child = cmd.spawn().map_err(|e| format!("couldn't start the splitter's Python: {e}"))?;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(bool, String)>();
    if let Some(out) = child.stdout.take() {
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                let _ = tx.send((false, l));
            }
        });
    }
    if let Some(err) = child.stderr.take() {
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                let _ = tx.send((true, l));
            }
        });
    }
    drop(tx);
    let (mut last, mut last_err) = (String::new(), String::new());
    let never = Notify::new();
    let stop = stop.unwrap_or(&never);
    loop {
        tokio::select! {
            line = rx.recv() => {
                let Some((is_err, l)) = line else { break };
                let progress = l.trim().strip_prefix("progress ").and_then(|n| n.trim().parse::<f64>().ok());
                match progress {
                    Some(n) if is_err == progress_on_stderr => report((n / 100.0).clamp(0.0, 1.0)),
                    _ if l.trim().is_empty() => {}
                    _ if is_err => {
                        if let Some(f) = log.as_mut() {
                            let _ = writeln!(f, "{l}");
                        }
                        last_err = l.trim().to_string();
                    }
                    _ => last = l.trim().to_string(),
                }
            }
            _ = stop.notified() => {
                let _ = child.kill().await;
                return Err(REMOVED.into());
            }
        }
    }
    let status = child.wait().await.map_err(|e| e.to_string())?;
    if status.success() {
        Ok(last)
    } else if last_err.is_empty() {
        Err(format!("the splitter stopped ({status})"))
    } else {
        Err(last_err)
    }
}

/// Whether the splitter is installed (its scripts, and Python with Demucs and PyTorch).
async fn check(app: &Shared) -> bool {
    let scripts = &app.cfg.scripts;
    if !scripts.join("split.py").is_file() || !scripts.join("align.py").is_file() {
        app.splitter.set_status(false, format!("The splitter's scripts (split.py and align.py) aren't in {}. Set RMS_SCRIPTS to their folder.", scripts.display()));
        return false;
    }
    let cmd = python(app, &["-c".into(), CHECK_PY.into()]);
    let mut none = None;
    let ok = tokio::time::timeout(Duration::from_secs(300), run_step(cmd, None, false, &|_| {}, None, &mut none)).await;
    match ok {
        Ok(Ok(said)) if said.starts_with("ok") => {
            if !app.splitter.status().ready {
                let version = said.trim_start_matches("ok").trim();
                crate::log(&if version.is_empty() { "the splitter is ready".to_string() } else { format!("the splitter is ready (PyTorch {version})") });
            }
            app.splitter.set_status(true, "Ready.".into());
            true
        }
        other => {
            let why = match other {
                Ok(Err(e)) => e,
                Ok(Ok(said)) => said,
                Err(_) => "it took too long to answer".into(),
            };
            app.splitter.set_status(
                false,
                format!(
                    "The splitter isn't installed on this server yet: Python with Demucs and PyTorch (RMS_PYTHON is {}; it said: {why}). Songs wait in the queue until it is. The Docker image comes with it; install.sh sets it up.",
                    app.cfg.python
                ),
            );
            false
        }
    }
}

fn models_ready(app: &Shared) -> bool {
    app.cfg.models().join(".ready").exists()
}

fn folder_bytes(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    entries.flatten().map(|e| match e.metadata() {
        Ok(m) if m.is_dir() => folder_bytes(&e.path()),
        Ok(m) => m.len(),
        Err(_) => 0,
    }).sum()
}

/// Downloads the two models into data/models, its progress read from what's
/// arrived so far (the downloads write into that folder as they go).
async fn download_models(app: &Shared) {
    crate::log("downloading the splitting models (about 1.3 GB, once)");
    let start = folder_bytes(&app.cfg.models());
    app.splitter.set_models(|m| {
        m.state = "downloading";
        m.progress = 0.0;
        m.message = "Downloading the splitting model (about 80 MB)…".into();
    });
    let dir = app.cfg.models();
    let a = app.clone();
    let watcher = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let got = folder_bytes(&dir).saturating_sub(start) as f64;
            a.splitter.set_models(|m| m.progress = (got / MODELS_BYTES).min(0.99));
        }
    });
    let mut cmd = python(app, &["-c".into(), MODELS_PY.into()]);
    cmd.current_dir(app.cfg.models());
    let mut log = log_file(app);
    let a = app.clone();
    let second = move |p: f64| {
        if p > 0.05 {
            a.splitter.set_models(|m| m.message = "Downloading the word-timing model (about 1.2 GB)…".into());
        }
    };
    let result = run_step(cmd, None, false, &second, None, &mut log).await;
    watcher.abort();
    match result {
        Ok(said) if said.trim() == "ok" => {
            let _ = std::fs::write(app.cfg.models().join(".ready"), "htdemucs mms_fa\n");
            app.splitter.set_models(|m| {
                m.state = "ready";
                m.progress = 1.0;
                m.message = "Ready.".into();
            });
            crate::log("the splitting models are ready");
        }
        other => {
            let why = match other {
                Ok(said) => said,
                Err(e) => e,
            };
            crate::log(&format!("the models' download stopped: {why}"));
            app.splitter.set_models(|m| {
                m.state = "failed";
                m.message = format!("The models' download stopped ({why}). It tries again in a minute.");
                m.failed_ms = now_ms();
            });
        }
    }
}

async fn rest(app: &Shared, secs: u64) {
    tokio::select! {
        _ = app.splitter.wake.notified() => {}
        _ = tokio::time::sleep(Duration::from_secs(secs)) => {}
    }
}

/// The queue's worker: the splitter checked, its models downloaded (first
/// start), then one song after another.
pub async fn worker(app: Shared) {
    // songs that were being split when the server stopped start over
    let _ = app.db().execute("UPDATE jobs SET state = 'waiting', progress = 0, note = NULL, started_ms = NULL WHERE state IN ('splitting', 'timing')", []);
    if models_ready(&app) {
        app.splitter.set_models(|m| {
            m.state = "ready";
            m.progress = 1.0;
            m.message = "Ready.".into();
        });
    }
    loop {
        let s = app.splitter.status();
        let ready = if s.ready || now_ms() - s.checked_ms < 30_000 { s.ready } else { check(&app).await };
        if !ready {
            app.splitter.set_models(|m| {
                if m.state != "ready" {
                    m.state = "waiting";
                    m.message = "Waiting for the splitter to be installed.".into();
                }
            });
            rest(&app, 30).await;
            continue;
        }
        if !models_ready(&app) {
            let m = app.splitter.models();
            if m.state == "failed" && now_ms() - m.failed_ms < 60_000 {
                rest(&app, 10).await;
            } else {
                download_models(&app).await;
            }
            continue;
        }
        let next: Option<(i64, i64)> = {
            let conn = app.db();
            turns(&conn).ok().and_then(|t| t.first().copied()).and_then(|(job, _)| conn.query_row("SELECT id, song_id FROM jobs WHERE id = ?1", params![job], |r| Ok((r.get(0)?, r.get(1)?))).ok())
        };
        match next {
            Some((job, song)) => run(&app, job, song).await,
            None => rest(&app, 300).await,
        }
    }
}

fn set_job(app: &Shared, job: i64, state: &str, progress: f64, note: Option<&str>) {
    let now = now_ms();
    let _ = app.db().execute(
        "UPDATE jobs SET state = ?1, progress = ?2, note = ?3,
           started_ms = CASE WHEN ?1 = 'splitting' AND started_ms IS NULL THEN ?4 ELSE started_ms END,
           finished_ms = CASE WHEN ?1 IN ('done', 'failed', 'removed') THEN ?4 ELSE finished_ms END
         WHERE id = ?5 AND state != 'removed'",
        params![state, progress, note, now, job],
    );
}

async fn run(app: &Shared, job: i64, song: i64) {
    let stop = Arc::new(Notify::new());
    *app.splitter.running.lock().unwrap_or_else(|e| e.into_inner()) = Some((job, stop.clone()));
    let work = app.cfg.work().join(format!("job-{job}"));
    let result = split_one(app, job, song, &work, &stop).await;
    *app.splitter.running.lock().unwrap_or_else(|e| e.into_inner()) = None;
    let _ = std::fs::remove_dir_all(&work);
    match result {
        Ok(note) => set_job(app, job, "done", 1.0, Some(&note)),
        Err(e) if e == REMOVED => {}
        Err(e) => {
            crate::log(&format!("split {job} failed: {e}"));
            set_job(app, job, "failed", 0.0, Some(&format!("It didn't split: {e}")));
        }
    }
}

async fn split_one(app: &Shared, job: i64, song_id: i64, work: &Path, stop: &Notify) -> Result<String, String> {
    let song = crate::db::song(&app.db(), song_id).map_err(|e| e.to_string())?.ok_or("the song isn't in the library anymore")?;
    let dir = app.cfg.library();
    let input = dir.join(&song.file);
    if !input.is_file() {
        return Err("the song's file isn't in the library anymore".into());
    }
    let stem = song.file.rsplit_once('.').map(|(s, _)| s.to_string()).unwrap_or_else(|| song.file.clone());
    // what its lyrics need, known before (so the bar's share is right)
    let yaml = std::fs::read_to_string(dir.join(format!("{stem}.lyrics.yaml"))).ok();
    let lrc = std::fs::read_to_string(dir.join(format!("{stem}.lrc"))).ok();
    let plan = lyrics::plan(yaml.as_deref(), lrc.as_deref());
    let share = if matches!(plan, lyrics::Plan::Align { .. }) { SPLIT_SHARE } else { 1.0 };
    let _ = std::fs::remove_dir_all(work);
    std::fs::create_dir_all(work).map_err(|e| e.to_string())?;
    let mut log = log_file(app);
    if let Some(f) = log.as_mut() {
        let _ = writeln!(f, "== job {job}: {} ({})", song.file, now_ms());
    }
    set_job(app, job, "splitting", 0.0, None);
    crate::log(&format!("splitting {}", song.file));
    let shown = AtomicI64::new(-1);
    let report = |base: f64, span: f64, state: &'static str| {
        let a = app.clone();
        let shown = &shown;
        move |p: f64| {
            let p = base + span * p;
            let pct = (p * 100.0) as i64;
            if shown.swap(pct, Ordering::Relaxed) != pct {
                set_job(&a, job, state, p, None);
            }
        }
    };
    // 1. the parts
    let out = work.join("out");
    let cmd = python(app, &[app.cfg.scripts.join("split.py").into(), input.clone().into(), out.clone().into()]);
    let said = run_step(cmd, None, false, &report(0.0, share, "splitting"), Some(stop), &mut log).await?;
    let files: Vec<String> = said.strip_prefix("files ").ok_or_else(|| format!("the splitter said: {said}"))?.split_whitespace().map(String::from).collect();
    let missing: Vec<&str> = library::FOUR.iter().copied().filter(|p| !files.iter().any(|f| f.split('.').next() == Some(*p))).collect();
    if !missing.is_empty() {
        return Err(format!("the split lacks its {}", missing.join(", ")));
    }
    for name in &files {
        let from = out.join(name);
        if std::fs::metadata(&from).map(|m| m.len()).unwrap_or(0) == 0 {
            return Err(format!("{name} came out empty"));
        }
        put(&from, &dir.join(format!("{stem}.{name}"))).map_err(|e| format!("couldn't keep {name}: {e}"))?;
    }
    let mut note = "Split into drums, bass, other and vocals".to_string();
    // 2. each word's time, from its Vocals part
    match plan {
        lyrics::Plan::Skip(why) => note += &format!("; no word timing ({why})."),
        lyrics::Plan::Align { given, singers } => {
            set_job(app, job, "timing", share, None);
            shown.store((share * 100.0) as i64, Ordering::Relaxed);
            let vocals = files.iter().find(|f| f.starts_with("vocals.")).map(|f| dir.join(format!("{stem}.{f}"))).ok_or("no Vocals part")?;
            let given_file = work.join("lyrics.json");
            let aligned_file = work.join("lyrics.lyricsfile.yaml");
            std::fs::write(&given_file, given.to_string()).map_err(|e| e.to_string())?;
            let cmd = python(app, &[app.cfg.scripts.join("align.py").into(), vocals.into(), aligned_file.clone().into()]);
            match run_step(cmd, Some(&given_file), true, &report(share, 1.0 - share, "timing"), Some(stop), &mut log).await {
                Ok(_) => match std::fs::read_to_string(&aligned_file) {
                    Ok(aligned) if aligned.contains("lines:") => {
                        let kept = lyrics::finish(&aligned, singers.as_deref(), yaml.as_deref());
                        let tmp = work.join("lyrics.yaml");
                        std::fs::write(&tmp, kept).map_err(|e| e.to_string())?;
                        put(&tmp, &dir.join(format!("{stem}.lyrics.yaml"))).map_err(|e| e.to_string())?;
                        note += "; every word is timed.";
                    }
                    _ => note += "; the word timing found no words.",
                },
                Err(e) if e == REMOVED => return Err(e),
                Err(e) => note += &format!("; the word timing didn't work ({e})."),
            }
        }
    }
    crate::log(&format!("split {}", song.file));
    Ok(note)
}

/// The waiting songs in the order they'll be split: people take turns
/// (owner, 2026-11-01), one song each, whoever had a song split longest ago (or
/// never) first; each person's own songs in the order they asked for them.
/// (job, who asked)
pub fn turns(conn: &rusqlite::Connection) -> rusqlite::Result<Vec<(i64, Option<i64>)>> {
    let waiting: Vec<(i64, Option<i64>)> = conn.prepare("SELECT id, asked_by FROM jobs WHERE state = 'waiting' ORDER BY id")?.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
    // when each person's last turn began (a song being split now counts)
    let mut last: std::collections::HashMap<Option<i64>, i64> = conn
        .prepare("SELECT asked_by, MAX(started_ms) FROM jobs WHERE state != 'waiting' AND started_ms IS NOT NULL GROUP BY asked_by")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let mut lines: Vec<(Option<i64>, std::collections::VecDeque<i64>)> = vec![];
    for (job, who) in waiting {
        match lines.iter_mut().find(|(p, _)| *p == who) {
            Some((_, line)) => line.push_back(job),
            None => lines.push((who, [job].into())),
        }
    }
    let mut order = vec![];
    let mut turn = i64::MAX / 2;
    while let Some(pick) = lines.iter().enumerate().filter(|(_, (_, l))| !l.is_empty()).min_by_key(|(_, (p, l))| (last.get(p).copied().unwrap_or(-1), l[0])).map(|(i, _)| i) {
        let (who, line) = &mut lines[pick];
        order.push((line.pop_front().unwrap_or_default(), *who));
        last.insert(*who, turn);
        turn += 1;
    }
    Ok(order)
}

fn job_json(app: &Shared, row: &JobRow, ahead: Option<usize>) -> Value {
    let note = match (row.state.as_str(), &row.note) {
        ("waiting", _) => app.splitter.holdup().unwrap_or_else(|| match ahead {
            Some(0) | None => "Next up.".into(),
            Some(1) => "Waiting: 1 song before it.".into(),
            Some(n) => format!("Waiting: {n} songs before it."),
        }),
        ("splitting", _) => format!("Splitting ({:.0} %).", row.progress * 100.0),
        ("timing", _) => format!("Timing its words ({:.0} %).", row.progress * 100.0),
        (_, Some(n)) => n.clone(),
        (s, None) => s.to_string(),
    };
    json!({
        "id": row.id,
        "song": { "id": row.song, "artist": row.artist, "title": row.title },
        "asked_by": row.asked_by.as_ref().map(|(id, name)| json!({ "id": id, "name": name })),
        "state": row.state,
        "progress": (row.progress * 1000.0).round() / 1000.0,
        "note": note,
        "created_ms": row.created_ms,
        "started_ms": row.started_ms,
        "finished_ms": row.finished_ms,
    })
}

struct JobRow {
    id: i64,
    song: i64,
    artist: String,
    title: String,
    asked_by: Option<(i64, String)>,
    state: String,
    progress: f64,
    note: Option<String>,
    created_ms: i64,
    started_ms: Option<i64>,
    finished_ms: Option<i64>,
}

const JOB_SELECT: &str = "SELECT j.id, j.song_id, s.artist, s.title, j.asked_by, p.name, j.state, j.progress, j.note, j.created_ms, j.started_ms, j.finished_ms
    FROM jobs j JOIN songs s ON s.id = j.song_id LEFT JOIN people p ON p.id = j.asked_by";

fn job_row(r: &rusqlite::Row) -> rusqlite::Result<JobRow> {
    let by: Option<i64> = r.get(4)?;
    let name: Option<String> = r.get(5)?;
    Ok(JobRow { id: r.get(0)?, song: r.get(1)?, artist: r.get(2)?, title: r.get(3)?, asked_by: by.zip(name), state: r.get(6)?, progress: r.get(7)?, note: r.get(8)?, created_ms: r.get(9)?, started_ms: r.get(10)?, finished_ms: r.get(11)? })
}

#[derive(Deserialize)]
pub struct AskBody {
    song: Value,
}

/// POST /api/split {song}: the song goes in the queue (once: asking again
/// gives its place; a song split already says so).
pub async fn ask(State(app): State<Shared>, who: Who, Json(body): Json<AskBody>) -> ApiResult<(StatusCode, Json<Value>)> {
    if !who.can_split {
        return Err(crate::auth::switched_off(&app, "splitting songs"));
    }
    let song = library::resolve(&app, &body.song)?;
    let dir = app.cfg.library();
    let file = song.file.clone();
    let split = tokio::task::spawn_blocking(move || library::found(&dir, &file).is_some_and(|f| f.split())).await.unwrap_or(false);
    if split {
        return Ok((StatusCode::OK, Json(json!({ "already_split": true, "queued": false, "song": { "id": song.id, "artist": song.artist, "title": song.title } }))));
    }
    let (row, queued) = {
        let conn = app.db();
        let there: Option<i64> = conn.query_row("SELECT id FROM jobs WHERE song_id = ?1 AND state IN ('waiting', 'splitting', 'timing') ORDER BY id LIMIT 1", params![song.id], |r| r.get(0)).optional()?;
        let (id, queued) = match there {
            Some(id) => (id, false),
            None => {
                conn.execute("INSERT INTO jobs (song_id, asked_by, state, created_ms) VALUES (?1, ?2, 'waiting', ?3)", params![song.id, who.person, now_ms()])?;
                (conn.last_insert_rowid(), true)
            }
        };
        (conn.query_row(&format!("{JOB_SELECT} WHERE j.id = ?1"), params![id], job_row)?, queued)
    };
    if queued {
        crate::log(&format!("{} queued {} for splitting", who.name, song.file));
        app.splitter.wake.notify_one();
    }
    let ahead = turns(&app.db())?.iter().position(|(job, _)| *job == row.id);
    let status = if queued { StatusCode::CREATED } else { StatusCode::OK };
    Ok((status, Json(json!({ "already_split": false, "queued": queued, "job": job_json(&app, &row, ahead) }))))
}

/// GET /api/queue: the songs at work, the waiting ones in their turns (and
/// whose turn is next), the last day's finished ones, and the splitter's state
/// in plain words.
pub async fn queue(State(app): State<Shared>, _who: Who) -> ApiResult<Json<Value>> {
    let (mut rows, order): (Vec<JobRow>, Vec<(i64, Option<i64>)>) = {
        let conn = app.db();
        let order = turns(&conn)?;
        let mut st = conn.prepare(&format!(
            "{JOB_SELECT} WHERE j.state IN ('waiting', 'splitting', 'timing') OR (j.state IN ('done', 'failed') AND j.finished_ms > ?1)
             ORDER BY CASE j.state WHEN 'splitting' THEN 0 WHEN 'timing' THEN 0 WHEN 'waiting' THEN 1 ELSE 2 END,
                      CASE WHEN j.state IN ('done', 'failed') THEN -j.finished_ms ELSE j.id END
             LIMIT 200"
        ))?;
        let rows = st.query_map(params![now_ms() - 24 * 3600 * 1000], job_row)?.collect::<Result<_, _>>()?;
        (rows, order)
    };
    let place = |id: i64| order.iter().position(|(j, _)| *j == id);
    // running first, then the waiting ones in their turns, then the finished
    rows.sort_by_key(|r| match r.state.as_str() {
        "splitting" | "timing" => (0, 0),
        "waiting" => (1, place(r.id).unwrap_or(usize::MAX) as i64),
        _ => (2, -r.finished_ms.unwrap_or(0)),
    });
    let jobs: Vec<Value> = rows.iter().map(|r| job_json(&app, r, if r.state == "waiting" { place(r.id) } else { None })).collect();
    let next = order.first().and_then(|(job, _)| rows.iter().find(|r| r.id == *job)).and_then(|r| r.asked_by.as_ref()).map(|(id, name)| json!({ "id": id, "name": name }));
    Ok(Json(json!({ "splitter": app.splitter.status_json(), "models": app.splitter.models_json(), "threads": app.cfg.threads, "next_turn": next, "jobs": jobs })))
}

/// DELETE /api/queue/ID: a song comes out of the queue (stopped, if it's being
/// split). The owner, or whoever asked for it.
pub async fn remove(State(app): State<Shared>, who: Who, UrlPath(id): UrlPath<i64>) -> ApiResult<Json<Value>> {
    let found: Option<(String, Option<i64>)> = app.db().query_row("SELECT state, asked_by FROM jobs WHERE id = ?1", params![id], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
    let Some((state, asked_by)) = found else { return Err(not_found("That song isn't in the queue.")) };
    if !who.owner && asked_by != Some(who.person) {
        return Err(forbidden("Only the server's owner, or whoever asked for it, can take a song out of the queue."));
    }
    if !matches!(state.as_str(), "waiting" | "splitting" | "timing") {
        return Err(conflict("It's finished already."));
    }
    let _ = app.db().execute("UPDATE jobs SET state = 'removed', finished_ms = ?1 WHERE id = ?2", params![now_ms(), id]);
    if let Some((running, stop)) = app.splitter.running.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        if *running == id {
            stop.notify_one();
        }
    }
    app.splitter.wake.notify_one();
    Ok(Json(json!({ "ok": true })))
}
