//! The engine as one controller, for an app that isn't GTK (the Windows app, issue #45):
//! commands in (search, play this list, pause, …), a state to read and change events out.
//!
//! It runs on the engine's thread, which owns GLib's default main context for the whole process
//! (GStreamer's bus and the player's timers need one; glib's local timers always go to the
//! default context): commands are posted to it, background work comes back to it, and what an
//! app reads is a snapshot behind a lock. One controller at a time per process; each one is an
//! engine of its own on that thread (a new one never takes an old one's late answers).

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use radiance_core::queue::{self, Queue, Repeat, Step};
use radiance_core::search;
use radiance_core::trouble::{self, After};
use radiance_core::words::{self, Kind};

use crate::lyrics::{Line, Lyrics};
use crate::player::{Event, Player};
use crate::playing::{self, Streams};
use crate::relay::Relay;
use crate::settings::{self, Settings};
use crate::sources::Sources;
use crate::track::{Source, Track};

/// What changed (read it again with the getter of the same name).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Change {
    Now,
    Position,
    Queue,
    Results,
    Home,
    Lyrics,
    Online,
    Problem,
}

/// Told when something changed: on the engine's thread, so it must hand it on quickly.
pub trait Listener: Send + Sync + 'static {
    fn changed(&self, what: Change);
}

/// A list on screen that a song is played from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListRef {
    /// Search: your liked songs that match.
    Mine,
    /// Search: the free lossless libraries.
    Lossless,
    /// Search: YouTube Music.
    Songs,
    /// Home: Made for you.
    Mix,
    /// Home: Recently played.
    Recent,
    /// Home: Your liked songs.
    Liked,
    /// The queue.
    Queue,
}

/// Search's lists for `query` (None: still being searched).
#[derive(Clone, Debug, Default)]
pub struct Results {
    pub query: String,
    pub mine: Vec<Track>,
    pub lossless: Option<Vec<Track>>,
    pub songs: Option<Vec<Track>>,
}

/// The song playing (or picked: paused), where it is, and the player bar's state.
#[derive(Clone, Debug, Default)]
pub struct NowState {
    pub song: Option<Track>,
    pub playing: bool,
    /// Seconds into the song.
    pub at: f64,
    /// Its length in seconds (0: not known yet).
    pub length: f64,
    pub can_previous: bool,
    pub can_next: bool,
    pub shuffle: bool,
    pub repeat: Repeat,
    /// 0 to 1.
    pub volume: f64,
    pub lossless: bool,
    /// A stream's bit rate (0: a file, or not known).
    pub kbps: u32,
    /// The lyric lines lit now (core's sing::lit_lines).
    pub lit: Vec<usize>,
}

/// The song's lyrics.
#[derive(Clone, Debug, Default)]
pub enum LyricsState {
    /// Being looked for.
    #[default]
    Looking,
    None,
    Plain(String),
    Lines(Vec<Line>),
}

/// Home: the greeting, the mix's note, and its rows.
#[derive(Clone, Debug, Default)]
pub struct HomeState {
    pub greeting: String,
    pub mix_note: String,
    pub mix: Vec<Track>,
    pub recent: Vec<Track>,
    pub liked: Vec<Track>,
}

static RUNNING: AtomicBool = AtomicBool::new(false);

/// What an app reads (written on the engine's thread).
struct View {
    results: Results,
    home: HomeState,
    online: bool,
    problem: Option<String>,
    now: NowState,
    queue: (Vec<Track>, usize),
    lyrics: LyricsState,
}

/// Between the app's threads and the engine's.
pub(crate) struct Shared {
    listener: Arc<dyn Listener>,
    pub(crate) sources: Arc<dyn Sources>,
    quitting: AtomicBool,
    view: Mutex<View>,
}

impl Shared {
    /// Tells the app (never once it's quitting).
    fn tell(&self, what: Change) {
        if !self.quitting.load(Ordering::SeqCst) {
            self.listener.changed(what);
        }
    }

    fn view<T>(&self, f: impl FnOnce(&mut View) -> T) -> T {
        f(&mut self.view.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

/// Something for the engine's thread to do.
type Job = Box<dyn FnOnce() + Send>;

/// The engine's thread: it owns GLib's default main context and runs it until the process ends.
fn host() -> &'static async_channel::Sender<Job> {
    static HOST: OnceLock<async_channel::Sender<Job>> = OnceLock::new();
    HOST.get_or_init(|| {
        let (jobs, inbox) = async_channel::unbounded::<Job>();
        let _ = std::thread::Builder::new().name("radiance-engine".into()).spawn(move || {
            let ctx = glib::MainContext::default();
            let Ok(_owner) = ctx.acquire() else {
                eprintln!("Radiance Music's engine: GLib's default main context is taken by another thread");
                return;
            };
            ctx.spawn_local(async move {
                while let Ok(job) = inbox.recv().await {
                    job();
                }
            });
            glib::MainLoop::new(Some(&ctx), false).run();
        });
        jobs
    })
}

/// Each engine started gets a number: answers meant for an earlier one are dropped.
static EPOCH: AtomicU64 = AtomicU64::new(0);

/// The engine running on its thread. Every method of it runs there. (The controller holds no
/// GLib object: the engine's timers and player stay on the engine's thread.)
pub struct Controller {
    shared: Arc<Shared>,
    epoch: u64,
    running: AtomicBool,
}

thread_local! {
    static ENGINE: RefCell<Option<Rc<Engine>>> = const { RefCell::new(None) };
}

/// Runs `f` with the engine numbered `epoch` (on its thread; nothing once it's gone).
fn with_engine(epoch: u64, f: impl FnOnce(&Rc<Engine>)) {
    if let Some(e) = ENGINE.with(|x| x.borrow().clone()).filter(|e| e.epoch == epoch) {
        f(&e);
    }
}

impl Controller {
    /// Starts the engine: its thread (the first time), Home's lists, last time's song (paused).
    /// Err when one runs already.
    pub fn start(listener: Arc<dyn Listener>, sources: Arc<dyn Sources>) -> Result<Controller, String> {
        if RUNNING.swap(true, Ordering::SeqCst) {
            return Err("Radiance Music's engine is already running".into());
        }
        let shared = Arc::new(Shared {
            listener,
            sources,
            quitting: AtomicBool::new(false),
            view: Mutex::new(View {
                results: Results::default(),
                home: HomeState::default(),
                online: true,
                problem: None,
                now: NowState::default(),
                queue: (Vec::new(), 0),
                lyrics: LyricsState::None,
            }),
        });
        let epoch = EPOCH.fetch_add(1, Ordering::SeqCst) + 1;
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<()>();
        let s = shared.clone();
        let sent = host().try_send(Box::new(move || {
            let engine = Rc::new(Engine::new(s, epoch));
            ENGINE.with(|e| *e.borrow_mut() = Some(engine.clone()));
            engine.start_up();
            let _ = ready_tx.send(());
        }));
        if sent.is_err() || ready_rx.recv_timeout(std::time::Duration::from_secs(30)).is_err() {
            RUNNING.store(false, Ordering::SeqCst);
            return Err("Radiance Music's engine didn't start".into());
        }
        Ok(Controller { shared, epoch, running: AtomicBool::new(true) })
    }

    /// Posts `f` to the engine (dropped once it's quitting).
    fn post(&self, f: impl FnOnce(&Rc<Engine>) + Send + 'static) {
        if !self.shared.quitting.load(Ordering::SeqCst) {
            let epoch = self.epoch;
            let _ = host().try_send(Box::new(move || with_engine(epoch, f)));
        }
    }

    /// Searches (the free lossless libraries and YouTube Music); fewer than 2 letters clear it.
    pub fn search(&self, q: &str) {
        let q = q.to_string();
        self.post(move |e| e.search(q));
    }

    /// Home's mix made again.
    pub fn refresh_mix(&self) {
        self.post(|e| e.refresh_mix());
    }

    pub fn results(&self) -> Results {
        self.shared.view(|v| v.results.clone())
    }

    pub fn home(&self) -> HomeState {
        self.shared.view(|v| v.home.clone())
    }

    /// The internet seemed reachable the last time it was asked.
    pub fn online(&self) -> bool {
        self.shared.view(|v| v.online)
    }

    /// What went wrong last (a song that won't play), in plain words.
    pub fn problem(&self) -> Option<String> {
        self.shared.view(|v| v.problem.clone())
    }

    /// Plays `list` (as shown) from song `i`; the queue: from its song `i`.
    pub fn play_from(&self, list: ListRef, i: usize) {
        self.post(move |e| e.play_from(list, i));
    }

    /// "Shuffle": `list` in a random order (the songs you play most tend to come first).
    pub fn shuffle_from(&self, list: ListRef) {
        self.post(move |e| e.shuffle_from(list));
    }

    pub fn toggle_play(&self) {
        self.post(|e| e.toggle_play());
    }

    /// Next (1) or back (-1; more than 3 s in: the start of this song).
    pub fn skip(&self, by: i32) {
        self.post(move |e| e.skip(by));
    }

    /// To `at` seconds into the song.
    pub fn seek(&self, at: f64) {
        self.post(move |e| e.seek(at));
    }

    /// 0 to 1 (kept in the settings).
    pub fn set_volume(&self, level: f64) {
        self.post(move |e| e.set_volume(level));
    }

    pub fn set_shuffle(&self, on: bool) {
        self.post(move |e| e.set_shuffle(on));
    }

    /// Repeat: off → the whole queue → this song → off.
    pub fn cycle_repeat(&self) {
        self.post(|e| e.cycle_repeat());
    }

    pub fn now(&self) -> NowState {
        self.shared.view(|v| v.now.clone())
    }

    /// The queue's songs and the index of the one playing.
    pub fn queue(&self) -> (Vec<Track>, usize) {
        self.shared.view(|v| v.queue.clone())
    }

    pub fn lyrics(&self) -> LyricsState {
        self.shared.view(|v| v.lyrics.clone())
    }

    /// A cover as a PNG file of `size` pixels (kept in the cache: once made, at once).
    /// Blocks while it's fetched: call it off the UI thread. None: no such picture.
    pub fn art_file(&self, source: &str, size: u32) -> Option<PathBuf> {
        if source.is_empty() {
            return None;
        }
        let path = art_dir()?.join(format!("{}-{size}.png", crate::track::digest(source)));
        if path.exists() {
            return Some(path);
        }
        let px = crate::art::fetch(source, size)?;
        save_png(px, &path)
    }

    /// Now Playing's moving background for a cover: its wash (the art blurred, filling the
    /// page) and its blob (the soft patch that wanders and turns over it), as PNG files.
    pub fn backdrop_files(&self, source: &str) -> Option<(PathBuf, PathBuf)> {
        if source.is_empty() {
            return None;
        }
        let dir = art_dir()?;
        let key = crate::track::digest(source);
        let (wash, blob) = (dir.join(format!("{key}-wash.png")), dir.join(format!("{key}-blob.png")));
        if wash.exists() && blob.exists() {
            return Some((wash, blob));
        }
        let px = crate::art::backdrop(source)?;
        let patch = crate::art::blob(&px);
        Some((save_png(px, &wash)?, save_png(patch, &blob)?))
    }

    /// Stops the engine: where you are is saved, the player stops, no more changes are told.
    /// Safe to call twice.
    pub fn quit(&self) {
        if !self.running.swap(false, Ordering::SeqCst) {
            return;
        }
        self.shared.quitting.store(true, Ordering::SeqCst);
        let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
        let epoch = self.epoch;
        let sent = host().try_send(Box::new(move || {
            with_engine(epoch, |e| e.shut_down());
            if ENGINE.with(|x| x.borrow().as_ref().is_some_and(|e| e.epoch == epoch)) {
                ENGINE.with(|x| x.borrow_mut().take());
            }
            let _ = done_tx.send(());
        }));
        if sent.is_ok() {
            let _ = done_rx.recv_timeout(std::time::Duration::from_secs(30));
        }
        RUNNING.store(false, Ordering::SeqCst);
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        self.quit();
    }
}

/// The engine's own state, on its thread.
struct Engine {
    shared: Arc<Shared>,
    /// Which engine this is (late answers for an earlier one are dropped).
    epoch: u64,
    search_gen: Cell<u64>,
    query: RefCell<String>,
    /// What the search found, as found: (lossless, YouTube Music).
    found: RefCell<(Option<Vec<Track>>, Option<Vec<Track>>)>,
    liked: RefCell<Vec<Track>>,
    history: RefCell<Vec<(Track, i64)>>,
    mix: RefCell<Vec<Track>>,
    mix_gen: Cell<u64>,

    // playing
    player: RefCell<Option<Player>>,
    relay: Option<Relay>,
    streams: RefCell<Streams>,
    queue: RefCell<Queue>,
    /// The song playing (its saved copy, when there is one).
    current: RefCell<Option<Track>>,
    play_gen: Cell<u64>,
    kbps: Cell<u32>,
    /// Last time's song, picked up paused: Play starts it from `restore_at`.
    restored: Cell<bool>,
    restore_at: Cell<Option<f64>>,
    /// Where the song is (seconds), as last read.
    position: Cell<f64>,
    /// Songs in a row that wouldn't play.
    fails: Cell<u32>,
    sound_tries: Cell<u32>,
    /// The list ran out while Autoplay's songs were on their way.
    ran_out: Cell<bool>,
    autoplay_for: RefCell<Option<String>>,
    /// Each song's (plays, skips).
    counts: RefCell<HashMap<String, (usize, usize)>>,
    settings: RefCell<Settings>,
    /// The song's timed lines (the lit ones go in NowState).
    lines: RefCell<Vec<Line>>,
    last_save: Cell<Option<Instant>>,
}

impl Engine {
    fn new(shared: Arc<Shared>, epoch: u64) -> Engine {
        Engine {
            shared,
            epoch,
            search_gen: Cell::new(0),
            query: RefCell::default(),
            found: RefCell::new((Some(Vec::new()), Some(Vec::new()))),
            liked: RefCell::default(),
            history: RefCell::default(),
            mix: RefCell::default(),
            mix_gen: Cell::new(0),
            player: RefCell::default(),
            relay: Relay::start().ok(),
            streams: RefCell::default(),
            queue: RefCell::default(),
            current: RefCell::default(),
            play_gen: Cell::new(0),
            kbps: Cell::new(0),
            restored: Cell::new(false),
            restore_at: Cell::new(None),
            position: Cell::new(0.0),
            fails: Cell::new(0),
            sound_tries: Cell::new(0),
            ran_out: Cell::new(false),
            autoplay_for: RefCell::default(),
            counts: RefCell::default(),
            settings: RefCell::new(settings::load()),
            lines: RefCell::default(),
            last_save: Cell::new(None),
        }
    }

    /// Work off the engine's thread; `done` back on it with what it made.
    fn bg<T: Send + 'static>(&self, work: impl FnOnce() -> T + Send + 'static, done: impl FnOnce(&Rc<Engine>, T) + Send + 'static) {
        let epoch = self.epoch;
        std::thread::spawn(move || {
            let out = work();
            let _ = host().try_send(Box::new(move || with_engine(epoch, move |e| done(e, out))));
        });
    }

    fn start_up(self: &Rc<Self>) {
        *self.liked.borrow_mut() = crate::db::liked();
        *self.counts.borrow_mut() = crate::db::counts();
        let me = Rc::downgrade(self);
        match Player::new(move |ev| {
            if let Some(e) = me.upgrade() {
                e.on_player(ev);
            }
        }) {
            Ok(p) => {
                p.set_volume(self.settings.borrow().volume);
                *self.player.borrow_mut() = Some(p);
            }
            Err(err) => {
                crate::log::note(&format!("the player didn't start: {err}"));
                self.shared.view(|v| v.problem = Some(format!("The player didn't start: {err}")));
            }
        }
        self.restore_session();
        self.publish_home();
        self.refresh_mix();
        // five times a second: where the song is, the lines lit, and (every 5 s) where you are
        let me = Rc::downgrade(self);
        glib::timeout_add_local(Duration::from_millis(200), move || {
            let Some(e) = me.upgrade() else { return glib::ControlFlow::Break };
            e.tick();
            glib::ControlFlow::Continue
        });
    }

    fn shut_down(&self) {
        self.save_session();
        if let Some(p) = self.player.borrow().as_ref() {
            p.stop();
        }
        self.player.borrow_mut().take();
    }

    // ------------------------------------------------------------------ playing

    /// Last time's queue and song, paused where it was (Play carries on from there).
    fn restore_session(self: &Rc<Self>) {
        let s = crate::session::load();
        *self.history.borrow_mut() = s.history;
        let Some(t) = s.queue.get(s.index).cloned() else { return };
        self.queue.borrow_mut().set_tracks(s.queue, s.index);
        let t = playing::saved_copy(&self.liked.borrow(), &t);
        *self.current.borrow_mut() = Some(t.clone());
        self.restored.set(true);
        self.restore_at.set((s.position > 1.0).then_some(s.position));
        self.position.set(s.position);
        self.load_lyrics(&t, self.play_gen.get());
        self.publish_queue();
        self.publish_now();
    }

    fn save_session(&self) {
        let position = self.player.borrow().as_ref().and_then(|p| p.position()).or(self.restore_at.get()).unwrap_or(self.position.get());
        let (queue, index) = {
            let q = self.queue.borrow();
            (q.tracks().to_vec(), q.index())
        };
        if queue.is_empty() {
            return;
        }
        crate::session::save(&crate::session::Session { queue, index, position, history: self.history.borrow().clone() });
        self.last_save.set(Some(Instant::now()));
    }

    /// The songs of a list on screen.
    fn list(&self, list: ListRef) -> Vec<Track> {
        self.shared.view(|v| match list {
            ListRef::Mine => v.results.mine.clone(),
            ListRef::Lossless => v.results.lossless.clone().unwrap_or_default(),
            ListRef::Songs => v.results.songs.clone().unwrap_or_default(),
            ListRef::Mix => v.home.mix.clone(),
            ListRef::Recent => v.home.recent.clone(),
            ListRef::Liked => v.home.liked.clone(),
            ListRef::Queue => v.queue.0.clone(),
        })
    }

    fn play_from(self: &Rc<Self>, list: ListRef, i: usize) {
        if list == ListRef::Queue {
            if i < self.queue.borrow().len() {
                self.queue.borrow_mut().set_index(i);
                self.play_current();
            }
            return;
        }
        let tracks = self.list(list);
        if i < tracks.len() {
            self.play_list(tracks, i);
        }
    }

    fn shuffle_from(self: &Rc<Self>, list: ListRef) {
        let tracks = self.list(list);
        if !tracks.is_empty() {
            let order = self.shuffled(tracks);
            self.play_list(order, 0);
        }
    }

    /// A random order that still listens to you (core's queue::shuffled, your plays and skips).
    fn shuffled(&self, v: Vec<Track>) -> Vec<Track> {
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(7);
        let counts = self.counts.borrow();
        queue::shuffled(
            v,
            |t| {
                let (p, s) = counts.get(&t.key()).copied().unwrap_or_default();
                queue::liking(p as u32, s as u32)
            },
            seed,
        )
    }

    fn often_skipped(&self, t: &Track) -> bool {
        let (p, s) = self.counts.borrow().get(&t.key()).copied().unwrap_or_default();
        queue::often_skipped(p as u32, s as u32)
    }

    fn play_list(self: &Rc<Self>, tracks: Vec<Track>, i: usize) {
        self.note_skip_if_early(); // another song picked mid-song
        self.restored.set(false);
        self.restore_at.set(None);
        // (with Shuffle on: the song picked first, the rest in a random order)
        self.queue.borrow_mut().play_list(tracks, i, |rest| self.shuffled(rest));
        self.play_current();
    }

    /// The queue's song at its index, from its saved copy, a stream, or the Archive. (Stage 1
    /// of the Windows app: no parts, voices or videos yet; a video plays its sound.)
    fn play_current(self: &Rc<Self>) {
        let Some(t) = self.queue.borrow().at_index().cloned() else { return };
        let t = playing::saved_copy(&self.liked.borrow(), &t);
        let gen = self.play_gen.get() + 1;
        self.play_gen.set(gen);
        self.kbps.set(0);
        self.position.set(self.restore_at.get().unwrap_or(0.0));
        *self.current.borrow_mut() = Some(t.clone());
        self.load_lyrics(&t, gen);
        self.note_played(&t);
        let lookup = t.source == Source::YouTube && t.file.is_none() && self.streams.borrow().fresh(&t.id).is_none();
        if lookup {
            if let Some(p) = self.player.borrow().as_ref() {
                p.stop(); // not the last song under this one's name while it's found
            }
        }
        if let Some(f) = &t.file {
            use gio::prelude::*;
            self.start(&gio::File::for_path(f).uri(), gen);
        } else {
            match t.source {
                Source::Archive => {
                    let uri = playing::web_uri(self.relay.as_ref(), &t.id);
                    self.start(&uri, gen);
                }
                Source::Server => match playing::plain_uri(&t, self.relay.as_ref(), &self.streams.borrow()) {
                    Some(uri) => self.start(&uri, gen),
                    None => self.unavailable(&t.title, "you left its server"),
                },
                Source::YouTube => match self.streams.borrow().fresh(&t.id) {
                    Some((uri, kbps)) => {
                        self.kbps.set(kbps);
                        self.start(&uri, gen);
                    }
                    None => self.find_stream(&t, Some(gen)),
                },
            }
        }
        self.publish_queue();
        self.publish_now();
        self.prefetch_next();
        self.autoplay_if_last();
    }

    fn start(&self, uri: &str, gen: u64) {
        if gen != self.play_gen.get() {
            return; // another song was picked meanwhile
        }
        if let Some(p) = self.player.borrow().as_ref() {
            p.load(uri);
        }
        self.publish_now();
    }

    /// Asks for a YouTube song's stream; plays it if `play` is still the song wanted.
    fn find_stream(self: &Rc<Self>, t: &Track, play: Option<u64>) {
        let Some(relay) = self.relay.clone() else {
            self.unavailable(&t.title, "the local relay didn't start");
            return;
        };
        let (id, title) = (t.id.clone(), t.title.clone());
        if play.is_none() && !self.streams.borrow_mut().start_finding(&id) {
            return; // already being looked up
        }
        let sources = self.shared.sources.clone();
        let look = id.clone();
        self.bg(
            move || sources.stream(&look).map(|s| (relay.add(s.url, s.size, s.mime), s.kbps)),
            move |e, result| {
                e.streams.borrow_mut().done_finding(&id);
                match result {
                    Ok((uri, kbps)) => {
                        e.streams.borrow_mut().found(&id, uri.clone(), kbps);
                        if let Some(gen) = play {
                            if gen == e.play_gen.get() {
                                e.kbps.set(kbps);
                            }
                            e.start(&uri, gen);
                        }
                    }
                    Err(err) if play.is_some_and(|g| g == e.play_gen.get()) => e.unavailable(&title, &err),
                    Err(_) => {}
                }
            },
        );
    }

    /// The next YouTube song's stream, found while this one plays: no wait between songs.
    fn prefetch_next(self: &Rc<Self>) {
        let next = {
            let q = self.queue.borrow();
            q.get(q.index() + 1).cloned()
        };
        if let Some(t) = next {
            let saved = self.liked.borrow().iter().any(|x| x.key() == t.key() && x.file.is_some());
            if t.source == Source::YouTube && t.file.is_none() && !saved && self.streams.borrow().fresh(&t.id).is_none() {
                self.find_stream(&t, None);
            }
        }
    }

    /// Autoplay: when the queue's last song plays, songs like it come after it (core's
    /// autoplay_picks). Not with Repeat on.
    fn autoplay_if_last(self: &Rc<Self>) {
        if !self.settings.borrow().autoplay || self.queue.borrow().repeat() != Repeat::Off {
            return;
        }
        let Some(seed) = self.current.borrow().clone() else { return };
        let last = {
            let q = self.queue.borrow();
            q.index() + 1 >= q.len()
        };
        if !last || self.autoplay_for.borrow().as_deref() == Some(seed.key().as_str()) {
            return;
        }
        *self.autoplay_for.borrow_mut() = Some(seed.key());
        let sources = self.shared.sources.clone();
        let s = seed.clone();
        self.bg(
            move || sources.radio(&s),
            move |e, found| {
                let still = e.current.borrow().as_ref().is_some_and(|c| c.key() == seed.key()) && {
                    let q = e.queue.borrow();
                    q.index() + 1 >= q.len()
                };
                if !still || !e.settings.borrow().autoplay {
                    return;
                }
                let queued: HashSet<String> = e.queue.borrow().iter().map(|t| t.key()).collect();
                let lately: HashSet<String> = crate::session::recent(&e.history.borrow()).into_iter().take(30).map(|t| t.key()).collect();
                let more = queue::autoplay_picks(found, &queued, &lately, &seed, |t| e.often_skipped(t));
                if more.is_empty() {
                    return;
                }
                crate::log::note(&format!("autoplay: {} songs like “{}” after it", more.len(), seed.title));
                e.queue.borrow_mut().extend(more);
                e.publish_queue();
                e.publish_now();
                if e.ran_out.replace(false) {
                    e.skip(1); // the last song ended while they were on their way: on to them
                } else {
                    e.prefetch_next();
                }
            },
        );
    }

    fn on_player(self: &Rc<Self>, ev: Event) {
        let song = self.current.borrow().as_ref().map(|t| t.title.clone()).unwrap_or_default();
        match &ev {
            Event::Ready => crate::log::note(&format!("playing “{song}”")),
            Event::Ended => crate::log::note(&format!("ended “{song}”")),
            Event::Error(e) => crate::log::note(&format!("error playing “{song}”: {e}")),
        }
        match ev {
            Event::Ready => {
                self.fails.set(0);
                self.sound_tries.set(0);
                let streamed = self.current.borrow().as_ref().is_some_and(|t| t.file.is_none());
                if streamed && !self.shared.view(|v| v.online) {
                    self.set_online(true); // it came from the internet, so that's back
                }
                if let Some(at) = self.restore_at.take() {
                    if let Some(p) = self.player.borrow().as_ref() {
                        p.seek(at);
                    }
                }
                self.publish_now();
            }
            Event::Ended if self.queue.borrow().repeat() == Repeat::One => self.play_current(),
            Event::Ended => self.skip(1),
            Event::Error(e) if trouble::sound_device_gone(&e) && self.sound_tries.get() < 3 => self.carry_on(),
            Event::Error(e) => self.unavailable(&song, &e),
        }
    }

    /// The sound device went away mid-song: the same song again from where it was, on the
    /// device Windows uses now. Three tries, a moment apart (#38).
    fn carry_on(self: &Rc<Self>) {
        let tries = self.sound_tries.get() + 1;
        self.sound_tries.set(tries);
        let at = self.player.borrow().as_ref().and_then(|p| p.position()).unwrap_or(0.0);
        crate::log::note(&format!("the sound device went away (try {tries}): carries on at {at:.0} s"));
        if let Some(p) = self.player.borrow().as_ref() {
            p.stop(); // lets go of the speaker it was on
        }
        let gen = self.play_gen.get();
        let me = Rc::downgrade(self);
        glib::timeout_add_local_once(Duration::from_millis(400 * tries as u64), move || {
            let Some(e) = me.upgrade() else { return };
            if e.play_gen.get() == gen {
                e.restore_at.set((at > 1.0).then_some(at));
                e.play_current();
            }
        });
    }

    /// A song that won't play: why, in plain words, and on to the next one (core's trouble).
    fn unavailable(self: &Rc<Self>, title: &str, why: &str) {
        crate::log::note(&format!("“{title}” won't play: {why}"));
        let fails = self.fails.get() + 1;
        self.fails.set(fails);
        let online = self.shared.view(|v| v.online);
        let (reason, every_song) = trouble::why_not(why, online);
        if reason == "you're offline" {
            self.set_online(false);
        }
        let more = self.queue.borrow().can_next();
        if trouble::after_failure(fails, more, every_song) == After::Next {
            self.say(trouble::next_song_text(title, reason));
            self.skip(1);
            return;
        }
        if let Some(p) = self.player.borrow().as_ref() {
            p.stop();
        }
        self.say(trouble::cant_play_text(title, reason, fails, every_song));
        self.publish_now();
    }

    fn say(&self, text: String) {
        self.shared.view(|v| v.problem = Some(text));
        self.shared.tell(Change::Problem);
    }

    fn toggle_play(self: &Rc<Self>) {
        if self.current.borrow().is_none() {
            return;
        }
        if self.restored.replace(false) {
            self.play_current(); // last time's song, from where it was
            return;
        }
        if let Some(p) = self.player.borrow().as_ref() {
            let was = p.playing();
            p.set_playing(!was);
        }
        self.publish_now();
    }

    /// The song playing, left early (before 70 % and with more than 30 s to go): a skip.
    fn note_skip_if_early(&self) {
        let Some(t) = self.current.borrow().clone() else { return };
        let (at, len) = {
            let player = self.player.borrow();
            let Some(p) = player.as_ref() else { return };
            (p.position(), p.duration())
        };
        if let (Some(at), Some(len)) = (at, len) {
            if queue::is_early_skip(at, len) {
                crate::db::add_skip(&t.key());
                self.counts.borrow_mut().entry(t.key()).or_default().1 += 1;
            }
        }
    }

    fn skip(self: &Rc<Self>, by: i32) {
        if by > 0 {
            self.note_skip_if_early();
        }
        let at = self.player.borrow().as_ref().and_then(|p| p.position()).unwrap_or(0.0);
        let step = self.queue.borrow_mut().skip(by, at);
        match step {
            Step::Restart => {
                if let Some(p) = self.player.borrow().as_ref() {
                    p.seek(0.0); // back: the start of this song first
                }
                self.position.set(0.0);
                self.publish_now();
            }
            Step::End => {
                if by > 0 {
                    if let Some(p) = self.player.borrow().as_ref() {
                        p.stop(); // the end of the list
                    }
                    self.ran_out.set(true); // Autoplay's songs, if they're on their way, start when they come
                    self.publish_now();
                }
            }
            Step::Play(_) => {
                self.ran_out.set(false);
                self.play_current();
            }
        }
    }

    fn seek(&self, at: f64) {
        if self.restored.get() {
            self.restore_at.set(Some(at)); // not started yet: from there when it is
        } else if let Some(p) = self.player.borrow().as_ref() {
            p.seek(at);
        }
        self.position.set(at);
        self.publish_now();
    }

    fn set_volume(&self, level: f64) {
        let level = level.clamp(0.0, 1.0);
        self.settings.borrow_mut().volume = level;
        settings::save(&self.settings.borrow());
        if let Some(p) = self.player.borrow().as_ref() {
            p.set_volume(level);
        }
        self.publish_now();
    }

    fn set_shuffle(self: &Rc<Self>, on: bool) {
        let key = self.current.borrow().as_ref().map(|t| t.key());
        let order = |v: Vec<Track>| self.shuffled(v);
        self.queue.borrow_mut().set_shuffle(on, key.as_deref(), order);
        self.publish_queue();
        self.publish_now();
        self.prefetch_next();
    }

    fn cycle_repeat(&self) {
        self.queue.borrow_mut().cycle_repeat();
        self.publish_now();
    }

    /// A song started: into the history (Home's "Recently played") and its count.
    fn note_played(&self, t: &Track) {
        if self.history.borrow().first().is_none_or(|(x, _)| x.key() != t.key()) {
            self.counts.borrow_mut().entry(t.key()).or_default().0 += 1; // the same song again right away isn't counted twice
        }
        crate::session::played(&mut self.history.borrow_mut(), t);
        self.save_session();
        self.publish_home();
    }

    /// The song's lyrics: looked for (its file, its server, LRCLIB…), then shown.
    fn load_lyrics(&self, t: &Track, gen: u64) {
        self.lines.borrow_mut().clear();
        self.shared.view(|v| v.lyrics = LyricsState::Looking);
        self.shared.tell(Change::Lyrics);
        let sources = self.shared.sources.clone();
        let song = t.clone();
        self.bg(
            move || crate::lyrics::lyrics_and_source_with(&song, |x| sources.lyrics(x)).0,
            move |e, l| {
                if e.play_gen.get() != gen {
                    return; // another song since
                }
                let state = match l {
                    Lyrics::Timed { lines, .. } => {
                        *e.lines.borrow_mut() = lines.clone();
                        LyricsState::Lines(lines)
                    }
                    Lyrics::Plain(text) => LyricsState::Plain(text),
                    Lyrics::None => LyricsState::None,
                };
                e.shared.view(|v| v.lyrics = state);
                e.shared.tell(Change::Lyrics);
            },
        );
    }

    /// Five times a second while a song plays.
    fn tick(&self) {
        let (playing, at, length) = {
            let player = self.player.borrow();
            let Some(p) = player.as_ref() else { return };
            (p.playing(), p.position(), p.duration())
        };
        if !playing {
            return;
        }
        let Some(at) = at else { return };
        self.position.set(at);
        let lit = radiance_core::sing::lit_lines(self.lines.borrow().iter(), at);
        self.shared.view(|v| {
            v.now.at = at;
            v.now.playing = true;
            v.now.lit = lit;
            if let Some(l) = length {
                v.now.length = l;
            }
        });
        self.shared.tell(Change::Position);
        if self.last_save.get().is_none_or(|t| t.elapsed() > Duration::from_secs(5)) {
            self.save_session(); // where you are, in case the app closes
        }
    }

    fn publish_now(&self) {
        let song = self.current.borrow().clone();
        let (playing, at, length) = {
            let player = self.player.borrow();
            match player.as_ref() {
                Some(p) if !self.restored.get() => (p.playing(), p.position().unwrap_or(self.position.get()), p.duration()),
                _ => (false, self.position.get(), None),
            }
        };
        let length = length.or_else(|| song.as_ref().map(|t| t.seconds as f64)).unwrap_or(0.0);
        let lit = radiance_core::sing::lit_lines(self.lines.borrow().iter(), at);
        let q = self.queue.borrow();
        let now = NowState {
            playing,
            at,
            length,
            can_previous: song.is_some(),
            can_next: song.is_some() && q.can_next(),
            shuffle: q.shuffle(),
            repeat: q.repeat(),
            volume: self.settings.borrow().volume,
            lossless: song.as_ref().is_some_and(|t| t.lossless()),
            kbps: self.kbps.get(),
            lit,
            song,
        };
        drop(q);
        self.shared.view(|v| v.now = now);
        self.shared.tell(Change::Now);
    }

    fn publish_queue(&self) {
        let q = self.queue.borrow();
        let queue = (q.tracks().to_vec(), q.index());
        drop(q);
        self.shared.view(|v| v.queue = queue);
        self.shared.tell(Change::Queue);
    }

    // ------------------------------------------------------------------ search

    fn search(self: &Rc<Self>, q: String) {
        let q = q.trim().to_string();
        let gen = self.search_gen.get() + 1;
        self.search_gen.set(gen);
        *self.query.borrow_mut() = q.clone();
        if q.chars().count() < 2 {
            *self.found.borrow_mut() = (Some(Vec::new()), Some(Vec::new()));
            self.publish_results();
            return;
        }
        *self.found.borrow_mut() = (None, None);
        self.publish_results();
        for kind in [Kind::Lossless, Kind::Songs] {
            let sources = self.shared.sources.clone();
            let q = q.clone();
            self.bg(
                move || {
                    let found = sources.search(kind, &q);
                    // YouTube Music found nothing: is it the song, or the internet?
                    let reached = kind != Kind::Songs || !found.is_empty() || sources.reachable();
                    (found, reached)
                },
                move |e, (found, reached)| {
                    if kind == Kind::Songs {
                        e.set_online(reached);
                    }
                    if e.search_gen.get() != gen {
                        return; // typed on since
                    }
                    {
                        let mut f = e.found.borrow_mut();
                        if kind == Kind::Lossless {
                            f.0 = Some(found);
                        } else {
                            f.1 = Some(found);
                        }
                    }
                    e.publish_results();
                },
            );
        }
    }

    fn publish_results(&self) {
        let query = self.query.borrow().clone();
        let (lossless, songs) = self.found.borrow().clone();
        let a = search::arrange(&self.liked.borrow(), lossless, songs, None, &query);
        let mine = if query.chars().count() < 2 { Vec::new() } else { a.mine };
        self.shared.view(|v| v.results = Results { query, mine, lossless: a.lossless, songs: a.youtube });
        self.shared.tell(Change::Results);
    }

    fn set_online(&self, on: bool) {
        let changed = self.shared.view(|v| std::mem::replace(&mut v.online, on) != on);
        if changed {
            self.shared.tell(Change::Online);
        }
    }

    // ------------------------------------------------------------------ home

    fn publish_home(&self) {
        let hour = glib::DateTime::now_local().map(|d| d.hour()).unwrap_or(12).max(0) as u32;
        let name = glib::real_name().to_string_lossy().to_string();
        let liked = self.liked.borrow().clone();
        let mix = self.mix.borrow().clone();
        let home = HomeState {
            greeting: words::greeting(hour, &name),
            mix_note: words::mix_note(mix.len(), liked.len()),
            recent: crate::session::recent(&self.history.borrow()),
            mix,
            liked,
        };
        self.shared.view(|v| v.home = home);
        self.shared.tell(Change::Home);
    }

    fn refresh_mix(self: &Rc<Self>) {
        let gen = self.mix_gen.get() + 1;
        self.mix_gen.set(gen);
        let liked = self.liked.borrow().clone();
        if liked.is_empty() {
            self.mix.borrow_mut().clear();
            self.publish_home();
            return;
        }
        let history = self.history.borrow().clone();
        self.bg(
            move || crate::home::made_for_you(&liked, &history, &crate::db::counts()),
            move |e, mix| {
                if e.mix_gen.get() == gen {
                    *e.mix.borrow_mut() = mix;
                    e.publish_home();
                }
            },
        );
    }
}

/// Where the Windows app's covers are kept as PNGs.
fn art_dir() -> Option<PathBuf> {
    let dir = glib::user_cache_dir().join("radiance-music/beta-art");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// `px` written to `path` (whole or not at all: another thread may be reading it).
fn save_png(px: crate::art::Pixels, path: &std::path::Path) -> Option<PathBuf> {
    let img = image::RgbaImage::from_raw(px.size, px.size, px.rgba)?;
    let part = path.with_extension(format!("{}.part", std::process::id()));
    img.save_with_format(&part, image::ImageFormat::Png).ok()?;
    std::fs::rename(&part, path).ok()?;
    Some(path.to_path_buf())
}
