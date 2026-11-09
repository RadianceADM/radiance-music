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
use std::time::Duration;

use serde_json::Value;

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
