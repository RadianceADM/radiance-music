//! Radiance Music's engine for the Windows app (issue #45): the controller, through uniffi.
//! C# gets a MusicEngine (commands, snapshots as plain records) and tells it an EngineListener;
//! uniffi-bindgen-cs writes the C# side (namespace RadianceMusic.Native, uniffi.toml).

use std::sync::Arc;

use radiance_engine::controller::{self, Change, Controller, LyricsState, ListRef};
use radiance_engine::sources::{Real, Sources};
use radiance_engine::track::Track;

uniffi::setup_scaffolding!();

/// What changed (read it again with the method of the same name).
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ChangeKind {
    Now,
    Position,
    Queue,
    Results,
    Home,
    Lyrics,
    Online,
    Problem,
}

impl From<Change> for ChangeKind {
    fn from(c: Change) -> Self {
        match c {
            Change::Now => ChangeKind::Now,
            Change::Position => ChangeKind::Position,
            Change::Queue => ChangeKind::Queue,
            Change::Results => ChangeKind::Results,
            Change::Home => ChangeKind::Home,
            Change::Lyrics => ChangeKind::Lyrics,
            Change::Online => ChangeKind::Online,
            Change::Problem => ChangeKind::Problem,
        }
    }
}

/// A list on screen that songs play from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ListKind {
    Mine,
    Lossless,
    Songs,
    Mix,
    Recent,
    Liked,
    Queue,
}

impl From<ListKind> for ListRef {
    fn from(l: ListKind) -> Self {
        match l {
            ListKind::Mine => ListRef::Mine,
            ListKind::Lossless => ListRef::Lossless,
            ListKind::Songs => ListRef::Songs,
            ListKind::Mix => ListRef::Mix,
            ListKind::Recent => ListRef::Recent,
            ListKind::Liked => ListRef::Liked,
            ListKind::Queue => ListRef::Queue,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum RepeatMode {
    Off,
    All,
    One,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LyricsKind {
    Looking,
    None,
    Plain,
    Lines,
}

/// A song as the screens show it.
#[derive(Clone, Debug, uniffi::Record)]
pub struct SongView {
    pub key: String,
    pub title: String,
    pub byline: String,
    pub album: String,
    /// Its cover (an address or a file; MusicEngine::art_file makes it a PNG).
    pub art: String,
    /// "4:05".
    pub length: String,
    pub seconds: f64,
    pub lossless: bool,
    pub video: bool,
}

impl From<&Track> for SongView {
    fn from(t: &Track) -> Self {
        SongView {
            key: t.key(),
            title: t.title.clone(),
            byline: t.byline(),
            album: t.album.clone(),
            art: radiance_engine::art::source_of(t),
            length: t.length(),
            seconds: t.seconds as f64,
            lossless: t.lossless(),
            video: t.video,
        }
    }
}

fn views(v: &[Track]) -> Vec<SongView> {
    v.iter().map(SongView::from).collect()
}

/// The song playing and the player bar.
#[derive(Clone, Debug, uniffi::Record)]
pub struct NowView {
    pub song: Option<SongView>,
    pub playing: bool,
    pub at: f64,
    pub length: f64,
    pub can_previous: bool,
    pub can_next: bool,
    pub shuffle: bool,
    pub repeat: RepeatMode,
    pub volume: f64,
    pub lossless: bool,
    pub kbps: u32,
    pub lit: Vec<u32>,
}

/// Search's lists, with the notes the GTK app shows over them (None: still searching).
#[derive(Clone, Debug, uniffi::Record)]
pub struct ResultsView {
    pub query: String,
    pub mine: Vec<SongView>,
    pub mine_note: String,
    pub lossless: Option<Vec<SongView>>,
    pub songs: Option<Vec<SongView>>,
    pub lossless_note: String,
    pub songs_note: String,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct HomeView {
    pub greeting: String,
    pub mix_note: String,
    pub mix: Vec<SongView>,
    pub recent: Vec<SongView>,
    pub liked: Vec<SongView>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct LineView {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct LyricsView {
    pub kind: LyricsKind,
    pub plain: String,
    pub lines: Vec<LineView>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct BrowseTile {
    pub label: String,
    pub query: String,
}

/// Now Playing's moving background: the art's wash and its blob, as PNG files.
#[derive(Clone, Debug, uniffi::Record)]
pub struct BackdropFiles {
    pub wash: String,
    pub blob: String,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct QueueView {
    pub songs: Vec<SongView>,
    pub index: u32,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum EngineError {
    #[error("{0}")]
    Failed(String),
}

/// Told when something changed (on the engine's thread: hand it to the UI thread at once).
#[uniffi::export(with_foreign)]
pub trait EngineListener: Send + Sync {
    fn changed(&self, what: ChangeKind);
}

/// Told when another copy of the app asks this one to show its window (and maybe open a link).
#[uniffi::export(with_foreign)]
pub trait ShowListener: Send + Sync {
    fn show(&self, link: Option<String>);
}

struct Bridge(Arc<dyn EngineListener>);

impl controller::Listener for Bridge {
    fn changed(&self, what: Change) {
        self.0.changed(what.into());
    }
}

/// The engine: commands in, snapshots out (see radiance_engine::controller).
#[derive(uniffi::Object)]
pub struct MusicEngine {
    inner: Controller,
}

impl MusicEngine {
    fn with(listener: Arc<dyn EngineListener>, sources: Arc<dyn Sources>) -> Result<Arc<Self>, EngineError> {
        let inner = Controller::start(Arc::new(Bridge(listener)), sources).map_err(EngineError::Failed)?;
        Ok(Arc::new(MusicEngine { inner }))
    }

    #[cfg(test)]
    fn start_with(listener: Arc<dyn EngineListener>, sources: Arc<dyn Sources>) -> Result<Arc<Self>, EngineError> {
        Self::with(listener, sources)
    }
}

#[uniffi::export]
impl MusicEngine {
    /// Starts the engine (the internet's real sources). Err when one runs already.
    #[uniffi::constructor]
    pub fn start(listener: Arc<dyn EngineListener>) -> Result<Arc<Self>, EngineError> {
        Self::with(listener, Arc::new(Real))
    }

    pub fn search(&self, q: String) {
        self.inner.search(&q);
    }

    pub fn refresh_mix(&self) {
        self.inner.refresh_mix();
    }

    pub fn play_from(&self, list: ListKind, index: u32) {
        self.inner.play_from(list.into(), index as usize);
    }

    pub fn shuffle_from(&self, list: ListKind) {
        self.inner.shuffle_from(list.into());
    }

    pub fn toggle_play(&self) {
        self.inner.toggle_play();
    }

    pub fn skip(&self, by: i32) {
        self.inner.skip(by);
    }

    pub fn seek(&self, at: f64) {
        self.inner.seek(at);
    }

    pub fn set_volume(&self, level: f64) {
        self.inner.set_volume(level);
    }

    pub fn set_shuffle(&self, on: bool) {
        self.inner.set_shuffle(on);
    }

    pub fn cycle_repeat(&self) {
        self.inner.cycle_repeat();
    }

    /// Saves where you are and stops; nothing is told after it returns.
    pub fn quit(&self) {
        self.inner.quit();
    }

    pub fn now(&self) -> NowView {
        let n = self.inner.now();
        NowView {
            song: n.song.as_ref().map(SongView::from),
            playing: n.playing,
            at: n.at,
            length: n.length,
            can_previous: n.can_previous,
            can_next: n.can_next,
            shuffle: n.shuffle,
            repeat: match n.repeat {
                radiance_core::queue::Repeat::Off => RepeatMode::Off,
                radiance_core::queue::Repeat::All => RepeatMode::All,
                radiance_core::queue::Repeat::One => RepeatMode::One,
            },
            volume: n.volume,
            lossless: n.lossless,
            kbps: n.kbps,
            lit: n.lit.iter().map(|&i| i as u32).collect(),
        }
    }

    pub fn results(&self) -> ResultsView {
        use radiance_core::words::{self, Kind};
        let r = self.inner.results();
        let off = !self.inner.online();
        ResultsView {
            mine_note: words::liked_matches(r.mine.len()),
            lossless_note: words::search_note(Kind::Lossless, r.lossless.as_ref().map(Vec::len), off, &r.query),
            songs_note: words::search_note(Kind::Songs, r.songs.as_ref().map(Vec::len), off, &r.query),
            mine: views(&r.mine),
            lossless: r.lossless.as_deref().map(views),
            songs: r.songs.as_deref().map(views),
            query: r.query,
        }
    }

    pub fn home(&self) -> HomeView {
        let h = self.inner.home();
        HomeView { greeting: h.greeting, mix_note: h.mix_note, mix: views(&h.mix), recent: views(&h.recent), liked: views(&h.liked) }
    }

    pub fn queue(&self) -> QueueView {
        let (songs, index) = self.inner.queue();
        QueueView { songs: views(&songs), index: index as u32 }
    }

    pub fn lyrics(&self) -> LyricsView {
        match self.inner.lyrics() {
            LyricsState::Looking => LyricsView { kind: LyricsKind::Looking, plain: String::new(), lines: Vec::new() },
            LyricsState::None => LyricsView { kind: LyricsKind::None, plain: String::new(), lines: Vec::new() },
            LyricsState::Plain(text) => LyricsView { kind: LyricsKind::Plain, plain: text, lines: Vec::new() },
            LyricsState::Lines(lines) => LyricsView {
                kind: LyricsKind::Lines,
                plain: String::new(),
                lines: lines.into_iter().map(|l| LineView { text: l.text, start: l.start, end: l.end }).collect(),
            },
        }
    }

    pub fn online(&self) -> bool {
        self.inner.online()
    }

    pub fn problem(&self) -> Option<String> {
        self.inner.problem()
    }

    /// A cover as a PNG file (blocks while it's fetched: call it off the UI thread).
    pub fn art_file(&self, source: String, size: u32) -> Option<String> {
        self.inner.art_file(&source, size).map(|p| p.display().to_string())
    }

    /// Now Playing's background files for a cover (blocks: off the UI thread).
    pub fn backdrop(&self, source: String) -> Option<BackdropFiles> {
        self.inner.backdrop_files(&source).map(|(w, b)| BackdropFiles { wash: w.display().to_string(), blob: b.display().to_string() })
    }
}

/// The Windows app's version: its own beta line.
#[uniffi::export]
pub fn beta_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// The engine's version: the computer app's.
#[uniffi::export]
pub fn engine_version() -> String {
    radiance_engine::VERSION.to_string()
}

/// Search's tiles before anything is typed.
#[uniffi::export]
pub fn browse_tiles() -> Vec<BrowseTile> {
    radiance_core::words::BROWSE.iter().map(|(label, query)| BrowseTile { label: label.to_string(), query: query.to_string() }).collect()
}

/// The self-check's lines (the beta's version first).
#[uniffi::export]
pub fn self_check() -> Vec<String> {
    let mut out = vec![format!("beta {}", beta_version())];
    out.extend(radiance_engine::check::lines());
    out
}

/// Another copy runs (today's app or this one): it was asked to show its window. Then quit.
#[uniffi::export]
pub fn ask_the_other(link: Option<String>) -> bool {
    radiance_engine::one_copy::ask_the_other(link.as_deref())
}

/// This is the copy: the next ones are heard (on a thread of their own).
#[uniffi::export]
pub fn listen_for_others(on_show: Arc<dyn ShowListener>) {
    radiance_engine::one_copy::listen(move |link| on_show.show(link));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use radiance_core::words::Kind;
    use radiance_engine::lyrics::Lyrics;
    use radiance_engine::sources::Sources;
    use radiance_engine::track::Track;
    use radiance_engine::youtube::Stream;

    /// Every search takes a second and finds nothing.
    struct Slow;
    impl Sources for Slow {
        fn search(&self, _kind: Kind, _q: &str) -> Vec<Track> {
            std::thread::sleep(Duration::from_secs(1));
            Vec::new()
        }
        fn reachable(&self) -> bool {
            true
        }
        fn stream(&self, _id: &str) -> Result<Stream, String> {
            Err("none".into())
        }
        fn radio(&self, _seed: &Track) -> Vec<Track> {
            Vec::new()
        }
        fn lyrics(&self, _t: &Track) -> Lyrics {
            Lyrics::None
        }
    }

    #[derive(Default)]
    struct Heard(Mutex<Vec<(ChangeKind, Instant)>>);
    impl EngineListener for Heard {
        fn changed(&self, what: ChangeKind) {
            self.0.lock().unwrap().push((what, Instant::now()));
        }
    }

    fn setup() {
        let base = std::env::temp_dir().join(format!("radiance-ffi-test-{}", std::process::id()));
        for (var, dir) in [("XDG_DATA_HOME", "data"), ("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache")] {
            std::fs::create_dir_all(base.join(dir)).unwrap();
            std::env::set_var(var, base.join(dir));
        }
        std::env::set_var("RADIANCE_SILENT", "1");
    }

    #[test]
    fn quit_while_searching_is_quiet() {
        setup();
        let heard = Arc::new(Heard::default());
        let engine = MusicEngine::start_with(heard.clone(), Arc::new(Slow)).unwrap();
        engine.search("binhi".into());
        let asked = Instant::now();
        engine.quit();
        let quit = Instant::now();
        assert!(quit - asked < Duration::from_secs(2), "quit took {:?}", quit - asked);
        std::thread::sleep(Duration::from_millis(1500)); // the searches answer meanwhile
        assert!(heard.0.lock().unwrap().iter().all(|(_, at)| *at <= quit), "told something after quit");
        assert!(engine.results().songs.is_none(), "the search's answer came in after quit");
    }

    #[test]
    fn the_versions_and_the_browse_tiles() {
        assert_eq!(beta_version(), "0.8.0-beta.1");
        assert_eq!(engine_version(), radiance_engine::VERSION);
        let tiles = browse_tiles();
        assert_eq!(tiles.len(), 12);
        assert_eq!((tiles[0].label.as_str(), tiles[0].query.as_str()), ("Pinoy hip-hop", "pinoy hip hop"));
    }

    #[test]
    fn songs_as_the_screens_show_them() {
        let mut t = Track { source: radiance_engine::track::Source::Archive, id: "https://archive.org/x.flac".into(), title: "Binhi".into(), artist: "Arthur Nery".into(), album: "Letters Never Sent".into(), seconds: 245, art: "https://example.com/a.jpg".into(), format: "FLAC".into(), file: None, video: false, genres: None };
        let v = SongView::from(&t);
        assert_eq!((v.title.as_str(), v.length.as_str(), v.seconds, v.lossless, v.art.as_str()), ("Binhi", "4:05", 245.0, true, "https://example.com/a.jpg"));
        assert_eq!(v.byline, t.byline());
        t.video = true;
        assert!(SongView::from(&t).video);
    }
}
