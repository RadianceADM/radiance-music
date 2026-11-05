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
    // (other tests' plays share this data folder: these two, newest first)
    let at = |id: &str| h.recent.iter().position(|t| t.id == id);
    assert!(at("second").is_some() && at("first").is_some(), "{:?}", h.recent.iter().map(|t| &t.id).collect::<Vec<_>>());
    assert!(at("second") < at("first"));
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

// ---------------------------------------------------------------------------------- playback (Task 6)

use radiance_engine::controller::LyricsState;
use radiance_engine::lyrics::{Line, Word};
use radiance_core::queue::Repeat;

/// A quiet `secs`-second WAV (a soft sine; silent anyway: RADIANCE_SILENT), named for its test.
fn wav(name: &str, secs: f64) -> Track {
    let dir = std::env::temp_dir().join(format!("radiance-controller-test-{}", std::process::id())).join("songs");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.wav"));
    let rate = 44_100u32;
    let n = (secs * rate as f64) as u32;
    let mut b = Vec::with_capacity(44 + 2 * n as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + 2 * n).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&1u16.to_le_bytes()); // mono
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&(2 * n).to_le_bytes());
    for i in 0..n {
        let s = (0.05 * (i as f64 * 440.0 * std::f64::consts::TAU / rate as f64).sin() * 32767.0) as i16;
        b.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(&path, b).unwrap();
    let mut t = song(Source::YouTube, &format!("file-{name}"), &format!("Song {name}"));
    t.seconds = secs.ceil() as u32;
    t.file = Some(path.display().to_string());
    t
}

/// A controller whose Search's YouTube list is `songs` (searched as `q`).
fn with_songs(q: &str, songs: Vec<Track>) -> (Controller, Arc<Fake>, Receiver<(Change, Instant)>) {
    let fake = Arc::new(Fake::default());
    fake.finds(Kind::Songs, q, songs);
    let (heard, rx) = listener();
    let c = Controller::start(heard, fake.clone()).unwrap();
    c.search(q);
    let end = Instant::now() + Duration::from_secs(5);
    while c.results().songs.as_ref().is_none_or(|v| v.is_empty()) && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(20));
    }
    (c, fake, rx)
}

/// Waits (up to `secs`) until `ok` says yes.
fn until(secs: f64, ok: impl Fn() -> bool) -> bool {
    let end = Instant::now() + Duration::from_secs_f64(secs);
    while Instant::now() < end {
        if ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    ok()
}

fn now_id(c: &Controller) -> String {
    c.now().song.map(|t| t.id).unwrap_or_default()
}

#[test]
fn a_list_plays_and_moves_on() {
    let _l = setup();
    let (c, _f, rx) = with_songs("files one", vec![wav("move-a", 1.5), wav("move-b", 1.5)]);
    c.play_from(ListRef::Songs, 0, "yt:file-move-a");
    assert!(until(3.0, || c.now().playing), "never played");
    assert_eq!(now_id(&c), "file-move-a");
    assert!(wait_for(&rx, Change::Position, 2.0));
    assert!(until(2.0, || c.now().at > 0.4), "the time didn't move");
    assert!(until(4.0, || now_id(&c) == "file-move-b"), "didn't go on to the next song");
    assert!(c.now().can_previous);
    c.quit();
}

#[test]
fn the_song_picked_last_wins() {
    let _l = setup();
    let late = wav("late-stream", 2.0);
    let mut slow = song(Source::YouTube, "slow", "Slow to find");
    slow.seconds = 2;
    let (c, fake, _rx) = with_songs("files two", vec![slow, wav("picked-last", 3.0)]);
    fake.streams.lock().unwrap().insert("slow".into(), (Duration::from_millis(400), Ok(format!("file://{}", late.file.unwrap()))));
    c.play_from(ListRef::Songs, 0, "yt:slow");
    c.play_from(ListRef::Songs, 1, "yt:file-picked-last");
    std::thread::sleep(Duration::from_millis(1000));
    assert_eq!(now_id(&c), "file-picked-last");
    assert!(c.now().playing);
    c.quit();
}

#[test]
fn a_song_that_wont_play_is_skipped_then_it_stops_after_three() {
    let _l = setup();
    let gone: Vec<Track> = (1..=4).map(|i| song(Source::YouTube, &format!("gone{i}"), &format!("Gone {i}"))).collect();
    let (c, _f, rx) = with_songs("files three", gone);
    c.play_from(ListRef::Songs, 0, "yt:gone1");
    let mut problems = 0;
    let end = Instant::now() + Duration::from_secs(6);
    while problems < 3 && Instant::now() < end {
        if wait_for(&rx, Change::Problem, 1.0) {
            problems += 1;
        }
    }
    assert_eq!(problems, 3);
    assert!(until(1.0, || c.queue().1 == 2));
    assert!(!c.now().playing);
    assert_eq!(c.problem().map(|p| p.text).as_deref(), Some("3 songs in a row wouldn't play: its source didn't answer"));
    assert!(!wait_for(&rx, Change::Problem, 0.7), "a fourth try");
    c.quit();
}

#[test]
fn back_restarts_after_three_seconds() {
    let _l = setup();
    let (c, _f, _rx) = with_songs("files four", vec![wav("back-a", 2.0), wav("back-b", 6.0)]);
    c.play_from(ListRef::Songs, 1, "yt:file-back-b");
    assert!(until(6.0, || c.now().at >= 3.5), "never got to 3.5 s");
    c.skip(-1);
    assert!(until(1.0, || c.now().at < 1.0), "didn't go back to the start");
    assert_eq!(now_id(&c), "file-back-b");
    c.quit();
}

#[test]
fn repeat_one_plays_it_again() {
    let _l = setup();
    let (c, _f, _rx) = with_songs("files five", vec![wav("again", 1.5), wav("never", 1.5)]);
    c.cycle_repeat();
    c.cycle_repeat();
    assert!(until(1.0, || c.now().repeat == Repeat::One));
    c.play_from(ListRef::Songs, 0, "yt:file-again");
    assert!(until(3.0, || c.now().at >= 1.0));
    assert!(until(3.0, || c.now().playing && c.now().at < 0.7), "didn't start again");
    assert_eq!(now_id(&c), "file-again");
    c.cycle_repeat();
    c.quit();
}

#[test]
fn the_session_comes_back_paused() {
    let _l = setup();
    let (c, _f, _rx) = with_songs("files six", vec![wav("kept", 6.0)]);
    c.play_from(ListRef::Songs, 0, "yt:file-kept");
    assert!(until(3.0, || c.now().at >= 0.5));
    c.seek(2.0);
    assert!(until(1.0, || c.now().at >= 2.0));
    c.quit();
    let (heard, _rx) = listener();
    let again = Controller::start(heard, Arc::new(Fake::default())).unwrap();
    let n = again.now();
    assert_eq!(n.song.map(|t| t.id).as_deref(), Some("file-kept"));
    assert!(!n.playing);
    assert!(n.at >= 2.0);
    again.toggle_play();
    assert!(until(3.0, || again.now().playing && again.now().at >= 2.0), "didn't carry on from where it was");
    again.quit();
}

#[test]
fn lyrics_lines_light_with_the_song() {
    let _l = setup();
    let (c, fake, rx) = with_songs("files seven", vec![wav("sung", 3.0)]);
    let line = |s: f64, e: f64, t: &str| Line { start: s, end: e, text: t.into(), words: vec![Word { start: s, end: e, text: t.into() }], singer: None };
    *fake.lyrics.lock().unwrap() = Some(Lyrics::Timed { lines: vec![line(0.0, 1.0, "one"), line(1.2, 2.5, "two")], exact: false });
    c.play_from(ListRef::Songs, 0, "yt:file-sung");
    assert!(wait_for(&rx, Change::Lyrics, 3.0)); // looking for them…
    assert!(until(3.0, || matches!(c.lyrics(), LyricsState::Lines(l) if l.len() == 2)), "{:?}", c.lyrics());
    assert!(until(2.0, || c.now().at > 0.2 && c.now().lit == [0]));
    assert!(until(3.0, || c.now().lit.contains(&1)));
    c.quit();
}

#[test]
fn art_files_are_cached_pngs() {
    let _l = setup();
    let dir = std::env::temp_dir().join(format!("radiance-controller-test-{}", std::process::id()));
    let cover = dir.join("cover.png");
    image::RgbaImage::from_pixel(32, 32, image::Rgba([200, 90, 40, 255])).save(&cover).unwrap();
    let (heard, _rx) = listener();
    let c = Controller::start(heard, Arc::new(Fake::default())).unwrap();
    let a = c.art_file(&cover.display().to_string(), 300).expect("an art file");
    assert!(a.extension().is_some_and(|e| e == "png") && a.exists());
    assert_eq!(c.art_file(&cover.display().to_string(), 300), Some(a));
    let (wash, blob) = c.backdrop_files(&cover.display().to_string()).expect("the backdrop's files");
    assert!(wash.exists() && blob.exists());
    assert_eq!(c.art_file("/no/such/cover.jpg", 300), None);
    c.quit();
}

// ---------------------------------------------------------------------------------- the review's fixes

#[test]
fn next_while_picked_up_starts_the_next_song_from_its_start() {
    let _l = setup();
    let (c, _f, _rx) = with_songs("files eight", vec![wav("picked-a", 6.0), wav("picked-b", 6.0)]);
    c.play_from(ListRef::Songs, 0, "yt:file-picked-a");
    assert!(until(4.0, || c.now().at >= 1.6), "A never got to 1.6 s");
    c.quit();
    let (heard, _rx) = listener();
    let again = Controller::start(heard, Arc::new(Fake::default())).unwrap();
    assert!(!again.now().playing); // picked up, paused, at A's place
    assert!(again.now().at >= 1.6);
    again.skip(1);
    assert!(until(3.0, || now_id(&again) == "file-picked-b" && again.now().playing), "Next didn't start the next song");
    assert!(until(1.0, || again.now().at > 0.0));
    assert!(again.now().at < 1.2, "the next song started at the last one's place: {}", again.now().at);
    again.toggle_play();
    assert!(until(1.0, || !again.now().playing), "Pause didn't pause");
    again.quit();
}

#[test]
fn try_again_starts_over_after_three_failures() {
    let _l = setup();
    let gone: Vec<Track> = (1..=4).map(|i| song(Source::YouTube, &format!("again{i}"), &format!("Again {i}"))).collect();
    let (c, _f, rx) = with_songs("files nine", gone);
    c.play_from(ListRef::Songs, 0, "yt:again1");
    assert!(until(6.0, || c.problem().is_some_and(|p| p.retry)), "never stopped");
    assert_eq!(c.problem().unwrap().text, "3 songs in a row wouldn't play: its source didn't answer");
    while rx.try_recv().is_ok() {}
    c.retry();
    assert!(wait_for(&rx, Change::Problem, 3.0));
    let p = c.problem().unwrap();
    assert_eq!(p.text, "“Again 3” can't play: its source didn't answer. On to the next song.");
    assert!(!p.retry, "a note that goes on to the next song has no Try Again");
    c.quit();
}

#[test]
fn a_click_plays_the_song_clicked_even_if_the_list_moved() {
    let _l = setup();
    let (c, _f, _rx) = with_songs("files ten", vec![wav("moved-a", 3.0), wav("moved-b", 3.0), wav("moved-c", 3.0)]);
    // the click was on c, at index 1 as the screen showed it then
    c.play_from(ListRef::Songs, 1, "yt:file-moved-c");
    assert!(until(3.0, || now_id(&c) == "file-moved-c"));
    // a song that's no longer in the list: nothing happens
    c.play_from(ListRef::Songs, 0, "yt:nowhere");
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(now_id(&c), "file-moved-c");
    c.quit();
}

#[test]
fn mute_keeps_the_saved_volume() {
    let _l = setup();
    let (heard, _rx) = listener();
    let c = Controller::start(heard, Arc::new(Fake::default())).unwrap();
    c.set_volume(0.4);
    assert!(until(1.0, || (c.now().volume - 0.4).abs() < 1e-9));
    c.set_muted(true);
    assert!(until(1.0, || c.now().muted));
    assert!((c.now().volume - 0.4).abs() < 1e-9, "muting changed the volume");
    assert!((radiance_engine::settings::load().volume - 0.4).abs() < 1e-9, "muting saved 0");
    c.set_muted(false);
    assert!(until(1.0, || !c.now().muted));
    assert!((c.now().volume - 0.4).abs() < 1e-9);
    c.quit();
}
