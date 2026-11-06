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
use radiance_core::trouble::{self, After};
use radiance_core::words::{self, Kind};

use crate::liking::{self, Like};
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
        let tracks: Vec<Track> = liked.iter().map(|(t, _)| t.clone()).collect();
        let (missing, videos) = liking::missing(&tracks);
        let n = missing.len().max(videos);
        let fixing = liked.iter().any(|(_, s)| *s == Saved::Saving);
        LikedPage {
            note: shelf::liked_note(liked.len(), &crate::library::music_dir().display().to_string(), q.trim(), songs.len()),
            letters: shelf::letters(&shown),
            fix: (n > 0).then(|| words::fix_label(n, fixing)),
            fixing,
            songs,
        }
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
    pub fn song_menu(&self, key: &str, place: Place) -> Vec<Vec<Item>> {
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
                .or_else(|| v.now.song.clone().filter(|t| t.key() == key));
            (t, liked)
        });
        // a playlist's song found nowhere else: the playlist's own copy (read outside the view's lock)
        let t = t.or_else(|| match place {
            Place::Playlist(id) => crate::db::playlist_songs(id).into_iter().find(|t| t.key() == key),
            _ => None,
        });
        let Some(t) = t else { return Vec::new() };
        let saved = t.file.as_ref().is_some_and(|f| std::path::Path::new(f).exists());
        let lists: Vec<(i64, String)> = crate::db::playlists().into_iter().map(|(id, name, _)| (id, name)).collect();
        menu::song_menu(&t, place, liked, saved, &lists)
    }

    /// A menu item for the song `key` (`index`: its place in the queue, for Remove from Queue).
    /// Play and New Playlist… are the app's (it knows the list, and asks for the name).
    pub fn do_menu(&self, key: &str, index: usize, action: Action) {
        match action {
            Action::Play | Action::NewPlaylistWith => {}
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
        Engine {
            shared,
            epoch,
            search_gen: Cell::new(0),
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
                self.shared.view(|v| v.problem = Some(Problem { text: format!("The player didn't start: {err}"), retry: false }));
            }
        }
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
        })
    }

    fn play_from(self: &Rc<Self>, list: ListRef, i: usize, key: &str) {
        let tracks = self.list(&list);
        // the song clicked: at its place, or (the list moved since) wherever it is now; gone: nothing
        let Some(i) = tracks.get(i).filter(|t| t.key() == key).map(|_| i).or_else(|| tracks.iter().position(|t| t.key() == key)) else { return };
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
        self.let_go_of_video(); // before the old video's pipeline goes (review I5)
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
        // "Loading…" until its sound flows (the Ready event), as the Linux app's bar says it
        self.loading.set(true);
        *self.quality.borrow_mut() = quality::text(Heard::Loading);
        if video {
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
                if let Some(p) = self.player.borrow().as_ref().filter(|p| p.is_video()) {
                    self.switching.replace(None); // the video plays: no way back needed
                    self.swapchain.set(p.video_swapchain()); // (the sink may make it only once it plays)
                }
                self.loading.set(false);
                self.update_quality();
                self.publish_now();
            }
            Event::Ended if self.queue.borrow().repeat() == Repeat::One => self.play_current(),
            Event::Ended => self.skip(1),
            Event::Error(e) if trouble::sound_device_gone(&e) && self.sound_tries.get() < 3 => self.carry_on(),
            Event::Error(e) => {
                if !self.back_from_video(&e) {
                    self.unavailable(&song, &e);
                }
            }
        }
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

    /// What's really playing, in the Linux app's words (core's quality): the video's, or the
    /// song's as decoded once its sound flows (before that, what it is).
    fn update_quality(&self) {
        let Some(t) = self.current.borrow().clone() else {
            self.quality.borrow_mut().clear();
            return;
        };
        let player = self.player.borrow();
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
                        } else {
                            t.file = Some(path);
                            crate::library::unsave(&t);
                        }
                    }
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
                self.publish_counts();
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
                // another song than the one picked up: from its start, and Play/Pause as usual
                self.restored.set(false);
                self.restore_at.set(None);
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
            song,
        };
        drop(q);
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
