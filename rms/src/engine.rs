//! What does the splitting: the queue (split.rs) hands each song to an engine
//! and shows what it says. The real one runs in this process, no Python (owner,
//! 2026-11-09: "nababagalan ako sa python", #72); the stand-in is the tests'.
//!
//! Every call blocks (the queue runs it on a blocking thread); `stop` is set
//! when the song is taken out of the queue, and the work ends with
//! `Err(REMOVED)` before its next step.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use ort::environment::Environment;
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;
use serde_json::Value;

use crate::{audio, demucs, fetch, mms};

/// What a stopped job ends with.
pub const REMOVED: &str = "removed";

pub type Progress<'a> = &'a (dyn Fn(f64) + Send + Sync);
/// How far the first start's downloads are (0-1), and what's arriving.
pub type FetchProgress<'a> = &'a (dyn Fn(f64, &str) + Send + Sync);

pub trait SplitEngine: Send + Sync {
    /// The set of models it needs, as kept in models/.ready: another set there
    /// (the Python splitter's, an older one) is fetched again.
    fn models_id(&self) -> String;
    /// Whether it can work here: what it runs on (for the log), or why not.
    fn check(&self) -> Result<String, String>;
    /// Gets its models (and what runs them) at the first start.
    fn fetch(&self, progress: FetchProgress) -> Result<(), String>;
    /// `song`'s parts into `out` (made if missing): their files' names
    /// (vocals, music, drums, bass, other).
    fn split(&self, song: &Path, out: &Path, progress: Progress, stop: &AtomicBool) -> Result<Vec<String>, String>;
    /// Each word's time in `vocals` for the lyrics `given` ({"lines": [...]} or
    /// {"plain": "..."}, as lyrics::plan makes them): the Lyricsfile's text.
    fn align(&self, vocals: &Path, given: &Value, progress: Progress, stop: &AtomicBool) -> Result<String, String>;
    /// The queue is empty: let go of what it holds (the graphics card's memory, for the owner's games).
    fn rest(&self) {}
}

/// Until a server has a real engine: says so, and splits nothing.
pub struct Unbuilt;

impl SplitEngine for Unbuilt {
    fn models_id(&self) -> String {
        "none".into()
    }
    fn check(&self) -> Result<String, String> {
        Err("this server was built without a splitter".into())
    }
    fn fetch(&self, _: FetchProgress) -> Result<(), String> {
        Err("this server was built without a splitter".into())
    }
    fn split(&self, _: &Path, _: &Path, _: Progress, _: &AtomicBool) -> Result<Vec<String>, String> {
        Err("this server was built without a splitter".into())
    }
    fn align(&self, _: &Path, _: &Value, _: Progress, _: &AtomicBool) -> Result<String, String> {
        Err("this server was built without a splitter".into())
    }
}

/// The tests' engine: splits at once into small made-up parts, takes its time
/// over a song with "Slow" in its name (until it's stopped), and notes in
/// `dir` what it was given (split-args.txt, given.json, fetches.txt, stopped.txt).
pub struct StandIn {
    dir: PathBuf,
    works: bool,
}

impl StandIn {
    pub fn new(dir: PathBuf, works: bool) -> StandIn {
        StandIn { dir, works }
    }

    fn note(&self, file: &str, line: &str) {
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(self.dir.join(file)) {
            let _ = writeln!(f, "{line}");
        }
    }
}

impl SplitEngine for StandIn {
    fn rest(&self) {
        self.note("rests.txt", "rested");
    }

    fn models_id(&self) -> String {
        "stand-in".into()
    }

    fn check(&self) -> Result<String, String> {
        if self.works {
            Ok("the stand-in".into())
        } else {
            Err("no splitter here (the stand-in)".into())
        }
    }

    fn fetch(&self, progress: FetchProgress) -> Result<(), String> {
        self.note("fetches.txt", "fetched");
        progress(0.5, "Downloading the stand-in's models…");
        Ok(())
    }

    fn split(&self, song: &Path, out: &Path, progress: Progress, stop: &AtomicBool) -> Result<Vec<String>, String> {
        let name = song.display().to_string();
        let steps = if name.contains("Slow") { 200 } else { 4 };
        for i in 0..steps {
            if stop.load(Ordering::Relaxed) {
                self.note("stopped.txt", &name);
                return Err(REMOVED.into());
            }
            std::thread::sleep(Duration::from_millis(50));
            progress((i + 1) as f64 / steps as f64);
        }
        std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
        let mut names = Vec::new();
        for p in ["vocals", "music", "drums", "bass", "other"] {
            let file = format!("{p}.ogg");
            std::fs::write(out.join(&file), vec![p.len() as u8; 4096]).map_err(|e| e.to_string())?;
            names.push(file);
        }
        self.note("split-args.txt", &format!("args {name}"));
        Ok(names)
    }

    fn align(&self, _vocals: &Path, given: &Value, progress: Progress, stop: &AtomicBool) -> Result<String, String> {
        std::fs::write(self.dir.join("given.json"), given.to_string()).map_err(|e| e.to_string())?;
        if stop.load(Ordering::Relaxed) {
            return Err(REMOVED.into());
        }
        progress(0.4);
        Ok("version: \"1.0\"\n# word times: MMS forced alignment (Meta, MMS_FA) on the song's Vocals part\nlines:\n  - text: \"hello there\"\n    start_ms: 1000\n    end_ms: 2000\n    words:\n      - text: \"hello \"\n        start_ms: 1000\n        end_ms: 1500\n      - text: \"there\"\n        start_ms: 1500\n        end_ms: 2000\n".into())
    }
}

/// ONNX Runtime's library, as fetch.rs keeps it.
#[cfg(windows)]
const RUNTIME: &str = "onnxruntime.dll";
#[cfg(not(windows))]
const RUNTIME: &str = "libonnxruntime.so";
const DEMUCS: &str = "htdemucs_fp16weights.onnx";
const MMS: &str = "mms_fa_fp16w.onnx";
/// The graphics card's free memory a song needs (Demucs and MMS, with room to spare), in MB.
const CARD_ROOM_MB: u64 = 2_500;

/// Whether a song goes on the graphics card: the card's runtime is here and the
/// card has room now (a game using it keeps it; a desktop's own use doesn't).
pub fn use_card(card_runtime: bool, free_mb: Option<u64>) -> bool {
    card_runtime && free_mb.is_some_and(|mb| mb >= CARD_ROOM_MB)
}

/// The NVIDIA card's free memory (MB), when there is one.
fn card_free_mb() -> Option<u64> {
    let mut c = std::process::Command::new("nvidia-smi");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let out = c.args(["--query-gpu=memory.free", "--format=csv,noheader,nounits"]).stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null()).output().ok()?;
    String::from_utf8_lossy(&out.stdout).lines().next()?.trim().parse().ok()
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn SetDllDirectoryW(path: *const u16) -> i32;
}

/// What a session runs on.
#[derive(Clone, Copy, PartialEq, Debug)]
enum On {
    Card,
    Processor,
}

#[derive(Default)]
struct Held {
    env: Option<Environment>,
    demucs: Option<(Session, On)>,
    mms: Option<(Session, On)>,
}

/// The real engine (owner, 2026-11-09: "Rust, no Python", #72): ONNX Runtime
/// fetched into models/, Demucs and MMS as ONNX, kept loaded from song to song
/// (let go when the queue empties), on the graphics card when there's an NVIDIA
/// one with room (Windows), else the processor with RMS_THREADS.
pub struct Onnx {
    models: PathBuf,
    threads: usize,
    windows: bool,
    nvidia: bool,
    held: Mutex<Held>,
}

impl Onnx {
    pub fn new(models: PathBuf, threads: usize) -> Onnx {
        let windows = cfg!(windows);
        // the graphics card's build of ONNX Runtime: Windows with an NVIDIA card (Linux: the processor, for now)
        let nvidia = windows && card_free_mb().is_some();
        Onnx { models, threads: threads.max(1), windows, nvidia, held: Mutex::new(Held::default()) }
    }

    fn card_runtime(&self) -> bool {
        self.models.join(if cfg!(windows) { "onnxruntime_providers_cuda.dll" } else { "libonnxruntime_providers_cuda.so" }).is_file()
    }

    fn on(&self) -> On {
        if use_card(self.card_runtime(), card_free_mb()) {
            On::Card
        } else {
            On::Processor
        }
    }

    /// ONNX Runtime, loaded once (only after it's been fetched: a failed load can't be tried again).
    fn env<'a>(&self, held: &'a mut Held) -> Result<&'a Environment, String> {
        if held.env.is_none() {
            let path = self.models.join(RUNTIME);
            if !path.is_file() {
                return Err("ONNX Runtime isn't downloaded yet".into());
            }
            #[cfg(windows)]
            {
                // what onnxruntime.dll needs (the C++ runtime, CUDA, cuDNN) is beside it
                use std::os::windows::ffi::OsStrExt;
                let wide: Vec<u16> = self.models.as_os_str().encode_wide().chain([0]).collect();
                unsafe { SetDllDirectoryW(wide.as_ptr()) };
            }
            let env = ort::init_from(&path).map_err(|e| format!("ONNX Runtime didn't load: {e}"))?.build().map_err(|e| format!("ONNX Runtime didn't start: {e}"))?;
            held.env = Some(env);
        }
        Ok(held.env.as_ref().expect("just made"))
    }

    fn session(&self, env: &Environment, model: &str, on: On) -> Result<Session, String> {
        let mut b = Session::builder(env)
            .map_err(|e| e.to_string())?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| e.to_string())?
            .with_intra_threads(self.threads)
            .map_err(|e| e.to_string())?
            .with_inter_threads(1)
            .map_err(|e| e.to_string())?;
        if on == On::Card {
            b = b.with_execution_providers([ort::ep::CUDA::default().build().error_on_failure()]).map_err(|e| format!("the graphics card: {e}"))?;
        }
        b.commit_from_file(self.models.join(model)).map_err(|e| format!("{model}: {e}"))
    }

    /// `work` with `model`'s session on `on` (made, or kept from the last song).
    fn with<T>(&self, model: &str, on: On, work: impl FnOnce(&mut Session) -> Result<T, String>) -> Result<T, String> {
        let mut held = self.held.lock().unwrap_or_else(|e| e.into_inner());
        let kept_on = if model == DEMUCS { held.demucs.as_ref().map(|(_, o)| *o) } else { held.mms.as_ref().map(|(_, o)| *o) };
        if kept_on != Some(on) {
            // the other one first (the card's memory), then this one again
            if model == DEMUCS {
                held.demucs = None;
            } else {
                held.mms = None;
            }
            let session = {
                let env = self.env(&mut held)?;
                self.session(env, model, on)?
            };
            if model == DEMUCS {
                held.demucs = Some((session, on));
            } else {
                held.mms = Some((session, on));
            }
        }
        let slot = if model == DEMUCS { &mut held.demucs } else { &mut held.mms };
        work(&mut slot.as_mut().expect("just made").0)
    }

    /// `work` on the card when it has room; if the card fails, again on the processor.
    fn either<T>(&self, what: &str, mut work: impl FnMut(On) -> Result<T, String>) -> Result<T, String> {
        let on = self.on();
        match work(on) {
            Err(e) if on == On::Card && e != REMOVED => {
                crate::log(&format!("{what} on the graphics card didn't work ({e}); on the processor instead"));
                let mut held = self.held.lock().unwrap_or_else(|e| e.into_inner());
                held.demucs = None;
                held.mms = None;
                drop(held);
                work(On::Processor)
            }
            r => r,
        }
    }

    fn tools(&self) -> Result<audio::Tools, String> {
        audio::tools(&self.models).ok_or_else(|| "ffmpeg isn't installed".to_string())
    }
}

fn run_demucs(session: &mut Session, piece: &[f32]) -> Result<Vec<f32>, String> {
    let input = Tensor::from_array(([1usize, 2, demucs::N], piece.to_vec())).map_err(|e| e.to_string())?;
    let outputs = session.run(ort::inputs![input]).map_err(|e| e.to_string())?;
    let (_, y) = outputs[0].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
    Ok(y.to_vec())
}

fn run_mms(session: &mut Session, wav: &[f32]) -> Result<(Vec<f32>, usize), String> {
    let input = Tensor::from_array(([1usize, wav.len()], wav.to_vec())).map_err(|e| e.to_string())?;
    let outputs = session.run(ort::inputs![input]).map_err(|e| e.to_string())?;
    let (shape, y) = outputs[0].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
    let frames = if shape.len() >= 2 { shape[shape.len() - 2] as usize } else { 0 };
    Ok((y.to_vec(), frames))
}

impl SplitEngine for Onnx {
    fn models_id(&self) -> String {
        format!("onnx ort-{} {} htdemucs_fp16weights mms_fa_fp16w", fetch::ORT, if self.nvidia { "cuda12" } else { "cpu" })
    }

    fn check(&self) -> Result<String, String> {
        if !(cfg!(target_arch = "x86_64") && (cfg!(windows) || cfg!(target_os = "linux"))) {
            return Err("the splitter runs on 64-bit Windows or Linux on an Intel or AMD processor".into());
        }
        // Windows: ffmpeg comes with the models when there's none
        if !self.windows && audio::tools(&self.models).is_none() {
            return Err("ffmpeg isn't installed: the Docker image and install.sh bring it".into());
        }
        Ok(if self.nvidia {
            format!("ONNX Runtime {}: the graphics card when it has room, else {} threads", fetch::ORT, self.threads)
        } else {
            format!("ONNX Runtime {}: the processor, {} threads", fetch::ORT, self.threads)
        })
    }

    fn fetch(&self, progress: FetchProgress) -> Result<(), String> {
        let items = fetch::wanted(self.windows, self.nvidia, audio::tools(&self.models).is_some());
        fetch::fetch_all(&self.models, &items, progress)
    }

    fn split(&self, song: &Path, out: &Path, progress: Progress, stop: &AtomicBool) -> Result<Vec<String>, String> {
        let t = self.tools()?;
        self.either("splitting", |on| {
            if on == On::Card {
                crate::log("splitting on the graphics card");
            }
            self.with(DEMUCS, on, |s| demucs::split(&t, &mut |piece| run_demucs(s, piece), song, out, progress, stop))
        })
    }

    fn align(&self, vocals: &Path, given: &Value, progress: Progress, stop: &AtomicBool) -> Result<String, String> {
        let t = self.tools()?;
        // as align.py: the Vocals part at its own rate, its two sides' mean, to 16 kHz as torchaudio does
        let x = audio::decode(&t, vocals, demucs::RATE, 2)?;
        let mono: Vec<f32> = x.chunks_exact(2).map(|c| (c[0] + c[1]) / 2.0).collect();
        let wav = mms::resample(&mono, demucs::RATE, mms::RATE);
        self.either("word timing", |on| self.with(MMS, on, |s| mms::align(&mut |w| run_mms(s, w), &wav, given, progress, stop)))
    }

    fn rest(&self) {
        let mut held = self.held.lock().unwrap_or_else(|e| e.into_inner());
        held.demucs = None;
        held.mms = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_card_only_with_room() {
        assert!(use_card(true, Some(4_800)));
        assert!(use_card(true, Some(2_500)));
        assert!(!use_card(true, Some(1_900)), "a game has it");
        assert!(!use_card(true, None), "no card");
        assert!(!use_card(false, Some(6_000)), "no card runtime fetched");
    }

    #[test]
    fn the_engine_says_what_it_runs_on() {
        let e = Onnx::new(PathBuf::from("/nowhere"), 4);
        assert_eq!(e.check().unwrap(), format!("ONNX Runtime {}: the processor, 4 threads", fetch::ORT));
        assert_eq!(e.models_id(), format!("onnx ort-{} cpu htdemucs_fp16weights mms_fa_fp16w", fetch::ORT));
        // nothing fetched: it says so, and can still be asked again later
        let stop = AtomicBool::new(false);
        let e2 = e.split(Path::new("/nowhere/a.flac"), Path::new("/nowhere/out"), &|_| {}, &stop).unwrap_err();
        assert!(e2.contains("couldn't read a.flac") || e2.contains("isn't downloaded"), "{e2}");
    }

    /// The real downloads, from their real addresses, checked (by hand: RMS_REAL_MODELS, about 800 MB).
    #[test]
    #[ignore]
    fn the_real_downloads() {
        let models = PathBuf::from(std::env::var("RMS_REAL_MODELS").expect("RMS_REAL_MODELS"));
        std::fs::create_dir_all(&models).unwrap();
        let e = Onnx::new(models.clone(), 4);
        let last = std::sync::Mutex::new(String::new());
        e.fetch(&|p, what| {
            let mut l = last.lock().unwrap();
            if *l != what.split(" (").next().unwrap_or("") {
                *l = what.split(" (").next().unwrap_or("").to_string();
                eprintln!("{:.0} % {what}", p * 100.0);
            }
        })
        .unwrap();
        for f in [RUNTIME, DEMUCS, MMS] {
            assert!(models.join(f).is_file(), "{f}");
        }
    }

    /// The real models against today's Python (ribs02, by hand): RMS_REAL_MODELS (fetched models),
    /// RMS_REAL_SONG, RMS_REAL_REF (split.py's vocals.ogg and align.py's words.yaml of that song,
    /// and the given.json it was given).
    #[test]
    #[ignore]
    fn the_real_models_match_todays_python() {
        let var = |k: &str| PathBuf::from(std::env::var(k).unwrap_or_else(|_| panic!("{k}")));
        let (models, song, refs) = (var("RMS_REAL_MODELS"), var("RMS_REAL_SONG"), var("RMS_REAL_REF"));
        let e = Onnx::new(models.clone(), 4);
        let out = std::env::temp_dir().join(format!("rms-real-{}", std::process::id()));
        let stop = AtomicBool::new(false);
        let t0 = std::time::Instant::now();
        let names = e.split(&song, &out, &|_| {}, &stop).unwrap();
        eprintln!("split in {:.1} s: {names:?}", t0.elapsed().as_secs_f64());
        let t = audio::tools(&models).unwrap();
        let ours = audio::decode(&t, &out.join(&names[0]), demucs::RATE, 2).unwrap();
        let reference = ["vocals.flac", "vocals.ogg"].iter().map(|n| refs.join(n)).find(|p| p.is_file()).expect("the reference vocals");
        let theirs = audio::decode(&t, &reference, demucs::RATE, 2).unwrap();
        let n = ours.len().min(theirs.len());
        let (sig, err) = (0..n).fold((0.0f64, 0.0f64), |(s, e), i| (s + (theirs[i] as f64).powi(2), e + ((theirs[i] - ours[i]) as f64).powi(2)));
        let db = 10.0 * (sig / err.max(1e-12)).log10();
        eprintln!("vocals against PyTorch's: {db:.1} dB");
        // PyTorch's Demucs shifts the song at random each time: two of its own runs on this clip differ by
        // 25.7 dB, and ours is 22-25 dB from either (2026-11-09, Avercage - Embers)
        assert!(db >= 20.0, "{db}");
        let given: Value = serde_json::from_str(&std::fs::read_to_string(refs.join("given.json")).unwrap()).unwrap();
        let t0 = std::time::Instant::now();
        let yaml = e.align(&reference, &given, &|_| {}, &stop).unwrap();
        eprintln!("timed in {:.1} s", t0.elapsed().as_secs_f64());
        let starts = |y: &str| y.lines().filter(|l| l.trim_start().starts_with("start_ms:")).map(|l| l.trim().trim_start_matches("start_ms:").trim().parse::<i64>().unwrap()).collect::<Vec<_>>();
        let (a, b) = (starts(&yaml), starts(&std::fs::read_to_string(refs.join("words.yaml")).unwrap()));
        assert_eq!(a.len(), b.len());
        let close = a.iter().zip(&b).filter(|(x, y)| (*x - *y).abs() <= 20).count();
        eprintln!("{close} of {} starts within 20 ms of align.py's", a.len());
        assert!(close as f64 >= 0.95 * a.len() as f64);
    }
}
