//! The engine's controller (issue #45), as the Windows app drives it: its own thread, search,
//! Home, and (Task 6) playback. No network and no sound: fake sources, RADIANCE_SILENT, and a
//! data folder of its own under TMPDIR. One test at a time (one controller per process).

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, Once};
use std::time::{Duration, Instant};

use radiance_core::library::GroupKind;
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
        // a home of their own too: GLib's Trash is the one on the home's disk (unliking trashes files)
        std::env::set_var("HOME", &base);
        // saved songs go to a music folder of the tests' own
        let music = base.join("music");
        std::fs::create_dir_all(&music).unwrap();
        radiance_engine::library::set_music_dir(Some(music));
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
    /// How long a download (a liked song being saved) takes.
    pub download_wait: Mutex<Duration>,
    /// What a genre lookup finds.
    pub genres: Mutex<Vec<String>>,
    /// Songs' other versions (their music videos), by the song's id.
    pub counterparts: Mutex<HashMap<String, Track>>,
    /// Videos' streams (picture, sound), by the video's id.
    pub videos: Mutex<HashMap<String, Result<(String, String), String>>>,
    /// A seed's radio (by its id), and how long YouTube Music takes to answer.
    pub radios: Mutex<HashMap<String, (Duration, Vec<Track>)>>,
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
    fn radio(&self, seed: &Track) -> Vec<Track> {
        let (wait, list) = self.radios.lock().unwrap().get(&seed.id).cloned().unwrap_or_default();
        std::thread::sleep(wait);
        list
    }
    fn lyrics(&self, _t: &Track) -> Lyrics {
        self.lyrics.lock().unwrap().clone().unwrap_or(Lyrics::None)
    }
    fn download(&self, _url: &str, to: &std::path::Path) -> Result<(), String> {
        let wait = *self.download_wait.lock().unwrap();
        std::thread::sleep(wait);
        std::fs::write(to, b"fLaC a test song").map_err(|e| e.to_string())
    }
    fn genres(&self, _t: &Track) -> Vec<String> {
        self.genres.lock().unwrap().clone()
    }
    fn counterpart(&self, t: &Track) -> Option<Track> {
        self.counterparts.lock().unwrap().get(&t.id).cloned()
    }
    fn video_streams(&self, id: &str) -> Result<(Stream, Stream), String> {
        let answer = self.videos.lock().unwrap().get(id).cloned().unwrap_or_else(|| Err("no video in this test".into()));
        answer.map(|(picture, sound)| (Stream { url: picture, size: 0, mime: "video/mp4".into(), kbps: 720 }, Stream { url: sound, size: 0, mime: "audio/webm".into(), kbps: 160 }))
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

/// `songs` as a list that plays on through (a playlist named `q`, in that order): a search pick
/// plays its radio instead (#47), so the queue's own tests take their songs from here.
fn with_list(q: &str, songs: Vec<Track>) -> (Controller, Arc<Fake>, Receiver<(Change, Instant)>, ListRef) {
    let (c, fake, rx) = with_songs(q, songs.clone());
    let id = c.create_playlist(q, Some(&songs[0].key())).expect("a playlist for the test");
    for t in &songs[1..] {
        c.add_to_playlist(id, &t.key());
    }
    let end = Instant::now() + Duration::from_secs(5);
    while c.playlist(id).is_none_or(|p| p.songs.len() < songs.len()) && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(20));
    }
    (c, fake, rx, ListRef::Playlist(id))
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
    let (c, _f, rx, list) = with_list("files one", vec![wav("move-a", 1.5), wav("move-b", 1.5)]);
    c.play_from(list.clone(), 0, "yt:file-move-a");
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
    let (c, _f, rx, list) = with_list("files three", gone);
    c.play_from(list.clone(), 0, "yt:gone1");
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
    // (this song's own place: a song picked up from an earlier test can show one already)
    assert!(until(3.0, || c.now().playing && now_id(&c) == "file-kept" && c.now().at >= 0.5));
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
    let (c, _f, _rx, list) = with_list("files eight", vec![wav("picked-a", 6.0), wav("picked-b", 6.0)]);
    c.play_from(list.clone(), 0, "yt:file-picked-a");
    // (A's own place: a song picked up from an earlier test can show one already)
    assert!(until(4.0, || now_id(&c) == "file-picked-a" && c.now().playing && c.now().at >= 1.6), "A never got to 1.6 s");
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
    let (c, _f, rx, list) = with_list("files nine", gone);
    c.play_from(list.clone(), 0, "yt:again1");
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

// ------------------------------------------------------------------ liking and saving (stage 2)

/// Toasts as they come (each once), for `secs`.
fn toasts_for(c: &Controller, rx: &Receiver<(Change, Instant)>, secs: f64) -> Vec<String> {
    let mut got = vec![];
    let end = Instant::now() + Duration::from_secs_f64(secs);
    while let Some(left) = end.checked_duration_since(Instant::now()) {
        if let Ok((Change::Toast, _)) = rx.recv_timeout(left) {
            while let Some(t) = c.toast() {
                got.push(t);
            }
        }
    }
    got
}

fn saved_files(title: &str) -> usize {
    std::fs::read_dir(radiance_engine::library::music_dir()).map(|d| d.flatten().filter(|e| e.file_name().to_string_lossy().contains(title)).count()).unwrap_or(0)
}

/// A controller whose Search's lossless list is `songs` (searched as `q`).
fn with_lossless(q: &str, songs: Vec<Track>) -> (Controller, Arc<Fake>, Receiver<(Change, Instant)>) {
    let fake = Arc::new(Fake::default());
    fake.finds(Kind::Lossless, q, songs);
    let (heard, rx) = listener();
    let c = Controller::start(heard, fake.clone()).unwrap();
    c.search(q);
    let end = Instant::now() + Duration::from_secs(5);
    while c.results().lossless.as_ref().is_none_or(|v| v.is_empty()) && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(20));
    }
    (c, fake, rx)
}

#[test]
fn liking_puts_the_song_first_and_says_so() {
    let _g = setup();
    let one = song(Source::Archive, "https://example.invalid/liked-one.flac", "Liked One");
    let (c, _fake, rx) = with_lossless("liking one", vec![one.clone()]);
    c.toggle_like(&one.key());
    assert!(wait_for(&rx, Change::Liked, 5.0), "Liked Songs changed");
    assert!(until(3.0, || c.liked().first().is_some_and(|(t, _)| t.key() == one.key())), "first in Liked Songs");
    let toasts = toasts_for(&c, &rx, 3.0);
    assert!(toasts.contains(&"Saved “Liked One”".to_string()), "{toasts:?}");
    assert!(saved_files("Liked One") >= 1, "its file is in the music folder");
    assert_eq!(c.liked()[0].1, radiance_core::library::Saved::Saved);
    // a YouTube song in a build that doesn't save them: kept, and it streams
    let yt = song(Source::YouTube, "liking-yt", "Streams On");
    if !radiance_engine::library::saves(&yt) {
        let (c2, _f2, rx2) = { drop(c); with_songs("liking yt", vec![yt.clone()]) };
        c2.toggle_like(&yt.key());
        assert!(toasts_for(&c2, &rx2, 2.0).contains(&"Liked “Streams On”".to_string()));
        assert_eq!(c2.liked()[0].1, radiance_core::library::Saved::Streams);
        c2.toggle_like(&yt.key());
        let _ = toasts_for(&c2, &rx2, 1.0);
    } else {
        c.toggle_like(&one.key());
        let _ = toasts_for(&c, &rx, 1.0);
    }
}

#[test]
fn unliking_removes_it_and_says_so() {
    let _g = setup();
    let two = song(Source::Archive, "https://example.invalid/liked-two.flac", "Liked Two");
    let (c, _fake, rx) = with_lossless("liking two", vec![two.clone()]);
    c.toggle_like(&two.key());
    assert!(toasts_for(&c, &rx, 3.0).contains(&"Saved “Liked Two”".to_string()));
    c.toggle_like(&two.key());
    let toasts = toasts_for(&c, &rx, 2.0);
    assert!(toasts.contains(&"Removed “Liked Two” from Liked; its saved copy is in the Trash".to_string()), "{toasts:?}");
    assert!(c.liked().iter().all(|(t, _)| t.key() != two.key()));
    assert!(until(3.0, || saved_files("Liked Two") == 0), "its file went to the Trash");
}

#[test]
fn unliking_while_saving_leaves_nothing() {
    let _g = setup();
    let three = song(Source::Archive, "https://example.invalid/liked-three.flac", "Liked Three");
    let (c, fake, rx) = with_lossless("liking three", vec![three.clone()]);
    *fake.download_wait.lock().unwrap() = Duration::from_millis(1500);
    c.toggle_like(&three.key());
    assert!(until(1.0, || c.liked().first().is_some_and(|(_, s)| *s == radiance_core::library::Saved::Saving)), "it shows as saving");
    c.toggle_like(&three.key());
    let toasts = toasts_for(&c, &rx, 4.0);
    assert!(!toasts.iter().any(|t| t.starts_with("Saved")), "{toasts:?}");
    assert_eq!(saved_files("Liked Three"), 0, "nothing left behind");
}

#[test]
fn the_bar_heart_follows_the_song() {
    let _g = setup();
    let a = wav("heart", 30.0);
    let (c, _fake, rx) = with_songs("heart", vec![a.clone()]);
    c.play_from(ListRef::Songs, 0, &a.key());
    assert!(until(5.0, || c.now().song.is_some_and(|t| t.key() == a.key())));
    assert!(!c.now().liked);
    c.toggle_like(&a.key());
    assert!(until(3.0, || c.now().liked), "the heart fills");
    c.toggle_like(&a.key());
    assert!(until(3.0, || !c.now().liked));
    let _ = toasts_for(&c, &rx, 1.0);
}

// ------------------------------------------------------------------ the library's pages (stage 2)

/// Liked songs kept before the controller starts (as last time's); `fake` answers for it.
fn with_liked(liked: &[Track], fake: Arc<Fake>) -> (Controller, Receiver<(Change, Instant)>) {
    radiance_engine::library::store(liked);
    let (heard, rx) = listener();
    let c = Controller::start(heard, fake).unwrap();
    (c, rx)
}

/// A liked song with its file here (a short WAV).
fn liked_song(name: &str, title: &str, artist: &str, album: &str, genres: Option<Vec<&str>>) -> Track {
    let mut t = wav(name, 20.0);
    t.title = title.into();
    t.artist = artist.into();
    t.album = album.into();
    t.genres = genres.map(|g| g.into_iter().map(String::from).collect());
    t
}

fn titles(v: &[(Track, radiance_core::library::Saved)]) -> Vec<String> {
    v.iter().map(|(t, _)| t.title.clone()).collect()
}

#[test]
fn the_liked_page_filters_like_the_linux_app() {
    let _g = setup();
    let liked = vec![liked_song("halo", "Halo", "Beyoncé", "I Am", Some(vec![])), liked_song("another", "Another", "Zed", "", Some(vec![])), liked_song("cielo", "Cielo", "Cara", "", Some(vec![]))];
    let (c, _rx) = with_liked(&liked, Arc::new(Fake::default()));
    let page = c.liked_page("");
    assert_eq!(titles(&page.songs), ["Another", "Cielo", "Halo"], "A to Z");
    assert_eq!(page.note, format!("3 songs, saved in {}", radiance_engine::library::music_dir().display()));
    // songs all here; a build that saves YouTube songs also counts their music videos missing
    let videos_count = radiance_engine::library::saves(&liked[0]);
    assert_eq!(page.fix, videos_count.then(|| "Fix missing (3)".to_string()));
    let page = c.liked_page("beyonce");
    assert_eq!(titles(&page.songs), ["Halo"], "accents and capitals aside");
    assert_eq!(page.note, "1 of your 3 liked songs matches “beyonce”");
    assert_eq!(page.letters[7], ('H', Some(0)));
    assert_eq!(page.letters[0], ('A', None), "no match under A: faint");
    c.play_from(ListRef::LikedShown, 0, &liked[0].key());
    assert!(until(5.0, || now_id(&c) == "file-halo"), "plays what's shown");
    c.quit();
    radiance_engine::library::store(&[]);
}

#[test]
fn albums_artists_and_genres_come_from_the_liked_songs() {
    let _g = setup();
    let liked = vec![liked_song("one", "One", "Ann & Bo", "Blue", Some(vec!["Pop"])), liked_song("two", "Two", "Bo", "Blue", Some(vec![])), liked_song("three", "Three", "Cy", "", Some(vec![]))];
    let (c, _rx) = with_liked(&liked, Arc::new(Fake::default()));
    let albums = c.library(GroupKind::Albums);
    assert_eq!(albums.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["Blue", "Singles"]);
    assert_eq!((albums[0].sub.as_str(), albums[0].round), ("Ann & Bo", false));
    let artists = c.library(GroupKind::Artists);
    assert_eq!(artists.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["Ann", "Bo", "Cy"]);
    assert_eq!((artists[1].sub.as_str(), artists[1].round), ("2 songs", true));
    assert_eq!(c.library(GroupKind::Genres).iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["Other", "Pop"]);
    let blue = c.group(GroupKind::Albums, "Blue").expect("the album");
    assert_eq!((blue.title.as_str(), blue.sub.as_str()), ("Blue", "Ann & Bo · 2 songs"));
    assert_eq!(blue.songs.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["One", "Two"]);
    let bo = c.group(GroupKind::Artists, "Bo").expect("the artist");
    assert_eq!((bo.sub.as_str(), bo.round), ("2 songs", true));
    c.play_from(ListRef::Group(GroupKind::Albums, "Blue".into()), 1, &liked[1].key());
    assert!(until(5.0, || now_id(&c) == "file-two"));
    c.quit();
    radiance_engine::library::store(&[]);
}

#[test]
fn genres_are_looked_up_for_liked_songs() {
    let _g = setup();
    let fake = Arc::new(Fake::default());
    *fake.genres.lock().unwrap() = vec!["Jazz".into()];
    let liked = vec![liked_song("jazzy", "Jazzy", "Dee", "", None)];
    let (c, rx) = with_liked(&liked, fake);
    assert_eq!(c.library(GroupKind::Genres)[0].name, "Finding the genre…");
    assert!(wait_for(&rx, Change::Library, 5.0), "the library changed once they came");
    assert!(until(2.0, || c.library(GroupKind::Genres).first().is_some_and(|t| t.name == "Jazz")));
    c.quit();
    radiance_engine::library::store(&[]);
}

#[test]
fn history_groups_today() {
    let _g = setup();
    let a = wav("history", 20.0);
    let (c, _f, rx) = with_songs("history", vec![a.clone()]);
    c.play_from(ListRef::Songs, 0, &a.key());
    assert!(wait_for(&rx, Change::History, 5.0));
    // (the start tells History too: wait for this song's play)
    assert!(until(5.0, || c.history().first().and_then(|d| d.plays.first()).is_some_and(|p| p.track.key() == a.key())));
    let days = c.history();
    assert_eq!(days[0].label, "Today");
    assert_eq!(days[0].plays[0].track.key(), a.key());
    c.play_from(ListRef::History, 0, &a.key());
    assert!(until(3.0, || now_id(&c) == "file-history"));
    c.quit();
}

// ------------------------------------------------------------------ playlists (stage 2)

#[test]
fn a_playlist_is_made_named_and_emptied() {
    let _g = setup();
    let a = wav("pl-a", 10.0);
    let (c, _f, rx) = with_songs("playlists", vec![a.clone()]);
    let before = c.playlists().len();
    let name = c.next_playlist_name();
    assert_eq!(name, format!("My Playlist {}", before + 1));
    let id = c.create_playlist(&name, Some(&a.key())).expect("made");
    assert!(until(3.0, || c.playlist(id).is_some_and(|p| p.songs.len() == 1)));
    assert!(toasts_for(&c, &rx, 1.5).contains(&format!("Added to “{name}”")));
    let page = c.playlist(id).unwrap();
    assert_eq!((page.name.as_str(), page.count.as_str(), page.empty.is_none()), (name.as_str(), "1 song", true));
    assert!(c.playlists().iter().any(|(i, n, count)| *i == id && *n == name && *count == 1));
    c.rename_playlist(id, "Road Trip");
    assert_eq!(c.playlist(id).unwrap().name, "Road Trip");
    c.remove_from_playlist(id, &a.key());
    assert_eq!(c.playlist(id).unwrap().empty.as_deref(), Some("No songs in it yet. Add one from any song's menu: Add to Playlist."));
    c.delete_playlist(id);
    assert!(c.playlist(id).is_none());
    assert_eq!(c.playlists().len(), before);
    c.quit();
}

#[test]
fn adding_twice_says_already_in() {
    let _g = setup();
    let a = wav("pl-twice", 10.0);
    let (c, _f, rx) = with_songs("playlists twice", vec![a.clone()]);
    let id = c.create_playlist("Twice", Some(&a.key())).unwrap();
    assert!(until(3.0, || c.playlist(id).is_some_and(|p| p.songs.len() == 1)));
    let _ = toasts_for(&c, &rx, 0.5);
    c.add_to_playlist(id, &a.key());
    assert!(toasts_for(&c, &rx, 1.5).contains(&"Already in “Twice”".to_string()));
    assert_eq!(c.playlist(id).unwrap().songs.len(), 1);
    c.delete_playlist(id);
    c.quit();
}

// ------------------------------------------------------------------ the queue panel (stage 2)

#[test]
fn queue_commands_follow_the_key() {
    let _g = setup();
    let songs = vec![wav("q-a", 30.0), wav("q-b", 30.0), wav("q-c", 30.0)];
    let (c, _f, _rx, list) = with_list("queue keys", songs.clone());
    c.play_from(list.clone(), 0, &songs[0].key());
    assert!(until(5.0, || now_id(&c) == "file-q-a"));
    let p = c.queue_panel();
    assert_eq!((p.start, p.now, p.rows.len()), (0, true, 3));
    assert_eq!(p.note, "2 songs next");
    c.unqueue(2, &songs[1].key()); // not the song at 2 now: nothing
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(c.queue_panel().rows.len(), 3);
    c.unqueue(2, &songs[2].key());
    assert!(until(2.0, || c.queue_panel().rows.len() == 2));
    c.move_in_queue(1, 0, &songs[0].key()); // the key of what's at 1 isn't A's: nothing
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(c.queue_panel().rows.iter().map(|t| t.id.clone()).collect::<Vec<_>>(), ["file-q-a", "file-q-b"]);
    c.quit();
}

#[test]
fn nothing_moves_above_the_song_playing() {
    let _g = setup();
    let songs = vec![wav("m-a", 30.0), wav("m-b", 30.0), wav("m-c", 30.0), wav("m-d", 30.0)];
    let (c, _f, _rx, list) = with_list("queue moves", songs.clone());
    c.play_from(list.clone(), 1, &songs[1].key());
    assert!(until(5.0, || now_id(&c) == "file-m-b"));
    c.move_in_queue(3, 0, &songs[3].key());
    std::thread::sleep(Duration::from_millis(400));
    let (q, i) = c.queue();
    let d = q.iter().position(|t| t.id == "file-m-d").unwrap();
    assert!(d > i, "D stays after the song playing");
    c.move_in_queue(3, 2, &songs[3].key());
    assert!(until(2.0, || c.queue().0.get(2).is_some_and(|t| t.id == "file-m-d")), "and moves among the songs after it");
    c.quit();
}

#[test]
fn played_before_lists_the_last_five_oldest_first() {
    let _g = setup();
    let songs: Vec<Track> = (1..=7).map(|i| wav(&format!("pb{i}"), 20.0)).collect();
    let (c, _f, _rx) = with_songs("played before", songs.clone());
    for (i, t) in songs.iter().enumerate() {
        c.play_from(ListRef::Songs, i, &t.key());
        assert!(until(5.0, || now_id(&c) == t.id));
    }
    assert!(until(2.0, || c.queue_panel().before.len() == 5));
    assert_eq!(c.queue_panel().before.iter().map(|t| t.id.clone()).collect::<Vec<_>>(), ["file-pb2", "file-pb3", "file-pb4", "file-pb5", "file-pb6"]);
    c.play_again(&songs[2].key());
    assert!(until(3.0, || now_id(&c) == "file-pb3"), "played again now");
    c.quit();
}

#[test]
fn next_and_queue_say_so_and_a_radio_with_nothing_says_so() {
    let _g = setup();
    let songs = vec![wav("n-a", 30.0), wav("n-b", 30.0)];
    let (c, _f, rx) = with_songs("next and queue", songs.clone());
    c.play_from(ListRef::Songs, 0, &songs[0].key());
    assert!(until(5.0, || now_id(&c) == "file-n-a"));
    c.play_next(&songs[1].key());
    c.enqueue(&songs[1].key());
    c.start_radio(&songs[0].key());
    let toasts = toasts_for(&c, &rx, 2.0);
    for want in ["“Song n-b” plays next", "Added “Song n-b” to the queue", "Starting “Song n-a” radio…", "YouTube Music has no radio for “Song n-a”"] {
        assert!(toasts.contains(&want.to_string()), "{want}: {toasts:?}");
    }
    c.quit();
}

// ------------------------------------------------------------------ lyrics word by word (stage 2)

#[test]
fn sung_follows_core_sing_and_two_voices_light_together() {
    use radiance_engine::lyrics::{Line, Word};
    let _g = setup();
    let fake = Arc::new(Fake::default());
    let w = |start: f64, text: &str| Word { start, end: start + 0.3, text: text.into() };
    let lines = vec![
        Line { start: 10.0, end: 14.0, text: "Hello world".into(), words: vec![w(10.0, "Hello"), w(11.0, "world")], singer: Some("Ann".into()) },
        Line { start: 11.0, end: 13.0, text: "Oh".into(), words: vec![], singer: Some("Bo".into()) },
        Line { start: 14.0, end: 15.0, text: "".into(), words: vec![], singer: None },
    ];
    *fake.lyrics.lock().unwrap() = Some(Lyrics::Timed { lines, exact: true });
    let a = wav("sung", 20.0);
    fake.finds(Kind::Songs, "sung", vec![a.clone()]);
    let (heard, _rx) = listener();
    let c = Controller::start(heard, fake).unwrap();
    c.search("sung");
    assert!(until(5.0, || c.results().songs.is_some_and(|v| !v.is_empty())));
    c.play_from(ListRef::Songs, 0, &a.key());
    assert!(until(5.0, || matches!(c.lyrics(), radiance_engine::controller::LyricsState::Lines(ref l) if l.len() == 3)));
    let radiance_engine::controller::LyricsState::Lines(shown) = c.lyrics() else { unreachable!() };
    assert_eq!(shown.iter().map(|l| l.head.clone()).collect::<Vec<_>>(), [Some("Ann".to_string()), Some("Bo".to_string()), None]);
    assert_eq!(shown[2].line.text, "♪");
    assert_eq!(c.sung(9.95), vec![(0, 5)]);
    assert_eq!(c.sung(12.0), vec![(0, 11), (1, 2)], "two voices at once");
    // the clock the beta counts on with
    let n = c.now();
    assert!(n.since <= radiance_engine::controller::clock_ms());
    c.quit();
}

/// A small music video of `secs` seconds (a test picture and a quiet tone, Matroska), made by
/// GStreamer: the file's path.
fn test_video(name: &str, secs: u32) -> String {
    let dir = std::env::temp_dir().join(format!("radiance-controller-test-{}", std::process::id())).join("videos");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.mkv"));
    gst::init().unwrap();
    use gst::prelude::*;
    let desc = format!(
        "videotestsrc num-buffers={} ! video/x-raw,format=I420,width=64,height=36,framerate=5/1 ! queue ! mux. \
         audiotestsrc num-buffers={} samplesperbuffer=4410 volume=0.05 ! audio/x-raw,format=S16LE,rate=44100,channels=1 ! queue ! mux. \
         matroskamux name=mux ! filesink name=out",
        secs * 5,
        secs * 10
    );
    let p = gst::parse::launch(&desc).unwrap();
    // (the path set here, not in the text: there, Windows' backslashes would be escapes)
    p.downcast_ref::<gst::Bin>().unwrap().by_name("out").unwrap().set_property("location", path.to_str().unwrap());
    p.set_state(gst::State::Playing).unwrap();
    let done = p.bus().unwrap().timed_pop_filtered(gst::ClockTime::from_seconds(60), &[gst::MessageType::Eos, gst::MessageType::Error]);
    p.set_state(gst::State::Null).unwrap();
    assert_eq!(done.map(|m| m.type_()), Some(gst::MessageType::Eos), "the test video wasn't made");
    path.display().to_string()
}

/// A song's music video (a YouTube video, `video` set), as a counterpart search finds it.
fn clip(id: &str, title: &str) -> Track {
    let mut t = song(Source::YouTube, id, title);
    t.video = true;
    t.seconds = 30;
    t
}

#[test]
fn switching_versions_keeps_the_place() {
    let _l = setup();
    let s = wav("versioned", 30.0);
    let v = clip("vid-versioned", "Song versioned (Official Video)");
    // its video saved on this computer: played from the file
    radiance_engine::db::add_video(&v.id, &s.key(), &test_video("versioned", 30));
    let (c, fake, _rx) = with_songs("files video", vec![s.clone()]);
    fake.counterparts.lock().unwrap().insert(s.id.clone(), v.clone());
    c.play_from(ListRef::Songs, 0, "yt:file-versioned");
    assert!(until(3.0, || c.now().playing && now_id(&c) == "file-versioned" && c.now().at > 0.2), "the song didn't play");
    assert!(until(3.0, || c.now().has_counterpart), "its video wasn't found");
    assert!(!c.now().video);
    c.seek(12.0);
    assert!(until(2.0, || now_id(&c) == "file-versioned" && c.now().at >= 12.0));
    c.switch_version(true);
    assert!(until(5.0, || now_id(&c) == "vid-versioned" && c.now().video && c.now().playing && c.now().at >= 11.5), "the video didn't play from 12 s: {:?}", c.now().at);
    let at = c.now().at;
    assert!(at < 14.5, "the video started at {at} s");
    assert_eq!(c.now().quality, "Video · 720p");
    assert!(c.now().has_counterpart, "the way back to the song");
    assert_eq!(c.queue().0[c.queue().1].id, "vid-versioned", "the queue holds the version playing");
    c.switch_version(false);
    assert!(until(5.0, || now_id(&c) == "file-versioned" && !c.now().video && c.now().playing && c.now().at >= 11.5), "didn't come back near the same place: {:?}", c.now().at);
    assert!(c.now().at < 17.0, "came back at {} s", c.now().at);
    assert!(c.now().has_counterpart, "the way to the video again");
    assert!(c.video_swapchain().is_none());
    c.quit();
}

#[test]
fn a_video_that_cant_play_leaves_the_song() {
    let _l = setup();
    let s = wav("no-video", 20.0);
    let v = clip("vid-gone", "No Video (Official Video)");
    let (c, fake, rx) = with_songs("files no video", vec![s.clone()]);
    fake.counterparts.lock().unwrap().insert(s.id.clone(), v.clone());
    fake.videos.lock().unwrap().insert(v.id.clone(), Err("ERROR: [youtube] vid-gone: Video unavailable".into()));
    c.play_from(ListRef::Songs, 0, "yt:file-no-video");
    assert!(until(3.0, || c.now().playing && now_id(&c) == "file-no-video" && c.now().has_counterpart && c.now().at > 0.5), "the song didn't play");
    c.switch_version(true);
    assert!(wait_for(&rx, Change::Problem, 3.0), "it didn't say why");
    assert_eq!(c.problem().map(|p| p.text).as_deref(), Some("“No Video (Official Video)” can't play: it isn't on YouTube anymore"));
    let n = c.now();
    assert!(n.playing && !n.video, "the song stopped");
    assert_eq!(now_id(&c), "file-no-video");
    let at = n.at;
    assert!(until(2.0, || c.now().at > at + 0.3), "the song isn't going on");
    assert_eq!(c.queue().0[c.queue().1].id, "file-no-video");
    c.quit();
}

/// The folder picked in Storage (settings.json's music_dir) is where liked songs go: the engine
/// takes it at start, as the GTK app does (the Windows beta saved them to ~/Music whatever was picked).
#[test]
fn the_picked_music_folder_is_where_likes_go() {
    let _l = setup();
    let picked = std::env::temp_dir().join(format!("radiance-controller-test-{}", std::process::id())).join("picked-music");
    let mut s = radiance_engine::settings::load();
    let before = s.music_dir.clone();
    s.music_dir = Some(picked.display().to_string());
    radiance_engine::settings::save(&s);
    radiance_engine::library::set_music_dir(None);
    let (heard, _rx) = listener();
    let c = Controller::start(heard, Arc::new(Fake::default())).unwrap();
    let end = Instant::now() + Duration::from_secs(3);
    while radiance_engine::library::music_dir() != picked && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(20));
    }
    let found = radiance_engine::library::music_dir();
    c.quit();
    s.music_dir = before;
    radiance_engine::settings::save(&s);
    radiance_engine::library::set_music_dir(None);
    assert_eq!(found, picked);
}

/// A song picked from Search plays, and Up next is its radio, not the other results (#47: as
/// YouTube Music does); quietly, no radio toasts.
#[test]
fn a_search_pick_plays_its_radio_not_the_other_results() {
    let _l = setup();
    let (c, fake, rx) = with_songs("radio pick", vec![wav("radio-pick", 3.0), wav("other-result", 3.0), wav("another-result", 3.0)]);
    fake.radios.lock().unwrap().insert("file-radio-pick".into(), (Duration::ZERO, vec![wav("like-it-one", 3.0), wav("like-it-two", 3.0)]));
    c.play_from(ListRef::Songs, 0, "yt:file-radio-pick");
    let end = Instant::now() + Duration::from_secs(5);
    while c.queue().0.len() != 3 && Instant::now() < end {
        wait_for(&rx, Change::Queue, 0.5);
    }
    let ids: Vec<String> = c.queue().0.iter().map(|t| t.id.clone()).collect();
    assert_eq!(ids, ["file-radio-pick", "file-like-it-one", "file-like-it-two"]);
    assert_eq!(c.queue().1, 0);
    let mut said = Vec::new();
    while let Some(t) = c.toast() {
        said.push(t);
    }
    assert!(!said.iter().any(|t| t.contains("radio")), "{said:?}");
    c.quit();
}

/// A search pick's radio that answers after another song was picked never takes over.
#[test]
fn a_late_radio_never_takes_over() {
    let _l = setup();
    let (c, fake, _rx) = with_songs("late radio", vec![wav("late-radio-pick", 3.0), wav("picked-after", 3.0)]);
    fake.radios.lock().unwrap().insert("file-late-radio-pick".into(), (Duration::from_millis(600), vec![wav("late-like-it", 3.0)]));
    c.play_from(ListRef::Songs, 0, "yt:file-late-radio-pick");
    std::thread::sleep(Duration::from_millis(100));
    c.play_from(ListRef::Songs, 1, "yt:file-picked-after");
    std::thread::sleep(Duration::from_millis(1200));
    assert_eq!(now_id(&c), "file-picked-after");
    assert!(!c.queue().0.iter().any(|t| t.id == "file-late-like-it"), "{:?}", c.queue().0.iter().map(|t| t.id.clone()).collect::<Vec<_>>());
    c.quit();
}
