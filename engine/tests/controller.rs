//! The engine's controller (issue #45), as the Windows app drives it: its own thread, search,
//! Home, and (Task 6) playback. No network and no sound: fake sources, RADIANCE_SILENT, and a
//! data folder of its own under TMPDIR. One test at a time (one controller per process).

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, Once};
use std::time::{Duration, Instant};

use radiance_core::library::GroupKind;
use radiance_engine::controller::{Change, Controller, ListRef, Listener, RateOutcome, SetupState, SplitMark, UpdateStep};
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
    /// Splits (stage 3): how long each takes, the songs that fail, and every job started (id, in two), in order.
    pub split_wait: Mutex<Duration>,
    pub split_fails: Mutex<std::collections::HashSet<String>>,
    pub splits_started: Mutex<Vec<(String, bool)>>,
    /// A split lays the song's four parts down (short FLACs), as a real one would.
    pub split_makes_parts: std::sync::atomic::AtomicBool,
    /// MMS word timing asked for (stage 3, Task 7).
    pub time_words_calls: AtomicUsize,
    /// What stops splitting being set up here (too few cores, too little memory or disk); and installs started.
    pub setup_problems: Mutex<Vec<String>>,
    pub installs: AtomicUsize,
    /// How often the split queue was drawn (publish_splits asks can_split each time).
    pub can_split_calls: AtomicUsize,
    /// Updates (stage 4a): the newest release found, and a download that waits until it's stopped.
    pub release: Mutex<Option<radiance_engine::update::Release>>,
    pub download_waits: std::sync::atomic::AtomicBool,
    /// Updates arranged to go on once the app has closed (Restart to Update).
    pub put_on: AtomicUsize,
    /// Report a Problem and the rating (stage 4a): notes sent, whether a free moment asks, the
    /// account (none: a sign-in first), the answers sent.
    pub reports: Mutex<Vec<String>>,
    pub rating_due: std::sync::atomic::AtomicBool,
    pub account: Mutex<Option<radiance_engine::feedback::Account>>,
    pub answers: Mutex<Vec<radiance_engine::feedback::Answer>>,
    /// Offline and suggestions (stage 4a): the network down at the start; YouTube Music's
    /// suggestions (and how long they take) by what's typed; the fast split's files fetched.
    pub network_down: std::sync::atomic::AtomicBool,
    pub suggestions: Mutex<HashMap<String, (Duration, Vec<String>)>>,
    pub two_part_fetches: AtomicUsize,
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
    fn can_split(&self) -> bool {
        self.can_split_calls.fetch_add(1, Ordering::SeqCst);
        true
    }
    fn can_split_two(&self) -> bool {
        true
    }
    fn install(&self) -> radiance_engine::update::Install {
        radiance_engine::update::Install::Beta(std::env::temp_dir().join("rm-fake-beta"))
    }
    fn latest_release(&self, _: &radiance_engine::update::Install) -> Result<radiance_engine::update::Release, String> {
        self.release.lock().unwrap().clone().ok_or_else(|| "couldn't reach GitHub (offline)".into())
    }
    fn download_update(&self, r: &radiance_engine::update::Release, _: &radiance_engine::update::Install, stop: &std::sync::atomic::AtomicBool, progress: &mut dyn FnMut(u64, u64)) -> Result<std::path::PathBuf, String> {
        progress(r.size / 2, r.size);
        while self.download_waits.load(Ordering::SeqCst) {
            if stop.load(Ordering::SeqCst) {
                return Err("stopped".into());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        progress(r.size, r.size);
        Ok(std::env::temp_dir().join(format!("{}-{}", r.version, radiance_engine::update::BETA_ASSET)))
    }
    fn count_update(&self, _: &radiance_engine::update::Release, _: &radiance_engine::update::Install) {}
    fn put_on_update(&self, _: &radiance_engine::update::Install, _: &std::path::Path) -> Result<(), String> {
        self.put_on.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn send_report(&self, _: &radiance_engine::report::Report, note: &str) -> Result<String, radiance_engine::feedback::Failure> {
        self.reports.lock().unwrap().push(note.to_string());
        Ok("RM-0042".into())
    }
    fn watches_network(&self) -> bool {
        false
    }
    fn network_available(&self) -> bool {
        !self.network_down.load(Ordering::SeqCst)
    }
    fn suggestions(&self, q: &str) -> Vec<String> {
        let (wait, found) = self.suggestions.lock().unwrap().get(q).cloned().unwrap_or_default();
        std::thread::sleep(wait);
        found
    }
    fn fetch_two_part(&self) {
        self.two_part_fetches.fetch_add(1, Ordering::SeqCst);
    }
    fn rating_due(&self) -> bool {
        self.rating_due.load(Ordering::SeqCst)
    }
    fn account(&self) -> Option<radiance_engine::feedback::Account> {
        self.account.lock().unwrap().clone()
    }
    fn sign_in(&self, _: &std::sync::atomic::AtomicBool, open: &dyn Fn(&str)) -> Result<radiance_engine::feedback::Account, radiance_engine::feedback::Failure> {
        open("https://admin.example/music/sign-in?state=x");
        std::thread::sleep(Duration::from_millis(200));
        let a = radiance_engine::feedback::Account { token: "t".into(), name: "Bassi".into(), email: "b@example.com".into() };
        *self.account.lock().unwrap() = Some(a.clone());
        Ok(a)
    }
    fn send_feedback(&self, _: &str, a: &radiance_engine::feedback::Answer) -> Result<bool, radiance_engine::feedback::Failure> {
        self.answers.lock().unwrap().push(a.clone());
        Ok(a.ratings[0] >= 4)
    }
    fn setup_problems(&self) -> Vec<String> {
        self.setup_problems.lock().unwrap().clone()
    }
    fn install_splitter(&self, threads: u32, say: &(dyn Fn(&str, f64) + Send + Sync)) -> Result<radiance_engine::settings::LocalSplitter, String> {
        self.installs.fetch_add(1, Ordering::SeqCst);
        if self.offline.load(Ordering::SeqCst) {
            return Err("Temporary failure in name resolution".into());
        }
        say("Downloading Demucs…", 0.3);
        std::thread::sleep(Duration::from_millis(300));
        say("Checking it works…", 0.9);
        std::thread::sleep(Duration::from_millis(300));
        // (settings keep a splitter only when its files are there)
        let dir = std::env::temp_dir().join(format!("radiance-fake-splitter-{}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let (python, script) = (dir.join("python"), dir.join("split.py"));
        for f in [&python, &script] {
            std::fs::write(f, b"").map_err(|e| e.to_string())?;
        }
        Ok(radiance_engine::settings::LocalSplitter { python: python.display().to_string(), script: script.display().to_string(), threads })
    }
    fn time_words(&self, _t: &Track) -> Result<(), String> {
        self.time_words_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn find_lyrics_again(&self, t: &Track) -> Result<bool, String> {
        // as relook_lyrics: the ones found take the kept ones' place
        let found = self.lyrics.lock().unwrap().clone();
        match found {
            Some(l @ Lyrics::Timed { .. }) => {
                let (_, text) = radiance_engine::lyrics::to_file(&l, Some(t), &[]).ok_or("nothing to keep")?;
                std::fs::write(radiance_engine::lyrics::lyrics_file(t), text).map_err(|e| e.to_string())?;
                Ok(false)
            }
            _ => Err("No other lyrics found for this song: yours stay. Edit Lyrics… to type them in.".into()),
        }
    }
    fn split(&self, t: &Track, two: bool, _uri: &str, report: std::sync::Arc<dyn Fn(radiance_engine::splitting::Phase) + Send + Sync>) -> Result<(), String> {
        self.splits_started.lock().unwrap().push((t.id.clone(), two));
        report(radiance_engine::splitting::Phase::Splitting(0.5));
        let wait = *self.split_wait.lock().unwrap();
        std::thread::sleep(wait);
        if self.split_fails.lock().unwrap().contains(&t.id) {
            return Err("the splitter stopped".into());
        }
        if self.split_makes_parts.load(Ordering::SeqCst) {
            make_parts(t, &["drums", "bass", "other", "vocals"], 6);
        }
        Ok(())
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
    radiance_engine::library::store(&[]); // none liked, whichever test ran before
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

/// A song says "Loading…" (bar and Now Playing, as the Linux app) from its pick until its sound
/// flows, then what's really playing (#55, owner: "ay asan yung loading???").
#[test]
fn a_song_says_loading_until_its_sound_flows() {
    let _l = setup();
    let mut slow = song(Source::YouTube, "slow-load", "Slow to load");
    slow.seconds = 2;
    let (c, fake, _rx) = with_songs("files loading", vec![slow, wav("loading-file", 2.0)]);
    // (a stream plays through the relay, which a test's file can't: this one is only ever being found)
    fake.streams.lock().unwrap().insert("slow-load".into(), (Duration::from_millis(3000), Err("still looking".into())));
    c.play_from(ListRef::Songs, 0, "yt:slow-load");
    assert!(until(0.5, || now_id(&c) == "slow-load"), "never picked");
    let n = c.now();
    assert!(n.loading, "not loading while its stream is found");
    assert_eq!(n.quality, "Loading…");
    assert!(!n.playing);
    c.play_from(ListRef::Songs, 1, "yt:file-loading-file");
    assert!(until(4.0, || now_id(&c) == "file-loading-file" && c.now().playing && !c.now().loading), "still loading after it played");
    assert_eq!(c.now().quality, "Compressed · Opus");
    c.quit();
}

/// One that stops for good (Try Again) isn't left saying "Loading…" (the Linux app's 2026-10-31
/// audit: "Loading…" for ever): nothing is said until it plays.
#[test]
fn a_song_that_stops_for_good_isnt_left_loading() {
    let _l = setup();
    let gone: Vec<Track> = (1..=3).map(|i| song(Source::YouTube, &format!("stuck{i}"), &format!("Stuck {i}"))).collect();
    let (c, _f, _rx, list) = with_list("files stuck", gone);
    c.play_from(list.clone(), 0, "yt:stuck1");
    assert!(until(6.0, || c.problem().is_some_and(|p| p.retry)), "never stopped");
    assert!(!c.now().loading);
    assert_eq!(c.now().quality, "");
    c.quit();
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

/// Downloading (owner, 2026-11-07): a liked song being saved is listed until it's saved.
#[test]
fn downloading_lists_a_liked_song_until_its_saved() {
    let _g = setup();
    let four = song(Source::Archive, "https://example.invalid/liked-four.flac", "Liked Four");
    let (c, fake, rx) = with_lossless("liking four", vec![four.clone()]);
    *fake.download_wait.lock().unwrap() = Duration::from_millis(800);
    assert!(c.downloading().iter().all(|(t, _)| t.key() != four.key()));
    c.toggle_like(&four.key());
    assert!(until(1.0, || c.downloading().first().is_some_and(|(t, s)| t.key() == four.key() && *s == radiance_core::library::Saved::Saving)), "it isn't listed while saving");
    assert!(until(5.0, || c.downloading().iter().all(|(t, _)| t.key() != four.key())), "still listed once saved");
    // its file deleted: listed again, as Fix missing counts it
    let file = c.liked().into_iter().find(|(t, _)| t.key() == four.key()).and_then(|(t, _)| t.file).expect("saved");
    std::fs::remove_file(&file).unwrap();
    assert!(c.downloading().iter().any(|(t, s)| t.key() == four.key() && *s == radiance_core::library::Saved::NotSaved), "a liked song whose file is gone isn't listed");
    c.toggle_like(&four.key());
    let _ = toasts_for(&c, &rx, 2.0);
    c.quit();
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
    // (its quality comes with its Ready event, a moment after it plays: "Loading…" until then, #55)
    assert!(until(3.0, || c.now().quality == "Video · 720p"), "{}", c.now().quality);
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
    // (its own queue: the one picked up from the last test's session could have 3 songs too)
    let mine = |q: &[Track]| q.len() == 3 && q[0].id == "file-radio-pick";
    let end = Instant::now() + Duration::from_secs(5);
    while !mine(&c.queue().0) && Instant::now() < end {
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

/// The beta counts its lyrics on from the engine's last reading (at, since): `since` stays fresh
/// while a song plays (5 readings a second), or the words light up to a second early (review C1).
#[test]
fn the_clock_reading_stays_fresh_while_playing() {
    let _l = setup();
    let (c, _f, _rx, list) = with_list("fresh clock", vec![wav("fresh-clock", 6.0)]);
    c.play_from(list.clone(), 0, "yt:file-fresh-clock");
    assert!(until(5.0, || c.now().playing && c.now().at > 0.2));
    std::thread::sleep(Duration::from_millis(1600));
    let age = radiance_engine::controller::clock_ms().saturating_sub(c.now().since);
    assert!(age < 400, "the reading is {age} ms old");
    c.quit();
}

/// A playlist's song found nowhere else (not in Search, Liked, the queue or History) keeps its
/// menu and its commands: Remove from This Playlist, Play Next (review I3).
#[test]
fn a_playlist_song_found_nowhere_else_keeps_its_menu() {
    use radiance_core::menu::{Action, Place};
    let _l = setup();
    let (c, _f, _rx, list) = with_list("menu only here", vec![wav("only-here", 3.0), wav("only-here-two", 3.0)]);
    let ListRef::Playlist(id) = list else { unreachable!() };
    c.search("something else entirely");
    assert!(until(5.0, || c.results().query == "something else entirely" && c.results().songs.is_some()));
    let key = "yt:file-only-here-two";
    let words: Vec<String> = c.song_menu(key, Place::Playlist(id)).into_iter().flatten().map(|i| i.label).collect();
    assert!(words.iter().any(|w| w == "Remove from This Playlist"), "{words:?}");
    c.do_menu(key, 0, Action::PlayNext);
    assert!(until(3.0, || c.queue().0.iter().any(|t| t.key() == key)), "Play Next found the song");
    c.do_menu(key, 0, Action::RemoveFromPlaylist(id));
    assert!(until(3.0, || c.playlist(id).is_some_and(|p| p.songs.len() == 1)));
    c.quit();
}

// ---------------------------------------------------------------------------------- splitting (stage 3, Task 4)

fn started(f: &Fake) -> Vec<(String, bool)> {
    f.splits_started.lock().unwrap().clone()
}

#[test]
fn a_split_asked_for_goes_first() {
    let _l = setup();
    let songs: Vec<Track> = ["sa", "sb", "sc"].iter().map(|i| song(Source::YouTube, i, &format!("Split {i}"))).collect();
    let (c, fake, _rx) = with_songs("split order", songs);
    *fake.split_wait.lock().unwrap() = Duration::from_millis(400);
    c.split("yt:sa", false);
    assert!(until(1.0, || started(&fake).len() == 1), "the first never started");
    c.split("yt:sb", false);
    c.split("yt:sc", false);
    assert!(until(1.0, || c.splits().waiting.len() == 2));
    assert_eq!(c.splits().waiting.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["sc", "sb"], "asked by hand: first in line");
    assert!(matches!(c.split_mark("yt:sb"), SplitMark::Waiting(2)));
    assert!(until(3.0, || started(&fake).len() == 3));
    assert_eq!(started(&fake).iter().map(|x| x.0.as_str()).collect::<Vec<_>>(), ["sa", "sc", "sb"]);
    c.quit();
}

#[test]
fn pausing_finishes_the_song_being_split_then_waits() {
    let _l = setup();
    let songs: Vec<Track> = ["pa", "pb"].iter().map(|i| song(Source::YouTube, i, &format!("Pause {i}"))).collect();
    let (c, fake, _rx) = with_songs("split pause", songs);
    *fake.split_wait.lock().unwrap() = Duration::from_millis(500);
    c.split("yt:pb", false);
    c.split("yt:pa", false);
    assert!(until(1.0, || !started(&fake).is_empty()));
    c.set_split_paused(true);
    assert!(until(2.0, || c.splits().now.is_none()), "the one being split finishes");
    std::thread::sleep(Duration::from_millis(1000));
    assert_eq!(started(&fake).len(), 1, "nothing else starts while paused");
    assert!(c.splits().paused);
    c.set_split_paused(false);
    assert!(until(2.0, || started(&fake).len() == 2), "it carries on once resumed");
    c.quit();
}

/// #61 M3: pausing and resuming splitting say so, as the Linux app's toasts do.
#[test]
fn pausing_and_resuming_splitting_say_so() {
    let _l = setup();
    let (c, _fake, _rx) = with_songs("split pause words", vec![song(Source::YouTube, "pw", "Pause words")]);
    let said = std::cell::RefCell::new(Vec::<String>::new());
    let heard = |want: &str| {
        while let Some(t) = c.toast() {
            said.borrow_mut().push(t);
        }
        said.borrow().iter().any(|t| t == want)
    };
    c.set_split_paused(true);
    assert!(until(1.0, || heard("Splitting paused: the song being split finishes, the rest wait")), "{:?}", said.borrow());
    c.set_split_paused(false);
    assert!(until(1.0, || heard("Splitting resumed")), "{:?}", said.borrow());
    c.quit();
}

/// Final review I5: while splitting is paused, the queue looks again once in a while, once, however
/// many songs wait (each queued song started a sleeper thread of its own before).
#[test]
fn a_paused_queue_looks_again_once_however_many_wait() {
    let _l = setup();
    std::env::set_var("RADIANCE_SPLIT_PAUSE_POLL_MS", "100");
    let ids = ["q1", "q2", "q3", "q4", "q5", "q6"];
    let songs: Vec<Track> = ids.iter().map(|i| song(Source::YouTube, i, &format!("Queued {i}"))).collect();
    let (c, fake, _rx) = with_songs("split pause poll", songs);
    c.set_split_paused(true);
    std::thread::sleep(Duration::from_millis(200));
    for i in ids {
        c.split(&format!("yt:{i}"), false);
    }
    std::thread::sleep(Duration::from_millis(300));
    let before = fake.can_split_calls.load(Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(1000));
    let looks = fake.can_split_calls.load(Ordering::SeqCst) - before;
    assert!(looks <= 14, "{looks} looks in a second at 100 ms: one sleeper per waiting song");
    c.set_split_paused(false);
    std::env::remove_var("RADIANCE_SPLIT_PAUSE_POLL_MS");
    c.quit();
}

// ------------------------------------------------------------------ updates (stage 4a)

fn newer_release() -> radiance_engine::update::Release {
    radiance_engine::update::Release { version: "9.0.0-beta.1".into(), notes: "- **One** thing\n- two".into(), page: "https://x/r".into(), url: "https://x/b.exe".into(), size: 150_000_000, sha256: "abc".into() }
}

/// An update found is offered (asked about); Remind Me Later waits a day and Skip This Version
/// never asks about that version again (the pop-up's own moment is the app's: Task 8).
#[test]
fn the_update_waits_for_a_free_moment() {
    let _l = setup();
    radiance_engine::update::forget();
    let (c, fake, _rx) = with_songs("update offered", vec![]);
    *fake.release.lock().unwrap() = Some(newer_release());
    c.look_for_updates(false);
    assert!(until(2.0, || c.update().step == UpdateStep::Out), "{:?}", c.update().step);
    assert!(c.update().asks);
    assert_eq!(c.update().release.unwrap().version, "9.0.0-beta.1");
    c.update_later();
    assert!(until(1.0, || !c.update().asks));
    c.look_for_updates(false);
    std::thread::sleep(Duration::from_millis(300));
    assert!(!c.update().asks, "asked again within the day");
    radiance_engine::update::forget();
    c.update_skip();
    c.look_for_updates(false);
    std::thread::sleep(Duration::from_millis(300));
    assert!(!c.update().asks, "the skipped version asked again");
    c.look_for_updates(true);
    assert!(until(1.0, || c.update().asks), "asked for by hand, it shows");
    radiance_engine::update::forget();
    c.quit();
}

#[test]
fn update_now_downloads_then_ready() {
    let _l = setup();
    radiance_engine::update::forget();
    let (c, fake, _rx) = with_songs("update download", vec![]);
    *fake.release.lock().unwrap() = Some(newer_release());
    c.look_for_updates(true);
    assert!(until(2.0, || c.update().step == UpdateStep::Out));
    c.update_now();
    assert!(until(2.0, || c.update().step == UpdateStep::Ready), "{:?}", c.update().step);
    radiance_engine::update::forget();
    c.quit();
}

/// The real update test on ribs03 (2026-11-07) found the beta never closed on Restart to Update: the
/// installer's script waited for it and forced it shut. Once the update is arranged, the app is told
/// to close, as the Linux app's quit_app.
#[test]
fn restart_to_update_tells_the_app_to_close() {
    let _l = setup();
    radiance_engine::update::forget();
    let (c, fake, _rx) = with_songs("update restart", vec![]);
    *fake.release.lock().unwrap() = Some(newer_release());
    c.look_for_updates(true);
    assert!(until(2.0, || c.update().step == UpdateStep::Out));
    c.update_now();
    assert!(until(2.0, || c.update().step == UpdateStep::Ready), "{:?}", c.update().step);
    let r = c.update().release.unwrap();
    let file = radiance_engine::update::file(&r, &radiance_engine::update::Install::Beta(std::env::temp_dir().join("rm-fake-beta")));
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, b"the installer").unwrap();
    assert!(!c.update().quit, "told to close before Restart to Update");
    c.update_restart();
    assert!(until(2.0, || c.update().quit), "never told to close: {:?}", c.update().step);
    assert_eq!(c.update().step, UpdateStep::Restarting);
    assert_eq!(fake.put_on.load(Ordering::SeqCst), 1);
    let _ = std::fs::remove_file(&file);
    radiance_engine::update::forget();
    c.quit();
}

#[test]
fn update_cancel_goes_back_to_out() {
    let _l = setup();
    radiance_engine::update::forget();
    let (c, fake, _rx) = with_songs("update cancel", vec![]);
    *fake.release.lock().unwrap() = Some(newer_release());
    fake.download_waits.store(true, Ordering::SeqCst);
    c.look_for_updates(true);
    assert!(until(2.0, || c.update().step == UpdateStep::Out));
    c.update_now();
    assert!(until(2.0, || matches!(c.update().step, UpdateStep::Downloading(f) if f > 0.4)), "{:?}", c.update().step);
    c.update_cancel();
    assert!(until(2.0, || c.update().step == UpdateStep::Out), "{:?}", c.update().step);
    fake.download_waits.store(false, Ordering::SeqCst);
    radiance_engine::update::forget();
    c.quit();
}

// ------------------------------------------------------------------ report and rating (stage 4a)

#[test]
fn a_report_is_sent_with_its_note() {
    let _l = setup();
    let (c, fake, _rx) = with_songs("report", vec![]);
    c.report_gather();
    assert!(until(3.0, || c.report().gathered.is_some()), "never gathered");
    c.report_send("it stopped after a song");
    assert!(until(2.0, || c.report().outcome == Some(Ok("RM-0042".into()))), "{:?}", c.report().outcome);
    assert_eq!(*fake.reports.lock().unwrap(), ["it stopped after a song"]);
    c.quit();
}

/// Final plan Review Focus 5: the rating is due only at a free moment (a song's end, a pause),
/// never at the start or mid-song; shown, it isn't due any more.
#[test]
fn the_rating_waits_for_a_free_moment() {
    let _l = setup();
    let t = wav("rating-moment", 2.0);
    let (c, fake, _rx, list) = with_list("rating moment", vec![t.clone()]);
    fake.rating_due.store(true, Ordering::SeqCst);
    c.play_from(list, 0, &t.key());
    assert!(until(2.0, || c.now().playing), "never got going");
    assert!(!c.rating().due, "asked mid-song");
    c.toggle_play();
    assert!(until(2.0, || c.rating().due), "a pause is a free moment");
    c.rating_shown();
    assert!(until(1.0, || !c.rating().due));
    c.quit();
}

/// Final review: a due rating the window couldn't show then (minimised, in the tray, a dialog up)
/// waited and popped up later, mid-song. That moment is let go, as GTK's feedback_moment, and not
/// counted as asked: the next free moment may ask.
#[test]
fn a_rating_moment_the_window_misses_is_let_go() {
    let _l = setup();
    let t = wav("rating-missed", 3.0);
    let (c, fake, _rx, list) = with_list("rating missed", vec![t.clone()]);
    fake.rating_due.store(true, Ordering::SeqCst);
    c.play_from(list, 0, &t.key());
    assert!(until(2.0, || c.now().playing), "never got going");
    c.toggle_play();
    assert!(until(2.0, || c.rating().due), "a pause is a free moment");
    c.rating_missed();
    assert!(until(1.0, || !c.rating().due), "still due after the moment passed");
    c.toggle_play();
    assert!(until(2.0, || c.now().playing));
    assert!(!c.rating().due, "due again mid-song");
    c.toggle_play();
    assert!(until(2.0, || c.rating().due), "the next free moment may ask (it wasn't counted as asked)");
    c.quit();
}

#[test]
fn a_rating_without_sign_in_asks_for_one_then_sends() {
    let _l = setup();
    let (c, fake, _rx) = with_songs("rating send", vec![]);
    let answer = radiance_engine::feedback::Answer { enjoying: true, ratings: [5, 4, 5, 4, 3], comment: "nice".into() };
    c.rate(answer.clone());
    assert!(until(2.0, || c.rating().sign_in_url.is_some()), "the browser's sign-in never asked for");
    assert!(until(3.0, || c.rating().outcome == Some(RateOutcome::Sent { share: true })), "{:?}", c.rating().outcome);
    assert_eq!(c.rating().account, Some(("Bassi".into(), "b@example.com".into())));
    assert_eq!(*fake.answers.lock().unwrap(), [answer]);
    c.quit();
}

// ------------------------------------------------------------------ storage, Autoplay, the guide (stage 4a)

#[test]
fn storage_counts_and_moves_the_songs() {
    let _l = setup();
    let (c, _f, rx) = with_songs("storage", vec![]);
    let from = radiance_engine::library::music_dir();
    std::fs::create_dir_all(&from).unwrap();
    std::fs::write(from.join("Storage Song.opus"), vec![0u8; 1000]).unwrap();
    c.storage_count();
    assert!(until(3.0, || c.storage().usage.is_some()), "never counted");
    assert_eq!(c.storage().folder, from.display().to_string());
    let to = std::env::temp_dir().join(format!("rm-move-{}", std::process::id())).join("Radiance Music");
    c.move_music(&to.display().to_string());
    let toasts = toasts_for(&c, &rx, 3.0);
    // (the tests' music folder may hold other tests' saved songs too: they all move)
    assert!(toasts.iter().any(|t| t.starts_with("Moved ") && t.contains("files")), "{toasts:?}");
    assert_eq!(radiance_engine::settings::load().music_dir, Some(to.display().to_string()));
    assert!(until(2.0, || c.storage().folder == to.display().to_string()));
    assert!(to.join("Storage Song.opus").exists());
    // back where it was, for the other tests
    c.move_music(&from.display().to_string());
    let _ = toasts_for(&c, &rx, 3.0);
    let mut st = radiance_engine::settings::load();
    st.music_dir = None;
    radiance_engine::settings::save(&st);
    radiance_engine::library::set_music_dir(Some(from.clone())); // (the tests' own music folder again)
    let _ = std::fs::remove_dir_all(to.parent().unwrap());
    // (saved files are liked songs to the other tests: none left behind)
    let _ = std::fs::remove_file(from.join("Storage Song.opus"));
    c.quit();
}

#[test]
fn autoplay_is_kept() {
    let _l = setup();
    let (c, _f, rx) = with_songs("autoplay", vec![]);
    c.set_autoplay(false);
    let toasts = toasts_for(&c, &rx, 2.0);
    assert!(toasts.contains(&"Autoplay off: the music stops when the queue ends".to_string()), "{toasts:?}");
    assert!(!radiance_engine::settings::load().autoplay);
    assert!(!c.autoplay());
    c.set_autoplay(true);
    assert!(until(1.0, || radiance_engine::settings::load().autoplay));
    c.quit();
}

/// Between songs (stage 4a-2): the transition and the crossfade length are kept in the settings both apps share, the
/// length as the Linux app's slider has it (whole seconds, 1 to 12); a transition picked says so.
#[test]
fn transition_and_fade_are_kept() {
    use radiance_engine::settings::Transition;
    let _l = setup();
    let (c, _f, rx) = with_songs("transition", vec![]);
    c.set_transition(Transition::DjMix);
    c.set_fade(9.4);
    let toasts = toasts_for(&c, &rx, 2.0);
    assert!(toasts.contains(&"DJ mix: beats lined up between songs (a crossfade when the tempos are too far apart)".to_string()), "{toasts:?}");
    let s = radiance_engine::settings::load();
    assert_eq!((s.transition, s.fade), (Transition::DjMix, 9.0));
    assert_eq!((c.transition(), c.fade()), (Transition::DjMix, 9.0));
    c.set_fade(30.0);
    assert!(until(1.0, || c.fade() == 12.0), "{}", c.fade());
    c.set_transition(Transition::Off);
    assert!(until(1.0, || radiance_engine::settings::load().transition == Transition::Off));
    c.quit();
}

// ------------------------------------------------------------------ between songs (stage 4a-2)

/// The transition back to Off when a test ends (the settings file is shared by the tests; a song of 40 s or more
/// would otherwise mix in the next test).
fn transition_off(c: &Controller) {
    c.set_transition(radiance_engine::settings::Transition::Off);
    let _ = until(1.0, || radiance_engine::settings::load().transition == radiance_engine::settings::Transition::Off);
}

/// Stage 4a-2 (GTK's transitions): with Crossfade, the next song comes in before the first ends, at its start.
#[test]
fn crossfade_starts_the_next_song_before_the_end() {
    use radiance_engine::settings::Transition;
    let _l = setup();
    let (a, b) = (wav("xf-a", 45.0), wav("xf-b", 45.0));
    let (c, _f, rx, list) = with_list("crossfade", vec![a.clone(), b.clone()]);
    c.set_transition(Transition::Crossfade);
    c.set_fade(3.0);
    c.play_from(list, 0, &a.key());
    assert!(until(4.0, || c.now().playing), "never got going");
    c.seek(34.0);
    assert!(until(12.0, || c.mixing()), "it never mixed");
    assert!(until(1.0, || now_id(&c) == b.id), "the next song isn't the one playing once the mix starts");
    assert!(c.now().at < 5.0, "the next song should be at its start: {}", c.now().at);
    assert!(until(6.0, || !c.mixing()), "the mix never finished");
    assert!(c.now().playing && now_id(&c) == b.id);
    transition_off(&c);
    let _ = toasts_for(&c, &rx, 0.5);
    c.quit();
}

/// No Transition: the song plays to its end, then the next one starts (never a mix).
#[test]
fn no_transition_waits_for_the_end() {
    let _l = setup();
    let (a, b) = (wav("nt-a", 45.0), wav("nt-b", 45.0));
    let (c, _f, rx, list) = with_list("no transition", vec![a.clone(), b.clone()]);
    transition_off(&c);
    c.play_from(list, 0, &a.key());
    assert!(until(4.0, || c.now().playing), "never got going");
    c.seek(40.0);
    let mixed = std::cell::Cell::new(false);
    assert!(until(10.0, || {
        mixed.set(mixed.get() || c.mixing());
        now_id(&c) == b.id
    }), "the next song never came");
    assert!(!mixed.get(), "it mixed with No Transition");
    let _ = toasts_for(&c, &rx, 0.5);
    c.quit();
}

/// Repeat One: the song plays again, never mixes into another.
#[test]
fn repeat_one_never_mixes() {
    use radiance_core::queue::Repeat;
    use radiance_engine::settings::Transition;
    let _l = setup();
    let (a, b) = (wav("r1-a", 45.0), wav("r1-b", 45.0));
    let (c, _f, rx, list) = with_list("repeat one mix", vec![a.clone(), b.clone()]);
    c.set_transition(Transition::Crossfade);
    c.set_fade(3.0);
    c.play_from(list, 0, &a.key());
    assert!(until(4.0, || c.now().playing), "never got going");
    while c.now().repeat != Repeat::One {
        c.cycle_repeat();
        let _ = until(1.0, || false);
    }
    c.seek(38.0);
    let mixed = std::cell::Cell::new(false);
    let _ = until(9.0, || {
        mixed.set(mixed.get() || c.mixing());
        false
    });
    assert!(!mixed.get(), "it mixed with Repeat One");
    assert_eq!(now_id(&c), a.id, "Repeat One: the same song again");
    while c.now().repeat != Repeat::Off {
        c.cycle_repeat();
        let _ = until(1.0, || false);
    }
    transition_off(&c);
    let _ = toasts_for(&c, &rx, 0.5);
    c.quit();
}

/// A song put next while the next one is cued: the mix goes into the song that's next now (review focus 1).
#[test]
fn a_song_put_next_while_cued_is_the_one_mixed_into() {
    use radiance_engine::settings::Transition;
    let _l = setup();
    let (a, b, x) = (wav("pn-a", 45.0), wav("pn-b", 45.0), wav("pn-x", 45.0));
    let (c, _f, rx, list) = with_list("put next mix", vec![a.clone(), b.clone(), x.clone()]);
    c.set_transition(Transition::Crossfade);
    c.set_fade(3.0);
    c.play_from(list, 0, &a.key());
    assert!(until(4.0, || c.now().playing), "never got going");
    c.seek(34.0);
    let _ = until(2.0, || false); // b is cued meanwhile
    c.play_next(&x.key());
    assert!(until(12.0, || c.mixing()), "it never mixed");
    assert!(until(1.0, || now_id(&c) == x.id), "it mixed into {} rather than the song put next", now_id(&c));
    let _ = until(5.0, || !c.mixing());
    transition_off(&c);
    let _ = toasts_for(&c, &rx, 0.5);
    c.quit();
}

/// A mix into the queue's last song still gets Autoplay's songs after it (GTK's begin_mix never asked: review focus 3).
#[test]
fn a_mix_into_the_last_song_still_gets_autoplay() {
    use radiance_engine::settings::Transition;
    let _l = setup();
    let (a, b) = (wav("ap-a", 45.0), wav("ap-b", 45.0));
    let (c, fake, rx, list) = with_list("mix autoplay", vec![a.clone(), b.clone()]);
    let more: Vec<Track> = (0..3).map(|i| song(Source::YouTube, &format!("ap-more-{i}"), &format!("More {i}"))).collect();
    fake.radios.lock().unwrap().insert(b.id.clone(), (Duration::ZERO, more));
    c.set_autoplay(true);
    c.set_transition(Transition::Crossfade);
    c.set_fade(3.0);
    c.play_from(list, 0, &a.key());
    assert!(until(4.0, || c.now().playing), "never got going");
    c.seek(34.0);
    assert!(until(12.0, || c.mixing() && now_id(&c) == b.id), "it never mixed into the last song");
    assert!(until(4.0, || c.queue().0.len() > 2), "Autoplay added nothing after a mix into the last song: {:?}", c.queue().0.iter().map(|t| &t.id).collect::<Vec<_>>());
    let _ = until(5.0, || !c.mixing());
    transition_off(&c);
    let _ = toasts_for(&c, &rx, 0.5);
    c.quit();
}

/// Paused during a crossfade: the fade waits (it went on silently before, GTK too: review focus 2).
#[test]
fn pausing_holds_a_crossfade() {
    use radiance_engine::settings::Transition;
    let _l = setup();
    let (a, b) = (wav("hold-a", 45.0), wav("hold-b", 45.0));
    let (c, _f, rx, list) = with_list("hold mix", vec![a.clone(), b.clone()]);
    c.set_transition(Transition::Crossfade);
    c.set_fade(4.0);
    c.play_from(list, 0, &a.key());
    assert!(until(4.0, || c.now().playing), "never got going");
    c.seek(33.0);
    assert!(until(12.0, || c.mixing()), "it never mixed");
    c.toggle_play();
    assert!(until(2.0, || !c.now().playing), "it didn't pause");
    let _ = until(5.0, || false); // longer than the whole fade
    assert!(c.mixing(), "the fade went on while paused");
    c.toggle_play();
    assert!(until(7.0, || !c.mixing()), "the fade never finished once playing again");
    transition_off(&c);
    let _ = toasts_for(&c, &rx, 0.5);
    c.quit();
}

/// DJ Mix (by the Beat): two 120 BPM click tracks are analysed and mixed (beats lined up, the bass swapped).
#[test]
fn dj_mix_mixes_click_tracks() {
    use radiance_engine::settings::Transition;
    let _l = setup();
    let (a, b) = (clicks("dj-a", 50.0), clicks("dj-b", 50.0));
    let (c, _f, rx, list) = with_list("dj mix", vec![a.clone(), b.clone()]);
    c.set_transition(Transition::DjMix);
    c.play_from(list, 0, &a.key());
    assert!(until(4.0, || c.now().playing), "never got going");
    c.seek(30.0);
    assert!(until(20.0, || c.mixing()), "it never mixed");
    assert!(until(1.0, || now_id(&c) == b.id));
    assert!(until(18.0, || !c.mixing()), "the mix never finished");
    transition_off(&c);
    let _ = toasts_for(&c, &rx, 0.5);
    c.quit();
}

// ------------------------------------------------------------------ sharing (stage 4b)

/// A request to the share as the phone makes it: the reply's body.
fn phone(port: u16, method: &str, path: &str, body: &[u8]) -> String {
    use std::io::{Read, Write};
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).expect("the share listens");
    write!(s, "{method} {path} HTTP/1.1\r\nHost: test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
    s.write_all(body).unwrap();
    let mut out = String::new();
    let _ = s.read_to_string(&mut out);
    out.split_once("\r\n\r\n").map(|(_, b)| b.to_string()).unwrap_or(out)
}

/// A value from the QR's text (radiancemusic://share?v=1&m=…&k=…&p=…).
fn ticket_part(qr: &str, name: &str) -> String {
    qr.split(['?', '&']).find_map(|p| p.strip_prefix(&format!("{name}="))).unwrap_or_default().to_string()
}

/// The phone's plan for one song: its key, its track, and its song file's size.
fn phone_plan(key: &str, id: &str, title: &str, artist: &str, size: usize) -> String {
    serde_json::json!({"songs": [{"key": key, "track": {"source": "youtube", "id": id, "title": title, "artist": artist, "album": "", "seconds": 3, "art": "", "format": "m4a"}, "files": {"song": {"name": format!("{title}.m4a"), "size": size}}}]}).to_string()
}

/// Stage 4b (the Linux app's Share Songs with a Phone, From the Phone): a phone's song comes by the share's protocol
/// (plan, only what's missing, the file, done) and is a liked song here with its file; the share closed stops.
#[test]
fn a_phone_puts_a_song_and_it_is_liked() {
    let _l = setup();
    radiance_engine::library::store(&[]);
    let (c, _f, rx) = with_songs("from the phone", vec![]);
    c.share_start(false);
    assert!(until(3.0, || c.share().qr.is_some()), "no QR: {:?}", c.share());
    let qr = c.share().qr.unwrap();
    assert!(qr.starts_with("radiancemusic://share?") && qr.contains("m=put"), "{qr}");
    let (port, key): (u16, String) = (ticket_part(&qr, "p").parse().unwrap(), ticket_part(&qr, "k"));
    let song = b"not really audio: the share doesn't look".to_vec();
    let plan = phone_plan("yt:phone-1", "phone-1", "From The Phone", "Ana", song.len());
    let want = phone(port, "POST", &format!("/plan?k={key}"), plan.as_bytes());
    assert!(want.contains(r#""yt:phone-1":["song"]"#), "{want}");
    assert_eq!(phone(port, "PUT", &format!("/file?k={key}&key=yt:phone-1&kind=song"), &song), "ok");
    assert_eq!(phone(port, "POST", &format!("/done?k={key}&key=yt:phone-1"), b""), "ok");
    assert!(until(3.0, || c.liked().iter().any(|(t, _)| t.id == "phone-1" && t.file.is_some())), "it isn't in Liked with its file");
    assert!(until(1.0, || c.share().progress.as_deref() == Some("Done: 1 song here")), "{:?}", c.share().progress);
    let again = phone(port, "POST", &format!("/plan?k={key}"), plan.as_bytes());
    assert!(again.contains(r#""want":{}"#), "the same song again: nothing's missing now: {again}");
    c.share_stop();
    assert!(until(2.0, || std::net::TcpStream::connect(("127.0.0.1", port)).is_err()), "the share still listens once closed");
    radiance_engine::library::store(&[]);
    let _ = toasts_for(&c, &rx, 0.5);
    c.quit();
}

/// A song from the phone that's already liked here gets its file, never a second row (review focus 2).
#[test]
fn a_song_already_liked_gets_its_file_not_a_second_row() {
    let _l = setup();
    let liked = song(Source::YouTube, "phone-2", "Already Liked");
    let (c, _rx) = with_liked(&[liked], Arc::new(Fake::default()));
    c.share_start(false);
    assert!(until(3.0, || c.share().qr.is_some()), "no QR: {:?}", c.share());
    let qr = c.share().qr.unwrap();
    let (port, key): (u16, String) = (ticket_part(&qr, "p").parse().unwrap(), ticket_part(&qr, "k"));
    let song = b"the phone's copy".to_vec();
    let plan = phone_plan("yt:phone-2", "phone-2", "Already Liked", "Test Artist", song.len());
    assert!(phone(port, "POST", &format!("/plan?k={key}"), plan.as_bytes()).contains("yt:phone-2"));
    assert_eq!(phone(port, "PUT", &format!("/file?k={key}&key=yt:phone-2&kind=song"), &song), "ok");
    assert_eq!(phone(port, "POST", &format!("/done?k={key}&key=yt:phone-2"), b""), "ok");
    assert!(until(3.0, || c.liked().iter().any(|(t, _)| t.id == "phone-2" && t.file.is_some())), "no file: {:?}", c.liked());
    assert_eq!(c.liked().iter().filter(|(t, _)| t.title == "Already Liked").count(), 1, "a second row");
    c.share_stop();
    radiance_engine::library::store(&[]);
    c.quit();
}

/// To the Phone: the liked songs saved here are offered (the phone takes only what it hasn't got).
#[test]
fn to_the_phone_offers_the_saved_liked_songs() {
    let _l = setup();
    let saved = liked_song("share-saved", "Saved Here", "Someone", "", None);
    let (c, _rx) = with_liked(&[saved], Arc::new(Fake::default()));
    c.share_start(true);
    assert!(until(3.0, || c.share().qr.is_some()), "no QR: {:?}", c.share());
    let st = c.share();
    let qr = st.qr.unwrap();
    assert!(qr.contains("m=get"), "{qr}");
    assert!(st.caption.contains("It takes your 1 liked songs"), "{}", st.caption);
    let (port, key): (u16, String) = (ticket_part(&qr, "p").parse().unwrap(), ticket_part(&qr, "k"));
    let list = phone(port, "GET", &format!("/list?k={key}"), b"");
    assert!(list.contains("Saved Here") && list.contains(r#""song""#), "{list}");
    assert_eq!(phone(port, "GET", "/list?k=wrong", b""), "not this share");
    c.share_stop();
    radiance_engine::library::store(&[]);
    c.quit();
}

#[test]
fn the_guide_is_seen_once() {
    let _l = setup();
    let mut st = radiance_engine::settings::load();
    st.guide_seen = false;
    radiance_engine::settings::save(&st);
    let (c, _f, _rx) = with_songs("guide", vec![]);
    assert!(!c.guide_seen());
    c.set_guide_seen();
    assert!(until(1.0, || radiance_engine::settings::load().guide_seen && c.guide_seen()));
    c.quit();
}

// ------------------------------------------------------------------ offline and suggestions (stage 4a)

#[test]
fn back_online_says_so_and_searches_again() {
    let _l = setup();
    let (c, fake, rx) = with_songs("back online", vec![]);
    c.search("rebelution");
    assert!(until(2.0, || fake.searches.load(Ordering::SeqCst) >= 2), "never searched");
    std::thread::sleep(Duration::from_millis(200));
    let before = fake.searches.load(Ordering::SeqCst);
    c.network_changed(false);
    assert!(until(1.0, || !c.online()));
    let fetched = fake.two_part_fetches.load(Ordering::SeqCst);
    c.network_changed(true);
    let toasts = toasts_for(&c, &rx, 2.0);
    assert!(toasts.contains(&"Back online".to_string()), "{toasts:?}");
    assert!(fake.searches.load(Ordering::SeqCst) > before, "the search didn't run again");
    assert!(fake.two_part_fetches.load(Ordering::SeqCst) > fetched, "the fast split's files weren't fetched");
    c.quit();
}

#[test]
fn the_start_reads_the_network() {
    let _l = setup();
    let fake = Arc::new(Fake::default());
    fake.network_down.store(true, Ordering::SeqCst);
    let (heard, _rx) = listener();
    let c = Controller::start(heard, fake.clone()).unwrap();
    assert!(until(2.0, || !c.online()), "it started online with no network");
    c.quit();
}

#[test]
fn suggestions_follow_the_typing() {
    let _l = setup();
    let (c, fake, _rx) = with_songs("suggest", vec![]);
    fake.suggestions.lock().unwrap().insert("a".into(), (Duration::from_millis(600), vec!["a late".into()]));
    fake.suggestions.lock().unwrap().insert("ab".into(), (Duration::ZERO, vec!["ab now".into()]));
    c.suggest("a");
    c.suggest("ab");
    assert!(until(2.0, || c.suggestions().iter().any(|s| s.text == "ab now")), "{:?}", c.suggestions());
    std::thread::sleep(Duration::from_millis(800));
    assert!(!c.suggestions().iter().any(|s| s.text == "a late"), "an older answer replaced the newer: {:?}", c.suggestions());
    c.quit();
}

#[test]
fn liked_songs_are_split_by_themselves() {
    let _l = setup();
    std::env::set_var("RADIANCE_AUTO_SPLIT_S", "0");
    let fake = Arc::new(Fake::default());
    let (a, b) = (liked_song("auto-a", "Auto A", "Someone", "", None), liked_song("auto-b", "Auto B", "Someone", "", None));
    let (c, _rx) = with_liked(&[a, b], fake.clone());
    let got = until(4.0, || started(&fake).len() == 2);
    std::env::remove_var("RADIANCE_AUTO_SPLIT_S");
    assert!(got, "liked songs weren't split by themselves: {:?}", started(&fake));
    assert!(started(&fake).iter().all(|(_, two)| !two), "in four parts");
    c.quit();
    radiance_engine::library::store(&[]); // (Home's test wants none liked)
}

#[test]
fn a_failed_split_asked_for_says_so() {
    let _l = setup();
    let (c, fake, rx) = with_songs("split fails", vec![song(Source::YouTube, "sf", "Will Not Split")]);
    fake.split_fails.lock().unwrap().insert("sf".into());
    c.split("yt:sf", false);
    let toasts = toasts_for(&c, &rx, 3.0);
    assert!(toasts.iter().any(|t| t == "Couldn't split “Will Not Split” right now"), "{toasts:?}");
    assert!(matches!(c.split_mark("yt:sf"), SplitMark::None));
    c.quit();
}

#[test]
fn the_fast_split_takes_its_own_lane() {
    let _l = setup();
    let songs = vec![song(Source::YouTube, "f4", "Four Parts"), song(Source::YouTube, "f2", "Two Parts")];
    let (c, fake, _rx) = with_songs("split lanes", songs);
    *fake.split_wait.lock().unwrap() = Duration::from_millis(800);
    c.split("yt:f4", false);
    assert!(until(1.0, || started(&fake).len() == 1));
    c.split("yt:f2", true);
    assert!(until(0.5, || started(&fake).len() == 2), "the fast split waited for the four-part one");
    assert_eq!(started(&fake)[1], ("f2".to_string(), true));
    assert_eq!(c.splits().two_waiting.len(), 0);
    c.quit();
}

// ---------------------------------------------------------------------------------- the first start's setup (Task 5)

fn no_local_splitter() {
    let mut st = radiance_engine::settings::load();
    st.local_split = None;
    radiance_engine::settings::save(&st);
}

/// A controller at its very first start (no splitter yet): RADIANCE_SETUP_TEST makes it set one up here.
fn first_start(fake: Arc<Fake>) -> Controller {
    no_local_splitter();
    std::env::set_var("RADIANCE_SETUP_TEST", "1");
    let (heard, _rx) = listener();
    let c = Controller::start(heard, fake).unwrap();
    std::env::remove_var("RADIANCE_SETUP_TEST");
    c
}

/// #61 M7: a developer run (the beta's screenshots and measures) downloads no splitter at a first start.
#[test]
fn a_developer_run_downloads_no_splitter() {
    let _l = setup();
    std::env::set_var("RADIANCE_SETUP", "off");
    let fake = Arc::new(Fake::default());
    let c = first_start(fake.clone());
    std::thread::sleep(Duration::from_millis(800));
    std::env::remove_var("RADIANCE_SETUP");
    assert_eq!(fake.installs.load(Ordering::SeqCst), 0, "it downloaded a splitter");
    assert_eq!(c.setup(), SetupState::NotNeeded, "it held the start for a splitter");
    // (final review: nor the fast split's files, about 50 MB onto the test computer's C:)
    assert_eq!(fake.two_part_fetches.load(Ordering::SeqCst), 0, "it fetched the fast split's files");
    c.quit();
}

/// Final review I3: a computer that can't split (too little disk, memory or cores) goes on without it,
/// as GTK's get_splitter_ready: no hold, no endless retry.
#[test]
fn a_computer_that_cant_split_goes_on_without_it() {
    let _l = setup();
    let fake = Arc::new(Fake::default());
    *fake.setup_problems.lock().unwrap() = vec!["It needs 6 GB free on the drive with your home folder; 1.0 GB is free.".into()];
    let c = first_start(fake.clone());
    std::thread::sleep(Duration::from_millis(800));
    assert_eq!(c.setup(), SetupState::NotNeeded, "it held the start for a splitter that can't be");
    assert_eq!(fake.installs.load(Ordering::SeqCst), 0);
    c.quit();
}

/// Final review I2: with a split server set, nothing is downloaded at the first start; a splitter whose
/// Python is gone is set up again (GTK's guard).
#[test]
fn a_split_server_needs_no_first_start_download() {
    let _l = setup();
    let mut st = radiance_engine::settings::load();
    st.split = Some(radiance_engine::settings::SplitServer { host: "server".into(), user: "me".into(), python: "python3".into(), script: "split.py".into(), work: "/tmp".into(), library: String::new() });
    radiance_engine::settings::save(&st);
    let fake = Arc::new(Fake::default());
    let c = first_start(fake.clone());
    std::thread::sleep(Duration::from_millis(800));
    assert_eq!(c.setup(), SetupState::NotNeeded);
    assert_eq!(fake.installs.load(Ordering::SeqCst), 0, "it downloaded a splitter the server makes unneeded");
    c.quit();
    let mut st = radiance_engine::settings::load();
    st.split = None;
    st.local_split = Some(radiance_engine::settings::LocalSplitter { python: "/nowhere/python".into(), script: "/nowhere/split.py".into(), threads: 2 });
    radiance_engine::settings::save(&st);
    let fake = Arc::new(Fake::default());
    std::env::set_var("RADIANCE_SETUP_TEST", "1");
    let (heard, _rx) = listener();
    let c = Controller::start(heard, fake.clone()).unwrap();
    std::env::remove_var("RADIANCE_SETUP_TEST");
    assert!(until(4.0, || fake.installs.load(Ordering::SeqCst) == 1), "a splitter whose Python is gone wasn't set up again");
    c.quit();
    no_local_splitter();
}

#[test]
fn the_first_start_gets_splitting_ready_with_progress() {
    let _l = setup();
    let c = first_start(Arc::new(Fake::default()));
    let mut steps: Vec<String> = vec![];
    // (15 s: the whole suite on a 4-core VM took longer than 4 s now and then; it's done as soon as it's Ready)
    let end = Instant::now() + Duration::from_secs(15);
    while Instant::now() < end && c.setup() != SetupState::Ready {
        if let SetupState::Getting { step, .. } = c.setup() {
            if !step.is_empty() && steps.last() != Some(&step) {
                steps.push(step);
            }
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    assert_eq!(c.setup(), SetupState::Ready);
    assert_eq!(steps, ["Downloading Demucs…", "Checking it works…"]);
    assert!(radiance_engine::settings::load().local_split.is_some(), "kept for the next start");
    c.quit();
    no_local_splitter();
}

#[test]
fn offline_first_start_lets_you_in_and_gets_ready_later() {
    let _l = setup();
    let fake = Arc::new(Fake::default());
    fake.offline.store(true, Ordering::SeqCst);
    let c = first_start(fake.clone());
    assert!(until(2.0, || c.setup() == SetupState::Offline), "{:?}", c.setup());
    fake.offline.store(false, Ordering::SeqCst);
    c.search("back online");
    assert!(until(4.0, || c.setup() == SetupState::Ready), "it didn't get ready once online: {:?}", c.setup());
    c.quit();
    no_local_splitter();
}

/// Final review I4: offline at the first start, it tries again by itself (GTK's 30 s; here
/// RADIANCE_SETUP_RETRY_MS), with no search or song needed to notice the internet's back.
#[test]
fn offline_first_start_tries_again_by_itself() {
    let _l = setup();
    std::env::set_var("RADIANCE_SETUP_RETRY_MS", "300");
    let fake = Arc::new(Fake::default());
    fake.offline.store(true, Ordering::SeqCst);
    let c = first_start(fake.clone());
    assert!(until(2.0, || c.setup() == SetupState::Offline), "{:?}", c.setup());
    fake.offline.store(false, Ordering::SeqCst);
    assert!(until(4.0, || c.setup() == SetupState::Ready), "it never tried again: {:?}", c.setup());
    std::env::remove_var("RADIANCE_SETUP_RETRY_MS");
    c.quit();
    no_local_splitter();
}

#[test]
fn cores_are_kept_for_the_next_split() {
    let _l = setup();
    let c = first_start(Arc::new(Fake::default()));
    assert!(until(4.0, || c.setup() == SetupState::Ready));
    c.set_split_cores(3);
    assert!(until(1.0, || radiance_engine::settings::load().local_split.is_some_and(|l| l.threads == 3)));
    assert_eq!(c.split_cores().0, 3);
    assert!(c.split_cores().1 >= 1);
    c.quit();
    no_local_splitter();
}

// ---------------------------------------------------------------------------------- parts and the Mixer (Task 6)

use radiance_engine::effects::Sound;
use radiance_engine::split::Voice;

/// A song's parts, laid down where the Mixer finds them (beside its file): short quiet FLACs.
fn make_parts(t: &Track, parts: &[&str], secs: u32) {
    use gst::prelude::*;
    gst::init().unwrap();
    let base = radiance_engine::split::stems_base(t);
    for part in parts {
        let path = base.with_file_name(format!("{}.{part}.flac", base.file_name().unwrap().to_string_lossy()));
        let desc = format!("audiotestsrc num-buffers={} samplesperbuffer=4410 volume=0.05 ! audio/x-raw,format=S16LE,rate=44100,channels=2 ! flacenc ! filesink name=out", secs * 10);
        let p = gst::parse::launch(&desc).unwrap();
        p.downcast_ref::<gst::Bin>().unwrap().by_name("out").unwrap().set_property("location", path.to_str().unwrap());
        p.set_state(gst::State::Playing).unwrap();
        let done = p.bus().unwrap().timed_pop_filtered(gst::ClockTime::from_seconds(30), &[gst::MessageType::Eos, gst::MessageType::Error]);
        p.set_state(gst::State::Null).unwrap();
        assert_eq!(done.map(|m| m.type_()), Some(gst::MessageType::Eos), "the part {part} wasn't made");
    }
}

fn part_names(c: &Controller) -> Vec<String> {
    c.now().mix.parts.iter().map(|(p, _)| p.clone()).collect()
}

fn gain(c: &Controller, part: &str) -> f64 {
    c.now().mix.parts.iter().find(|(p, _)| p == part).map(|(_, g)| *g).unwrap_or(-1.0)
}

#[test]
fn a_split_song_plays_its_parts() {
    let _l = setup();
    let t = wav("parts-play", 4.0);
    make_parts(&t, &["drums", "bass", "other", "vocals"], 4);
    let (c, _f, _rx, list) = with_list("parts play", vec![t.clone()]);
    c.play_from(list, 0, &t.key());
    assert!(until(4.0, || c.now().playing && c.now().mix.plays_parts), "it didn't play its parts");
    assert_eq!(part_names(&c), ["drums", "bass", "other", "vocals"]);
    // all up, it's the song's own file that's heard (its quality); a part down, the Mixer's
    c.set_part("vocals", 0.5);
    assert!(until(2.0, || c.now().quality == "Mixer · its four parts"), "{}", c.now().quality);
    c.quit();
}

/// #61 M1: a song split in two says so in the Mixer's quality words (the Linux app said "its four parts").
#[test]
fn a_song_split_in_two_says_music_and_vocals() {
    let _l = setup();
    let t = wav("parts-two", 4.0);
    make_parts(&t, &["music", "vocals"], 4);
    let (c, _f, _rx, list) = with_list("parts two", vec![t.clone()]);
    c.play_from(list, 0, &t.key());
    assert!(until(4.0, || c.now().playing && c.now().mix.plays_parts), "it didn't play its parts");
    c.set_part("vocals", 0.5);
    assert!(until(2.0, || c.now().quality == "Mixer · Music and Vocals"), "{}", c.now().quality);
    c.quit();
}

#[test]
fn the_mixer_shows_the_parts_the_song_has() {
    let _l = setup();
    let (four, two, none) = (wav("mix-four", 3.0), wav("mix-two", 3.0), wav("mix-none", 3.0));
    make_parts(&four, &["drums", "bass", "other", "vocals"], 3);
    make_parts(&two, &["music", "vocals"], 3);
    let (c, _f, _rx, list) = with_list("mix parts", vec![four.clone(), two.clone(), none.clone()]);
    c.play_from(list.clone(), 0, &four.key());
    assert!(until(4.0, || part_names(&c) == ["drums", "bass", "other", "vocals"]), "{:?}", part_names(&c));
    c.set_voice(Voice::NoVocals);
    assert!(until(1.0, || gain(&c, "vocals") == 0.0 && gain(&c, "drums") == 1.0), "No vocals is a preset on four parts");
    c.set_voice(Voice::Song);
    c.play_from(list.clone(), 1, &two.key());
    assert!(until(4.0, || part_names(&c) == ["music", "vocals"]), "{:?}", part_names(&c));
    c.set_voice(Voice::VocalsOnly);
    assert!(until(1.0, || gain(&c, "music") == 0.0 && gain(&c, "vocals") == 1.0), "on halves too");
    c.set_voice(Voice::Song);
    c.play_from(list, 2, &none.key());
    assert!(until(4.0, || now_id(&c) == none.id && part_names(&c).is_empty()), "{:?}", part_names(&c));
    c.quit();
}

/// Final review C2: No vocals on a song not split yet asks for its split and, once it lands, plays it
/// without the singer (the presets were dropped: it came back with every part at full), saying so as
/// GTK's split_landed does ("“Title”: no vocals").
#[test]
fn no_vocals_on_a_song_not_split_plays_without_the_singer_once_split() {
    let _l = setup();
    let t = wav("voice-unsplit", 8.0);
    let (c, fake, rx, list) = with_list("voice unsplit", vec![t.clone()]);
    fake.split_makes_parts.store(true, Ordering::SeqCst);
    c.play_from(list, 0, &t.key());
    assert!(until(4.0, || c.now().playing && c.now().at >= 0.5), "never got going");
    *fake.split_wait.lock().unwrap() = Duration::from_millis(1500);
    c.set_voice(Voice::NoVocals);
    // until it's split, the Mixer says so (GTK's voice status)
    assert!(until(1.0, || c.now().mix.note == "Plays as the full song until it's split."), "{:?}", c.now().mix.note);
    let toasts = toasts_for(&c, &rx, 5.0);
    assert!(until(6.0, || c.now().mix.plays_parts), "its parts never played");
    assert_eq!(c.now().mix.note, "", "the note stays once it's split");
    assert_eq!(gain(&c, "vocals"), 0.0, "the singer is back: {:?}", c.now().mix.parts);
    assert!(toasts.iter().any(|m| m == &format!("“{}”: no vocals", t.title)), "{toasts:?}");
    c.set_voice(Voice::Song);
    c.quit();
}

/// Owner, 2026-11-07, the beta's Mixer pop-up: "pag nag next song need ko pa pumunta sa song then bumalek sa no vocals
/// para lang mag split???", "diba dapat automatic sya babalek sa song kase di pa downloaded", "walang loading indicator sa
/// mixer kung splitting pa". No vocals carries on to the next song: it's split by itself (the one after it ahead, as GTK's
/// play start), the Mixer shows Song (what's heard) and that it's splitting for No vocals till its parts land, then No vocals.
#[test]
fn no_vocals_carries_to_the_next_song_and_splits_it() {
    let _l = setup();
    let (a, b, n) = (wav("carry-a", 8.0), wav("carry-b", 8.0), wav("carry-n", 8.0));
    let (c, fake, rx, list) = with_list("voice carries", vec![a.clone(), b.clone(), n.clone()]);
    fake.split_makes_parts.store(true, Ordering::SeqCst);
    *fake.split_wait.lock().unwrap() = Duration::from_millis(1500);
    c.play_from(list, 0, &a.key());
    assert!(until(4.0, || c.now().playing), "never got going");
    c.set_voice(Voice::NoVocals);
    // the next song is split ahead (ready by the time it plays)
    assert!(until(2.0, || c.splits().two_waiting.iter().chain(c.splits().two_now.iter()).any(|t| t.key() == b.key())), "the next song wasn't split ahead: {:?}", c.splits().two_waiting);
    c.skip(1);
    assert!(until(4.0, || now_id(&c) == b.id && c.now().playing), "Next didn't play the next song");
    // till its parts land: Song shown (it's what's heard), splitting for No vocals said, and the one after it ahead
    let m = c.now().mix;
    if !m.plays_parts {
        assert_eq!((m.voice, m.splitting_for), (Voice::Song, Some(Voice::NoVocals)), "{m:?}");
    }
    assert!(until(2.0, || c.splits().two_waiting.iter().chain(c.splits().two_now.iter()).any(|t| t.key() == n.key()) || c.now().mix.plays_parts), "the song after it wasn't split ahead");
    assert!(until(8.0, || c.now().mix.plays_parts), "the next song was never split (it needed Song, then No vocals again)");
    let m = c.now().mix;
    assert_eq!((m.voice, m.splitting_for), (Voice::NoVocals, None), "{m:?}");
    assert_eq!(gain(&c, "vocals"), 0.0, "the singer is still there: {:?}", m.parts);
    c.set_voice(Voice::Song);
    let _ = toasts_for(&c, &rx, 1.0);
    c.quit();
}

/// Owner, 2026-11-08 (the beta's Mixer): "bakit di ko paren nakikita sa mixer kung splitting pa lang", "walang option sa
/// mixer to split 4 ways". Any split of the song playing is said in the Mixer, and it offers the splits (Four Parts too).
#[test]
fn the_mixer_says_the_song_is_splitting_and_offers_four_parts() {
    use radiance_core::menu::Action;
    let _l = setup();
    let t = wav("mixer-split", 8.0);
    let (c, fake, rx, list) = with_list("mixer split", vec![t.clone()]);
    fake.split_makes_parts.store(true, Ordering::SeqCst);
    *fake.split_wait.lock().unwrap() = Duration::from_millis(1500);
    c.play_from(list, 0, &t.key());
    assert!(until(4.0, || c.now().playing), "never got going");
    let actions = |c: &Controller| c.mixer_split().items.iter().map(|i| i.action.clone()).collect::<Vec<_>>();
    assert!(until(2.0, || actions(&c).contains(&Action::SplitFour)), "Four Parts isn't offered: {:?}", c.mixer_split());
    assert_eq!(c.mixer_split().line, None);
    c.split(&t.key(), false);
    assert!(until(1.0, || c.mixer_split().line.is_some_and(|l| l.starts_with("Splitting into four parts"))), "{:?}", c.mixer_split());
    assert!(!c.mixer_split().for_voice, "no voice waits: no Turn Off");
    assert!(!actions(&c).contains(&Action::SplitFour), "offered again while it splits");
    assert!(until(6.0, || c.mixer_split().line.is_none()), "the line stayed once it was split: {:?}", c.mixer_split());
    let _ = toasts_for(&c, &rx, 1.0);
    c.quit();
}

/// Turned back to Song while the song splits: nothing's waiting for it any more.
#[test]
fn song_while_splitting_for_no_vocals_waits_for_nothing() {
    let _l = setup();
    let t = wav("carry-off", 8.0);
    let (c, fake, rx, list) = with_list("voice off", vec![t.clone()]);
    fake.split_makes_parts.store(true, Ordering::SeqCst);
    *fake.split_wait.lock().unwrap() = Duration::from_millis(1500);
    c.play_from(list, 0, &t.key());
    assert!(until(4.0, || c.now().playing), "never got going");
    c.set_voice(Voice::NoVocals);
    assert!(until(1.0, || c.now().mix.splitting_for == Some(Voice::NoVocals)), "{:?}", c.now().mix);
    c.set_voice(Voice::Song);
    assert!(until(1.0, || c.now().mix.splitting_for.is_none()), "{:?}", c.now().mix);
    let m = c.now().mix;
    assert_eq!((m.voice, m.splitting_for, m.note.as_str()), (Voice::Song, None, ""), "{m:?}");
    let _ = toasts_for(&c, &rx, 2.0);
    c.quit();
}

#[test]
fn parts_landing_while_playing_switch_over_in_place() {
    let _l = setup();
    let t = wav("parts-land", 6.0);
    let (c, fake, _rx, list) = with_list("parts land", vec![t.clone()]);
    fake.split_makes_parts.store(true, Ordering::SeqCst);
    c.play_from(list, 0, &t.key());
    assert!(until(4.0, || c.now().playing && c.now().at >= 1.0), "never got going");
    c.split(&t.key(), false);
    assert!(until(6.0, || c.now().mix.plays_parts), "it didn't switch to its parts");
    assert!(c.now().at >= 0.8, "it went back to the start: {}", c.now().at);
    c.quit();
}

/// Final review C1: parts landing for a paused song (or last time's song, shown at the start) leave it
/// paused, as GTK's split_landed (it never starts the parts; the next Play or a slider picks them up).
#[test]
fn parts_landing_while_paused_stay_paused() {
    let _l = setup();
    let t = wav("parts-land-paused", 6.0);
    let (c, fake, _rx, list) = with_list("parts land paused", vec![t.clone()]);
    fake.split_makes_parts.store(true, Ordering::SeqCst);
    c.play_from(list, 0, &t.key());
    assert!(until(4.0, || c.now().playing && c.now().at >= 1.0), "never got going");
    c.toggle_play();
    assert!(until(2.0, || !c.now().playing), "it didn't pause");
    c.split(&t.key(), false);
    assert!(until(6.0, || !fake.splits_started.lock().unwrap().is_empty()), "the split never started");
    assert!(until(6.0, || c.splits().now.is_none() && c.splits().waiting.is_empty()), "the split never landed");
    std::thread::sleep(std::time::Duration::from_millis(800));
    assert!(!c.now().playing, "the landing parts started it playing");
    c.quit();
}

#[test]
fn effects_are_kept_in_settings() {
    let _l = setup();
    let (c, _f, _rx) = with_songs("effects kept", vec![]);
    let s = Sound { low_cut: 120.0, high_cut: 9000.0, reverb: 30.0 };
    c.set_effects(s);
    assert!(until(1.0, || radiance_engine::settings::load().effects == s), "not saved");
    assert_eq!(radiance_engine::effects::now(), s);
    assert!(until(1.0, || c.now().mix.effects == s));
    c.set_effects(Sound::OFF);
    assert!(until(1.0, || radiance_engine::settings::load().effects == Sound::OFF));
    c.quit();
}

#[test]
fn reset_puts_every_part_back_at_full() {
    let _l = setup();
    let t = wav("parts-reset", 4.0);
    make_parts(&t, &["drums", "bass", "other", "vocals"], 4);
    let (c, _f, _rx, list) = with_list("parts reset", vec![t.clone()]);
    c.play_from(list, 0, &t.key());
    assert!(until(4.0, || c.now().mix.plays_parts));
    c.set_part("vocals", 0.2);
    c.set_effects(Sound { low_cut: 200.0, high_cut: HIGH, reverb: 0.0 });
    assert!(until(1.0, || gain(&c, "vocals") == 0.2));
    c.reset_mix();
    assert!(until(1.0, || c.now().mix.parts.iter().all(|(_, g)| *g == 1.0) && c.now().mix.effects == Sound::OFF));
    c.quit();
}

const HIGH: f64 = radiance_engine::effects::HIGH_CUT.1;

// ---------------------------------------------------------------------------------- lyrics: editing, finding again, timing (Task 7)

fn timed(texts: &[&str]) -> Lyrics {
    let lines = texts
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let (start, end) = (i as f64 * 1.5, i as f64 * 1.5 + 1.5);
            Line { start, end, text: t.to_string(), words: radiance_engine::lyrics::spread(t, start, end), singer: None }
        })
        .collect();
    Lyrics::Timed { lines, exact: false }
}

fn shown_lines(c: &Controller) -> Vec<String> {
    match c.lyrics() {
        LyricsState::Lines(l) => l.iter().map(|x| x.line.text.clone()).collect(),
        _ => vec![],
    }
}

#[test]
fn edited_lyrics_are_kept_and_never_replaced() {
    let _l = setup();
    let t = wav("edited-kept", 4.0);
    let (c, fake, rx, list) = with_list("edited kept", vec![t.clone()]);
    *fake.lyrics.lock().unwrap() = Some(timed(&["someone else's", "lines"]));
    c.save_lyrics(&t.key(), vec![(Some(0.5), "mine one".into(), None), (Some(1.5), "mine two".into(), Some("Nairud".into()))]);
    assert!(toasts_for(&c, &rx, 2.0).iter().any(|x| x == "Lyrics saved"));
    c.play_from(list, 0, &t.key());
    assert!(until(4.0, || shown_lines(&c) == ["mine one", "mine two"]), "{:?}", shown_lines(&c));
    let (rows, names) = c.lyrics_text(&t.key()).expect("the editor's rows");
    assert_eq!(rows.iter().map(|r| r.1.as_str()).collect::<Vec<_>>(), ["mine one", "mine two"]);
    assert!(names.contains(&"Nairud".to_string()) && names.contains(&"Test Artist".to_string()), "{names:?}");
    c.quit();
}

#[test]
fn found_again_lyrics_replace_the_kept_ones() {
    let _l = setup();
    let t = wav("found-again", 4.0);
    let (c, fake, rx, list) = with_list("found again", vec![t.clone()]);
    c.save_lyrics(&t.key(), vec![(Some(0.5), "old one".into(), None)]);
    c.play_from(list, 0, &t.key());
    assert!(until(4.0, || shown_lines(&c) == ["old one"]));
    *fake.lyrics.lock().unwrap() = Some(timed(&["new one", "new two"]));
    c.find_lyrics_again(&t.key());
    let toasts = toasts_for(&c, &rx, 3.0);
    assert!(toasts.iter().any(|x| x == "Finding “Song found-again”'s lyrics again…"), "{toasts:?}");
    assert!(toasts.iter().any(|x| x == "New lyrics found"), "{toasts:?}");
    assert!(until(3.0, || shown_lines(&c) == ["new one", "new two"]), "{:?}", shown_lines(&c));
    c.quit();
}

#[test]
fn lyrics_come_back_when_online_again() {
    let _l = setup();
    let t = wav("back-online", 6.0);
    let (c, fake, _rx, list) = with_list("lyrics back", vec![t.clone()]);
    fake.offline.store(true, Ordering::SeqCst);
    c.search("nothing while offline");
    assert!(until(2.0, || !c.online()), "never went offline");
    c.play_from(list, 0, &t.key());
    assert!(until(3.0, || matches!(c.lyrics(), LyricsState::None)), "lyrics while offline?");
    fake.offline.store(false, Ordering::SeqCst);
    *fake.lyrics.lock().unwrap() = Some(timed(&["back", "again"]));
    c.search("online again");
    assert!(until(4.0, || shown_lines(&c) == ["back", "again"]), "they didn't come back: {:?}", shown_lines(&c));
    c.quit();
}

#[test]
fn words_are_timed_after_a_split_lands() {
    let _l = setup();
    let (c, fake, _rx) = with_songs("timed after", vec![song(Source::YouTube, "tw", "Timed After")]);
    c.split("yt:tw", true);
    assert!(until(3.0, || fake.time_words_calls.load(Ordering::SeqCst) == 1), "the words weren't timed after the fast split");
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(fake.time_words_calls.load(Ordering::SeqCst), 1, "once");
    c.quit();
}

// ---------------------------------------------------------------------------------- the beat for the background (Task 8, #56)

/// A WAV of clicks, one every half second (a beat for beat::pulse to find), named for its test.
fn clicks(name: &str, secs: f64) -> Track {
    let mut t = wav(name, secs);
    let rate = 44_100usize;
    let n = (secs * rate as f64) as usize;
    let mut b = std::fs::read(t.file.as_ref().unwrap()).unwrap();
    for i in 0..n {
        let in_click = i % (rate / 2) < rate / 100; // 10 ms
        let v: i16 = if in_click { if i % 20 < 10 { 12_000 } else { -12_000 } } else { 0 };
        b[44 + 2 * i..46 + 2 * i].copy_from_slice(&v.to_le_bytes());
    }
    std::fs::write(t.file.as_ref().unwrap(), b).unwrap();
    t.title = format!("Clicks {name}");
    t
}

#[test]
fn a_playing_song_gets_its_pulse() {
    let _l = setup();
    // (beat::pulse wants more than 8 s of a song)
    let (beat, next) = (clicks("pulse-a", 10.0), wav("pulse-b", 3.0));
    let (c, _f, rx, list) = with_list("pulse", vec![beat.clone(), next.clone()]);
    c.play_from(list.clone(), 0, &beat.key());
    assert!(until(9.0, || c.pulse().is_some_and(|(k, p)| k == beat.key() && p.fps > 0.0 && !p.level.is_empty())), "no pulse for the song");
    assert!(wait_for(&rx, Change::Pulse, 1.0), "the pulse was never told (Now Playing waits for it)");
    c.play_from(list, 1, &next.key());
    assert!(until(2.0, || c.pulse().is_none_or(|(k, _)| k != beat.key())), "the last song's pulse stayed");
    c.quit();
}


// ---------------------------------------------------------------- stage 4c: Radiance Music Servers

/// A Radiance Music Server standing in, for the rest of the process: /api/status, /api/join (code ABCDEFGHJKMN),
/// /api/me, /api/library, /api/party, the revoke; `mode` 0: as it should, 1: not answering (the line closes with no
/// reply), 2: the key revoked (401 for every keyed call). Anything else: 404 (the events line too: it tries again).
fn rms_stand_in(mode: Arc<std::sync::atomic::AtomicU8>) -> String {
    use std::io::{BufRead, BufReader, Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(mut conn) = conn else { continue };
            let mode = mode.load(Ordering::SeqCst);
            let mut reader = BufReader::new(conn.try_clone().unwrap());
            let mut first = String::new();
            if reader.read_line(&mut first).is_err() {
                continue;
            }
            let (mut keyed, mut length) = (false, 0usize);
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
                    break;
                }
                let low = line.to_ascii_lowercase();
                keyed |= low.trim() == "authorization: bearer tok-4c";
                if let Some(v) = low.strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0u8; length];
            let _ = reader.read_exact(&mut body);
            if mode == 1 {
                continue; // not answering
            }
            let mut parts = first.split_whitespace();
            let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or("").split('?').next().unwrap_or(""));
            let me = r#"{"person":{"id":3,"name":"Ana","role":"friend"},"device":{"id":7,"name":"laptop"},"server":{"name":"Bassi's server","id":"srv-4c"},"can":{"add":true,"split":true,"host":false}}"#;
            let library = r#"{"server":"Bassi's server","count":2,"songs":[
                {"id":4,"artist":"Hev Abi","title":"Para Sa Streets","file":"Hev Abi - Para Sa Streets.opus","url":"/files/Hev%20Abi%20-%20Para%20Sa%20Streets.opus","duration_ms":201500,"split":false,"parts":[],"files":{"song":{"name":"Hev Abi - Para Sa Streets.opus","size":3,"url":"/files/Hev%20Abi%20-%20Para%20Sa%20Streets.opus"}},"added_by":{"id":1,"name":"Bassi"}},
                {"id":5,"artist":"Maki","title":"Dilaw","file":"Maki - Dilaw.flac","url":"/files/Maki%20-%20Dilaw.flac","duration_ms":250000,"split":false,"parts":[],"files":{},"added_by":null}]}"#;
            let (status, json): (u16, String) = match (method, path, keyed, mode) {
                ("GET", "/api/status", _, _) => (200, r#"{"server":"Bassi's server","id":"srv-4c","needs_setup":false}"#.into()),
                ("POST", "/api/join", _, _) if String::from_utf8_lossy(&body).contains("ABCDEFGHJKMN") => {
                    (201, r#"{"token":"tok-4c","person":{"id":3,"name":"Ana","role":"friend"},"device":{"id":7,"name":"laptop"},"server":{"name":"Bassi's server"}}"#.into())
                }
                (_, _, true, 2) | (_, _, false, _) => (401, r#"{"error":"This needs your device's key"}"#.into()),
                ("GET", "/api/me", true, _) => (200, me.into()),
                ("GET", "/api/library", true, _) => (200, library.into()),
                ("GET", "/api/party", true, _) => (200, r#"{"party":null}"#.into()),
                ("POST", p, true, _) if p.starts_with("/api/devices/") => (200, r#"{"ok":true}"#.into()),
                _ => (404, r#"{"error":"no such page"}"#.into()),
            };
            let reply = format!("HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{json}", json.len());
            let _ = conn.write_all(reply.as_bytes());
        }
    });
    address
}

/// Joining by an invite (stage 4c-1, as the Linux app's Servers…): the server is listed with what you can do there,
/// its page has its songs (searchable, who added each), the same invite again says it's joined already, and Leave
/// tells the server and forgets it.
#[test]
fn joining_by_invite_lists_the_server() {
    let _l = setup();
    let (c, _f, rx) = with_songs("servers join", vec![]);
    let address = rms_stand_in(Arc::new(std::sync::atomic::AtomicU8::new(0)));
    assert!(c.servers().rows.is_empty(), "{:?}", c.servers());
    c.server_join(&format!("{address}/join/ABCD-EFGH-JKMN"));
    assert!(until(5.0, || c.servers().rows.first().is_some_and(|r| r.songs == Some(2))), "{:?}", c.servers());
    let s = c.servers();
    assert_eq!(s.note, Some(("Joined Bassi's server. Its songs are under Library in the sidebar.".to_string(), false)));
    let row = &s.rows[0];
    assert_eq!((row.address.as_str(), row.name.as_str()), (address.as_str(), "Bassi's server"));
    assert_eq!(row.subtitle, format!("{address} · you're Ana"));
    assert_eq!(row.can, [true, true, true, false]);
    assert_eq!(row.problem, None);
    let page = c.server_page(&address, "");
    assert_eq!(page.line, "2 songs · You're on it as Ana");
    assert_eq!(page.songs.iter().map(|(t, _)| t.title.as_str()).collect::<Vec<_>>(), ["Para Sa Streets", "Dilaw"]);
    assert_eq!(page.songs[0].0.source, Source::Server);
    assert_eq!((page.songs[0].1.as_str(), page.songs[1].1.as_str()), ("Hev Abi · added by Bassi", "Maki"));
    assert_eq!(page.note, "Click one to play it. Like it (the heart) to save it, with its parts, lyrics and cover, into your own library.");
    assert_eq!(c.server_page(&address, "streets").songs.len(), 1);
    assert_eq!(c.server_page(&address, "streets").note, "1 of 2 songs match.");
    assert_eq!(c.server_page(&address, "nothing like it").note, "No songs on Bassi's server match “nothing like it”.");
    // its songs are known to the player (their key goes on as they play)
    assert!(radiance_engine::servers::of_track(&page.songs[0].0).is_some(), "the joined list isn't set");
    // the same invite again
    c.server_join(&format!("{address}/join/ABCD-EFGH-JKMN"));
    assert!(until(2.0, || c.servers().note.as_ref().is_some_and(|n| n.0.starts_with("This computer is on Bassi's server already"))), "{:?}", c.servers().note);
    // not an invite
    c.server_join("hello there");
    assert!(until(2.0, || c.servers().note.as_ref().is_some_and(|n| n.1)), "an error line: {:?}", c.servers().note);
    // Leave: told, forgotten
    c.server_leave(&address);
    assert!(until(5.0, || c.servers().rows.is_empty()), "{:?}", c.servers());
    assert!(toasts_for(&c, &rx, 1.0).iter().any(|t| t == "Left Bassi's server"));
    assert!(radiance_engine::servers::of_track(&page.songs[0].0).is_none(), "still known after leaving");
    c.quit();
}

/// A server that isn't answering, or that took this computer's key away, says why in plain words on its row and
/// its page (its songs as last sent stay); back, the problem goes (review focus 1 and 2).
#[test]
fn a_server_down_or_signed_out_says_why() {
    let _l = setup();
    let (c, _f, _rx) = with_songs("servers down", vec![]);
    let mode = Arc::new(std::sync::atomic::AtomicU8::new(0));
    let address = rms_stand_in(mode.clone());
    c.server_join(&format!("{address}/join/ABCD-EFGH-JKMN"));
    assert!(until(5.0, || c.servers().rows.first().is_some_and(|r| r.songs == Some(2))), "{:?}", c.servers());
    mode.store(1, Ordering::SeqCst);
    c.server_refresh(&address);
    assert!(until(15.0, || c.servers().rows[0].problem.as_deref().is_some_and(|p| p.contains("isn't answering"))), "{:?}", c.servers());
    let page = c.server_page(&address, "");
    assert_eq!(page.songs.len(), 2, "its songs as last sent stay");
    assert!(page.problem.as_deref().is_some_and(|p| p.starts_with("Bassi's server isn't answering")), "{:?}", page.problem);
    assert!(!page.signed_out);
    assert_eq!(page.note, "As Bassi's server last sent them.");
    mode.store(2, Ordering::SeqCst);
    c.server_refresh(&address);
    assert!(until(5.0, || c.servers().rows[0].signed_out), "{:?}", c.servers());
    assert!(c.servers().rows[0].problem.as_deref().is_some_and(|p| p.starts_with("This computer isn't joined to Bassi's server anymore")));
    mode.store(0, Ordering::SeqCst);
    c.server_refresh(&address);
    assert!(until(5.0, || c.servers().rows[0].problem.is_none() && !c.servers().rows[0].signed_out), "{:?}", c.servers());
    c.server_leave(&address);
    assert!(until(5.0, || c.servers().rows.is_empty()));
    c.quit();
}

/// The joined servers are known from the start (the Linux app's set_joined): a server song in the queue or History
/// plays with this computer's key after the app opens again, not "you left its server".
#[test]
fn a_server_song_is_known_after_a_restart() {
    let _l = setup();
    let address = rms_stand_in(Arc::new(std::sync::atomic::AtomicU8::new(0)));
    let (c, _f, _rx) = with_songs("servers restart", vec![]);
    c.server_join(&format!("{address}/join/ABCD-EFGH-JKMN"));
    assert!(until(5.0, || c.servers().rows.first().is_some_and(|r| r.songs == Some(2))), "{:?}", c.servers());
    let t = c.server_page(&address, "").songs[0].0.clone();
    c.quit();
    radiance_engine::servers::set_joined(&[]); // as a fresh start: nothing known yet
    let (c, _f, _rx) = with_songs("servers restart 2", vec![]);
    assert!(until(3.0, || radiance_engine::servers::of_track(&t).is_some()), "not known at the start");
    assert!(radiance_engine::servers::signed(&t.id).contains("t=tok-4c"), "{}", radiance_engine::servers::signed(&t.id));
    assert_eq!(c.servers().rows.len(), 1);
    c.server_leave(&address);
    assert!(until(5.0, || c.servers().rows.is_empty()));
    c.quit();
}

// ---------------------------------------------------------------- listening parties (stage 4c-2)

/// A Radiance Music Server's party, stood in: its clock (`ahead_ms` ahead of this computer's), the party as its API
/// and WebSocket send it, the controls asked for, a song file, and its WebSocket dropped on demand. This computer is
/// Ana (3), who may host; Bassi (1) hosts the party a test starts by itself.
#[derive(Default)]
struct PartySim {
    on: bool,
    host: (i64, String),
    everyone: bool,
    song: Option<serde_json::Value>,
    playing: bool,
    started: f64,
    paused_at: f64,
    queue: Vec<serde_json::Value>,
    members: Vec<(i64, String)>,
    reactions: Vec<serde_json::Value>,
    ahead_ms: f64,
    actions: Vec<String>,
    line_down: bool,
    next: u64,
    sockets: Vec<std::sync::mpsc::Sender<String>>,
}

const PARTY_LEAD_MS: f64 = 800.0;

impl PartySim {
    fn server_now(&self) -> f64 {
        radiance_engine::servers::now_ms() + self.ahead_ms
    }

    fn entry(&mut self, title: &str, by: (i64, &str)) -> serde_json::Value {
        self.next += 1;
        serde_json::json!({"entry": self.next, "id": 9, "artist": "Party Band", "title": title, "url": "/files/party.wav", "cover": null,
            "duration_ms": 120000.0, "added_by": {"id": by.0, "name": by.1}})
    }

    /// Bassi's party, its song `title` playing since `ago_ms` (server time), Bassi listening.
    fn bassis_party(&mut self, title: &str, ago_ms: f64) {
        self.on = true;
        self.host = (1, "Bassi".into());
        self.members = vec![(1, "Bassi".into())];
        let e = self.entry(title, (1, "Bassi"));
        self.song = Some(e);
        self.playing = true;
        self.started = self.server_now() - ago_ms;
    }

    fn state(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "state", "on": self.on, "host": {"id": self.host.0, "name": self.host.1}, "everyone_controls": self.everyone,
            "song": self.song, "playing": self.playing, "started_at_server_ms": self.started, "paused_at_ms": self.paused_at,
            "lead_ms": PARTY_LEAD_MS, "queue": self.queue,
            "members": self.members.iter().map(|(id, name)| serde_json::json!({"id": id, "name": name, "host": *id == self.host.0})).collect::<Vec<_>>(),
            "reactions": self.reactions,
        })
    }

    /// The party as it is now, to everyone on its WebSocket.
    fn tell(&mut self) {
        let text = self.state().to_string();
        self.sockets.retain(|s| s.send(text.clone()).is_ok());
    }

    fn control(&mut self, who: i64, body: &serde_json::Value) -> Result<(), String> {
        let action = body["action"].as_str().unwrap_or("").to_string();
        if !(self.on && (self.host.0 == who || self.everyone)) && action != "remove" {
            return Err("Only the host plays, pauses and skips in this party.".into());
        }
        self.actions.push(action.clone());
        let now = self.server_now();
        match action.as_str() {
            "play" if !self.playing => {
                self.playing = true;
                self.started = now + PARTY_LEAD_MS - self.paused_at;
            }
            "pause" if self.playing => {
                self.playing = false;
                self.paused_at = now - self.started;
            }
            "skip" => {
                self.song = if self.queue.is_empty() { None } else { Some(self.queue.remove(0)) };
                self.playing = self.song.is_some();
                self.started = now + PARTY_LEAD_MS;
                self.paused_at = 0.0;
            }
            "seek" => {
                let at = body["position_ms"].as_f64().unwrap_or(0.0);
                if self.playing {
                    self.started = now + PARTY_LEAD_MS - at;
                } else {
                    self.paused_at = at;
                }
            }
            "allow" => self.everyone = body["everyone"].as_bool().unwrap_or(false),
            "remove" => {
                let entry = body["entry"].as_u64();
                self.queue.retain(|e| e["entry"].as_u64() != entry);
            }
            "reorder" => {
                let entry = body["entry"].as_u64();
                if let Some(from) = self.queue.iter().position(|e| e["entry"].as_u64() == entry) {
                    let e = self.queue.remove(from);
                    let to = (body["to"].as_u64().unwrap_or(0) as usize).min(self.queue.len());
                    self.queue.insert(to, e);
                }
            }
            _ => {}
        }
        Ok(())
    }
}

/// A tone `secs` long, 8 kHz mono WAV (a party song the stand-in serves).
fn tone(secs: f64) -> Vec<u8> {
    let rate = 8_000u32;
    let n = (secs * rate as f64) as u32;
    let mut b = Vec::with_capacity(44 + 2 * n as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + 2 * n).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&(2 * n).to_le_bytes());
    for i in 0..n {
        let s = (0.05 * (i as f64 * 330.0 * std::f64::consts::TAU / rate as f64).sin() * 32767.0) as i16;
        b.extend_from_slice(&s.to_le_bytes());
    }
    b
}

/// The party stand-in on 127.0.0.1 (a thread per connection: the player keeps its song's open).
fn party_stand_in() -> (String, Arc<Mutex<PartySim>>) {
    use std::io::{BufRead, BufReader, Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    // a server of its own each time (the tests share settings.json: one id for all would be one server that moved)
    let id = Arc::new(format!("srv-party-{}", listener.local_addr().unwrap().port()));
    let sim = Arc::new(Mutex::new(PartySim::default()));
    let song = Arc::new(tone(120.0));
    let shared = sim.clone();
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(conn) = conn else { continue };
            let (sim, song, id) = (shared.clone(), song.clone(), id.clone());
            std::thread::spawn(move || {
                let mut peek = [0u8; 256];
                let n = conn.peek(&mut peek).unwrap_or(0);
                let first = String::from_utf8_lossy(&peek[..n]).lines().next().unwrap_or("").to_string();
                let keyed_ws = first.contains("t=tok-party");
                if first.starts_with("GET /api/party/ws") || first.starts_with("GET /api/events") {
                    let events = first.starts_with("GET /api/events");
                    if !keyed_ws || (!events && sim.lock().unwrap().line_down) {
                        let mut c = conn;
                        let _ = c.write_all(b"HTTP/1.1 503 X\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                        return;
                    }
                    let Ok(mut ws) = tungstenite::accept(conn) else { return };
                    let (tx, rx) = std::sync::mpsc::channel::<String>();
                    if events {
                        let _ = ws.send(tungstenite::Message::text(serde_json::json!({"type": "hello", "server": {"id": *id, "name": "Party server"}}).to_string()));
                    } else {
                        let mut s = sim.lock().unwrap();
                        let _ = ws.send(tungstenite::Message::text(s.state().to_string()));
                        s.sockets.push(tx);
                    }
                    loop {
                        if !events && sim.lock().unwrap().line_down {
                            return; // dropped, as a network that went
                        }
                        match rx.recv_timeout(Duration::from_millis(100)) {
                            Ok(text) => {
                                if ws.send(tungstenite::Message::text(text)).is_err() {
                                    return;
                                }
                            }
                            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                            Err(_) if events => std::thread::sleep(Duration::from_millis(100)),
                            Err(_) => return,
                        }
                    }
                }
                let mut conn = conn;
                let mut reader = BufReader::new(conn.try_clone().unwrap());
                let mut line = String::new();
                let _ = reader.read_line(&mut line);
                let (mut keyed, mut length, mut range) = (false, 0usize, None::<usize>);
                loop {
                    let mut h = String::new();
                    if reader.read_line(&mut h).is_err() || h.trim().is_empty() {
                        break;
                    }
                    let low = h.to_ascii_lowercase();
                    keyed |= low.trim() == "authorization: bearer tok-party";
                    if let Some(v) = low.strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap_or(0);
                    }
                    if let Some(v) = low.trim().strip_prefix("range: bytes=") {
                        range = v.split('-').next().and_then(|s| s.parse().ok());
                    }
                }
                let mut body = vec![0u8; length];
                let _ = reader.read_exact(&mut body);
                let body: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
                let mut parts = line.split_whitespace();
                let method = parts.next().unwrap_or("").to_string();
                let target = parts.next().unwrap_or("").to_string();
                keyed |= target.contains("t=tok-party");
                let path = target.split('?').next().unwrap_or("").to_string();
                if path == "/files/party.wav" && keyed {
                    let from = range.unwrap_or(0).min(song.len());
                    let head = if range.is_some() {
                        format!("HTTP/1.1 206 X\r\nContent-Type: audio/wav\r\nAccept-Ranges: bytes\r\nContent-Range: bytes {from}-{}/{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", song.len() - 1, song.len(), song.len() - from)
                    } else {
                        format!("HTTP/1.1 200 X\r\nContent-Type: audio/wav\r\nAccept-Ranges: bytes\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", song.len())
                    };
                    let _ = conn.write_all(head.as_bytes());
                    let _ = conn.write_all(&song[from..]);
                    return;
                }
                let me = serde_json::json!({"person": {"id": 3, "name": "Ana", "role": "friend"}, "device": {"id": 7, "name": "laptop"},
                    "server": {"name": "Party server", "id": *id}, "can": {"add": true, "split": false, "host": true}}).to_string();
                let library = r#"{"server":"Party server","count":1,"songs":[
                    {"id":9,"artist":"Party Band","title":"Party Song","file":"party.wav","url":"/files/party.wav","duration_ms":120000,"split":false,"parts":[],"files":{},"added_by":{"id":1,"name":"Bassi"}}]}"#;
                let mut s = sim.lock().unwrap();
                let (status, json): (u16, String) = match (method.as_str(), path.as_str(), keyed) {
                    ("GET", "/api/status", _) => (200, serde_json::json!({"server": "Party server", "id": *id, "needs_setup": false}).to_string()),
                    ("POST", "/api/join", _) => (201, r#"{"token":"tok-party","person":{"id":3,"name":"Ana","role":"friend"},"device":{"id":7,"name":"laptop"},"server":{"name":"Party server"}}"#.into()),
                    // (no key needed for the time: rms/src/party.rs time)
                    ("GET", "/api/time", _) => (200, serde_json::json!({ "now_ms": s.server_now() }).to_string()),
                    (_, _, false) => (401, r#"{"error":"This needs your device's key"}"#.into()),
                    ("GET", "/api/me", _) => (200, me),
                    ("GET", "/api/library", _) => (200, library.into()),
                    ("GET", "/api/party", _) => (200, s.state().to_string()),
                    ("POST", "/api/party/start", _) => {
                        s.on = true;
                        s.host = (3, "Ana".into());
                        s.everyone = body["everyone_controls"].as_bool().unwrap_or(false);
                        s.members = vec![(3, "Ana".into())];
                        s.tell();
                        (200, s.state().to_string())
                    }
                    ("POST", "/api/party/add", _) => {
                        let e = s.entry(body["song"]["title"].as_str().unwrap_or("Party Song"), (3, "Ana"));
                        if s.song.is_none() {
                            s.song = Some(e);
                            s.playing = true;
                            s.started = s.server_now() + PARTY_LEAD_MS;
                        } else {
                            s.queue.push(e);
                        }
                        s.tell();
                        (200, s.state().to_string())
                    }
                    ("POST", "/api/party/control", _) => match s.control(3, &body) {
                        Ok(()) => {
                            s.tell();
                            (200, s.state().to_string())
                        }
                        Err(e) => (403, serde_json::json!({ "error": e }).to_string()),
                    },
                    ("POST", "/api/party/react", _) => {
                        let at = s.server_now();
                        s.reactions.push(serde_json::json!({"emoji": body["emoji"], "by": {"name": "Ana"}, "at_server_ms": at}));
                        s.tell();
                        (200, r#"{"ok":true}"#.into())
                    }
                    ("POST", "/api/party/end", _) => {
                        *s = PartySim { ahead_ms: s.ahead_ms, sockets: std::mem::take(&mut s.sockets), next: s.next, ..Default::default() };
                        s.tell();
                        (200, r#"{"ok":true}"#.into())
                    }
                    ("POST", p, _) if p.starts_with("/api/devices/") => (200, r#"{"ok":true}"#.into()),
                    _ => (404, r#"{"error":"no such page"}"#.into()),
                };
                drop(s);
                let reply = format!("HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{json}", json.len());
                let _ = conn.write_all(reply.as_bytes());
            });
        }
    });
    (address, sim)
}

/// The party server joined (its songs fetched).
fn on_party_server(c: &Controller, address: &str) {
    c.server_join(&format!("{address}/join/ABCD-EFGH-JKMN"));
    assert!(until(5.0, || c.servers().rows.iter().any(|r| r.address == address && r.songs == Some(1))), "{:?}", c.servers());
}

/// The party server left at a test's end (the tests share settings.json: the next test starts with no server).
fn off_party_server(c: &Controller, address: &str) {
    c.server_leave(address);
    assert!(until(5.0, || !c.servers().rows.iter().any(|r| r.address == address)), "{:?}", c.servers());
}

/// Where the party's song is now, by the stand-in's clock (s).
fn party_at(sim: &Arc<Mutex<PartySim>>) -> f64 {
    let s = sim.lock().unwrap();
    if s.playing {
        (s.server_now() - s.started) / 1000.0
    } else {
        s.paused_at / 1000.0
    }
}

/// Every toast said within `secs` until one has `words` in it (all of them, for the message).
fn toast_with(c: &Controller, words: &str, secs: f64) -> Result<String, Vec<String>> {
    let mut seen = vec![];
    let end = Instant::now() + Duration::from_secs_f64(secs);
    while Instant::now() < end {
        while let Some(t) = c.toast() {
            if t.contains(words) {
                return Ok(t);
            }
            seen.push(t);
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    Err(seen)
}

/// Joining a party (stage 4c-2, the Linux app's join_party and party_sync): its song plays here where everyone is, the
/// in-party view says whose party it is, and the bar's Previous and (not the host's) Next are off.
#[test]
fn joining_a_party_plays_its_song_at_its_place() {
    let _l = setup();
    let (address, sim) = party_stand_in();
    sim.lock().unwrap().bassis_party("Party Song", 20_000.0);
    let (c, _f, _rx) = with_songs("party join", vec![]);
    on_party_server(&c, &address);
    assert!(until(5.0, || c.party_card(&address).join == "Join the Party"), "{:?}", c.party_card(&address));
    let card = c.party_card(&address);
    assert_eq!(card.heading, "Bassi is hosting a listening party");
    assert_eq!(card.line, "Playing “Party Song” by Party Band. 1 person listening. Join, and you hear it here in step with everyone.");
    c.party_join(&address);
    assert!(until(10.0, || c.now().song.is_some_and(|t| t.title == "Party Song") && c.now().playing), "{:?}", c.now());
    assert!(until(8.0, || (c.now().at - party_at(&sim)).abs() < 0.3), "here {:.2} s, the party {:.2} s", c.now().at, party_at(&sim));
    let p = c.party();
    assert_eq!(p.server, address);
    assert_eq!(p.heading, "You're in Bassi's party");
    assert_eq!(p.pill, "Playing");
    assert!(!p.control && !p.host && !p.can_end && p.hearing);
    assert_eq!(p.controls_note, "Bassi (the host) plays, pauses and skips.");
    assert!(until(5.0, || c.party().sync_line.starts_with("In time with the party")), "{}", c.party().sync_line);
    assert!(c.party().sync_line.contains(" · clock ±"), "{}", c.party().sync_line);
    assert!(c.party_card(&address).in_it);
    let now = c.now();
    assert!(!now.can_previous && !now.can_next, "Previous never, Next only for who controls it");
    c.party_leave();
    off_party_server(&c, &address);
    c.quit();
}

/// The party's place follows the server's clock, not this computer's (the clock offset measured as you join).
#[test]
fn the_party_follows_the_clock_offset() {
    let _l = setup();
    let (address, sim) = party_stand_in();
    {
        let mut s = sim.lock().unwrap();
        s.ahead_ms = 7_000.0; // the server's clock is 7 s ahead of this one
        s.bassis_party("Party Song", 15_000.0);
    }
    let (c, _f, _rx) = with_songs("party clock", vec![]);
    on_party_server(&c, &address);
    c.party_join(&address);
    assert!(until(10.0, || c.now().playing), "{:?}", c.now());
    assert!(until(8.0, || (c.now().at - party_at(&sim)).abs() < 0.3), "here {:.2} s, the party {:.2} s", c.now().at, party_at(&sim));
    c.party_leave();
    off_party_server(&c, &address);
    c.quit();
}

/// Leaving the party brings back your own music where it was (the song, its place, playing as it was).
#[test]
fn leaving_brings_back_your_own_music() {
    let _l = setup();
    let (address, sim) = party_stand_in();
    sim.lock().unwrap().bassis_party("Party Song", 5_000.0);
    let mine = vec![wav("party-mine-a", 30.0), wav("party-mine-b", 30.0)];
    let (c, _f, rx, list) = with_list("party leave", mine.clone());
    c.play_from(list, 1, &mine[1].key());
    assert!(until(5.0, || c.now().playing && c.now().at > 1.0), "{:?}", c.now());
    on_party_server(&c, &address);
    c.party_join(&address);
    assert!(until(10.0, || c.now().song.is_some_and(|t| t.title == "Party Song")), "{:?}", c.now());
    let was = c.queue();
    assert_eq!(was.0.len(), 1, "the party's song alone while in it");
    c.party_leave();
    assert_eq!(toast_with(&c, "back to your own music", 3.0).map(|_| ()), Ok(()));
    assert!(until(5.0, || c.now().song.is_some_and(|t| t.key() == mine[1].key()) && c.now().playing), "{:?}", c.now());
    assert!(c.now().at >= 1.0, "from where it was: {:.2}", c.now().at);
    assert_eq!(c.queue().0.len(), 2, "your own queue back");
    assert!(c.party().server.is_empty());
    let _ = rx;
    off_party_server(&c, &address);
    c.quit();
}

/// Only the host plays, pauses and skips (the Linux app's party_play_pause / party_skip): for the others Play/Pause
/// is hearing it here or not and Next says the host skips; once the host lets everyone, it's for everyone.
#[test]
fn only_the_host_controls_unless_everyone_may() {
    let _l = setup();
    let (address, sim) = party_stand_in();
    sim.lock().unwrap().bassis_party("Party Song", 5_000.0);
    let (c, _f, _rx) = with_songs("party control", vec![]);
    on_party_server(&c, &address);
    c.party_join(&address);
    assert!(until(10.0, || c.now().playing), "{:?}", c.now());
    c.toggle_play();
    assert!(until(3.0, || !c.party().hearing && !c.now().playing), "{:?}", c.party());
    assert_eq!(c.party().sync_line.split(" · ").next(), Some("Not heard here: the party plays on. Hear It Here to be back in step."));
    assert!(sim.lock().unwrap().playing, "the party plays on");
    c.toggle_play();
    assert!(until(5.0, || c.party().hearing && c.now().playing), "{:?}", c.party());
    c.skip(1);
    assert_eq!(toast_with(&c, "skips songs", 3.0), Ok("Bassi (the host) skips songs in this party".to_string()));
    assert!(sim.lock().unwrap().actions.is_empty(), "nothing asked of the server");
    // the host lets everyone
    {
        let mut s = sim.lock().unwrap();
        s.everyone = true;
        s.tell();
    }
    assert!(until(3.0, || c.party().control), "{:?}", c.party());
    assert!(c.now().can_next);
    c.toggle_play();
    assert!(until(3.0, || !sim.lock().unwrap().playing), "a pause for everyone");
    assert!(until(3.0, || c.party().pill == "Paused" && !c.now().playing), "{:?}", c.party());
    assert_eq!(sim.lock().unwrap().actions, ["pause"]);
    c.party_leave();
    off_party_server(&c, &address);
    c.quit();
}

/// The party's line drops: the view says so ("Lost the party: joining again…") and it's back by itself.
#[test]
fn a_lost_party_joins_again() {
    let _l = setup();
    let (address, sim) = party_stand_in();
    sim.lock().unwrap().bassis_party("Party Song", 5_000.0);
    let (c, _f, _rx) = with_songs("party lost", vec![]);
    on_party_server(&c, &address);
    c.party_join(&address);
    assert!(until(10.0, || c.now().playing), "{:?}", c.now());
    sim.lock().unwrap().line_down = true;
    assert!(until(5.0, || c.party().lost), "{:?}", c.party());
    assert_eq!(c.party().sync_line, "Lost the party: joining again…");
    assert!(!c.party().server.is_empty(), "still in it");
    sim.lock().unwrap().line_down = false;
    assert!(until(10.0, || !c.party().lost), "{:?}", c.party());
    c.party_leave();
    off_party_server(&c, &address);
    c.quit();
}

/// A song of your own picked in a party leaves it (the Linux app's play_current), with a toast.
#[test]
fn picking_your_own_song_leaves_the_party() {
    let _l = setup();
    let (address, sim) = party_stand_in();
    sim.lock().unwrap().bassis_party("Party Song", 5_000.0);
    let mine = vec![wav("party-own-a", 30.0)];
    let (c, _f, _rx, list) = with_list("party own", mine.clone());
    on_party_server(&c, &address);
    c.party_join(&address);
    assert!(until(10.0, || c.now().song.is_some_and(|t| t.title == "Party Song")), "{:?}", c.now());
    c.play_from(list, 0, &mine[0].key());
    assert_eq!(toast_with(&c, "own music", 3.0), Ok("You left the party to play your own music".to_string()));
    assert!(until(5.0, || c.now().song.is_some_and(|t| t.key() == mine[0].key())), "{:?}", c.now());
    assert!(c.party().server.is_empty());
    off_party_server(&c, &address);
    c.quit();
}

/// Hosting: Start a Party puts you in it as its host (the controls, End), a song clicked on the server's page goes
/// into the party, and End ends it for everyone (your own music back).
#[test]
fn hosting_a_party_from_start_to_end() {
    let _l = setup();
    let (address, sim) = party_stand_in();
    let (c, _f, _rx) = with_songs("party host", vec![]);
    on_party_server(&c, &address);
    assert!(until(5.0, || c.party_card(&address).can_start), "{:?}", c.party_card(&address));
    assert_eq!(c.party_card(&address).line, "Nobody's having a party right now. In one, everyone hears the same song at the same moment and anyone adds songs.");
    c.party_start(&address, false);
    assert_eq!(toast_with(&c, "party is on", 5.0), Ok("The party is on. Add a song: it starts at once.".to_string()));
    assert!(until(5.0, || c.party().host), "{:?}", c.party());
    let p = c.party();
    assert_eq!(p.heading, "You're hosting the party");
    assert!(p.control && p.can_end);
    assert_eq!(p.pill, "Quiet");
    assert_eq!(p.members, ["Ana · host"]);
    c.play_from(ListRef::Server(address.clone(), String::new()), 0, &c.server_page(&address, "").songs[0].0.key());
    assert_eq!(toast_with(&c, "to the party", 3.0), Ok("Added “Party Song” to the party".to_string()));
    assert!(until(10.0, || c.now().playing && c.party().now.is_some()), "{:?}", c.party());
    c.party_react("🔥");
    assert!(until(3.0, || c.party().reactions.first().is_some_and(|r| r.tip == "Ana: Fire")), "{:?}", c.party().reactions);
    c.party_end();
    assert!(until(5.0, || c.party().server.is_empty()), "{:?}", c.party());
    assert!(!sim.lock().unwrap().on);
    off_party_server(&c, &address);
    c.quit();
}
