//! The engine as one controller, for an app that isn't GTK (the Windows app, issue #45):
//! commands in (search, play this list, pause, …), a state to read and change events out.
//!
//! It runs on the engine's thread, which owns GLib's default main context for the whole process
//! (GStreamer's bus and the player's timers need one; glib's local timers always go to the
//! default context): commands are posted to it, background work comes back to it, and what an
//! app reads is a snapshot behind a lock. One controller at a time per process; each one is an
//! engine of its own on that thread (a new one never takes an old one's late answers).

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use radiance_core::library::{self as shelf, GroupKind, Saved};
use radiance_core::menu::{self, Action, Item, Place};
use radiance_core::quality::{self, Heard};
use radiance_core::queue::{self, Queue, Repeat, Step};
use radiance_core::search;
use radiance_core::split_queue::{self, SplitQueue};
use radiance_core::trouble::{self, After};
use radiance_core::words::{self, Kind};

use crate::liking::{self, Like};
use crate::lyrics::{Line, Lyrics};
use crate::player::{Event, Player};
use crate::playing::{self, Streams};
use crate::relay::Relay;
use crate::beat;
use crate::settings::{self, Settings, Transition, FADE_RANGE};
use crate::sources::Sources;
use crate::split;
use crate::splitting::Phase;
use crate::effects::{self, Sound};
use radiance_core::mix::{part_volumes, preset, Voice};
use crate::track::{Source, Track};

mod party;
mod rms;
pub use party::{PartyCard, PartyNext, PartyReaction, PartyState};
pub use rms::{ServerPage, ServerRow, ServersState};

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
    /// Liked Songs: a song liked, taken out, or saved (and the bar's heart).
    Liked,
    /// A short note to show (read it with toast()).
    Toast,
    /// Albums, Artists, Genres (the liked songs grouped; genres as they're found).
    Library,
    /// A song played (History).
    History,
    /// A playlist made, renamed, deleted, or a song added or taken out.
    Playlists,
    /// The split queue: a song being split (and how far), waiting, done, paused (stage 3).
    Splits,
    /// Getting splitting ready at the first start (the sunrise holds for it).
    Setup,
    /// The song's beat for Now Playing's background, worked out (or cleared for a new song) (#56).
    Pulse,
    /// An update found, downloading, ready, or what happened (stage 4a).
    Update,
    /// Report a Problem: what will be sent is read, or how sending went.
    Report,
    /// The rating pop-up: due at a free moment, signing in, or how sending went.
    Rating,
    /// Storage: the songs' folder and the space used (counted, or moved).
    Storage,
    /// Search suggestions for what's typed (yours at once, YouTube Music's after).
    Suggestions,
    /// Share Songs with a Phone: its QR, how far, a song received (stage 4b).
    Share,
    /// Radiance Music Servers: one joined or left, a server's songs or what it lets you do, the invite box's line (stage 4c).
    Servers,
    /// The listening party you're in, or a server's party card (stage 4c-2).
    Party,
}

/// Share Songs with a Phone as its dialog shows it (stage 4b, the Linux app's src/ui/sharing.rs): which way, the QR's
/// text (none: no network, or it couldn't get ready), the line under it, how far (0..1; -1: not known), done, going
/// gently (Bluetooth), and whether the firewall row shows (this app's rule isn't there yet).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShareState {
    pub on: bool,
    pub to_phone: bool,
    pub qr: Option<String>,
    pub caption: String,
    pub progress: Option<String>,
    pub fraction: f64,
    pub done: bool,
    pub gentle: bool,
    pub firewall: bool,
}

/// Storage as its section shows it: where the songs are, and the space used (None: counting…).
#[derive(Clone, Default)]
pub struct StorageState {
    pub folder: String,
    pub usage: Option<crate::storage::Usage>,
    /// A move under way.
    pub moving: bool,
}

/// Report a Problem as its dialog shows it.
#[derive(Clone, Debug, Default)]
pub struct ReportState {
    /// What will be sent (read on a worker; None: reading the app's log…).
    pub gathered: Option<crate::report::Report>,
    pub sending: bool,
    /// Sent: its code to quote; or why not.
    pub outcome: Option<Result<String, String>>,
}

/// How a rating went.
#[derive(Clone, Debug, PartialEq)]
pub enum RateOutcome {
    /// Sent; `share`: 4–5 stars, the thanks offers Share Radiance Music.
    Sent { share: bool },
    /// Why not, in the GTK app's words (Failure::words).
    Failed(String),
}

/// The rating pop-up's state.
#[derive(Clone, Debug, Default)]
pub struct RatingState {
    /// A free moment came and this one asks (the app shows it when it's free, then says so).
    pub due: bool,
    /// Signed in as (name, email).
    pub account: Option<(String, String)>,
    /// What it's doing ("Finish signing in with Google in your browser…"), or nothing.
    pub busy: String,
    /// The browser's sign-in address, for the app to open (once).
    pub sign_in_url: Option<String>,
    pub outcome: Option<RateOutcome>,
}

/// Where an update is (the GTK app's pop-up steps, src/ui/update.rs).
#[derive(Clone, Debug, PartialEq)]
pub enum UpdateStep {
    /// Nothing newer (or not looked yet).
    None,
    /// A newer version is out.
    Out,
    /// Downloading it, how far (0..1).
    Downloading(f64),
    /// Downloaded and checked: Restart to Update.
    Ready,
    /// Closing for the installer (the app quits now).
    Restarting,
    /// It didn't work: why ("Couldn't update: …").
    Failed(String),
}

/// The update as the pop-up shows it.
#[derive(Clone, Debug)]
pub struct UpdateState {
    pub step: UpdateStep,
    pub release: Option<crate::update::Release>,
    /// It asks now (not within a day of Remind Me Later, not the skipped version; or asked for by hand).
    pub asks: bool,
    /// This copy can't put it on itself: the release page instead (a copy run from elsewhere).
    pub page_only: bool,
    /// Windows' installer puts it on (Ready's words).
    pub installer: bool,
    /// The version on now.
    pub current: String,
    /// Restart to Update arranged the update: the app closes now (the new version waits for it).
    pub quit: bool,
}

impl Default for UpdateState {
    fn default() -> Self {
        UpdateState { step: UpdateStep::None, release: None, asks: false, page_only: false, installer: false, current: crate::update::version_here(), quit: false }
    }
}

/// Liked Songs as a page: the songs shown (A to Z, the search applied), the words under it,
/// the A to Z rail's letters (each with its first row; None: faint), and Fix missing's label
/// (None: nothing missing) while `fixing`.
#[derive(Clone, Debug)]
pub struct LikedPage {
    pub songs: Vec<(Track, Saved)>,
    pub note: String,
    pub letters: Vec<(char, Option<usize>)>,
    pub fix: Option<String>,
    pub fixing: bool,
}

/// An album's, an artist's or a genre's tile: its name, the line under it, the art of its
/// first song, round for an artist.
#[derive(Clone, Debug)]
pub struct Tile {
    pub name: String,
    pub sub: String,
    pub art: String,
    pub round: bool,
}

/// An album's, artist's or genre's page.
#[derive(Clone, Debug)]
pub struct GroupPage {
    pub title: String,
    pub sub: String,
    pub art: String,
    pub round: bool,
    pub songs: Vec<Track>,
}

/// The queue panel: Played before (at most five, oldest first), the queue from the song playing
/// (`start`: that song's place in the queue; `now`: a song is playing or picked, so it's the first
/// row), at most a hundred rows, and the words under it.
#[derive(Clone, Debug, Default)]
pub struct QueuePanel {
    pub before: Vec<Track>,
    pub rows: Vec<Track>,
    pub start: usize,
    pub now: bool,
    pub note: String,
}

/// A playlist's page: its name, "N songs", its songs, and the words when it's empty.
#[derive(Clone, Debug)]
pub struct PlaylistPage {
    pub name: String,
    pub count: String,
    pub songs: Vec<Track>,
    pub empty: Option<String>,
}

/// Liked songs as Liked Songs shows them: A to Z by title, those matching `q`.
fn shown_liked(liked: &[(Track, Saved)], q: &str) -> Vec<(Track, Saved)> {
    let q = q.trim();
    let mut v: Vec<(Track, Saved)> = liked.iter().filter(|(t, _)| q.is_empty() || crate::track::matches(t, q)).cloned().collect();
    v.sort_by_cached_key(|(t, _)| crate::track::file_key(&t.title));
    v
}

/// Told when something changed: on the engine's thread, so it must hand it on quickly.
pub trait Listener: Send + Sync + 'static {
    fn changed(&self, what: Change);
}

/// A list on screen that a song is played from.
#[derive(Clone, Debug, PartialEq, Eq)]
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
    /// Liked Songs as shown: A to Z, its last search (liked_page).
    LikedShown,
    /// An album's, an artist's or a genre's liked songs.
    Group(GroupKind, String),
    /// History's plays, newest first.
    History,
    /// A playlist's songs.
    Playlist(i64),
    /// A server's songs as its page shows them: its address, and its page's search (stage 4c).
    Server(String, String),
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
    /// 0 to 1 (the level kept; muted or not).
    pub volume: f64,
    /// Muted: silent, the volume kept as it was.
    pub muted: bool,
    pub lossless: bool,
    /// A stream's bit rate (0: a file, or not known).
    pub kbps: u32,
    /// The lyric lines lit now (core's sing::lit_lines).
    pub lit: Vec<usize>,
    /// The song is liked (the bar's heart).
    pub liked: bool,
    /// When `at` was read, on clock_ms()'s clock (the app counts on from there while playing).
    pub since: u64,
    /// A music video is playing (its picture: video_swapchain()).
    pub video: bool,
    /// The song's other version is known (the video button: its music video, or back to the song).
    pub has_counterpart: bool,
    /// What's really playing, in the Linux app's words ("Lossless · FLAC · 24-bit · 96 kHz");
    /// "Loading…" while `loading`; empty when nothing is (a song that stopped, or last time's, paused).
    pub quality: String,
    /// Picked and not playing yet: its stream being found, or its sound not flowing yet (#55).
    pub loading: bool,
    /// The Mixer (stage 3): the song's parts with their volumes, the voice button, the effects, and
    /// whether its parts play now.
    pub mix: MixState,
}

/// The Mixer as Now Playing shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct MixState {
    /// (part, volume 0..1), as split::mix_parts lists them (none: not split).
    pub parts: Vec<(String, f64)>,
    pub voice: Voice,
    pub effects: Sound,
    pub plays_parts: bool,
    /// What the Mixer says under its buttons ("Plays as the full song until it's split."), or nothing.
    pub note: String,
    /// The voice picked that waits for this song's split (its Mixer shows Song, what's heard, till then), or None.
    pub splitting_for: Option<Voice>,
}

impl Default for MixState {
    fn default() -> Self {
        MixState { parts: Vec::new(), voice: Voice::Song, effects: Sound::OFF, plays_parts: false, note: String::new(), splitting_for: None }
    }
}

/// Milliseconds on the engine's monotonic clock (NowState::since's).
pub fn clock_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// The song's lyrics.
#[derive(Clone, Debug, Default)]
pub enum LyricsState {
    /// Being looked for.
    #[default]
    Looking,
    None,
    Plain(String),
    /// Timed lines as shown: "♪" for a break, the singer where the voice changes, each word's place.
    Lines(Vec<radiance_core::sing::SungLine>),
}

/// What went wrong, in plain words; `retry`: it stopped (Try Again starts the song over; a note
/// that went on to the next song has none), as the GTK app's toasts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub text: String,
    pub retry: bool,
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

/// A song's split mark on its row (stage 3): waiting (its place in line), being split, or split.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitMark {
    None,
    Waiting(usize),
    Splitting,
    /// In two: Music and Vocals.
    Two,
    /// In four parts.
    Four,
}

/// The split queue as the app shows it (the Splits tab): the song being split, how far and at what,
/// the songs waiting (in four parts; in two, the fast lane), paused or not, and what can split here.
#[derive(Clone, Debug, Default)]
pub struct SplitsState {
    pub now: Option<(Track, Phase)>,
    pub waiting: Vec<Track>,
    pub two_now: Option<Track>,
    pub two_waiting: Vec<Track>,
    pub paused: bool,
    pub can_split: bool,
    pub can_split_two: bool,
    /// Your songs in a server's split queue (beta.11): each one and where it is ("On NAME · number 2 in line").
    pub away: Vec<(Track, String)>,
}

/// Splitting set up at the first start (stage 3; the GTK app's get_splitter_ready): not needed (set up
/// already, or not on this computer), getting ready (the step, how far), offline (it starts once the
/// internet's back), failed (tried again in a while), or ready.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum SetupState {
    #[default]
    NotNeeded,
    Getting { step: String, done: f64 },
    Offline,
    Failed { why: String },
    Ready,
}

static RUNNING: AtomicBool = AtomicBool::new(false);

/// What an app reads (written on the engine's thread).
struct View {
    results: Results,
    home: HomeState,
    online: bool,
    problem: Option<Problem>,
    now: NowState,
    queue: (Vec<Track>, usize),
    lyrics: LyricsState,
    /// Liked Songs (newest first), each as it's kept here.
    liked: Vec<(Track, Saved)>,
    /// Notes to show, oldest first.
    toasts: VecDeque<String>,
    /// History's plays, newest first (with when, unix seconds).
    history: Vec<(Track, i64)>,
    panel: QueuePanel,
    /// Each song's (plays, skips), by key.
    counts: HashMap<String, (usize, usize)>,
    /// The music video's swapchain, while one plays (Windows).
    /// The music video's swapchain while one plays, held (review I5): no one hands out a pointer
    /// the sink may have let go of.
    video: Option<crate::comref::ComRef>,
    splits: SplitsState,
    setup: SetupState,
    /// The playing song's beat (its key with it), for Now Playing's background (#56).
    pulse: Option<(String, Arc<crate::beat::Pulse>)>,
    /// Songs split while the app runs, and how (their rows' marks; the files tell the rest).
    split_done: HashMap<String, SplitMark>,
    update: UpdateState,
    report: ReportState,
    rating: RatingState,
    storage: StorageState,
    share: ShareState,
    autoplay: bool,
    guide_seen: bool,
    /// Between songs (stage 4a-2): the transition, the crossfade length, and a mix under way.
    transition: Transition,
    fade: f64,
    mixing: bool,
    suggestions: Vec<radiance_core::suggest::Chip>,
    /// Radiance Music Servers (stage 4c): Servers…, and each joined server's page (all its songs; searched as asked).
    servers: ServersState,
    party: PartyState,
    party_cards: HashMap<String, PartyCard>,
    server_pages: HashMap<String, ServerPage>,
}

/// Between the app's threads and the engine's.
pub(crate) struct Shared {
    listener: Arc<dyn Listener>,
    pub(crate) sources: Arc<dyn Sources>,
    quitting: AtomicBool,
    view: Mutex<View>,
    /// Liked Songs' search as last shown (what LikedShown plays).
    liked_q: Mutex<String>,
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
                liked: Vec::new(),
                toasts: VecDeque::new(),
                history: Vec::new(),
                panel: QueuePanel::default(),
                counts: HashMap::new(),
                video: None,
                splits: SplitsState::default(),
                setup: SetupState::default(),
                pulse: None,
                split_done: HashMap::new(),
                update: UpdateState::default(),
                report: ReportState::default(),
                rating: RatingState::default(),
                storage: StorageState::default(),
                share: ShareState::default(),
                autoplay: true,
                guide_seen: false,
                transition: Transition::Off,
                fade: 6.0,
                mixing: false,
                suggestions: Vec::new(),
                servers: ServersState::default(),
                party: PartyState::default(),
                party_cards: HashMap::new(),
                server_pages: HashMap::new(),
            }),
            liked_q: Mutex::new(String::new()),
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
    pub fn problem(&self) -> Option<Problem> {
        self.shared.view(|v| v.problem.clone())
    }

    /// Plays `list` (as shown) from song `i`, whose key is `key` (the song clicked: if the list
    /// moved since, the song with that key; gone from it, nothing); the queue: from its song `i`.
    pub fn play_from(&self, list: ListRef, i: usize, key: &str) {
        let key = key.to_string();
        self.post(move |e| e.play_from(list, i, &key));
    }

    /// Try Again (after it stopped): the song over, the count of failures from 0, as the GTK app.
    pub fn retry(&self) {
        self.post(|e| e.retry());
    }

    /// Mute (silent) or not; the volume stays as it was.
    pub fn set_muted(&self, on: bool) {
        self.post(move |e| e.set_muted(on));
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

    /// The song's other version, from where it is: its music video (`want_video`) or the song.
    pub fn switch_version(&self, want_video: bool) {
        self.post(move |e| e.switch_version(want_video));
    }

    /// The music video's picture while one plays: its DXGI swapchain (Windows), for a
    /// SwapChainPanel, with a reference of the caller's own (AddRef'd under the view's lock while
    /// the engine holds one): the caller Releases it once its panel holds the swapchain. None: no
    /// video, or no swapchain (not Windows).
    pub fn video_swapchain(&self) -> Option<u64> {
        self.shared.view(|v| v.video.as_ref().map(|r| r.give()))
    }

    /// The video's panel is `w`×`h` pixels now (the swapchain follows).
    pub fn resize_video(&self, w: u32, h: u32) {
        self.post(move |e| e.resize_video(w, h));
    }

    /// The heart: likes the song (it's saved, where it's saved) or takes it out of Liked.
    pub fn toggle_like(&self, key: &str) {
        let key = key.to_string();
        self.post(move |e| e.toggle_like(&key));
    }

    /// Saves every liked song whose file isn't on this computer (in the background).
    pub fn fix_missing(&self) {
        self.post(|e| e.fix_missing());
    }

    /// Liked Songs, newest first, each as it's kept here.
    pub fn liked(&self) -> Vec<(Track, Saved)> {
        self.shared.view(|v| v.liked.clone())
    }

    /// Downloading: the liked songs not saved on this computer yet, the ones saving now first (owner, 2026-11-07).
    pub fn downloading(&self) -> Vec<(Track, Saved)> {
        self.shared.view(|v| shelf::downloading(&v.liked, |f| std::path::Path::new(f).exists()))
    }

    /// The oldest note not shown yet (each is given once).
    pub fn toast(&self) -> Option<String> {
        self.shared.view(|v| v.toasts.pop_front())
    }

    /// Liked Songs as a page, searched for `q` (remembered: LikedShown plays what it shows).
    pub fn liked_page(&self, q: &str) -> LikedPage {
        *self.shared.liked_q.lock().unwrap_or_else(|e| e.into_inner()) = q.trim().to_string();
        let liked = self.shared.view(|v| v.liked.clone());
        let songs = shown_liked(&liked, q);
        let shown: Vec<Track> = songs.iter().map(|(t, _)| t.clone()).collect();
        let (fix, fixing) = fix_state(&liked);
        LikedPage {
            note: shelf::liked_note(liked.len(), &crate::library::music_dir().display().to_string(), q.trim(), songs.len()),
            letters: shelf::letters(&shown),
            fix,
            fixing,
            songs,
        }
    }

    /// Fix missing's label (None: nothing missing) and whether it's at work: Liked Songs' and Downloading's.
    pub fn fix_missing_state(&self) -> (Option<String>, bool) {
        self.shared.view(|v| fix_state(&v.liked))
    }

    /// The Albums, Artists or Genres page's tiles (A to Z).
    pub fn library(&self, kind: GroupKind) -> Vec<Tile> {
        let liked: Vec<Track> = self.shared.view(|v| v.liked.iter().map(|(t, _)| t.clone()).collect());
        shelf::groups(&liked, kind)
            .into_iter()
            .map(|g| {
                let round = kind == GroupKind::Artists;
                Tile {
                    sub: if kind == GroupKind::Albums { g.songs[0].credit() } else { shelf::songs_count(g.songs.len()) },
                    art: crate::art::source_of(&g.songs[0]),
                    round,
                    name: g.name,
                }
            })
            .collect()
    }

    /// An album's, artist's or genre's page.
    pub fn group(&self, kind: GroupKind, name: &str) -> Option<GroupPage> {
        let liked: Vec<Track> = self.shared.view(|v| v.liked.iter().map(|(t, _)| t.clone()).collect());
        let g = shelf::groups(&liked, kind).into_iter().find(|g| g.name == name)?;
        let n = shelf::songs_count(g.songs.len());
        Some(GroupPage {
            title: g.name,
            sub: if kind == GroupKind::Albums { format!("{} · {n}", g.songs[0].credit()) } else { n },
            art: crate::art::source_of(&g.songs[0]),
            round: kind == GroupKind::Artists,
            songs: g.songs,
        })
    }

    /// The playlists: (id, name, how many songs).
    pub fn playlists(&self) -> Vec<(i64, String, usize)> {
        crate::db::playlists()
    }

    /// A playlist's page (None: there's no such playlist).
    pub fn playlist(&self, id: i64) -> Option<PlaylistPage> {
        let name = crate::db::playlists().into_iter().find(|(i, _, _)| *i == id)?.1;
        let songs = crate::db::playlist_songs(id);
        Some(PlaylistPage { name, count: shelf::songs_count(songs.len()), empty: songs.is_empty().then(|| words::PLAYLIST_EMPTY.to_string()), songs })
    }

    /// A new playlist's suggested name ("My Playlist N").
    pub fn next_playlist_name(&self) -> String {
        words::next_playlist_name(crate::db::playlists().len())
    }

    /// Makes a playlist (with `first` in it, from a song's menu); its id.
    pub fn create_playlist(&self, name: &str, first: Option<&str>) -> Option<i64> {
        let id = crate::db::create_playlist(name)?;
        self.shared.tell(Change::Playlists);
        if let Some(key) = first {
            self.add_to_playlist(id, key);
        }
        Some(id)
    }

    pub fn rename_playlist(&self, id: i64, name: &str) {
        crate::db::rename_playlist(id, name);
        self.shared.tell(Change::Playlists);
    }

    pub fn delete_playlist(&self, id: i64) {
        crate::db::delete_playlist(id);
        self.shared.tell(Change::Playlists);
    }

    /// Adds the song to the playlist (said: "Added to …" or "Already in …").
    pub fn add_to_playlist(&self, id: i64, key: &str) {
        let key = key.to_string();
        self.post(move |e| e.add_to_playlist(id, &key));
    }

    pub fn remove_from_playlist(&self, id: i64, key: &str) {
        crate::db::remove_from_playlist(id, key);
        self.shared.tell(Change::Playlists);
    }

    /// Each song's (plays, skips), by key (a row's "N×" and its words).
    pub fn counts(&self) -> HashMap<String, (usize, usize)> {
        self.shared.view(|v| v.counts.clone())
    }

    /// The lyric lines lit at `at` (seconds), each with how many characters are sung: what the
    /// beta lights between the engine's readings (read here, not on the engine's thread).
    pub fn sung(&self, at: f64) -> Vec<(usize, usize)> {
        self.shared.view(|v| match &v.lyrics {
            LyricsState::Lines(lines) => radiance_core::sing::sung(lines, at),
            _ => Vec::new(),
        })
    }

    /// The queue panel.
    pub fn queue_panel(&self) -> QueuePanel {
        self.shared.view(|v| v.panel.clone())
    }

    /// Plays a song again now (from Played before): the queue carries on after it; no skip counted.
    pub fn play_again(&self, key: &str) {
        let key = key.to_string();
        self.post(move |e| e.play_again(&key));
    }

    /// The song plays next (said).
    pub fn play_next(&self, key: &str) {
        let key = key.to_string();
        self.post(move |e| e.play_next(&key));
    }

    /// The song goes at the end of the queue (said).
    pub fn enqueue(&self, key: &str) {
        let key = key.to_string();
        self.post(move |e| e.enqueue(&key));
    }

    /// Takes the queue's song at `index` out, if it's still the song `key`.
    pub fn unqueue(&self, index: usize, key: &str) {
        let key = key.to_string();
        self.post(move |e| e.unqueue(index, &key));
    }

    /// Moves the queue's song at `from` (if it's still `key`) to `to`; never above the song playing.
    pub fn move_in_queue(&self, from: usize, to: usize, key: &str) {
        let key = key.to_string();
        self.post(move |e| e.move_in_queue(from, to, &key));
    }

    /// The song's radio: it, then songs like it (said).
    pub fn start_radio(&self, key: &str) {
        let key = key.to_string();
        self.post(move |e| e.start_radio(&key));
    }

    /// The song's menu where it was opened.
    /// A song's page on the web, for Copy Link (stage 4b): the song the menu was opened on.
    pub fn song_link(&self, key: &str, place: Place) -> Option<String> {
        self.menu_song(key, place).map(|(t, _)| crate::share::song_link(&t))
    }

    /// The song a menu was opened on, and whether it's liked: wherever it's shown, else its playlist's copy.
    fn menu_song(&self, key: &str, place: Place) -> Option<(Track, bool)> {
        let (t, liked) = self.shared.view(|v| {
            let liked = v.liked.iter().any(|(t, _)| t.key() == key);
            let hit = |l: &[Track]| l.iter().find(|t| t.key() == key).cloned();
            let t = v.liked.iter().map(|(t, _)| t).find(|t| t.key() == key).cloned()
                .or_else(|| hit(&v.queue.0))
                .or_else(|| hit(&v.results.mine))
                .or_else(|| v.results.lossless.as_deref().and_then(hit))
                .or_else(|| v.results.songs.as_deref().and_then(hit))
                .or_else(|| hit(&v.home.mix))
                .or_else(|| hit(&v.home.recent))
                .or_else(|| v.history.iter().map(|(t, _)| t).find(|t| t.key() == key).cloned())
                .or_else(|| v.now.song.clone().filter(|t| t.key() == key))
                // a server's song, on its page (beta.11: its menu was empty)
                .or_else(|| v.server_pages.values().flat_map(|p| p.songs.iter().map(|(t, _)| t)).find(|t| t.key() == key).cloned());
            (t, liked)
        });
        // a playlist's song found nowhere else: the playlist's own copy (read outside the view's lock)
        let t = t.or_else(|| match place {
            Place::Playlist(id) => crate::db::playlist_songs(id).into_iter().find(|t| t.key() == key),
            _ => None,
        });
        t.map(|t| (t, liked))
    }

    pub fn song_menu(&self, key: &str, place: Place) -> Vec<Vec<Item>> {
        let Some((t, liked)) = self.menu_song(key, place) else { return Vec::new() };
        let saved = t.file.as_ref().is_some_and(|f| std::path::Path::new(f).exists());
        let lists: Vec<(i64, String)> = crate::db::playlists().into_iter().map(|(id, name, _)| (id, name)).collect();
        let split = self.shared.view(|v| split_ability(v, &t));
        let mut sections = menu::song_menu(&t, place, liked, saved, &lists, &split);
        // a server's song its server can split: Split on NAME instead of splitting here (the Linux menu's), not while
        // it waits there or once it has its parts
        let split_there = self.shared.view(|v| rms::split_there(&t, &v.servers.rows, v.splits.away.iter().any(|(a, _)| a.key() == t.key())));
        if let Some(name) = split_there {
            for s in sections.iter_mut() {
                s.retain(|i| !matches!(i.action, Action::SplitTwo | Action::SplitFour));
            }
            if split.parts == 0 && !name.is_empty() {
                if let Some(mine) = sections.iter_mut().find(|s| s.iter().any(|i| i.action == Action::Radio)) {
                    mine.push(menu::Item { label: radiance_core::server_words::split_on(&name), action: Action::SplitOnServer });
                }
            }
        }
        // a Radiance Music Server's (the Linux menu's, after liking and the radio): saving its song, adding yours
        let server = self.shared.view(|v| rms::server_items(&t, liked, saved, &v.servers.rows));
        if !server.is_empty() {
            let at = sections.iter().position(|s| s.iter().any(|i| i.action == Action::Radio)).map(|p| p + 1).unwrap_or(sections.len());
            sections.insert(at, server);
        }
        sections
    }

    /// A menu item for the song `key` (`index`: its place in the queue, for Remove from Queue).
    /// Play and New Playlist… are the app's (it knows the list, and asks for the name).
    pub fn do_menu(&self, key: &str, index: usize, action: Action) {
        match action {
            Action::Play | Action::NewPlaylistWith | Action::ShowLyrics | Action::EditLyrics | Action::CopyLink => {}
            Action::SplitOnServer => {
                let key = key.to_string();
                self.post(move |e| e.rms_split_on_key(&key));
            }
            Action::AddToServer(address) => {
                let key = key.to_string();
                self.post(move |e| e.rms_add_to(&key, &address));
            }
            Action::SplitTwo => self.split(key, true),
            Action::SplitFour => self.split(key, false),
            Action::FindLyricsAgain => self.find_lyrics_again(key),
            Action::PlayNext => self.play_next(key),
            Action::AddToQueue => self.enqueue(key),
            Action::RemoveFromQueue => self.unqueue(index, key),
            Action::RemoveFromPlaylist(id) => self.remove_from_playlist(id, key),
            Action::AddToPlaylist(id) => self.add_to_playlist(id, key),
            Action::Like | Action::Unlike => self.toggle_like(key),
            Action::Radio => self.start_radio(key),
            Action::GoToArtist(q) | Action::GoToAlbum(q) => self.search(&q),
            Action::ShowInFiles => {
                let key = key.to_string();
                self.post(move |e| e.show_in_files(&key));
            }
        }
    }

    /// History by day.
    pub fn history(&self) -> Vec<crate::history::Day> {
        let plays = self.shared.view(|v| v.history.clone());
        let now = glib::DateTime::now_local().map(|d| d.to_unix()).unwrap_or_default();
        crate::history::days(&plays, now)
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

    /// The Mixer's split line and choices for the song playing (core's mixer_split; owner, 2026-11-08): read again on
    /// every Now and Splits change (a four-part split's progress comes as a Splits change).
    pub fn mixer_split(&self) -> radiance_core::mix::MixerSplit {
        use radiance_core::mix::{self, SplitState};
        self.shared.view(|v| {
            let Some(t) = v.now.song.clone() else { return mix::MixerSplit::default() };
            let key = t.key();
            let s = &v.splits;
            let state = if s.two_now.as_ref().is_some_and(|x| x.key() == key) {
                SplitState::Two
            } else if let Some((_, p)) = s.now.as_ref().filter(|(x, _)| x.key() == key) {
                SplitState::Four(match p {
                    Phase::Splitting(f) => Some(*f),
                    _ => None,
                })
            } else if let Some(i) = s.two_waiting.iter().position(|x| x.key() == key) {
                SplitState::Waiting(i + usize::from(s.two_now.is_some()))
            } else if let Some(i) = s.waiting.iter().position(|x| x.key() == key) {
                SplitState::Waiting(i + usize::from(s.now.is_some()))
            } else {
                SplitState::None
            };
            mix::mixer_split(&t, &split_ability(v, &t), v.now.mix.splitting_for, state)
        })
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
    /// The Mixer (stage 3): a part's volume (0..1), Reset, the voice buttons, the effects, and each part's
    /// loudness now (its meter).
    pub fn set_part(&self, part: &str, gain: f64) {
        let part = part.to_string();
        self.post(move |e| e.set_part(&part, gain));
    }

    pub fn reset_mix(&self) {
        self.post(|e| e.reset_mix());
    }

    pub fn set_voice(&self, v: Voice) {
        self.post(move |e| e.set_voice(v));
    }

    pub fn set_effects(&self, s: Sound) {
        self.post(move |e| e.set_effects(s));
    }

    /// Each part's loudness right now (the Mixer's meters, polled while it's open).
    pub fn part_levels(&self) -> Vec<(String, f64)> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.post(move |e| {
            let _ = tx.send(e.player.borrow().as_ref().map(|p| p.part_levels()).unwrap_or_default());
        });
        rx.recv_timeout(Duration::from_millis(500)).unwrap_or_default()
    }

    /// The playing song's beat (its key with it) for Now Playing's background (#56); none yet, or none found.
    pub fn pulse(&self) -> Option<(String, Arc<crate::beat::Pulse>)> {
        self.shared.view(|v| v.pulse.clone())
    }

    /// Find Lyrics Again (the song's menu).
    pub fn find_lyrics_again(&self, key: &str) {
        let key = key.to_string();
        self.post(move |e| e.find_lyrics_again(&key));
    }

    /// The lyrics editor's Save: its rows (start, text, singer).
    pub fn save_lyrics(&self, key: &str, rows: Vec<(Option<f64>, String, Option<String>)>) {
        let key = key.to_string();
        self.post(move |e| e.save_lyrics(&key, rows));
    }

    /// The lyrics editor's rows for a song (start, text, singer) and who can sing a line (its artists,
    /// and anyone already named), as the GTK app's editor opens. Looked up when needed: call it off the
    /// app's own thread.
    pub fn lyrics_text(&self, key: &str) -> Option<(Vec<(Option<f64>, String, Option<String>)>, Vec<String>)> {
        let (tx, rx) = std::sync::mpsc::channel();
        let key = key.to_string();
        self.post(move |e| {
            let _ = tx.send(e.find_track(&key).map(|t| playing::saved_copy(&e.liked.borrow(), &t)));
        });
        let t = rx.recv_timeout(Duration::from_secs(2)).ok().flatten()?;
        let sources = self.shared.sources.clone();
        let l = crate::lyrics::lyrics_and_source_with(&t, |x| sources.lyrics(x)).0;
        let rows: Vec<(Option<f64>, String, Option<String>)> = match l {
            Lyrics::Timed { lines, .. } => lines.into_iter().map(|x| (Some(x.start), x.text, x.singer)).collect(),
            Lyrics::Plain(p) => p.lines().map(str::trim).filter(|x| !x.is_empty() && !(x.starts_with('[') && x.ends_with(']'))).map(|x| (None, x.to_string(), None)).collect(),
            Lyrics::None => vec![],
        };
        let mut names: Vec<String> = t.artists();
        for who in rows.iter().filter_map(|r| r.2.clone()) {
            if !names.contains(&who) {
                names.push(who);
            }
        }
        Some((rows, names))
    }

    /// Split a song by hand: first in line (stage 3). `two`: the fast split (Music + Vocals), else four parts.
    pub fn split(&self, key: &str, two: bool) {
        let key = key.to_string();
        self.post(move |e| e.split_by_hand(&key, two));
    }

    /// Pause splitting (the song being split finishes first) or carry on.
    pub fn set_split_paused(&self, on: bool) {
        self.post(move |e| e.set_split_paused(on));
    }

    /// The network came or went (GIO's monitor; Windows' own events too).
    pub fn network_changed(&self, available: bool) {
        self.post(move |e| e.set_online(available));
    }

    /// Search suggestions for what's typed (empty: your artists): yours at once, then YouTube
    /// Music's (an answer for older typing is dropped).
    pub fn suggest(&self, typed: &str) {
        let typed = typed.to_string();
        self.post(move |e| e.suggest(typed));
    }

    pub fn suggestions(&self) -> Vec<radiance_core::suggest::Chip> {
        self.shared.view(|v| v.suggestions.clone())
    }

    /// Storage: the songs' folder and the space used.
    pub fn storage(&self) -> StorageState {
        self.shared.view(|v| v.storage.clone())
    }

    /// Counts the space used (worker thread).
    pub fn storage_count(&self) {
        self.post(|e| e.storage_count());
    }

    /// Moves the songs to `to` (Storage's Change…: the folder picked; "Radiance Music" is added
    /// inside it as the Linux app does); a toast says how it went.
    pub fn move_music(&self, to: &str) {
        let to = std::path::PathBuf::from(to);
        self.post(move |e| e.move_music(to));
    }

    /// Autoplay When the Queue Ends.
    pub fn autoplay(&self) -> bool {
        self.shared.view(|v| v.autoplay)
    }

    pub fn set_autoplay(&self, on: bool) {
        self.post(move |e| e.set_autoplay(on));
    }

    /// Between songs (stage 4a-2): No Transition, Crossfade or DJ Mix (by the Beat), kept in the settings both apps share.
    pub fn transition(&self) -> Transition {
        self.shared.view(|v| v.transition)
    }

    pub fn set_transition(&self, t: Transition) {
        self.post(move |e| e.set_transition(t));
    }

    /// The crossfade length (whole seconds, 1 to 12).
    pub fn fade(&self) -> f64 {
        self.shared.view(|v| v.fade)
    }

    pub fn set_fade(&self, seconds: f64) {
        self.post(move |e| e.set_fade(seconds));
    }

    /// A crossfade or DJ mix under way (the two songs playing into each other).
    pub fn mixing(&self) -> bool {
        self.shared.view(|v| v.mixing)
    }

    /// Share Songs with a Phone (stage 4b): opens a share, To the Phone (its saved liked songs; never music videos,
    /// phones have none) or From the Phone; a new one replaces the last (its QR stops working).
    pub fn share_start(&self, to_phone: bool) {
        self.post(move |e| e.share_start(to_phone));
    }

    /// Closes the share (the dialog closed): its listener and its transfers stop.
    pub fn share_stop(&self) {
        self.post(|e| e.share_stop());
    }

    pub fn share(&self) -> ShareState {
        self.shared.view(|v| v.share.clone())
    }

    /// Let It In: this app's firewall rule for the sharing ports (one prompt), then the row goes.
    pub fn share_let_in(&self) {
        self.post(|e| e.share_let_in());
    }

    /// Servers… (stage 4c): the joined servers and the invite box's line.
    pub fn servers(&self) -> ServersState {
        self.shared.view(|v| v.servers.clone())
    }

    /// Join with an invite link (or its words pasted): the line under the box says how it went.
    pub fn server_join(&self, invite: &str) {
        let text = invite.to_string();
        self.post(move |e| e.rms_join(text));
    }

    /// Leave a server: it stops taking this computer's key, then it's forgotten here.
    pub fn server_leave(&self, address: &str) {
        let address = address.to_string();
        self.post(move |e| e.rms_leave(address));
    }

    /// A server's songs and what it lets you do, asked again (its page's Refresh, Try Again).
    pub fn server_refresh(&self, address: &str) {
        let address = address.to_string();
        self.post(move |e| e.rms_refresh(address));
    }

    /// A server's page as shown for its search `q` (play one with ListRef::Server(address, q)).
    pub fn server_page(&self, address: &str, q: &str) -> ServerPage {
        let page = self.shared.view(|v| v.server_pages.get(address).cloned());
        page.map(|p| rms::page_for(p, q)).unwrap_or_default()
    }

    /// The server page open now ("" : none): its party card is kept fresh while it's open (stage 4c-2).
    pub fn server_shown(&self, address: &str) {
        let address = address.to_string();
        self.post(move |e| e.party_shown(address));
    }

    /// The listening party you're in (`server` empty: none).
    pub fn party(&self) -> PartyState {
        self.shared.view(|v| v.party.clone())
    }

    /// A server page's party card: start one, join it, or (`in_it`) the party view.
    pub fn party_card(&self, address: &str) -> PartyCard {
        self.shared.view(|v| v.party_cards.get(address).cloned()).unwrap_or_default()
    }

    /// Start a Party on a server (`everyone`: everyone plays, pauses and skips); you're in it at once.
    pub fn party_start(&self, address: &str, everyone: bool) {
        let address = address.to_string();
        self.post(move |e| e.party_start(address, everyone));
    }

    /// Join a server's party: your own music waits, and comes back as it was when you leave.
    pub fn party_join(&self, address: &str) {
        let address = address.to_string();
        self.post(move |e| e.party_join(address));
    }

    /// Leave the Party: your own music back (a toast says so).
    pub fn party_leave(&self) {
        self.post(|e| {
            if e.in_party() {
                e.party_leave(true);
                e.toast(radiance_core::server_words::YOU_LEFT_PARTY.to_string());
            }
        });
    }

    /// End the Party, for everyone (the app asks first).
    pub fn party_end(&self) {
        self.post(|e| e.party_end());
    }

    /// The party view's Play / Pause (for everyone; you control it).
    pub fn party_play(&self, on: bool) {
        self.post(move |e| e.party_act(if on { "play" } else { "pause" }, serde_json::json!({})));
    }

    /// The party view's Skip (for everyone; you control it).
    pub fn party_skip(&self) {
        self.post(|e| e.party_act("skip", serde_json::json!({})));
    }

    /// Hear It Here, or not (the party plays on either way).
    pub fn party_hear(&self, on: bool) {
        self.post(move |e| e.party_hear(on));
    }

    /// Up next: a song to place `to` (Earlier, Later).
    pub fn party_move(&self, entry: u64, to: usize) {
        self.post(move |e| e.party_act("reorder", serde_json::json!({ "entry": entry, "to": to })));
    }

    /// Up next: a song taken out of the queue.
    pub fn party_remove(&self, entry: u64) {
        self.post(move |e| e.party_act("remove", serde_json::json!({ "entry": entry })));
    }

    /// The host's: everyone plays, pauses and skips, or only the host.
    pub fn party_allow(&self, everyone: bool) {
        self.post(move |e| e.party_act("allow", serde_json::json!({ "everyone": everyone })));
    }

    /// A reaction, for everyone in the party (one of servers::REACTIONS' emoji).
    pub fn party_react(&self, emoji: &str) {
        let emoji = emoji.to_string();
        self.post(move |e| e.party_react(emoji));
    }

    /// A song's menu offers Add to the Party: it's a song of the server whose party you're in.
    pub fn can_add_to_party(&self, key: &str) -> bool {
        self.shared.view(|v| !v.party.server.is_empty() && v.server_pages.get(&v.party.server).is_some_and(|p| p.songs.iter().any(|(t, _)| t.key() == key)))
    }

    /// Add to the Party (a server song's menu): into its queue, or playing at once when nothing does.
    pub fn party_add(&self, key: &str) {
        let key = key.to_string();
        self.post(move |e| {
            if let Some(t) = e.find_track(&key) {
                e.party_add(t);
            }
        });
    }

    /// The first-start guide was seen (in the settings both apps share).
    pub fn guide_seen(&self) -> bool {
        self.shared.view(|v| v.guide_seen)
    }

    pub fn set_guide_seen(&self) {
        self.post(|e| e.set_guide_seen());
    }

    /// Report a Problem: what will be sent and how sending went.
    pub fn report(&self) -> ReportState {
        self.shared.view(|v| v.report.clone())
    }

    /// Reads what will be sent (the app's log, scrubbed; the system; the install), on a worker.
    pub fn report_gather(&self) {
        self.post(|e| e.report_gather());
    }

    /// Sends it with your note.
    pub fn report_send(&self, note: &str) {
        let note = note.to_string();
        self.post(move |e| e.report_send(note));
    }

    /// The rating pop-up's state.
    pub fn rating(&self) -> RatingState {
        self.shared.view(|v| v.rating.clone())
    }

    /// It showed by itself (not again for 30 days, whatever is pressed).
    pub fn rating_shown(&self) {
        self.post(|e| e.rating_shown());
    }

    /// The free moment passed with the window not free (minimised, in the tray, a dialog up): let
    /// go, as GTK's feedback_moment; not counted as asked (the next free moment may ask).
    pub fn rating_missed(&self) {
        self.post(|e| e.set_rating(|r| r.due = false));
    }

    /// Don't Ask Again.
    pub fn rating_never(&self) {
        self.post(|_| crate::feedback::never_again());
    }

    /// Sends the answers: signed in, at once; else Google's sign-in in the browser first.
    pub fn rate(&self, answer: crate::feedback::Answer) {
        self.post(move |e| e.rate(answer));
    }

    /// Cancel Sign-In.
    pub fn cancel_sign_in(&self) {
        self.post(|e| e.rating_stop.store(true, Ordering::SeqCst));
    }

    /// Sign Out (here and there).
    pub fn sign_out(&self) {
        self.post(|e| e.sign_out());
    }

    /// The update: found, downloading, ready (stage 4a).
    pub fn update(&self) -> UpdateState {
        self.shared.view(|v| v.update.clone())
    }

    /// Looks for a newer version (`asked`: Look for Updates, by hand: it says so either way).
    pub fn look_for_updates(&self, asked: bool) {
        self.post(move |e| e.look_for_updates(asked));
    }

    /// Update Now: downloads it while the music plays.
    pub fn update_now(&self) {
        self.post(|e| e.update_now());
    }

    /// Restart to Update: the installer puts it on once this app has closed (the app quits on Restarting).
    pub fn update_restart(&self) {
        self.post(|e| e.update_restart());
    }

    /// Remind Me Later: not asked again for a day.
    pub fn update_later(&self) {
        self.post(|e| e.update_later());
    }

    /// Skip This Version: never asked about it again.
    pub fn update_skip(&self) {
        self.post(|e| e.update_skip());
    }

    /// Cancel the Download.
    pub fn update_cancel(&self) {
        self.post(|e| e.update_cancel());
    }

    /// Getting splitting ready at the first start (the beta's sunrise holds while it's Getting).
    pub fn setup(&self) -> SetupState {
        self.shared.view(|v| v.setup.clone())
    }

    /// Try Again (getting splitting ready failed, or it waits for the internet).
    pub fn retry_setup(&self) {
        self.post(|e| e.retry_setup());
    }

    /// "Cores it may use" for this computer's splitter, from the next split.
    pub fn set_split_cores(&self, n: u32) {
        self.post(move |e| e.set_split_cores(n));
    }

    /// The cores the splitter may use now, and this computer's.
    pub fn split_cores(&self) -> (u32, u32) {
        let now = settings::load().local_split.map(|l| l.threads).unwrap_or_else(crate::setup::half_the_cores);
        (now, crate::setup::cores() as u32)
    }

    pub fn splits(&self) -> SplitsState {
        self.shared.view(|v| v.splits.clone())
    }

    /// Every song with a split mark now: waiting (its place), being split, or split while the app runs.
    pub fn split_marks(&self) -> HashMap<String, SplitMark> {
        self.shared.view(|v| {
            let mut m: HashMap<String, SplitMark> = v.split_done.clone();
            for (i, t) in v.splits.waiting.iter().chain(v.splits.two_waiting.iter()).enumerate() {
                m.insert(t.key(), SplitMark::Waiting(i + 1));
            }
            for t in v.splits.now.iter().map(|(t, _)| t).chain(v.splits.two_now.iter()) {
                m.insert(t.key(), SplitMark::Splitting);
            }
            m
        })
    }

    /// A song's split mark (its row): waiting, being split, or split while the app runs.
    pub fn split_mark(&self, key: &str) -> SplitMark {
        self.shared.view(|v| {
            if v.splits.now.as_ref().is_some_and(|(t, _)| t.key() == key) || v.splits.two_now.as_ref().is_some_and(|t| t.key() == key) {
                return SplitMark::Splitting;
            }
            if let Some(i) = v.splits.waiting.iter().chain(v.splits.two_waiting.iter()).position(|t| t.key() == key) {
                return SplitMark::Waiting(i + 1);
            }
            v.split_done.get(key).copied().unwrap_or(SplitMark::None)
        })
    }

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
    suggest_gen: Cell<u64>,
    query: RefCell<String>,
    /// What the search found, as found: (lossless, YouTube Music).
    found: RefCell<(Option<Vec<Track>>, Option<Vec<Track>>)>,
    liked: RefCell<Vec<Track>>,
    /// Liked songs being saved now.
    saving: RefCell<HashSet<String>>,
    /// Liked songs' genres are being looked up.
    genres_busy: Cell<bool>,
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
    /// Share Songs with a Phone: the share open now, and which one (a closed one's events are let go).
    share: RefCell<Option<crate::share::Share>>,
    share_gen: Cell<u64>,
    /// Between songs (stage 4a-2, GTK's transitions): the DJ plan for (this song, the next), the pair being analysed,
    /// the song cued for the mix (its key, cued beat-matched, where it plays from), and a mix about to start.
    mix_plan: RefCell<Option<(String, String, beat::Plan)>>,
    analysing: RefCell<Option<(String, String)>>,
    cued: RefCell<Option<(String, bool, String)>>,
    mix_armed: Cell<bool>,
    /// Each song's (plays, skips).
    counts: RefCell<HashMap<String, (usize, usize)>>,
    settings: RefCell<Settings>,
    muted: Cell<bool>,
    /// The song's timed lines (the lit ones go in NowState).
    lines: RefCell<Vec<Line>>,
    last_save: Cell<Option<Instant>>,

    // music videos
    /// The playing song's other version: (the key it belongs to, that version).
    counterpart: RefCell<Option<(String, Track)>>,
    /// A video's streams found last: (its id, picture, sound, height, sound's kbps, when).
    found_video: RefCell<Option<FoundVideo>>,
    /// A switch to a music video under way: the song to go back to, where it was, the video's
    /// key (cleared once the video plays).
    switching: RefCell<Option<(Track, f64, String)>>,
    /// The video's picture height (the quality line's "720p").
    video_height: Cell<u32>,
    /// The video's swapchain (0: none).
    swapchain: Cell<u64>,
    /// What's really playing, in words (NowState::quality).
    quality: RefCell<String>,
    /// The song picked hasn't started yet (NowState::loading).
    loading: Cell<bool>,
    /// Splitting (stage 3): the four-part queue, the fast lane (song, asked by hand) and the song in it,
    /// and the songs asked for by hand (a failure says so).
    split_q: RefCell<SplitQueue>,
    /// A paused queue's one look-again pending (final review I5: one per waiting song before).
    pause_looking: Cell<bool>,
    /// How this copy updates (from Sources at the start), and the download's stop.
    install: RefCell<crate::update::Install>,
    update_stop: Arc<AtomicBool>,
    /// Cancel Sign-In.
    rating_stop: Arc<AtomicBool>,
    two_q: RefCell<VecDeque<(Track, bool)>>,
    /// Radiance Music Servers (stage 4c, controller/rms.rs).
    rms: rms::Rms,
    /// Listening parties (stage 4c-2, controller/party.rs).
    party: party::Parties,
    splitting_two: RefCell<Option<Track>>,
    split_asked: RefCell<HashSet<String>>,
    /// The Mixer's sliders for this run (not kept, as the GTK app's), and the voice button.
    part_gains: RefCell<HashMap<String, f64>>,
    voice: Cell<Voice>,
    /// The song's lyrics couldn't be looked up offline: looked up again once online (the GTK app's).
    lyrics_offline: Cell<bool>,
    /// The address the song plays from (its file, its stream through the relay), and the song its
    /// beat was worked out for.
    current_uri: RefCell<String>,
    pulse_for: RefCell<String>,
}

/// A music video's streams, as found (through the relay).
#[derive(Clone)]
struct FoundVideo {
    id: String,
    picture: String,
    sound: String,
    height: u32,
    kbps: u32,
    when: Instant,
}

/// How long a found video's streams are used again (YouTube's addresses last about 6 hours).
const VIDEO_FRESH: Duration = Duration::from_secs(3600);

impl Engine {
    fn new(shared: Arc<Shared>, epoch: u64) -> Engine {
        let settings = settings::load();
        // liked songs go to the folder picked in Storage (as the GTK app's start)
        crate::library::set_music_dir(settings.music_dir.clone().map(std::path::PathBuf::from));
        let (autoplay, guide_seen, transition, fade) = (settings.autoplay, settings.guide_seen, settings.transition, settings.fade);
        shared.view(|v| {
            v.autoplay = autoplay;
            v.guide_seen = guide_seen;
            v.transition = transition;
            v.fade = fade;
            v.storage.folder = crate::library::music_dir().display().to_string();
        });
        Engine {
            shared,
            epoch,
            search_gen: Cell::new(0),
            suggest_gen: Cell::new(0),
            query: RefCell::default(),
            found: RefCell::new((Some(Vec::new()), Some(Vec::new()))),
            liked: RefCell::default(),
            saving: RefCell::default(),
            genres_busy: Cell::new(false),
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
            share: RefCell::default(),
            share_gen: Cell::new(0),
            mix_plan: RefCell::default(),
            analysing: RefCell::default(),
            cued: RefCell::default(),
            mix_armed: Cell::new(false),
            counts: RefCell::default(),
            settings: RefCell::new(settings),
            muted: Cell::new(false),
            lines: RefCell::default(),
            last_save: Cell::new(None),
            counterpart: RefCell::default(),
            found_video: RefCell::default(),
            switching: RefCell::default(),
            video_height: Cell::new(0),
            swapchain: Cell::new(0),
            quality: RefCell::default(),
            loading: Cell::new(false),
            split_q: RefCell::default(),
            pause_looking: Cell::new(false),
            install: RefCell::new(crate::update::Install::Other),
            update_stop: Arc::new(AtomicBool::new(false)),
            rating_stop: Arc::new(AtomicBool::new(false)),
            two_q: RefCell::default(),
            splitting_two: RefCell::default(),
            split_asked: RefCell::default(),
            part_gains: RefCell::default(),
            voice: Cell::new(Voice::Song),
            lyrics_offline: Cell::new(false),
            current_uri: RefCell::default(),
            pulse_for: RefCell::default(),
            rms: rms::Rms::default(),
            party: party::Parties::default(),
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
        // the internet: GIO says when it comes and goes, as the GTK app's (final review I4: the first
        // start's download and offline lyrics waited for a search or a song to notice it was back)
        use gio::prelude::*;
        if !self.shared.sources.network_available() {
            self.set_online(false);
        }
        self.fetch_two_part();
        if self.shared.sources.watches_network() {
            let monitor = gio::NetworkMonitor::default();
            let me = Rc::downgrade(self);
            monitor.connect_network_changed(move |_, available| {
                if let Some(e) = me.upgrade() {
                    e.set_online(available);
                }
            });
        }
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
                self.shared.view(|v| v.problem = Some(Problem { text: format!("The player didn't start: {err}"), retry: false }));
            }
        }
        effects::set(self.settings.borrow().effects); // Low cut, High cut, Reverb: one setting, every song
        self.restore_session();
        self.publish_home();
        self.publish_liked();
        self.publish_history();
        self.publish_counts();
        self.fill_genres();
        self.refresh_mix();
        // likes that weren't saved yet (offline, or the app closed while saving)
        let unsaved: Vec<Track> = self.liked.borrow().iter().filter(|t| t.file.is_none() && crate::library::saves(t)).cloned().collect();
        unsaved.into_iter().for_each(|t| self.save_liked(t));
        // liked songs split by themselves, one after another, a while after the start (the GTK app's)
        if let Some(after) = split_queue::auto_after(std::env::var("RADIANCE_AUTO_SPLIT_S").ok().as_deref()) {
            self.bg(move || std::thread::sleep(Duration::from_secs(after)), |e, ()| e.auto_split_liked());
        }
        self.get_splitter_ready();
        // a look for updates a little after the start (the GTK app's 8 s), quietly
        *self.install.borrow_mut() = self.shared.sources.install();
        {
            let install = self.install.borrow().clone();
            self.shared.view(|v| {
                v.update.page_only = matches!(install, crate::update::Install::Other | crate::update::Install::Snap);
                v.update.installer = matches!(install, crate::update::Install::Windows(_) | crate::update::Install::Beta(_));
                v.update.current = crate::update::version_here();
            });
        }
        if let Ok(step) = std::env::var("RADIANCE_SHOT_UPDATE") {
            self.shot_update(&step); // (the pop-up's screenshots, as the GTK app's RADIANCE_SHOT_UPDATE)
        } else if std::env::var_os("RADIANCE_SHOT").is_none() {
            self.bg(|| std::thread::sleep(update_after()), |e, ()| e.look_for_updates(false));
        }
        self.rms_start(); // the joined servers: known to the player at once, their songs fetched quietly
        self.publish_splits();
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
        self.let_go_of_video();
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
        if self.in_party() {
            return; // a party's song isn't where you are (your own music was kept as you joined)
        }
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
    fn list(&self, list: &ListRef) -> Vec<Track> {
        let q = self.shared.liked_q.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let ListRef::Playlist(id) = list {
            return crate::db::playlist_songs(*id);
        }
        self.shared.view(|v| match list {
            ListRef::Mine => v.results.mine.clone(),
            ListRef::Lossless => v.results.lossless.clone().unwrap_or_default(),
            ListRef::Songs => v.results.songs.clone().unwrap_or_default(),
            ListRef::Mix => v.home.mix.clone(),
            ListRef::Recent => v.home.recent.clone(),
            ListRef::Liked => v.home.liked.clone(),
            ListRef::Queue => v.queue.0.clone(),
            ListRef::LikedShown => shown_liked(&v.liked, &q).into_iter().map(|(t, _)| t).collect(),
            ListRef::Group(kind, name) => {
                let liked: Vec<Track> = v.liked.iter().map(|(t, _)| t.clone()).collect();
                shelf::groups(&liked, *kind).into_iter().find(|g| &g.name == name).map(|g| g.songs).unwrap_or_default()
            }
            ListRef::History => v.history.iter().map(|(t, _)| t.clone()).collect(),
            ListRef::Playlist(_) => Vec::new(),
            ListRef::Server(address, q) => v.server_pages.get(address).map(|p| rms::page_for(p.clone(), q).songs.into_iter().map(|(t, _)| t).collect()).unwrap_or_default(),
        })
    }

    fn play_from(self: &Rc<Self>, list: ListRef, i: usize, key: &str) {
        let tracks = self.list(&list);
        // the song clicked: at its place, or (the list moved since) wherever it is now; gone: nothing
        let Some(i) = tracks.get(i).filter(|t| t.key() == key).map(|_| i).or_else(|| tracks.iter().position(|t| t.key() == key)) else { return };
        if let ListRef::Server(address, _) = &list {
            if self.in_party_on(address) {
                self.party_add(tracks[i].clone()); // in its party: into the party's queue (the Linux app's play_from_server)
                return;
            }
        }
        if list == ListRef::Queue {
            self.restored.set(false);
            self.restore_at.set(None);
            self.queue.borrow_mut().set_index(i);
            self.play_current();
            return;
        }
        if matches!(list, ListRef::Songs | ListRef::Lossless | ListRef::Mine) {
            self.play_with_radio(tracks[i].clone());
            return;
        }
        self.play_list(tracks, i);
    }

    /// A song picked from Search: it plays now, and its radio fills Up next (#47, as YouTube
    /// Music does: the other results aren't songs like it), quietly. A radio that answers after
    /// another song was picked is dropped; songs added to the queue meanwhile stay before it.
    fn play_with_radio(self: &Rc<Self>, seed: Track) {
        *self.autoplay_for.borrow_mut() = Some(seed.key()); // its radio is coming: Autoplay needn't
        self.play_list(vec![seed.clone()], 0);
        let sources = self.shared.sources.clone();
        let s = seed.clone();
        self.bg(move || sources.radio(&s), move |e, list| {
            if !e.current.borrow().as_ref().is_some_and(|c| c.key() == seed.key()) {
                return;
            }
            let more: Vec<Track> = {
                let q = e.queue.borrow();
                list.into_iter()
                    .filter(|x| x.key() != seed.key() && !x.same_song(&seed) && !e.often_skipped(x) && !q.iter().any(|t| t.key() == x.key()))
                    .collect()
            };
            if more.is_empty() {
                return;
            }
            e.queue.borrow_mut().extend(more);
            e.publish_queue();
            e.publish_now();
            if e.ran_out.replace(false) {
                e.skip(1); // the song ended while they were on their way: on to them
            } else {
                e.prefetch_next();
            }
        });
    }

    fn retry(self: &Rc<Self>) {
        self.fails.set(0);
        self.play_current();
    }

    fn set_muted(&self, on: bool) {
        self.muted.set(on);
        let level = if on { 0.0 } else { self.settings.borrow().volume };
        if let Some(p) = self.player.borrow().as_ref() {
            p.set_volume(level);
        }
        self.publish_now();
    }

    fn shuffle_from(self: &Rc<Self>, list: ListRef) {
        let tracks = self.list(&list);
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
        if self.in_party() {
            self.party_leave(false); // a song of your own picked: out of the party
            self.toast(radiance_core::server_words::LEFT_FOR_YOUR_OWN.to_string());
        }
        self.let_go_of_video(); // before the old video's pipeline goes (review I5)
        self.replan(); // a new song: its own plan and cue
        let Some(t) = self.queue.borrow().at_index().cloned() else { return };
        let t = playing::saved_copy(&self.liked.borrow(), &t);
        let gen = self.play_gen.get() + 1;
        self.play_gen.set(gen);
        self.kbps.set(0);
        self.position.set(self.restore_at.get().unwrap_or(0.0));
        *self.current.borrow_mut() = Some(t.clone());
        if self.switching.borrow().as_ref().is_some_and(|(_, _, key)| *key != t.key()) {
            self.switching.replace(None); // another song picked since the switch
        }
        self.load_lyrics(&t, gen);
        self.note_played(&t);
        self.find_counterpart(&t);
        self.video_height.set(0);
        let video = t.video && t.file.is_none();
        let lookup = if video {
            self.video_at_hand(&t.id).is_none()
        } else {
            t.source == Source::YouTube && t.file.is_none() && self.streams.borrow().fresh(&t.id).is_none()
        };
        if lookup {
            if let Some(p) = self.player.borrow().as_ref() {
                p.stop(); // not the last song under this one's name while it's found
            }
        }
        // a new song: its beat comes once it's a few seconds in
        self.current_uri.replace(String::new());
        if !self.pulse_for.replace(String::new()).is_empty() {
            self.shared.view(|v| v.pulse = None);
            self.shared.tell(Change::Pulse);
        }
        // "Loading…" until its sound flows (the Ready event), as the Linux app's bar says it
        self.loading.set(true);
        *self.quality.borrow_mut() = quality::text(Heard::Loading);
        if !video && self.play_parts(&t, 0.0, false) {
            // its parts at hand: the Mixer plays them (its own file among them)
        } else if video {
            self.play_video(&t, gen);
        } else if let Some(f) = &t.file {
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
        // No vocals or Vocals only carries on (owner, 2026-11-07: "pag nag next song need ko pa pumunta sa song then
        // bumalek sa no vocals para lang mag split???"): this song split for it now, the next ahead (GTK's play start)
        if self.voice.get() != Voice::Song {
            if !t.video && split::mix_parts(&t).is_empty() {
                self.split_fast(&t, true);
            }
            self.split_next_ahead();
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
        self.current_uri.replace(uri.to_string());
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
        let (look, song) = (id.clone(), t.clone());
        self.bg(
            // a joined server's copy first (rule 1), else YouTube's stream
            move || playing::stream_of(&song, &relay, || sources.stream(&look)),
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
        if self.party_event(&ev) {
            return; // a party's song: its end waits for the server's next one
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
                if let Some(p) = self.player.borrow().as_ref().filter(|p| p.is_video()) {
                    self.switching.replace(None); // the video plays: no way back needed
                    self.swapchain.set(p.video_swapchain()); // (the sink may make it only once it plays)
                }
                self.loading.set(false);
                self.update_quality();
                self.publish_now();
            }
            Event::Ended if self.queue.borrow().repeat() == Repeat::One => self.play_current(),
            Event::Ended => {
                self.skip(1);
                self.rating_moment(); // a free moment (GTK's feedback_moment)
            }
            Event::Error(e) if trouble::sound_device_gone(&e) && self.sound_tries.get() < 3 => self.carry_on(),
            Event::Error(e) => {
                if self.server_copy_broke(&e) {
                    return;
                }
                if !self.back_from_video(&e) {
                    self.unavailable(&song, &e);
                }
            }
        }
    }

    /// The song played from a joined server's copy (rule 1) and its stream broke (away from home): on from
    /// YouTube where it was, that server passed over for a minute. Whether it was that.
    fn server_copy_broke(self: &Rc<Self>, err: &str) -> bool {
        let Some(t) = self.current.borrow().clone().filter(|t| t.source == Source::YouTube && t.file.is_none()) else { return false };
        let Some(address) = crate::servers::stream_from(&t.id) else { return false };
        let at = self.player.borrow().as_ref().and_then(|p| p.position()).unwrap_or(0.0);
        crate::log::note(&format!("“{}” from {address} broke ({err}): from YouTube at {at:.0} s", t.title));
        crate::servers::skip_for_now(&address);
        crate::servers::forget_answer(&t);
        self.streams.borrow_mut().forget(&t.id);
        self.restore_at.set((at > 1.0).then_some(at));
        self.play_current();
        true
    }

    /// The sound device went away mid-song: the same song again from where it was, on the
    /// device Windows uses now. Three tries, a moment apart (#38).
    fn carry_on(self: &Rc<Self>) {
        let tries = self.sound_tries.get() + 1;
        self.sound_tries.set(tries);
        let at = self.player.borrow().as_ref().and_then(|p| p.position()).unwrap_or(0.0);
        crate::log::note(&format!("the sound device went away (try {tries}): carries on at {at:.0} s"));
        self.let_go_of_video();
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
            self.say(trouble::next_song_text(title, reason), false);
            self.skip(1);
            return;
        }
        self.let_go_of_video();
        if let Some(p) = self.player.borrow().as_ref() {
            p.stop();
        }
        // stopped: nothing's loading or playing (never "Loading…" for ever, the Linux app's 2026-10-31 audit)
        self.loading.set(false);
        self.quality.borrow_mut().clear();
        self.say(trouble::cant_play_text(title, reason, fails, every_song), true);
        self.publish_now();
    }

    /// A note for the app to show.
    fn toast(&self, text: String) {
        self.shared.view(|v| v.toasts.push_back(text));
        self.shared.tell(Change::Toast);
    }

    /// A song the app shows, by its key: playing, in the queue, liked, found, in the mix or the history.
    fn find_track(&self, key: &str) -> Option<Track> {
        let hit = |v: &[Track]| v.iter().find(|t| t.key() == key).cloned();
        if let Some(t) = self.current.borrow().as_ref().filter(|t| t.key() == key) {
            return Some(t.clone());
        }
        let found = self.found.borrow();
        hit(self.queue.borrow().tracks())
            .or_else(|| hit(&self.liked.borrow()))
            .or_else(|| found.0.as_deref().and_then(hit))
            .or_else(|| found.1.as_deref().and_then(hit))
            .or_else(|| hit(&self.mix.borrow()))
            .or_else(|| self.history.borrow().iter().map(|(t, _)| t).find(|t| t.key() == key).cloned())
            // a playlist's song found nowhere else (its page's menu, heart, Play Next)
            .or_else(|| crate::db::playlists().into_iter().find_map(|(id, _, _)| crate::db::playlist_songs(id).into_iter().find(|t| t.key() == key)))
            // a server's song (its page's menu: Add to the Party)
            .or_else(|| self.shared.view(|v| v.server_pages.values().flat_map(|p| p.songs.iter().map(|(t, _)| t)).find(|t| t.key() == key).cloned()))
    }

    fn saved_state(&self, t: &Track) -> Saved {
        if self.saving.borrow().contains(&t.key()) {
            Saved::Saving
        } else if t.file.is_some() {
            Saved::Saved
        } else if crate::library::saves(t) {
            Saved::NotSaved
        } else {
            Saved::Streams
        }
    }

    fn publish_liked(&self) {
        let list: Vec<(Track, Saved)> = self.liked.borrow().iter().map(|t| (t.clone(), self.saved_state(t))).collect();
        self.shared.view(|v| v.liked = list);
        self.shared.tell(Change::Liked);
        self.shared.tell(Change::Library);
    }

    fn publish_counts(&self) {
        let counts = self.counts.borrow().clone();
        self.shared.view(|v| v.counts = counts);
    }

    fn publish_history(&self) {
        let plays = self.history.borrow().clone();
        self.shared.view(|v| v.history = plays);
        self.shared.tell(Change::History);
    }

    /// Liked songs without genres get them looked up, gently (250 ms apart, in the background),
    /// as the Linux app does; then stored, and the library drawn again.
    fn fill_genres(self: &Rc<Self>) {
        if self.genres_busy.get() {
            return;
        }
        let todo: Vec<Track> = self.liked.borrow().iter().filter(|t| t.genres.is_none()).cloned().collect();
        if todo.is_empty() {
            return;
        }
        self.genres_busy.set(true);
        let sources = self.shared.sources.clone();
        self.bg(
            move || {
                todo.iter()
                    .map(|t| {
                        std::thread::sleep(Duration::from_millis(250)); // gently
                        (t.key(), sources.genres(t))
                    })
                    .collect::<Vec<_>>()
            },
            |e, found| {
                e.genres_busy.set(false);
                {
                    let mut liked = e.liked.borrow_mut();
                    for (key, g) in found {
                        if let Some(t) = liked.iter_mut().find(|t| t.key() == key) {
                            t.genres = Some(g);
                        }
                    }
                }
                crate::library::store(&e.liked.borrow());
                e.publish_liked();
                e.publish_home();
            },
        );
    }

    /// Liked Songs changed: it, the bar's heart, Home's row and Search's own matches.
    fn liked_changed(self: &Rc<Self>) {
        self.fill_genres();
        self.publish_liked();
        self.publish_now();
        self.publish_home();
        self.publish_results();
    }

    /// The other version (song <-> its official music video), from the same spot. A video's
    /// streams are found first, the song playing on meanwhile: if they can't be, it says why
    /// and the song goes on.
    fn switch_version(self: &Rc<Self>, want_video: bool) {
        let Some(cur) = self.current.borrow().clone() else { return };
        if cur.video == want_video {
            return;
        }
        let Some((key, other)) = self.counterpart.borrow().clone() else { return };
        if key != cur.key() {
            return;
        }
        if other.video && other.file.is_none() && self.video_at_hand(&other.id).is_none() {
            let gen = self.play_gen.get();
            let title = other.title.clone();
            self.find_video(&other, move |e, found| {
                if e.play_gen.get() != gen {
                    return; // another song since
                }
                match found {
                    Ok(()) => e.switch_version(want_video),
                    Err(why) => {
                        crate::log::note(&format!("“{title}” won't play: {why}"));
                        let (reason, _) = trouble::why_not(&why, e.shared.view(|v| v.online));
                        e.say(trouble::cant_play_text(&title, reason, 1, false), false);
                    }
                }
            });
            return;
        }
        let at = self.player.borrow().as_ref().and_then(|p| p.position()).unwrap_or(self.position.get());
        {
            let mut q = self.queue.borrow_mut();
            let i = q.index();
            q.set_at(i, other.clone());
        }
        // the way back is known already
        self.counterpart.replace(Some((other.key(), cur.clone())));
        self.switching.replace(other.video.then(|| (cur, at, other.key())));
        self.restored.set(false);
        self.restore_at.set((at > 1.0).then_some(at));
        self.play_current();
    }

    /// A video switched to didn't play: back to the song, from where it was, saying why.
    /// False: no switch was under way (an ordinary song that won't play).
    /// The video's swapchain no longer handed out, and the engine's reference to it gone: before
    /// its pipeline is stopped or replaced, so no one is given a swapchain the sink let go of.
    fn let_go_of_video(&self) {
        self.swapchain.set(0);
        self.shared.view(|v| v.video = None);
    }

    fn back_from_video(self: &Rc<Self>, why: &str) -> bool {
        self.let_go_of_video();
        let Some((song, at, key)) = self.switching.take() else { return false };
        let Some(video) = self.current.borrow().clone().filter(|c| c.key() == key) else { return false };
        crate::log::note(&format!("“{}” won't play: {why}", video.title));
        let (reason, _) = trouble::why_not(why, self.shared.view(|v| v.online));
        {
            let mut q = self.queue.borrow_mut();
            let i = q.index();
            q.set_at(i, song.clone());
        }
        self.counterpart.replace(Some((song.key(), video.clone())));
        self.restore_at.set((at > 1.0).then_some(at));
        self.play_current();
        self.say(trouble::cant_play_text(&video.title, reason, 1, false), false);
        true
    }

    /// Looks for the playing song's other version (its music video, or the song of a video).
    fn find_counterpart(self: &Rc<Self>, t: &Track) {
        let known = self.counterpart.borrow().as_ref().is_some_and(|(k, _)| *k == t.key());
        if known || t.source != Source::YouTube {
            return;
        }
        let sources = self.shared.sources.clone();
        let (song, key) = (t.clone(), t.key());
        self.bg(
            move || sources.counterpart(&song),
            move |e, found| {
                if e.current.borrow().as_ref().map(|c| c.key()) != Some(key.clone()) {
                    return; // another song since
                }
                if let Some(other) = found {
                    e.counterpart.replace(Some((key, other)));
                    e.publish_now();
                }
            },
        );
    }

    /// A video's picture and sound, when they're at hand: its saved file (both), or the streams
    /// found lately. (picture, sound, height, kbps)
    fn video_at_hand(&self, id: &str) -> Option<(String, String, u32, u32)> {
        if let Some(file) = crate::db::video_file(id) {
            use gio::prelude::*;
            let uri = gio::File::for_path(file).uri().to_string();
            return Some((uri.clone(), uri, 720, 0)); // saved: from the file, offline
        }
        let found = self.found_video.borrow();
        found.as_ref().filter(|f| f.id == id && f.when.elapsed() < VIDEO_FRESH).map(|f| (f.picture.clone(), f.sound.clone(), f.height, f.kbps))
    }

    /// Finds a video's streams (YouTube, through the relay); `done` with how it went.
    fn find_video(self: &Rc<Self>, t: &Track, done: impl FnOnce(&Rc<Engine>, Result<(), String>) + Send + 'static) {
        let Some(relay) = self.relay.clone() else {
            done(self, Err("the local relay didn't start".into()));
            return;
        };
        let sources = self.shared.sources.clone();
        let id = t.id.clone();
        self.bg(
            {
                let id = id.clone();
                move || {
                    let found = sources.video_streams(&id);
                    match &found {
                        Ok((v, a)) => crate::log::note(&format!("music video {id}: {} {}p ({} bytes) + {} ({} bytes)", v.mime, v.kbps, v.size, a.mime, a.size)),
                        Err(e) => crate::log::note(&format!("music video {id}: {e}")),
                    }
                    found.map(|(v, a)| (relay.add(v.url, v.size, v.mime), v.kbps, relay.add(a.url, a.size, a.mime), a.kbps))
                }
            },
            move |e, found| match found {
                Ok((picture, height, sound, kbps)) => {
                    e.found_video.replace(Some(FoundVideo { id, picture, sound, height, kbps, when: Instant::now() }));
                    done(e, Ok(()));
                }
                Err(why) => done(e, Err(why)),
            },
        );
    }

    /// A music video: its picture where the cover goes, its sound as a song's.
    fn play_video(self: &Rc<Self>, t: &Track, gen: u64) {
        if let Some((picture, sound, height, kbps)) = self.video_at_hand(&t.id) {
            self.start_video(&t.title, &picture, &sound, height, kbps, gen);
            return;
        }
        let title = t.title.clone();
        self.find_video(t, move |e, found| {
            if e.play_gen.get() != gen {
                return; // another song since
            }
            let at_hand = e.current.borrow().as_ref().and_then(|c| e.video_at_hand(&c.id));
            match (found, at_hand) {
                (Ok(()), Some((picture, sound, height, kbps))) => e.start_video(&title, &picture, &sound, height, kbps, gen),
                (Ok(()), None) => e.unavailable(&title, "YouTube didn't give this video"),
                (Err(why), _) => e.unavailable(&title, &why),
            }
        });
    }

    fn start_video(self: &Rc<Self>, title: &str, picture: &str, sound: &str, height: u32, kbps: u32, gen: u64) {
        if gen != self.play_gen.get() {
            return; // another song was picked meanwhile
        }
        self.video_height.set(height);
        self.kbps.set(kbps);
        let out = self.player.borrow().as_ref().map(|p| p.load_video_out(picture, sound));
        match out {
            Some(Ok(v)) => {
                self.swapchain.set(v.swapchain);
                self.publish_now(); // still "Loading…": what it is once it plays (Ready)
            }
            Some(Err(why)) => {
                if !self.back_from_video(&why) {
                    self.unavailable(title, &why);
                }
            }
            None => {}
        }
    }

    fn resize_video(&self, w: u32, h: u32) {
        if let Some(p) = self.player.borrow().as_ref() {
            p.resize_video(w, h);
        }
    }

    // ------------------------------------------------------------------ parts and the Mixer (stage 3)

    /// A song's parts as (name, file uri), its own file's uri and sample rate (the GTK app's parts_of).
    fn parts_of(&self, t: &Track) -> (Vec<(String, String)>, Option<String>, Option<u32>) {
        use gio::prelude::*;
        let parts = split::mix_parts(t).iter().filter_map(|p| split::ready_part(t, p).map(|f| (p.to_string(), gio::File::for_path(&f).uri().to_string()))).collect();
        let file = t.file.as_deref().filter(|f| std::path::Path::new(f).exists());
        (parts, file.map(|f| gio::File::for_path(f).uri().to_string()), file.and_then(split::file_rate))
    }

    fn gains_for(&self, t: &Track) -> HashMap<String, f64> {
        part_volumes(&self.part_gains.borrow(), split::wants_six(t))
    }

    /// Plays the song with its parts at hand, from `at` (`switch`: from the song playing as a plain
    /// file, fading over). False: none here, or they wouldn't load.
    fn play_parts(&self, t: &Track, at: f64, switch: bool) -> bool {
        let (parts, whole, rate) = self.parts_of(t);
        if parts.is_empty() {
            return false;
        }
        let gains = self.gains_for(t);
        match self.player.borrow().as_ref().map(|p| p.load_parts(whole.as_deref(), &parts, at, &gains, rate, switch)) {
            Some(Ok(())) => {
                crate::log::note(&format!("“{}” plays with its parts at hand{}", t.title, if switch { " (switched over, fading)" } else { "" }));
                self.current_uri.replace(whole.clone().unwrap_or_else(|| parts[0].1.clone()));
                true
            }
            Some(Err(e)) => {
                crate::log::note(&format!("stems: {e}"));
                false
            }
            None => false,
        }
    }

    /// The sliders to the player (the GTK app's apply_mix): with its parts playing, each part's
    /// volume, the song's own file blended in when all are up; a plain file, over to its parts.
    fn apply_mix(&self) {
        let Some(t) = self.current.borrow().clone() else { return };
        let have = split::mix_parts(&t);
        if have.is_empty() {
            return;
        }
        let gains = self.gains_for(&t);
        let full = have.iter().all(|p| gains.get(*p).is_none_or(|g| *g >= 0.999));
        let stems = self.player.borrow().as_ref().is_some_and(|p| p.is_stems());
        if stems {
            if let Some(p) = self.player.borrow().as_ref() {
                for (part, g) in &gains {
                    p.set_part(part, *g);
                }
                p.set_whole(full); // all up: the song's own file again, blended in
            }
            self.update_quality();
            return;
        }
        if !full {
            let at = self.player.borrow().as_ref().and_then(|p| p.position()).unwrap_or(0.0);
            self.play_parts(&t, at, true);
        }
    }

    fn set_part(&self, part: &str, gain: f64) {
        self.part_gains.borrow_mut().insert(part.to_string(), gain.clamp(0.0, 1.0));
        self.apply_mix();
        self.publish_now();
    }

    /// Reset: every part at full, the voice back to the song, the effects off.
    fn reset_mix(&self) {
        self.part_gains.borrow_mut().clear();
        self.voice.set(Voice::Song);
        self.set_effects(Sound::OFF);
        self.apply_mix();
        self.publish_now();
    }

    /// Song / No vocals / Vocals only: on a split song (four parts, or Music and Vocals), Mixer presets;
    /// not split, the fast split is asked for first (the button holds; its parts come in seconds).
    fn set_voice(self: &Rc<Self>, v: Voice) {
        self.voice.set(v);
        self.replan(); // the next song's cue with this voice's sliders
        let Some(t) = self.current.borrow().clone() else { return };
        // the preset always (a song not split yet gets it once its parts land: final review C2, it came
        // back with every part at full), and its split asked for
        {
            let mut g = self.part_gains.borrow_mut();
            for p in radiance_core::mix::SIX.iter().chain(["music"].iter()) {
                g.insert(p.to_string(), preset(v, p));
            }
        }
        if split::mix_parts(&t).is_empty() && v != Voice::Song {
            self.split_fast(&playing::saved_copy(&self.liked.borrow(), &t), true);
        }
        if v != Voice::Song {
            self.split_next_ahead();
        }
        self.apply_mix();
        self.publish_now();
    }

    /// With No vocals or Vocals only picked: the next song split ahead, ready by the time it plays (GTK's).
    fn split_next_ahead(self: &Rc<Self>) {
        let next = {
            let q = self.queue.borrow();
            q.get(q.index() + 1).cloned()
        };
        if let Some(n) = next.map(|n| playing::saved_copy(&self.liked.borrow(), &n)) {
            if !n.video && split::mix_parts(&n).is_empty() {
                self.split_fast(&n, false);
            }
        }
    }

    /// The song is being split, or waits in line for it (either lane).
    fn splitting(&self, t: &Track) -> bool {
        let key = t.key();
        let q = self.split_q.borrow();
        self.splitting_two.borrow().as_ref().is_some_and(|x| x.key() == key)
            || self.two_q.borrow().iter().any(|(x, _)| x.key() == key)
            || q.now().is_some_and(|x| x.key() == key)
            || q.waiting().iter().any(|x| x.key() == key)
    }

    /// Low cut, High cut, Reverb: one setting for every song, kept.
    fn set_effects(&self, s: Sound) {
        effects::set(s);
        self.settings.borrow_mut().effects = s;
        settings::save(&self.settings.borrow());
        self.publish_now();
    }

    fn mix_state(&self) -> MixState {
        let parts = match self.current.borrow().as_ref() {
            Some(t) => {
                let gains = self.gains_for(t);
                split::mix_parts(t).iter().map(|p| (p.to_string(), gains.get(*p).copied().unwrap_or(1.0))).collect()
            }
            None => Vec::new(),
        };
        // its parts are loaded (the Mixer is live; all up, the song's own file is what's heard)
        let plays_parts = self.player.borrow().as_ref().is_some_and(|p| p.is_stems());
        // a voice asked for on a song not split yet: the full song plays till its parts land (GTK's status), so the
        // Mixer shows Song (owner, 2026-11-07: "diba dapat automatic sya babalek sa song kase di pa downloaded") and
        // what it's splitting for; the voice stays picked and is on once they land
        let wanted = self.voice.get();
        let cur = self.current.borrow().clone();
        let ready = !parts.is_empty() || cur.is_none();
        let (voice, splitting_for) = radiance_core::mix::shown_voice(wanted, ready, cur.as_ref().is_some_and(|t| self.splitting(t)));
        let note = if voice != wanted { radiance_core::words::FULL_UNTIL_SPLIT.to_string() } else { String::new() };
        MixState { parts, voice, effects: effects::now(), plays_parts, note, splitting_for }
    }

    // ------------------------------------------------------------------ splitting (stage 3)

    /// Split as picked on the song's menu (the GTK app's split_as): in two, a few seconds; or in four,
    /// first in line, a few minutes.
    fn split_by_hand(self: &Rc<Self>, key: &str, two: bool) {
        let Some(t) = self.find_track(key) else { return };
        if t.video {
            return;
        }
        // a server's song its server can split: split there, for everyone (the Linux app's split_now), once
        if self.rms_splits_on(&t).is_some() {
            if servers_parts(&t) == 0 {
                self.rms_split_on(t);
            }
            return;
        }
        split::ask_two(&t, two);
        if two {
            self.split_fast(&t, true);
            self.toast(words::split_two_toast(&t.title));
        } else {
            self.split_asked.borrow_mut().insert(t.key());
            self.queue_split(&t, true);
            self.toast(words::split_four_toast(&t.title));
        }
    }

    /// In line for four parts (`first`: asked for by hand, or it's playing); asked for in two, the fast lane.
    fn queue_split(self: &Rc<Self>, t: &Track, first: bool) {
        if t.video || split::done(t) {
            return;
        }
        if split::wants_two(t) {
            self.split_fast(t, first);
            return;
        }
        if !self.shared.sources.can_split() {
            return;
        }
        if self.split_q.borrow_mut().ask(t.clone(), first) {
            self.run_splits();
        }
        self.publish_splits();
    }

    /// The fast lane (Music + Vocals inside the app): asked for by hand goes first.
    fn split_fast(self: &Rc<Self>, t: &Track, asked: bool) {
        if t.video || split::has_halves(t) || !self.can_split_two() {
            return;
        }
        let key = t.key();
        if self.splitting_two.borrow().as_ref().is_some_and(|x| x.key() == key) {
            return;
        }
        {
            let mut q = self.two_q.borrow_mut();
            let was_asked = q.iter().any(|(x, a)| x.key() == key && *a);
            q.retain(|(x, _)| x.key() != key);
            if asked {
                q.push_front((t.clone(), true));
            } else {
                q.push_back((t.clone(), was_asked));
            }
        }
        self.run_fast();
        self.publish_splits();
    }

    fn can_split_two(&self) -> bool {
        self.shared.sources.can_split_two() || self.shared.view(|v| v.online)
    }

    /// Splitting waits while ~/.config/radiance-music/split-paused exists (the GTK app's flag, shared).
    fn split_paused() -> bool {
        glib::user_config_dir().join("radiance-music/split-paused").exists()
    }

    /// Pauses or resumes splitting (the song being split finishes first).
    fn set_split_paused(self: &Rc<Self>, on: bool) {
        let flag = glib::user_config_dir().join("radiance-music/split-paused");
        if on {
            if let Some(dir) = flag.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(&flag, b"");
        } else {
            let _ = std::fs::remove_file(&flag);
        }
        self.toast((if on { words::SPLITTING_PAUSED } else { words::SPLITTING_RESUMED }).to_string());
        self.publish_splits();
        if !on {
            self.run_splits();
            self.run_fast();
        }
    }

    /// The song's address for a splitter: its file, else its stream through the relay.
    fn split_uri(&self, t: &Track) -> Option<String> {
        use gio::prelude::*;
        if let Some(f) = t.file.as_deref().filter(|f| std::path::Path::new(f).exists()) {
            return Some(gio::File::for_path(f).uri().to_string());
        }
        None
    }

    /// The next four-part split, one at a time (the GTK app's run_splits).
    fn run_splits(self: &Rc<Self>) {
        if self.split_q.borrow().now().is_some() || self.split_q.borrow().waiting().is_empty() {
            self.publish_splits();
            return;
        }
        if Self::split_paused() {
            self.publish_splits();
            if !self.pause_looking.replace(true) {
                self.bg(|| std::thread::sleep(pause_poll()), |e, ()| {
                    e.pause_looking.set(false);
                    e.run_splits();
                }); // once the pause is over
            }
            return;
        }
        let Some(t) = self.split_q.borrow_mut().take_next() else { return };
        self.publish_splits();
        let (plain, relay, sources, shared) = (self.split_uri(&t), self.relay.clone(), self.shared.sources.clone(), self.shared.clone());
        let song = t.clone();
        let report: Arc<dyn Fn(Phase) + Send + Sync> = Arc::new(move |p| {
            shared.view(|v| {
                if let Some(now) = v.splits.now.as_mut() {
                    now.1 = p;
                }
            });
            shared.tell(Change::Splits);
        });
        self.bg(
            move || {
                let uri = plain.or_else(|| sources.stream(&song.id).ok().zip(relay).map(|(s, r)| r.add(s.url, s.size, s.mime))).unwrap_or_default();
                sources.split(&song, false, &uri, report)
            },
            move |e, result: Result<(), String>| {
                e.split_q.borrow_mut().finish();
                let asked = e.split_asked.borrow_mut().remove(&t.key());
                match result {
                    Ok(()) => e.split_landed(&t, SplitMark::Four),
                    Err(why) => {
                        crate::log::note(&format!("split {}: {why}", t.title));
                        if asked {
                            e.toast(words::split_failed(&t.title));
                        }
                    }
                }
                e.publish_splits();
                e.run_splits();
            },
        );
    }

    /// The fast lane's next song (the GTK app's run_fast). While paused, only what was asked for by hand.
    fn run_fast(self: &Rc<Self>) {
        if self.splitting_two.borrow().is_some() {
            return;
        }
        let Some((t, asked)) = self.two_q.borrow_mut().pop_front() else {
            self.publish_splits();
            return;
        };
        if !asked && Self::split_paused() {
            self.run_fast();
            return;
        }
        self.splitting_two.replace(Some(t.clone()));
        self.publish_splits();
        let (plain, relay, sources) = (self.split_uri(&t), self.relay.clone(), self.shared.sources.clone());
        let song = t.clone();
        self.bg(
            move || {
                let uri = plain.or_else(|| sources.stream(&song.id).ok().zip(relay).map(|(s, r)| r.add(s.url, s.size, s.mime))).unwrap_or_default();
                sources.split(&song, true, &uri, Arc::new(|_| {}))
            },
            move |e, result: Result<(), String>| {
                e.splitting_two.replace(None);
                match result {
                    Ok(()) => {
                        e.split_landed(&t, SplitMark::Two);
                        e.time_words_later(&t);
                    }
                    Err(why) => {
                        crate::log::note(&format!("fast split {}: {why}", t.title));
                        if asked {
                            e.toast(words::split_failed(&t.title));
                        }
                    }
                }
                e.publish_splits();
                e.run_fast();
            },
        );
    }

    // ------------------------------------------------------------------ lyrics: editing, finding again (stage 3)

    /// After a fast split, while its halves already play: every word timed to its Vocals (on the split
    /// server, or this computer's splitter), then the lyrics again if it's still playing.
    fn time_words_later(self: &Rc<Self>, t: &Track) {
        if crate::word_times::aligned_by_mms(t) {
            return;
        }
        let (sources, song, key) = (self.shared.sources.clone(), t.clone(), t.key());
        self.bg(
            move || sources.time_words(&song),
            move |e, r: Result<(), String>| {
                if let Err(why) = r {
                    crate::log::note(&format!("word times after the fast split: {why}"));
                }
                let cur = e.current.borrow().clone();
                if let Some(cur) = cur.filter(|c| c.key() == key) {
                    e.load_lyrics(&cur, e.play_gen.get());
                }
            },
        );
    }

    /// Find Lyrics Again (the GTK app's): the kept ones go, the ones found take their place.
    fn find_lyrics_again(self: &Rc<Self>, key: &str) {
        let Some(t) = self.find_track(key) else { return };
        let t = playing::saved_copy(&self.liked.borrow(), &t);
        let timed = crate::word_times::Aligner::of(&self.settings.borrow()).is_some() && split::has_parts(&t);
        self.toast(words::finding_lyrics_again(&t.title, timed));
        let (sources, song) = (self.shared.sources.clone(), t.clone());
        self.bg(
            move || sources.find_lyrics_again(&song),
            move |e, r| {
                match r {
                    Ok(timed) => e.toast(words::found_lyrics_again(timed).to_string()),
                    Err(why) => e.toast(why),
                }
                let cur = e.current.borrow().clone();
                if let Some(cur) = cur.filter(|c| c.key() == t.key()) {
                    e.load_lyrics(&cur, e.play_gen.get());
                }
            },
        );
    }

    /// The lyrics editor's Save: kept for the song (by hand: never replaced), the lyrics again if it's
    /// playing, then every word timed when it's split.
    fn save_lyrics(self: &Rc<Self>, key: &str, rows: Vec<(Option<f64>, String, Option<String>)>) {
        let Some(t) = self.find_track(key) else { return };
        let t = playing::saved_copy(&self.liked.borrow(), &t);
        let l = radiance_core::lyrics::from_edited(&rows);
        if crate::word_times::save_edited(&t, &l).is_err() {
            self.toast(words::LYRICS_NOT_SAVED.to_string());
            return;
        }
        self.toast(words::LYRICS_SAVED.to_string());
        let cur = self.current.borrow().clone();
        if let Some(cur) = cur.filter(|c| c.key() == t.key()) {
            self.load_lyrics(&cur, self.play_gen.get());
        }
        if split::has_parts(&t) {
            let (sources, song) = (self.shared.sources.clone(), t.clone());
            self.bg(move || sources.time_words(&song), |_, r: Result<(), String>| {
                if let Err(why) = r {
                    crate::log::note(&format!("word times for edited lyrics: {why}"));
                }
            });
        }
    }

    // ------------------------------------------------------------------ the first start's setup (stage 3)

    /// Splitting set up at the first start, as the GTK app's get_splitter_ready: nothing to do when it's
    /// set up already (or its files are here: kept), and only Windows downloads its pack here.
    fn get_splitter_ready(self: &Rc<Self>) {
        // RADIANCE_SETUP_TEST=act: the download acted out for screenshot checks (even with a splitter here)
        if acted_setup() {
            self.set_splitter_up();
            return;
        }
        // RADIANCE_SETUP=off: the beta's developer runs download nothing onto the test computer (#61 M7)
        if std::env::var("RADIANCE_SETUP").as_deref() == Ok("off") {
            return;
        }
        {
            // GTK's guard: a split server needs no splitter here; one whose Python is gone is set up again
            let s = self.settings.borrow();
            if s.split.is_some() || s.local_split.as_ref().is_some_and(|l| std::path::Path::new(&l.python).exists()) {
                return;
            }
        }
        // a computer that can't split (too few cores, too little memory or disk) goes on without it, as
        // GTK's: no hold on the sunrise, no endless retry (final review I3)
        let problems = self.shared.sources.setup_problems();
        if !problems.is_empty() {
            crate::log::note(&format!("splitting can't be set up on this computer: {}", problems.join(" ")));
            return;
        }
        if std::env::var_os("RADIANCE_SETUP_TEST").is_none() {
            if let Some(local) = crate::setup::installed(crate::setup::half_the_cores()) {
                self.settings.borrow_mut().local_split = Some(local);
                settings::save(&self.settings.borrow());
                return;
            }
            if !cfg!(windows) {
                return;
            }
        }
        self.set_splitter_up();
    }

    fn set_setup(&self, s: SetupState) {
        self.shared.view(|v| v.setup = s);
        self.shared.tell(Change::Setup);
    }

    /// The download and install (worker thread), each step said as it comes; offline, it waits for the
    /// internet; another failure tries again in 30 s.
    fn set_splitter_up(self: &Rc<Self>) {
        // (one at a time: the internet coming back and the retry's timer could both ask)
        if matches!(self.shared.view(|v| v.setup.clone()), SetupState::Getting { .. }) {
            return;
        }
        crate::log::note("getting splitting ready on this computer (its first start)");
        self.set_setup(SetupState::Getting { step: String::new(), done: 0.0 });
        let (sources, shared) = (self.shared.sources.clone(), self.shared.clone());
        let threads = crate::setup::half_the_cores();
        self.bg(
            move || {
                let say = move |step: &str, done: f64| {
                    shared.view(|v| v.setup = SetupState::Getting { step: step.to_string(), done });
                    shared.tell(Change::Setup);
                };
                if acted_setup() {
                    return crate::setup::acted(&say, Duration::from_millis(900));
                }
                sources.install_splitter(threads, &say)
            },
            |e, result| match result {
                Ok(local) => {
                    crate::log::note("splitting is set up on this computer");
                    e.set_online(true); // (it came from the internet)
                    e.settings.borrow_mut().local_split = Some(local);
                    settings::save(&e.settings.borrow());
                    e.set_setup(SetupState::Ready);
                    e.publish_splits();
                    e.run_splits();
                }
                Err(why) => {
                    crate::log::note(&format!("getting splitting ready stopped: {why}"));
                    let online = e.shared.view(|v| v.online);
                    if trouble::why_not(&why, online).0 == "you're offline" {
                        e.set_setup(SetupState::Offline);
                        e.set_online(false); // (back online, it starts again)
                    } else {
                        e.set_setup(SetupState::Failed { why });
                    }
                    // tried again in a while, offline too, as GTK's (final review I4: offline it waited for
                    // a search or a song to notice the internet was back)
                    e.bg(|| std::thread::sleep(setup_retry()), |e, ()| {
                        if matches!(e.shared.view(|v| v.setup.clone()), SetupState::Failed { .. } | SetupState::Offline) {
                            e.set_splitter_up();
                        }
                    });
                }
            },
        );
    }

    // ------------------------------------------------------------------ suggestions, the fast split's files (stage 4a)

    fn suggest(self: &Rc<Self>, typed: String) {
        let gen = self.suggest_gen.get() + 1;
        self.suggest_gen.set(gen);
        let mine = radiance_core::suggest::mine(&self.liked.borrow(), &typed);
        self.shared.view(|v| v.suggestions = mine.clone());
        self.shared.tell(Change::Suggestions);
        if typed.trim().is_empty() {
            return;
        }
        let sources = self.shared.sources.clone();
        let q = typed.trim().to_string();
        self.bg(
            move || sources.suggestions(&q),
            move |e, found: Vec<String>| {
                if e.suggest_gen.get() != gen {
                    return; // typed on
                }
                let mut all = mine.clone();
                all.extend(radiance_core::suggest::add_youtube(&mine, &found));
                e.shared.view(|v| v.suggestions = all);
                e.shared.tell(Change::Suggestions);
            },
        );
    }

    /// The fast split's files fetched quietly if missing (never in a screenshot or test copy run,
    /// nor a beta developer run: RADIANCE_SETUP=off).
    fn fetch_two_part(self: &Rc<Self>) {
        if std::env::var_os("RADIANCE_SHOT").is_some() || std::env::var("RADIANCE_SETUP").as_deref() == Ok("off") {
            return;
        }
        let sources = self.shared.sources.clone();
        self.bg(move || sources.fetch_two_part(), |_, ()| {});
    }

    // ------------------------------------------------------------------ storage, Autoplay, the guide (stage 4a)

    fn set_storage(&self, f: impl FnOnce(&mut StorageState)) {
        self.shared.view(|v| f(&mut v.storage));
        self.shared.tell(Change::Storage);
    }

    fn storage_count(self: &Rc<Self>) {
        let dir = crate::library::music_dir();
        self.set_storage(|s| {
            s.folder = dir.display().to_string();
            s.usage = None;
        });
        self.bg(move || crate::storage::usage(&dir), |e, u| e.set_storage(|s| s.usage = Some(u)));
    }

    fn move_music(self: &Rc<Self>, to: std::path::PathBuf) {
        use radiance_core::words;
        let from = crate::library::music_dir();
        let to = crate::storage::target(&to);
        self.toast(words::MOVING.to_string());
        self.set_storage(|s| s.moving = true);
        let (a, b) = (from.clone(), to.clone());
        self.bg(
            move || crate::storage::move_and_switch(&a, &b, crate::library::set_music_dir, |x, y| crate::db::relocate(x, y)),
            move |e, result: Result<(usize, usize), String>| {
                match result {
                    Ok((moved, kept)) => {
                        e.settings.borrow_mut().music_dir = Some(to.display().to_string());
                        settings::save(&e.settings.borrow());
                        *e.liked.borrow_mut() = crate::db::liked();
                        e.publish_liked();
                        e.toast(words::moved(moved, kept, &to.display().to_string()));
                    }
                    Err(why) => e.toast(words::couldnt_move(&why)),
                }
                e.set_storage(|s| s.moving = false);
                e.storage_count();
            },
        );
    }

    fn set_autoplay(&self, on: bool) {
        self.settings.borrow_mut().autoplay = on;
        settings::save(&self.settings.borrow());
        self.shared.view(|v| v.autoplay = on);
        self.toast(if on { radiance_core::words::AUTOPLAY_ON } else { radiance_core::words::AUTOPLAY_OFF }.to_string());
    }

    fn set_transition(&self, t: Transition) {
        self.settings.borrow_mut().transition = t;
        settings::save(&self.settings.borrow());
        self.shared.view(|v| v.transition = t);
        self.replan();
        self.toast(radiance_core::words::transition_toast(t.id()).to_string());
    }

    /// The crossfade length as the Linux app's slider has it: whole seconds, within FADE_RANGE.
    fn set_fade(&self, seconds: f64) {
        let s = seconds.round().clamp(FADE_RANGE.0, FADE_RANGE.1);
        self.settings.borrow_mut().fade = s;
        settings::save(&self.settings.borrow());
        self.shared.view(|v| v.fade = s);
        self.replan();
    }

    // ------------------------------------------------------------------ Share Songs with a Phone (stage 4b)

    fn set_share(&self, f: impl FnOnce(&mut ShareState)) {
        self.shared.view(|v| f(&mut v.share));
        self.shared.tell(Change::Share);
    }

    fn share_start(self: &Rc<Self>, to_phone: bool) {
        use radiance_core::share_words as sw;
        self.share.replace(None); // the old QR stops working
        let gen = self.share_gen.get() + 1;
        self.share_gen.set(gen);
        let (tx, rx) = async_channel::unbounded();
        let saved: Vec<Track> = self.liked.borrow().iter().filter(|t| t.file.as_deref().is_some_and(|f| std::path::Path::new(f).exists())).cloned().collect();
        let opened = if to_phone { crate::share::Share::send(saved.clone(), tx) } else { crate::share::Share::receive(tx) };
        let mut st = ShareState { on: true, to_phone, fraction: -1.0, ..Default::default() };
        match opened {
            Ok(share) => {
                match share.ticket(&sw::device_name(&glib::host_name())) {
                    Some(t) => {
                        st.qr = Some(t);
                        st.caption = if to_phone { sw::to_phone_caption(saved.len()) } else { sw::FROM_PHONE_CAPTION.to_string() };
                    }
                    None => st.caption = sw::NO_NETWORK.to_string(),
                }
                self.share.replace(Some(share));
            }
            Err(e) => st.caption = sw::couldnt_get_ready(&e),
        }
        self.set_share(|s| *s = st);
        let me = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            while let Ok(ev) = rx.recv().await {
                let Some(e) = me.upgrade() else { break };
                if e.share_gen.get() != gen {
                    break; // a newer share, or closed
                }
                e.share_event(ev);
            }
        });
        // the firewall row: this app's rule isn't there yet (netsh on Windows: a worker)
        self.bg(crate::firewall::blocks, move |e, blocks| {
            if e.share_gen.get() == gen {
                e.set_share(|s| s.firewall = blocks);
            }
        });
    }

    fn share_event(self: &Rc<Self>, ev: crate::share::Event) {
        use crate::share::Event;
        use radiance_core::share_words as sw;
        match ev {
            Event::Progress { bytes, total, songs, of, now } => {
                let done = of > 0 && songs == of;
                self.set_share(|s| {
                    s.progress = Some(if done { sw::done(of) } else { sw::progress(songs, of, bytes, total, now.as_deref()) });
                    s.fraction = if total > 0 { (bytes as f64 / total as f64).min(1.0) } else { -1.0 };
                    s.done = done;
                });
            }
            Event::Received(t) => self.received_from_phone(t),
            Event::Gentle(on) => self.set_share(|s| s.gentle = on),
        }
    }

    /// A song from the phone: the liked one it is gets its file (same key, or the same title and artist), or it's
    /// liked here, first (the Linux app's received_from_phone).
    fn received_from_phone(self: &Rc<Self>, t: Track) {
        let key = t.key();
        {
            let mut liked = self.liked.borrow_mut();
            match liked.iter_mut().find(|x| x.key() == key || x.same_song(&t)) {
                Some(x) => x.file = t.file.clone(),
                None => liked.insert(0, t.clone()),
            }
        }
        crate::library::store(&self.liked.borrow());
        crate::log::note(&format!("from the phone: {}", t.title));
        self.liked_changed();
    }

    fn share_stop(&self) {
        self.share_gen.set(self.share_gen.get() + 1);
        self.share.replace(None);
        self.set_share(|s| *s = ShareState::default());
    }

    fn share_let_in(self: &Rc<Self>) {
        let gen = self.share_gen.get();
        self.bg(crate::firewall::let_in, move |e, ok| {
            if ok && e.share_gen.get() == gen {
                e.set_share(|s| s.firewall = false);
                e.toast(radiance_core::share_words::LET_IN_TOAST.to_string());
            }
        });
    }

    fn set_guide_seen(&self) {
        self.settings.borrow_mut().guide_seen = true;
        settings::save(&self.settings.borrow());
        self.shared.view(|v| v.guide_seen = true);
    }

    // ------------------------------------------------------------------ report and rating (stage 4a)

    fn set_report(&self, f: impl FnOnce(&mut ReportState)) {
        self.shared.view(|v| f(&mut v.report));
        self.shared.tell(Change::Report);
    }

    fn report_gather(self: &Rc<Self>) {
        self.set_report(|r| *r = ReportState::default());
        self.bg(crate::report::Report::gather, |e, r| e.set_report(|s| s.gathered = Some(r)));
    }

    fn report_send(self: &Rc<Self>, note: String) {
        let Some(report) = self.shared.view(|v| v.report.gathered.clone()) else { return };
        self.set_report(|r| {
            r.sending = true;
            r.outcome = None;
        });
        let sources = self.shared.sources.clone();
        self.bg(
            move || sources.send_report(&report, &note).map_err(|f| f.words()),
            |e, sent| {
                e.set_report(|r| {
                    r.sending = false;
                    r.outcome = Some(sent);
                });
            },
        );
    }

    fn set_rating(&self, f: impl FnOnce(&mut RatingState)) {
        self.shared.view(|v| f(&mut v.rating));
        self.shared.tell(Change::Rating);
    }

    /// A free moment (a song's end, a pause): does the rating ask now? The app shows it when it's
    /// free (never first open, never over a dialog) and then says rating_shown.
    fn rating_moment(&self) {
        if self.shared.view(|v| v.rating.due) || std::env::var_os("RADIANCE_SHOT").is_some() {
            return;
        }
        if self.shared.sources.rating_due() {
            crate::log::note("feedback: asking (a free moment, at random)");
            self.set_rating(|r| r.due = true);
        }
    }

    fn rating_shown(&self) {
        crate::feedback::asked_now();
        let account = self.shared.sources.account().map(|a| (a.name, a.email));
        self.set_rating(|r| {
            r.due = false;
            r.account = account;
            r.outcome = None;
        });
    }

    /// Sends the answers (signed in), or signs in first: the browser's address goes to the app.
    fn rate(self: &Rc<Self>, answer: crate::feedback::Answer) {
        self.rating_stop.store(false, Ordering::SeqCst);
        self.set_rating(|r| {
            r.outcome = None;
            r.sign_in_url = None;
            r.busy = radiance_core::words::SENDING.into();
        });
        let (sources, shared, stop) = (self.shared.sources.clone(), self.shared.clone(), self.rating_stop.clone());
        self.bg(
            move || {
                let account = match sources.account() {
                    Some(a) => a,
                    None => {
                        shared.view(|v| v.rating.busy = radiance_core::words::RATE_GETTING_READY.into());
                        shared.tell(Change::Rating);
                        let open = |url: &str| {
                            shared.view(|v| {
                                v.rating.sign_in_url = Some(url.to_string());
                                v.rating.busy = radiance_core::words::RATE_IN_BROWSER.into();
                            });
                            shared.tell(Change::Rating);
                        };
                        sources.sign_in(&stop, &open)?
                    }
                };
                shared.view(|v| {
                    v.rating.account = Some((account.name.clone(), account.email.clone()));
                    v.rating.busy = radiance_core::words::SENDING.into();
                });
                shared.tell(Change::Rating);
                sources.send_feedback(&account.token, &answer)
            },
            |e, sent: Result<bool, crate::feedback::Failure>| {
                if matches!(sent, Err(crate::feedback::Failure::SignInAgain)) {
                    crate::feedback::forget(); // its sign-in ended: asked for again next time
                }
                if sent.is_ok() {
                    crate::feedback::sent();
                    crate::log::note("feedback: sent");
                }
                e.set_rating(|r| {
                    r.busy.clear();
                    r.outcome = Some(match sent {
                        Ok(share) => RateOutcome::Sent { share },
                        Err(f) => RateOutcome::Failed(f.words()),
                    });
                });
            },
        );
    }

    fn sign_out(self: &Rc<Self>) {
        let account = self.shared.sources.account();
        crate::feedback::forget();
        self.set_rating(|r| r.account = None);
        if let Some(a) = account {
            self.bg(move || crate::feedback::sign_out_there(&a.token), |_, ()| {});
        }
    }

    // ------------------------------------------------------------------ updates (stage 4a)

    fn set_update(&self, f: impl FnOnce(&mut UpdateState)) {
        self.shared.view(|v| f(&mut v.update));
        self.shared.tell(Change::Update);
    }

    /// Looks for a newer version, as the GTK app's look_for_updates: old downloads cleared (not
    /// when asked), the newest release read; offline, the last one found is offered again.
    fn look_for_updates(self: &Rc<Self>, asked: bool) {
        if matches!(self.shared.view(|v| v.update.step.clone()), UpdateStep::Downloading(_) | UpdateStep::Restarting) {
            return;
        }
        let (sources, install) = (self.shared.sources.clone(), self.install.borrow().clone());
        self.bg(
            move || {
                if !asked {
                    crate::update::clean();
                }
                sources.latest_release(&install)
            },
            move |e, found: Result<crate::update::Release, String>| {
                let here = crate::update::version_here();
                match found {
                    Ok(r) => {
                        crate::update::looked(&r);
                        if crate::update::newer(&r.version, &here) {
                            e.offer(r, asked);
                        } else {
                            e.set_update(|u| {
                                u.step = UpdateStep::None;
                                u.release = None;
                                u.asks = false;
                            });
                            if asked {
                                e.toast(radiance_core::update_words::newest(&here));
                            }
                        }
                    }
                    Err(why) => {
                        crate::log::note(&format!("looking for updates: {why}"));
                        if asked {
                            e.toast(radiance_core::update_words::couldnt_look(&why));
                        } else if let Some(r) = crate::update::remembered().filter(|r| crate::update::newer(&r.version, &here)) {
                            e.offer(r, false);
                        }
                    }
                }
            },
        );
    }

    fn offer(&self, r: crate::update::Release, asked: bool) {
        let ready = crate::update::ready(&r, &self.install.borrow());
        let asks = asked || crate::update::asks(&r.version);
        self.set_update(|u| {
            u.step = if ready { UpdateStep::Ready } else { UpdateStep::Out };
            u.release = Some(r);
            u.asks = asks;
        });
    }

    /// Update Now: downloads it on a worker (the music plays on), how far along as it goes.
    fn update_now(self: &Rc<Self>) {
        let Some(r) = self.shared.view(|v| v.update.release.clone()) else { return };
        let install = self.install.borrow().clone();
        if matches!(install, crate::update::Install::Other | crate::update::Install::Snap) {
            return; // (the app opens the release's page)
        }
        self.update_stop.store(false, Ordering::SeqCst);
        self.set_update(|u| {
            u.step = UpdateStep::Downloading(0.0);
            u.asks = true;
        });
        let (sources, shared, stop) = (self.shared.sources.clone(), self.shared.clone(), self.update_stop.clone());
        self.bg(
            move || {
                let mut said = -1i64;
                let mut progress = |have: u64, of: u64| {
                    let done = if of > 0 { have as f64 / of as f64 } else { 0.0 };
                    if (done * 100.0) as i64 != said {
                        said = (done * 100.0) as i64;
                        shared.view(|v| {
                            if matches!(v.update.step, UpdateStep::Downloading(_)) {
                                v.update.step = UpdateStep::Downloading(done);
                            }
                        });
                        shared.tell(Change::Update);
                    }
                };
                let got = sources.download_update(&r, &install, &stop, &mut progress);
                if got.is_ok() {
                    sources.count_update(&r, &install);
                }
                got
            },
            |e, got: Result<std::path::PathBuf, String>| match got {
                Ok(_) => e.set_update(|u| u.step = UpdateStep::Ready),
                Err(_) if e.update_stop.load(Ordering::SeqCst) => e.set_update(|u| u.step = UpdateStep::Out),
                Err(why) => e.set_update(|u| u.step = UpdateStep::Failed(format!("Couldn't update: {why}"))),
            },
        );
    }

    /// RADIANCE_SHOT_UPDATE=out|downloading|ready|failed: the pop-up at that step with a made-up beta
    /// release (screenshots; nothing is fetched or put on).
    fn shot_update(&self, step: &str) {
        let r = crate::update::Release {
            version: "0.8.0-beta.9".into(),
            notes: "- **Settings**: Playing, Storage and Help\n- How to Use It and What's New\n- Report a Problem\n- Rate Radiance Music\n- (fifth)".into(),
            page: "https://github.com/RadianceADM/radiance-music/releases/latest".into(),
            url: "https://example.invalid/RadianceMusic-Beta-Setup.exe".into(),
            size: 149_000_000,
            sha256: String::new(),
        };
        let step = match step {
            "downloading" => UpdateStep::Downloading(0.42),
            "ready" => UpdateStep::Ready,
            "failed" => UpdateStep::Failed("Couldn't update: couldn't reach GitHub (offline)".into()),
            _ => UpdateStep::Out,
        };
        self.set_update(|u| {
            u.step = step;
            u.release = Some(r);
            u.asks = true;
        });
    }

    fn update_cancel(&self) {
        self.update_stop.store(true, Ordering::SeqCst);
        self.set_update(|u| u.step = UpdateStep::Out);
    }

    fn update_later(&self) {
        crate::update::remind_later();
        self.set_update(|u| u.asks = false);
    }

    fn update_skip(&self) {
        if let Some(r) = self.shared.view(|v| v.update.release.clone()) {
            crate::update::skip(&r.version);
        }
        self.set_update(|u| u.asks = false);
    }

    /// Restart to Update: where you are saved, then the installer arranged for once this app has
    /// closed (the app quits on Restarting); a download gone since is fetched again.
    fn update_restart(self: &Rc<Self>) {
        let Some(r) = self.shared.view(|v| v.update.release.clone()) else { return };
        let install = self.install.borrow().clone();
        let file = crate::update::file(&r, &install);
        if !file.exists() {
            return self.update_now();
        }
        self.set_update(|u| u.step = UpdateStep::Restarting);
        self.save_session();
        match self.shared.sources.put_on_update(&install, &file) {
            Ok(()) => {
                crate::log::note(&format!("update: {} goes on once this app has closed", r.version));
                self.set_update(|u| u.quit = true);
            }
            Err(why) => self.set_update(|u| u.step = UpdateStep::Failed(format!("Couldn't update: {why}"))),
        }
    }

    fn retry_setup(self: &Rc<Self>) {
        if matches!(self.shared.view(|v| v.setup.clone()), SetupState::Failed { .. } | SetupState::Offline) {
            self.set_splitter_up();
        }
    }

    fn set_split_cores(&self, n: u32) {
        let cores = crate::setup::cores() as u32;
        if let Some(local) = self.settings.borrow_mut().local_split.as_mut() {
            local.threads = n.clamp(1, cores.max(1));
        }
        settings::save(&self.settings.borrow());
    }

    /// A song's split is done: its mark (the playing song switches to its parts: Task 6).
    fn split_landed(self: &Rc<Self>, t: &Track, mark: SplitMark) {
        crate::log::note(&format!("“{}” split ({})", t.title, if mark == SplitMark::Two { "in two" } else { "in four" }));
        self.shared.view(|v| {
            v.split_done.insert(t.key(), mark);
        });
        if t.source == Source::Server {
            self.rms_share_parts(t); // a server's own song: its parts to it (the Linux app's split_landed)
        } else {
            // rule 2 (owner, 2026-11-09): up to the servers joined, with its parts, by itself
            let saved = self.liked.borrow().iter().find(|x| x.key() == t.key() && x.file.is_some()).cloned();
            if let Some(saved) = saved.or_else(|| t.file.is_some().then(|| t.clone())) {
                crate::uploads::enqueue(&saved);
                crate::uploads::kick();
            }
        }
        // the song playing: on with its parts from where it's got to (fading over), its lyrics again
        // (its words may be timed now)
        let cur = self.current.borrow().clone();
        if cur.as_ref().is_some_and(|c| c.key() == t.key()) {
            self.load_lyrics(t, self.play_gen.get());
        }
        // (only while it plays: paused, or last time's song shown at the start, it's left alone, as GTK's
        // split_landed; the next slider move picks the parts up. Final review C1: it started by itself.)
        let playing = self.player.borrow().as_ref().is_some_and(|p| p.playing());
        if let Some(cur) = cur.filter(|c| c.key() == t.key() && !c.video && playing) {
            let at = self.player.borrow().as_ref().and_then(|p| p.position()).unwrap_or(0.0);
            if self.play_parts(&cur, at, true) {
                self.publish_now();
                // a voice was asked for before it was split: it's heard now (GTK's words)
                let v = self.voice.get();
                if v != Voice::Song {
                    self.toast(format!("“{}”: {}", t.title, v.label().to_lowercase()));
                }
            }
        }
    }

    /// Every liked (saved) song gets split, one after another, in the background.
    fn auto_split_liked(self: &Rc<Self>) {
        if std::env::var_os("RADIANCE_SHOT").is_some() {
            return; // a screenshot copy splits nothing
        }
        let liked = split_queue::auto_list(&self.liked.borrow(), split::done);
        for t in liked {
            self.queue_split(&t, false);
        }
    }

    fn publish_splits(&self) {
        let (can_split, can_split_two) = (self.shared.sources.can_split(), self.can_split_two());
        let paused = Self::split_paused();
        let q = self.split_q.borrow();
        let now = q.now().cloned();
        let waiting = q.waiting().to_vec();
        drop(q);
        let two_now = self.splitting_two.borrow().clone();
        let two_waiting: Vec<Track> = self.two_q.borrow().iter().map(|(t, _)| t.clone()).collect();
        let away = self.rms_away_rows();
        self.shared.view(|v| {
            let phase = v.splits.now.as_ref().filter(|(t, _)| now.as_ref().is_some_and(|n| n.key() == t.key())).map(|(_, p)| *p).unwrap_or(Phase::Splitting(0.0));
            v.splits = SplitsState { now: now.map(|t| (t, phase)), waiting, two_now, two_waiting, paused, can_split, can_split_two, away };
        });
        self.shared.tell(Change::Splits);
        self.publish_now(); // the Mixer's "Splitting for No vocals…" comes and goes with the song's split
    }

    /// What's really playing, in the Linux app's words (core's quality): the video's, or the
    /// song's as decoded once its sound flows (before that, what it is).
    fn update_quality(&self) {
        let Some(t) = self.current.borrow().clone() else {
            self.quality.borrow_mut().clear();
            return;
        };
        let player = self.player.borrow();
        if let Some(heard) = player.as_ref().and_then(|p| p.heard_parts()) {
            *self.quality.borrow_mut() = quality::text(heard);
            return;
        }
        let text = if player.as_ref().is_some_and(|p| p.is_video()) || (t.video && t.file.is_none()) {
            quality::text(Heard::Video { height: self.video_height.get(), kbps: self.kbps.get() })
        } else {
            let flac = t.lossless() || t.file.as_deref().is_some_and(|f| f.ends_with(".flac"));
            match player.as_ref().and_then(|p| p.output()) {
                Some((rate, bits)) if flac => quality::text(Heard::Flac { bits: &bits, rate }),
                None if flac => quality::text(Heard::Flac { bits: "", rate: 0 }),
                _ if t.source == Source::Server && t.file.is_none() => quality::text(Heard::Server { format: &t.format }),
                _ => quality::text(Heard::Opus { kbps: self.kbps.get() }),
            }
        };
        *self.quality.borrow_mut() = text;
    }

    fn toggle_like(self: &Rc<Self>, key: &str) {
        let Some(t) = self.find_track(key) else { return };
        let like = liking::toggle(&mut self.liked.borrow_mut(), &t);
        match like {
            Like::Liked { save: true } => self.save_liked(t),
            Like::Liked { save: false } => self.toast(words::liked_toast(&t.title)),
            Like::Unliked { gone } => {
                self.toast(words::unliked_toast(&gone.title, gone.file.is_some()));
                self.bg(move || crate::library::unsave(&gone), |_, _| {});
            }
        }
        self.liked_changed();
    }

    /// A liked song saved in the background (one save per song at a time). Taken out of Liked
    /// while it was saving: what was saved goes again, and nothing is said.
    fn save_liked(self: &Rc<Self>, t: Track) {
        let key = t.key();
        if !self.saving.borrow_mut().insert(key.clone()) {
            return;
        }
        self.publish_liked();
        let sources = self.shared.sources.clone();
        self.bg(
            move || {
                let saved = liking::save(&t, sources.as_ref());
                (t, saved)
            },
            move |e, (mut t, saved)| {
                e.saving.borrow_mut().remove(&key);
                match saved {
                    Ok(path) => {
                        let kept = e.liked.borrow_mut().iter_mut().find(|x| x.key() == key).map(|x| x.file = Some(path.clone())).is_some();
                        if kept {
                            crate::library::store(&e.liked.borrow());
                            e.toast(words::saved_toast(&t.title));
                            // liked songs get split too (the GTK app's, once saved)
                            let saved = e.liked.borrow().iter().find(|x| x.key() == key).cloned();
                            if let Some(saved) = saved {
                                e.queue_split(&saved, false);
                            }
                        } else {
                            t.file = Some(path);
                            crate::library::unsave(&t);
                        }
                    }
                    // a build that only streams YouTube songs, and no joined server had it: liked, that's all
                    Err(err) if err == crate::library::STREAMS_ONLY => e.toast(words::liked_toast(&t.title)),
                    Err(err) => {
                        crate::log::note(&format!("save {}: {err}", t.title));
                        e.toast(words::save_failed(&t.title));
                    }
                }
                e.liked_changed();
            },
        );
    }

    fn play_again(self: &Rc<Self>, key: &str) {
        let Some(t) = self.find_track(key) else { return };
        if self.current.borrow().is_none() {
            self.play_list(vec![t], 0);
            return;
        }
        self.restored.set(false);
        self.restore_at.set(None);
        let at = self.queue.borrow_mut().insert_next(t);
        self.queue.borrow_mut().set_index(at);
        self.play_current();
        self.publish_queue();
        self.save_session();
    }

    fn play_next(self: &Rc<Self>, key: &str) {
        let Some(t) = self.find_track(key) else { return };
        if self.current.borrow().is_none() {
            self.play_list(vec![t], 0);
            return;
        }
        self.queue.borrow_mut().insert_next(t.clone());
        self.publish_queue();
        self.prefetch_next();
        self.toast(format!("“{}” plays next", t.title));
    }

    fn enqueue(self: &Rc<Self>, key: &str) {
        let Some(t) = self.find_track(key) else { return };
        if self.current.borrow().is_none() {
            self.play_list(vec![t], 0);
            return;
        }
        self.queue.borrow_mut().push(t.clone());
        self.publish_queue();
        self.prefetch_next();
        self.toast(format!("Added “{}” to the queue", t.title));
    }

    fn unqueue(self: &Rc<Self>, i: usize, key: &str) {
        if self.queue.borrow().get(i).is_none_or(|t| t.key() != key) {
            return; // the queue moved since: not that song any more
        }
        let removed = self.queue.borrow_mut().remove(i);
        match removed {
            None => return,
            // the song playing: on to the next one (now at the same place)
            Some(queue::Removed::Current { play: true }) => {
                self.restored.set(false);
                self.restore_at.set(None);
                self.play_current();
            }
            Some(queue::Removed::Current { play: false }) => {
                self.restored.set(false);
                self.restore_at.set(None);
                self.let_go_of_video();
                if let Some(p) = self.player.borrow().as_ref() {
                    p.stop();
                }
                *self.current.borrow_mut() = None;
                self.publish_now();
            }
            Some(queue::Removed::Before | queue::Removed::After) => {}
        }
        self.publish_queue();
    }

    fn move_in_queue(self: &Rc<Self>, from: usize, to: usize, key: &str) {
        if self.queue.borrow().get(from).is_none_or(|t| t.key() != key) {
            return;
        }
        let playing = self.current.borrow().is_some();
        // (never above the song playing; not moved: where it was)
        if self.queue.borrow_mut().move_item(from, to, playing) {
            self.publish_queue();
            self.prefetch_next();
        }
    }

    fn start_radio(self: &Rc<Self>, key: &str) {
        let Some(seed) = self.find_track(key) else { return };
        self.toast(format!("Starting “{}” radio…", seed.title));
        let sources = self.shared.sources.clone();
        let s = seed.clone();
        self.bg(move || sources.radio(&s), move |e, list| {
            if list.is_empty() {
                e.toast(format!("YouTube Music has no radio for “{}”", seed.title));
                return;
            }
            let mut queue = vec![seed.clone()];
            queue.extend(list.into_iter().filter(|x| x.key() != seed.key() && !x.same_song(&seed) && !e.often_skipped(x)));
            let n = queue.len() - 1;
            if e.current.borrow().as_ref().is_some_and(|c| c.key() == seed.key()) {
                e.queue.borrow_mut().set_tracks(queue, 0); // it's playing already: just what comes after
                e.publish_queue();
                e.prefetch_next();
            } else {
                e.play_list(queue, 0);
            }
            e.toast(format!("“{}” radio: {n} songs like it", seed.title));
        });
    }

    /// The song's saved file shown in the file manager (Windows: Explorer, the file selected).
    fn show_in_files(&self, key: &str) {
        let Some(file) = self.find_track(key).and_then(|t| t.file).filter(|f| std::path::Path::new(f).exists()) else { return };
        #[cfg(windows)]
        let _ = {
            use std::os::windows::process::CommandExt;
            std::process::Command::new("explorer").raw_arg(explorer_select(&file)).spawn()
        };
        #[cfg(not(windows))]
        if let Some(dir) = std::path::Path::new(&file).parent() {
            let _ = gio::AppInfo::launch_default_for_uri(&gio::prelude::FileExt::uri(&gio::File::for_path(dir)), gio::AppLaunchContext::NONE);
        }
    }

    fn add_to_playlist(&self, id: i64, key: &str) {
        let Some(t) = self.find_track(key) else { return };
        let Some(name) = crate::db::playlists().into_iter().find(|(i, _, _)| *i == id).map(|p| p.1) else { return };
        let added = crate::db::add_to_playlist(id, &t);
        self.toast(if added { words::added_to(&name) } else { words::already_in(&name) });
        self.shared.tell(Change::Playlists);
    }

    fn fix_missing(self: &Rc<Self>) {
        let (songs, videos) = liking::missing(&self.liked.borrow());
        let n = songs.len();
        songs.into_iter().for_each(|t| self.save_liked(t));
        self.toast(words::fixing_toast(n, videos));
    }

    fn say(&self, text: String, retry: bool) {
        self.shared.view(|v| v.problem = Some(Problem { text, retry }));
        self.shared.tell(Change::Problem);
    }

    fn toggle_play(self: &Rc<Self>) {
        if self.in_party() {
            self.party_play_pause();
            return;
        }
        if self.current.borrow().is_none() {
            return;
        }
        if self.restored.replace(false) {
            self.play_current(); // last time's song, from where it was
            return;
        }
        let paused = if let Some(p) = self.player.borrow().as_ref() {
            let was = p.playing();
            p.set_playing(!was);
            was
        } else {
            false
        };
        self.publish_now();
        if paused {
            self.rating_moment(); // a pause is a free moment (GTK's toggle_play)
        }
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
                self.publish_counts();
            }
        }
    }

    fn skip(self: &Rc<Self>, by: i32) {
        if self.in_party() {
            self.party_skip(by);
            return;
        }
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
                // another song than the one picked up: from its start, and Play/Pause as usual
                self.restored.set(false);
                self.restore_at.set(None);
                self.play_current();
            }
        }
    }

    fn seek(self: &Rc<Self>, at: f64) {
        if self.in_party() {
            self.party_seek(at);
            return;
        }
        self.drop_cue(); // (the player's seek resets the other deck: cued again from here)
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
        self.muted.set(false); // a new level unmutes, as the GTK app's slider
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
        self.publish_history();
        self.publish_counts();
        self.publish_queue(); // Played before
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
                        LyricsState::Lines(radiance_core::sing::prepare(lines))
                    }
                    Lyrics::Plain(text) => LyricsState::Plain(text),
                    Lyrics::None => LyricsState::None,
                };
                e.lyrics_offline.set(matches!(state, LyricsState::None) && !e.shared.view(|v| v.online));
                e.shared.view(|v| v.lyrics = state);
                e.shared.tell(Change::Lyrics);
            },
        );
    }

    /// The background's beat for the song playing (#56): worked out once per song, a few seconds in (so a
    /// stream's start isn't slowed by it), as the GTK app's load_pulse.
    fn load_pulse(&self) {
        let Some(t) = self.current.borrow().clone() else { return };
        let (key, uri) = (t.key(), self.current_uri.borrow().clone());
        if t.video || uri.is_empty() || *self.pulse_for.borrow() == key {
            return;
        }
        self.pulse_for.replace(key.clone());
        self.bg(
            move || crate::beat::pulse(&uri),
            move |e, p| {
                if *e.pulse_for.borrow() == key {
                    e.shared.view(|v| v.pulse = p.map(|p| (key, Arc::new(p))));
                    e.shared.tell(Change::Pulse);
                }
            },
        );
    }

    /// Five times a second while a song plays.
    fn tick(self: &Rc<Self>) {
        let (playing, at, length, mixing) = {
            let player = self.player.borrow();
            let Some(p) = player.as_ref() else { return };
            (p.playing(), p.position(), p.duration(), p.mixing())
        };
        if self.shared.view(|v| std::mem::replace(&mut v.mixing, mixing)) != mixing {
            self.publish_now();
        }
        if !playing {
            return;
        }
        // never silent after a crossfade (GTK's tick: a deck left at the fade's volume is brought back)
        if let Some(p) = self.player.borrow().as_ref() {
            p.heal_volume();
        }
        let Some(at) = at else { return };
        self.position.set(at);
        if at >= 3.0 {
            self.load_pulse();
        }
        let lit = radiance_core::sing::lit_lines(self.lines.borrow().iter(), at);
        self.shared.view(|v| {
            v.now.at = at;
            v.now.since = clock_ms(); // the reading's own time: the beta counts on from it
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
        if !self.in_party() {
            self.transitions(at); // (a party's songs follow the server, never a mix)
        }
    }

    // ------------------------------------------------------------------ between songs (stage 4a-2, GTK's transitions)

    /// Crossfade or DJ mix (GTK's transitions, ui.rs): the next song cued up to CUE_AHEAD_S before the plan's start,
    /// the mix started on the dot. None for a short song, a video, Repeat One or the queue's end (no next song).
    fn transitions(self: &Rc<Self>, at: f64) {
        let mode = self.settings.borrow().transition;
        let (len, mixing) = {
            let player = self.player.borrow();
            let Some(p) = player.as_ref() else { return };
            (p.duration().unwrap_or(0.0), p.mixing())
        };
        if mode == Transition::Off || len < MIX_MIN_S || mixing {
            return;
        }
        let Some(cur) = self.current.borrow().clone() else { return };
        let next = self.queue.borrow().next().cloned();
        let Some(next) = next else {
            self.drop_cue();
            return;
        };
        if cur.video || next.video {
            return; // a video just ends; the next one starts
        }
        let next = playing::saved_copy(&self.liked.borrow(), &next);
        let (a, b) = (cur.key(), next.key());
        // a cue for a song that isn't next any more goes (a song put next, the queue moved: review focus 1)
        if self.cued.borrow().as_ref().is_some_and(|(k, _, _)| *k != b) {
            self.drop_cue();
        }
        let fade = self.settings.borrow().fade;
        if mode == Transition::DjMix {
            self.analyse_pair(&cur, &next, len, fade);
        }
        let plan = match mode {
            Transition::DjMix => self.mix_plan.borrow().as_ref().filter(|(x, y, _)| *x == a && *y == b).map(|(_, _, p)| *p).unwrap_or_else(|| beat::plan(None, None, len, fade)),
            _ => beat::plan(None, None, len, fade),
        };
        if at > plan.at + plan.seconds / 2.0 {
            return; // jumped past it: this song just ends
        }
        // the DJ plan came after a crossfade's cue (or the other way): cued again to match (review focus 5)
        if !self.mix_armed.get() && self.cued.borrow().as_ref().is_some_and(|(_, bm, _)| *bm != plan.beat_matched) {
            self.drop_cue();
        }
        if plan.at - at <= CUE_AHEAD_S && self.cued.borrow().is_none() {
            self.cue_next(&next, &plan);
        }
        let ready = self.player.borrow().as_ref().is_some_and(|p| p.cued());
        if ready && self.cued.borrow().is_some() && plan.at - at <= ARM_S && !self.mix_armed.get() {
            self.mix_armed.set(true);
            let wait = Duration::from_secs_f64((plan.at - at).max(0.0));
            let (me, gen) = (Rc::downgrade(self), self.play_gen.get());
            glib::timeout_add_local_once(wait, move || {
                let Some(e) = me.upgrade() else { return };
                e.mix_armed.set(false);
                if e.play_gen.get() == gen {
                    e.begin_mix(plan);
                }
            });
        }
    }

    /// The pair (this song, the next) analysed for a DJ mix in the background, once (its plan kept in memory).
    fn analyse_pair(self: &Rc<Self>, cur: &Track, next: &Track, len: f64, fade: f64) {
        let pair = (cur.key(), next.key());
        let known = self.mix_plan.borrow().as_ref().is_some_and(|(x, y, _)| *x == pair.0 && *y == pair.1);
        if known || self.analysing.borrow().as_ref() == Some(&pair) {
            return;
        }
        let here = self.current_uri.borrow().clone();
        let a_uri = if here.is_empty() { self.uri_for(cur) } else { Some(here) };
        let (Some(a_uri), Some(b_uri)) = (a_uri, self.uri_for(next)) else { return };
        *self.analysing.borrow_mut() = Some(pair.clone());
        self.bg(
            move || {
                let x = beat::analyse(&a_uri, (len - 45.0).max(0.0), 45.0);
                let y = beat::analyse(&b_uri, 0.0, 40.0);
                beat::plan(x, y, len, fade)
            },
            move |e, plan| {
                if e.analysing.borrow().as_ref() == Some(&pair) {
                    crate::log::note(&format!("DJ mix planned: {}", if plan.beat_matched { "beats lined up" } else { "a crossfade (no clear beat, or the tempos too far apart)" }));
                    *e.mix_plan.borrow_mut() = Some((pair.0, pair.1, plan));
                    e.analysing.replace(None);
                }
            },
        );
    }

    /// Where a song plays from, for its cue or its analysis (as play_current would start it): its saved file, the
    /// Archive's file through the relay, a server's file, a YouTube song's stream once found (none yet: looked up).
    fn uri_for(self: &Rc<Self>, t: &Track) -> Option<String> {
        use gio::prelude::*;
        if let Some(f) = t.file.as_deref().filter(|f| std::path::Path::new(f).exists()) {
            return Some(gio::File::for_path(f).uri().to_string());
        }
        let uri = match t.source {
            Source::Archive => Some(playing::web_uri(self.relay.as_ref(), &t.id)),
            Source::Server => playing::plain_uri(t, self.relay.as_ref(), &self.streams.borrow()),
            Source::YouTube => self.streams.borrow().fresh(&t.id).map(|(u, _)| u),
        };
        if uri.is_none() {
            self.prefetch_next();
        }
        uri
    }

    /// The next song cued silently for the mix: with its parts (and the sliders as they are) when it has them and it
    /// isn't a beat-matched DJ mix (that's deck to deck), else on the other deck.
    fn cue_next(self: &Rc<Self>, next: &Track, plan: &beat::Plan) {
        let (parts, whole, rate) = self.parts_of(next);
        let uri = match whole.filter(|_| !parts.is_empty() && !plan.beat_matched) {
            Some(whole) => {
                if let Some(p) = self.player.borrow().as_ref() {
                    p.cue_parts(&whole, &parts, plan.from, &self.gains_for(next), rate);
                }
                whole
            }
            None => {
                let Some(uri) = self.uri_for(next) else { return };
                if let Some(p) = self.player.borrow().as_ref() {
                    p.cue(&uri, plan.from, plan.rate, plan.beat_matched);
                }
                uri
            }
        };
        *self.cued.borrow_mut() = Some((next.key(), plan.beat_matched, uri));
    }

    /// The mix starts (GTK's begin_mix): the cued song comes in over `plan.seconds`, and the queue moves on to it.
    fn begin_mix(self: &Rc<Self>, plan: beat::Plan) {
        let Some((key, _, uri)) = self.cued.borrow().clone() else { return };
        let next = self.queue.borrow().next().cloned();
        let Some(next) = next.filter(|t| t.key() == key) else {
            self.replan();
            return;
        };
        let started = {
            let player = self.player.borrow();
            let Some(p) = player.as_ref() else { return };
            p.retune_cued(&self.gains_for(&next)); // the sliders as they are now
            p.mix(plan.seconds, plan.beat_matched);
            p.mixing()
        };
        if !started {
            return;
        }
        self.cued.replace(None);
        self.mix_plan.replace(None);
        if !matches!(self.queue.borrow_mut().skip(1, 0.0), Step::Play(_)) {
            return;
        }
        self.mixed_into(next, uri);
    }

    /// The song mixed into is the one playing now: play_current's bookkeeping without loading it (it's already on),
    /// and Autoplay when it's the queue's last (GTK's begin_mix never asked: review focus 3).
    fn mixed_into(self: &Rc<Self>, t: Track, uri: String) {
        let t = playing::saved_copy(&self.liked.borrow(), &t);
        let gen = self.play_gen.get() + 1;
        self.play_gen.set(gen);
        self.kbps.set(0);
        self.position.set(0.0);
        self.restored.set(false);
        self.restore_at.set(None);
        self.ran_out.set(false);
        *self.current.borrow_mut() = Some(t.clone());
        self.switching.replace(None);
        self.load_lyrics(&t, gen);
        self.note_played(&t);
        self.find_counterpart(&t);
        self.video_height.set(0);
        self.current_uri.replace(uri);
        if !self.pulse_for.replace(String::new()).is_empty() {
            self.shared.view(|v| v.pulse = None);
            self.shared.tell(Change::Pulse);
        }
        self.loading.set(false);
        self.update_quality();
        crate::log::note(&format!("mixed into “{}”", t.title));
        if self.voice.get() != Voice::Song {
            if !t.video && split::mix_parts(&t).is_empty() {
                self.split_fast(&t, true);
            }
            self.split_next_ahead();
        }
        self.publish_queue();
        self.publish_now();
        self.prefetch_next();
        self.autoplay_if_last();
    }

    /// Plans and cues start again (the song, the queue, the voice or the setting changed: GTK's replan).
    fn replan(&self) {
        self.mix_plan.replace(None);
        self.analysing.replace(None);
        self.drop_cue();
    }

    fn drop_cue(&self) {
        if self.cued.replace(None).is_some() {
            if let Some(p) = self.player.borrow().as_ref() {
                p.drop_cue();
            }
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
        let video = self.player.borrow().as_ref().is_some_and(|p| p.is_video());
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
            muted: self.muted.get(),
            lossless: song.as_ref().is_some_and(|t| t.lossless()),
            kbps: self.kbps.get(),
            lit,
            liked: song.as_ref().is_some_and(|t| self.liked.borrow().iter().any(|x| x.key() == t.key())),
            since: clock_ms(),
            video,
            has_counterpart: song.as_ref().is_some_and(|t| self.counterpart.borrow().as_ref().is_some_and(|(k, _)| *k == t.key())),
            quality: self.quality.borrow().clone(),
            loading: self.loading.get() && song.is_some(),
            mix: self.mix_state(),
            song,
        };
        drop(q);
        // in a party: Next skips for everyone (when you control it), there's no going back, and Play/Pause shows the
        // party's (not this computer's short waits to stay in step)
        let mut now = now;
        if let Some((control, song, heard)) = self.party_bar() {
            now.can_previous = false;
            now.can_next = control && song;
            now.playing = heard;
        }
        let swapchain = if video { self.swapchain.get() } else { 0 };
        self.shared.view(|v| {
            v.now = now;
            // held while it's shown (the sink is alive: this engine thread is the one that stops it)
            if v.video.as_ref().map_or(0, |r| r.ptr()) != swapchain {
                v.video = (swapchain != 0).then(|| unsafe { crate::comref::ComRef::take(swapchain) });
            }
        });
        self.shared.tell(Change::Now);
    }

    fn publish_queue(&self) {
        let q = self.queue.borrow();
        let queue = (q.tracks().to_vec(), q.index());
        let current = self.current.borrow().as_ref().map(|t| t.key());
        let playing = current.is_some();
        let ahead = q.len().saturating_sub(q.index() + usize::from(playing));
        let adding = self.settings.borrow().autoplay && q.repeat() == Repeat::Off;
        let mut before: Vec<Track> = crate::session::recent(&self.history.borrow()).into_iter().filter(|t| Some(t.key()) != current).take(shelf::RECENT_ROWS).collect();
        before.reverse();
        let panel = QueuePanel {
            before,
            rows: q.tracks().iter().skip(q.index()).take(100).cloned().collect(),
            start: q.index(),
            now: playing,
            note: shelf::queue_note(q.is_empty() || (!playing && ahead == 0), ahead, adding),
        };
        drop(q);
        self.shared.view(|v| {
            v.queue = queue;
            v.panel = panel;
        });
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
                        e.set_online_by(reached, true);
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

    fn set_online(self: &Rc<Self>, on: bool) {
        self.set_online_by(on, false);
    }

    /// The internet came or went (`by_search`: a search found it; it isn't run again then).
    fn set_online_by(self: &Rc<Self>, on: bool, by_search: bool) {
        let changed = self.shared.view(|v| std::mem::replace(&mut v.online, on) != on);
        if changed {
            self.shared.tell(Change::Online);
        }
        // back online, as the GTK app's: it says so, the search runs again, the fast split's files come
        if changed && on {
            crate::log::note("back online");
            self.toast(radiance_core::words::BACK_ONLINE.to_string());
            let q = self.query.borrow().clone();
            if !by_search && !q.trim().is_empty() {
                self.search(q);
            }
            self.fetch_two_part();
            crate::uploads::kick(); // rule 2: what waited goes up now
        }
        // back online: the song's lyrics looked up again (none came offline), as the GTK app's
        if changed && on && self.lyrics_offline.replace(false) {
            if let Some(t) = self.current.borrow().clone() {
                self.load_lyrics(&t, self.play_gen.get());
            }
        }
        // back online: splitting's first-start download starts now (it waited for the internet)
        if changed && on && self.shared.view(|v| v.setup == SetupState::Offline) {
            self.set_splitter_up();
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

/// Explorer's argument selecting `file`: the path in its own quotes (Rust's would wrap the whole
/// argument, which Explorer doesn't read: a path with spaces opened Documents instead).
#[cfg_attr(not(windows), allow(dead_code))]
fn explorer_select(file: &str) -> String {
    format!("/select,\"{file}\"")
}

#[cfg(test)]
mod tests {
    /// Explorer's /select takes the path in its own quotes: a path with spaces (most songs')
    /// selects that file (review I8).
    #[test]
    fn explorer_selects_a_path_with_spaces() {
        assert_eq!(super::explorer_select(r"C:\Music\Radiance Music\A B - Song.flac"), r#"/select,"C:\Music\Radiance Music\A B - Song.flac""#);
    }
}

/// How often a paused split queue looks again (the pause is a file other copies can lift; 30 s, or
/// RADIANCE_SPLIT_PAUSE_POLL_MS in the tests).
fn pause_poll() -> Duration {
    std::env::var("RADIANCE_SPLIT_PAUSE_POLL_MS").ok().and_then(|v| v.parse().ok()).map(Duration::from_millis).unwrap_or(Duration::from_secs(30))
}

/// How long after the start updates are looked for (the GTK app's 8 s; RADIANCE_UPDATE_AFTER_MS: tests).
/// Between songs (GTK's transitions): no mix for a song shorter than this; the next song cued at most this long
/// before the mix; the mix's start timed once it's this close.
const MIX_MIN_S: f64 = 40.0;
const CUE_AHEAD_S: f64 = 12.0;
const ARM_S: f64 = 0.45;

fn update_after() -> Duration {
    std::env::var("RADIANCE_UPDATE_AFTER_MS").ok().and_then(|v| v.parse().ok()).map(Duration::from_millis).unwrap_or(Duration::from_secs(8))
}

/// What can be split for `t` now (the song menu's and the Mixer's split choices).
fn split_ability(v: &View, t: &Track) -> menu::SplitAbility {
    let key = t.key();
    menu::SplitAbility {
        online: v.online,
        parts: split::mix_parts(t).len(),
        can_two: v.splits.can_split_two,
        can_four: v.splits.can_split,
        busy_two: v.splits.two_now.as_ref().is_some_and(|x| x.key() == key) || v.splits.two_waiting.iter().any(|x| x.key() == key),
        busy_four: v.splits.now.as_ref().is_some_and(|(x, _)| x.key() == key) || v.splits.waiting.iter().any(|x| x.key() == key),
    }
}

/// Fix missing's label (None: nothing missing) and whether a song is saving now.
fn fix_state(liked: &[(Track, Saved)]) -> (Option<String>, bool) {
    let tracks: Vec<Track> = liked.iter().map(|(t, _)| t.clone()).collect();
    let (missing, videos) = liking::missing(&tracks);
    let n = missing.len().max(videos);
    let fixing = liked.iter().any(|(_, s)| *s == Saved::Saving);
    ((n > 0).then(|| words::fix_label(n, fixing)), fixing)
}

/// How long getting splitting ready waits before trying again (GTK's 30 s; RADIANCE_SETUP_RETRY_MS: the tests').
fn setup_retry() -> Duration {
    std::env::var("RADIANCE_SETUP_RETRY_MS").ok().and_then(|v| v.parse().ok()).map(Duration::from_millis).unwrap_or(Duration::from_secs(30))
}

/// RADIANCE_SETUP_TEST=act: the first start's download acted out (screenshot checks of the sunrise's
/// progress and its offline buttons); the engine tests' RADIANCE_SETUP_TEST=1 runs the Fake's install.
fn acted_setup() -> bool {
    std::env::var("RADIANCE_SETUP_TEST").as_deref() == Ok("act")
}

/// How many parts a server's song has on its server (as last sent).
fn servers_parts(t: &Track) -> usize {
    crate::servers::mix_parts(t).len()
}
