//! The engine's controller (issue #45), as the Windows app drives it: its own thread, search,
//! Home, and (Task 6) playback. No network and no sound: fake sources, RADIANCE_SILENT, and a
//! data folder of its own under TMPDIR. One test at a time (one controller per process).

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, Once};
use std::time::{Duration, Instant};

use radiance_engine::controller::{Change, Controller, ListRef, Listener};
use radiance_engine::lyrics::Lyrics;
use radiance_engine::sources::Sources;
use radiance_engine::track::{Source, Track};
use radiance_engine::youtube::Stream;
use radiance_core::words::Kind;

static SETUP: Once = Once::new();
static LOCK: Mutex<()> = Mutex::new(());

/// A data folder of its own (before anything asks GLib where it is), and no sound.
fn setup() -> std::sync::MutexGuard<'static, ()> {
    SETUP.call_once(|| {
        let base = std::env::temp_dir().join(format!("radiance-controller-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        for (var, dir) in [("XDG_DATA_HOME", "data"), ("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache")] {
            let d = base.join(dir);
            std::fs::create_dir_all(&d).unwrap();
            std::env::set_var(var, d);
        }
        std::env::set_var("RADIANCE_SILENT", "1");
    });
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn song(source: Source, id: &str, title: &str) -> Track {
    Track { source, id: id.into(), title: title.into(), artist: "Test Artist".into(), album: String::new(), seconds: 200, art: String::new(), format: String::new(), file: None, video: false, genres: None }
}

/// Sources that answer from the test: by query, with a wait if asked; counted.
#[derive(Default)]
pub struct Fake {
    pub found: Mutex<HashMap<(String, &'static str), Vec<Track>>>,
    pub waits: Mutex<HashMap<String, Duration>>,
    pub offline: std::sync::atomic::AtomicBool,
    pub searches: AtomicUsize,
    pub streams: Mutex<HashMap<String, (Duration, Result<String, String>)>>,
    pub lyrics: Mutex<Option<Lyrics>>,
}

fn kind_name(k: Kind) -> &'static str {
    match k {
        Kind::Lossless => "lossless",
        Kind::Songs => "songs",
        Kind::Videos => "videos",
    }
}

impl Fake {
    pub fn finds(&self, kind: Kind, q: &str, tracks: Vec<Track>) {
        self.found.lock().unwrap().insert((q.to_string(), kind_name(kind)), tracks);
    }
}

impl Sources for Fake {
    fn search(&self, kind: Kind, q: &str) -> Vec<Track> {
        self.searches.fetch_add(1, Ordering::SeqCst);
        let wait = self.waits.lock().unwrap().get(q).copied(); // (not held while it waits)
        if let Some(d) = wait {
            std::thread::sleep(d);
        }
        self.found.lock().unwrap().get(&(q.to_string(), kind_name(kind))).cloned().unwrap_or_default()
    }
    fn reachable(&self) -> bool {
        !self.offline.load(Ordering::SeqCst)
    }
    fn stream(&self, id: &str) -> Result<Stream, String> {
        let (wait, answer) = self.streams.lock().unwrap().get(id).cloned().unwrap_or((Duration::ZERO, Err("no stream in this test".into())));
        std::thread::sleep(wait);
        answer.map(|url| Stream { url, size: 0, mime: "audio/wav".into(), kbps: 128 })
    }
    fn radio(&self, _seed: &Track) -> Vec<Track> {
        Vec::new()
    }
    fn lyrics(&self, _t: &Track) -> Lyrics {
        self.lyrics.lock().unwrap().clone().unwrap_or(Lyrics::None)
    }
}

/// Records every change, and when.
pub struct Heard {
    tx: Mutex<Sender<(Change, Instant)>>,
}

impl Listener for Heard {
    fn changed(&self, what: Change) {
        let _ = self.tx.lock().unwrap().send((what, Instant::now()));
    }
}

pub fn listener() -> (Arc<Heard>, Receiver<(Change, Instant)>) {
    let (tx, rx) = channel();
    (Arc::new(Heard { tx: Mutex::new(tx) }), rx)
}

/// Waits (up to `secs`) for `what`; false if it never came.
pub fn wait_for(rx: &Receiver<(Change, Instant)>, what: Change, secs: f64) -> bool {
    let end = Instant::now() + Duration::from_secs_f64(secs);
    while let Some(left) = end.checked_duration_since(Instant::now()) {
        match rx.recv_timeout(left) {
            Ok((c, _)) if c == what => return true,
            Ok(_) => {}
            Err(_) => return false,
        }
    }
    false
}

fn ids(v: &Option<Vec<Track>>) -> Vec<String> {
    v.as_ref().map(|v| v.iter().map(|t| t.id.clone()).collect()).unwrap_or_default()
}

#[test]
fn a_search_fills_both_lists() {
    let _l = setup();
    let fake = Arc::new(Fake::default());
    fake.finds(Kind::Lossless, "binhi", vec![song(Source::Archive, "https://archive.example/a.flac", "Binhi Acoustic")]);
    fake.finds(Kind::Songs, "binhi", vec![song(Source::YouTube, "y1", "Binhi"), song(Source::YouTube, "y2", "Binhi Cover")]);
    let (heard, rx) = listener();
    let c = Controller::start(heard, fake.clone()).unwrap();
    c.search("binhi");
    let end = Instant::now() + Duration::from_secs(5);
    while (c.results().lossless.is_none() || c.results().songs.is_none()) && Instant::now() < end {
        wait_for(&rx, Change::Results, 0.5);
    }
    let r = c.results();
    assert_eq!(r.query, "binhi");
    assert_eq!(ids(&r.lossless), ["https://archive.example/a.flac"]);
    assert_eq!(ids(&r.songs), ["y1", "y2"]);
    assert!(r.mine.is_empty());
    c.quit();
}

#[test]
fn an_older_search_never_replaces_a_newer_one() {
    let _l = setup();
    let fake = Arc::new(Fake::default());
    fake.finds(Kind::Songs, "bi", vec![song(Source::YouTube, "old", "Bi")]);
    fake.finds(Kind::Songs, "binhi", vec![song(Source::YouTube, "new", "Binhi")]);
    fake.waits.lock().unwrap().insert("bi".into(), Duration::from_millis(500));
    let (heard, _rx) = listener();
    let c = Controller::start(heard, fake.clone()).unwrap();
    c.search("bi");
    c.search("binhi");
    std::thread::sleep(Duration::from_millis(1000));
    let r = c.results();
    assert_eq!(r.query, "binhi");
    assert_eq!(ids(&r.songs), ["new"]);
    c.quit();
}

#[test]
fn one_letter_clears_the_results() {
    let _l = setup();
    let fake = Arc::new(Fake::default());
    let (heard, rx) = listener();
    let c = Controller::start(heard, fake.clone()).unwrap();
    c.search("b");
    assert!(wait_for(&rx, Change::Results, 2.0));
    let r = c.results();
    assert_eq!((r.lossless.map(|v| v.len()), r.songs.map(|v| v.len())), (Some(0), Some(0)));
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(fake.searches.load(Ordering::SeqCst), 0);
    c.quit();
}

#[test]
fn nothing_found_and_no_internet_means_offline() {
    let _l = setup();
    let fake = Arc::new(Fake::default());
    fake.offline.store(true, Ordering::SeqCst);
    let (heard, rx) = listener();
    let c = Controller::start(heard, fake.clone()).unwrap();
    assert!(c.online());
    c.search("binhi");
    assert!(wait_for(&rx, Change::Online, 3.0));
    assert!(!c.online());
    c.quit();
}

#[test]
fn home_has_the_greeting_and_the_recent_plays() {
    let _l = setup();
    radiance_engine::db::add_play(&song(Source::YouTube, "first", "First"), 1_000);
    radiance_engine::db::add_play(&song(Source::YouTube, "second", "Second"), 2_000);
    let fake = Arc::new(Fake::default());
    let (heard, _rx) = listener();
    let c = Controller::start(heard, fake).unwrap();
    let h = c.home();
    assert!(h.greeting.starts_with("Good "), "{}", h.greeting);
    let recent: Vec<_> = h.recent.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(&recent[..2], ["second", "first"]);
    assert_eq!(h.mix_note, "Like a few songs and a mix made from them shows up here.");
    c.quit();
}

#[test]
fn a_second_controller_is_refused() {
    let _l = setup();
    let (heard, _rx) = listener();
    let c = Controller::start(heard.clone(), Arc::new(Fake::default())).unwrap();
    assert!(Controller::start(heard.clone(), Arc::new(Fake::default())).is_err());
    c.quit();
    // and once it's gone, another can start
    let again = Controller::start(heard, Arc::new(Fake::default())).unwrap();
    again.quit();
    let _ = ListRef::Songs;
}
