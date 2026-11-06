//! Radiance Music's engine for the Windows app (issue #45): the controller, through uniffi.
//! C# gets a MusicEngine (commands, snapshots as plain records) and tells it an EngineListener;
//! uniffi-bindgen-cs writes the C# side (namespace RadianceMusic.Native, uniffi.toml).

use std::collections::HashMap;
use std::sync::Arc;

use radiance_core::library::{counts_text, row_subtitle, Saved};
use radiance_core::menu;

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
    Liked,
    Toast,
    Library,
    History,
    Playlists,
    /// The split queue (stage 3).
    Splits,
    /// Getting splitting ready at the first start (stage 3).
    Setup,
    /// The song's beat for Now Playing's background (#56).
    Pulse,
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
            Change::Liked => ChangeKind::Liked,
            Change::Toast => ChangeKind::Toast,
            Change::Library => ChangeKind::Library,
            Change::History => ChangeKind::History,
            Change::Playlists => ChangeKind::Playlists,
            Change::Splits => ChangeKind::Splits,
            Change::Setup => ChangeKind::Setup,
            Change::Pulse => ChangeKind::Pulse,
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
    /// Liked (its row's heart), how it's kept, its second line ("Saving…", "… · streams"…),
    /// its plays ("N×") and their words ("Played 3 times · skipped once").
    pub liked: bool,
    pub saved: SavedState,
    pub subtitle: String,
    pub plays: u32,
    pub counts: String,
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
            liked: false,
            saved: if t.file.is_some() { SavedState::Saved } else { SavedState::NotSaved },
            subtitle: t.byline(),
            plays: 0,
            counts: String::new(),
        }
    }
}

/// Search's results, each row saying whether its song is liked (its heart) and saved, as Liked's.
fn results_view(r: controller::Results, off: bool, rows: &Rows) -> ResultsView {
    use radiance_core::words::{self, Kind};
    ResultsView {
        mine_note: words::liked_matches(r.mine.len()),
        lossless_note: words::search_note(Kind::Lossless, r.lossless.as_ref().map(Vec::len), off, &r.query),
        songs_note: words::search_note(Kind::Songs, r.songs.as_ref().map(Vec::len), off, &r.query),
        mine: rows.songs(&r.mine),
        lossless: r.lossless.as_deref().map(|v| rows.songs(v)),
        songs: r.songs.as_deref().map(|v| rows.songs(v)),
        query: r.query,
    }
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
    pub muted: bool,
    pub lossless: bool,
    pub kbps: u32,
    pub lit: Vec<u32>,
    /// The song is liked (the bar's heart).
    pub liked: bool,
    /// When `at` was read, on clock_ms()'s clock (count on from there while playing).
    pub since: u64,
    /// A music video is playing (its picture: video_swapchain()).
    pub video: bool,
    /// The song's other version is known: show the video button.
    pub has_counterpart: bool,
    /// What's really playing, in the Linux app's words ("Lossless · FLAC · 24-bit · 96 kHz"); "Loading…"
    /// while `loading`; empty when nothing is (stopped, or last time's song, paused).
    pub quality: String,
    /// Picked and not playing yet (#55).
    pub loading: bool,
}

/// What went wrong; `retry`: it stopped (show Try Again).
#[derive(Clone, Debug, uniffi::Record)]
pub struct ProblemView {
    pub text: String,
    pub retry: bool,
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
    /// Who sings it, shown small above it (only where the voice changes).
    pub head: Option<String>,
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

    /// The song clicked: its place in the list as shown, and its key (the list may have moved).
    pub fn play_from(&self, list: ListKind, index: u32, key: String) {
        self.inner.play_from(list.into(), index as usize, &key);
    }

    /// Try Again: the song over, the failures counted from 0.
    pub fn retry(&self) {
        self.inner.retry();
    }

    /// Mute or not; the volume stays as it was.
    pub fn set_muted(&self, on: bool) {
        self.inner.set_muted(on);
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
            muted: n.muted,
            lossless: n.lossless,
            kbps: n.kbps,
            lit: n.lit.iter().map(|&i| i as u32).collect(),
            liked: n.liked,
            since: n.since,
            video: n.video,
            has_counterpart: n.has_counterpart,
            quality: n.quality,
            loading: n.loading,
        }
    }

    pub fn results(&self) -> ResultsView {
        results_view(self.inner.results(), !self.inner.online(), &Rows::of(&self.inner))
    }

    pub fn home(&self) -> HomeView {
        let h = self.inner.home();
        let rows = Rows::of(&self.inner);
        HomeView { greeting: h.greeting, mix_note: h.mix_note, mix: rows.songs(&h.mix), recent: rows.songs(&h.recent), liked: rows.songs(&h.liked) }
    }

    pub fn queue(&self) -> QueueView {
        let (songs, index) = self.inner.queue();
        QueueView { songs: Rows::of(&self.inner).songs(&songs), index: index as u32 }
    }

    pub fn lyrics(&self) -> LyricsView {
        match self.inner.lyrics() {
            LyricsState::Looking => LyricsView { kind: LyricsKind::Looking, plain: String::new(), lines: Vec::new() },
            LyricsState::None => LyricsView { kind: LyricsKind::None, plain: String::new(), lines: Vec::new() },
            LyricsState::Plain(text) => LyricsView { kind: LyricsKind::Plain, plain: text, lines: Vec::new() },
            LyricsState::Lines(lines) => LyricsView {
                kind: LyricsKind::Lines,
                plain: String::new(),
                lines: lines.into_iter().map(|l| LineView { text: l.line.text, start: l.line.start, end: l.line.end, head: l.head }).collect(),
            },
        }
    }

    pub fn online(&self) -> bool {
        self.inner.online()
    }

    pub fn problem(&self) -> Option<ProblemView> {
        self.inner.problem().map(|p| ProblemView { text: p.text, retry: p.retry })
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
// ------------------------------------------------------------------ stage 2: the library

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum GroupKind {
    Albums,
    Artists,
    Genres,
}

impl From<GroupKind> for radiance_core::library::GroupKind {
    fn from(k: GroupKind) -> Self {
        match k {
            GroupKind::Albums => Self::Albums,
            GroupKind::Artists => Self::Artists,
            GroupKind::Genres => Self::Genres,
        }
    }
}

/// How a liked song is kept on this computer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum SavedState {
    Saving,
    NotSaved,
    Streams,
    Saved,
}

impl From<Saved> for SavedState {
    fn from(s: Saved) -> Self {
        match s {
            Saved::Saving => SavedState::Saving,
            Saved::NotSaved => SavedState::NotSaved,
            Saved::Streams => SavedState::Streams,
            Saved::Saved => SavedState::Saved,
        }
    }
}

/// Where a song's menu was opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum MenuPlace {
    Row,
    Queue,
    Playlist { id: i64 },
    History,
}

impl From<MenuPlace> for menu::Place {
    fn from(p: MenuPlace) -> Self {
        match p {
            MenuPlace::Row => menu::Place::Row,
            MenuPlace::Queue => menu::Place::Queue,
            MenuPlace::Playlist { id } => menu::Place::Playlist(id),
            MenuPlace::History => menu::Place::History,
        }
    }
}

/// What a menu item does (core::menu::Action).
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum MenuAction {
    Play,
    PlayNext,
    AddToQueue,
    RemoveFromQueue,
    RemoveFromPlaylist { id: i64 },
    AddToPlaylist { id: i64 },
    NewPlaylistWith,
    Like,
    Unlike,
    Radio,
    GoToArtist { q: String },
    GoToAlbum { q: String },
    ShowInFiles,
    /// Stage 3: split in two or four; the lyrics shown, edited, looked up again.
    SplitTwo,
    SplitFour,
    ShowLyrics,
    EditLyrics,
    FindLyricsAgain,
}

impl From<MenuAction> for menu::Action {
    fn from(a: MenuAction) -> Self {
        use menu::Action as A;
        match a {
            MenuAction::Play => A::Play,
            MenuAction::PlayNext => A::PlayNext,
            MenuAction::AddToQueue => A::AddToQueue,
            MenuAction::RemoveFromQueue => A::RemoveFromQueue,
            MenuAction::RemoveFromPlaylist { id } => A::RemoveFromPlaylist(id),
            MenuAction::AddToPlaylist { id } => A::AddToPlaylist(id),
            MenuAction::NewPlaylistWith => A::NewPlaylistWith,
            MenuAction::Like => A::Like,
            MenuAction::Unlike => A::Unlike,
            MenuAction::Radio => A::Radio,
            MenuAction::GoToArtist { q } => A::GoToArtist(q),
            MenuAction::GoToAlbum { q } => A::GoToAlbum(q),
            MenuAction::ShowInFiles => A::ShowInFiles,
            MenuAction::SplitTwo => A::SplitTwo,
            MenuAction::SplitFour => A::SplitFour,
            MenuAction::ShowLyrics => A::ShowLyrics,
            MenuAction::EditLyrics => A::EditLyrics,
            MenuAction::FindLyricsAgain => A::FindLyricsAgain,
        }
    }
}

impl From<menu::Action> for MenuAction {
    fn from(a: menu::Action) -> Self {
        use menu::Action as A;
        match a {
            A::Play => MenuAction::Play,
            A::PlayNext => MenuAction::PlayNext,
            A::AddToQueue => MenuAction::AddToQueue,
            A::RemoveFromQueue => MenuAction::RemoveFromQueue,
            A::RemoveFromPlaylist(id) => MenuAction::RemoveFromPlaylist { id },
            A::AddToPlaylist(id) => MenuAction::AddToPlaylist { id },
            A::NewPlaylistWith => MenuAction::NewPlaylistWith,
            A::Like => MenuAction::Like,
            A::Unlike => MenuAction::Unlike,
            A::Radio => MenuAction::Radio,
            A::GoToArtist(q) => MenuAction::GoToArtist { q },
            A::GoToAlbum(q) => MenuAction::GoToAlbum { q },
            A::ShowInFiles => MenuAction::ShowInFiles,
            A::SplitTwo => MenuAction::SplitTwo,
            A::SplitFour => MenuAction::SplitFour,
            A::ShowLyrics => MenuAction::ShowLyrics,
            A::EditLyrics => MenuAction::EditLyrics,
            A::FindLyricsAgain => MenuAction::FindLyricsAgain,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LetterView {
    pub letter: String,
    /// The first shown row with this letter (None: no song: faint).
    pub first: Option<u32>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct LikedPageView {
    pub songs: Vec<SongView>,
    pub note: String,
    pub letters: Vec<LetterView>,
    /// Fix missing's label (None: nothing missing), and whether it's at work.
    pub fix: Option<String>,
    pub fixing: bool,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct TileView {
    pub name: String,
    pub sub: String,
    pub art: String,
    pub round: bool,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct GroupPageView {
    pub title: String,
    pub sub: String,
    pub art: String,
    pub round: bool,
    pub songs: Vec<SongView>,
}

/// A play in History: the song, when ("8:05 PM"), and its place in the whole history (play_history).
#[derive(Clone, Debug, uniffi::Record)]
pub struct PlayView {
    pub song: SongView,
    pub time: String,
    pub index: u32,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct HistoryDayView {
    pub label: String,
    pub plays: Vec<PlayView>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct PlaylistView {
    pub id: i64,
    pub name: String,
    pub count: u32,
    /// The sidebar's tooltip ("Road Trip: 3 songs").
    pub tip: String,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct PlaylistPageView {
    pub name: String,
    pub count: String,
    pub songs: Vec<SongView>,
    pub empty: Option<String>,
}

/// The queue panel: Played before (oldest first), the queue from the song playing (`start`: its
/// place in the queue; `now`: the first row is the song playing), and the words under it.
#[derive(Clone, Debug, uniffi::Record)]
pub struct QueuePanelView {
    pub before: Vec<SongView>,
    pub rows: Vec<SongView>,
    pub start: u32,
    pub now: bool,
    pub note: String,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct MenuItemView {
    pub label: String,
    pub action: MenuAction,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct MenuSectionView {
    pub items: Vec<MenuItemView>,
}

/// A lyric line lit, and how many of its characters are sung.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LitView {
    pub line: u32,
    pub chars: u32,
}

/// What a row's words depend on (Liked Songs and the plays), read once per call.
#[derive(Default)]
struct Rows {
    liked: HashMap<String, Saved>,
    counts: HashMap<String, (usize, usize)>,
}

impl Rows {
    fn of(c: &Controller) -> Rows {
        Rows { liked: c.liked().into_iter().map(|(t, s)| (t.key(), s)).collect(), counts: c.counts() }
    }

    fn song(&self, t: &Track) -> SongView {
        let mut v = SongView::from(t);
        let key = t.key();
        if let Some(s) = self.liked.get(&key).copied() {
            v.liked = true;
            v.saved = s.into();
            v.subtitle = row_subtitle(&t.byline(), s);
        }
        let (plays, skips) = self.counts.get(&key).copied().unwrap_or_default();
        v.plays = plays as u32;
        v.counts = counts_text(plays, skips);
        v
    }

    fn songs(&self, v: &[Track]) -> Vec<SongView> {
        v.iter().map(|t| self.song(t)).collect()
    }
}

fn liked_page_view(p: controller::LikedPage, rows: &Rows) -> LikedPageView {
    LikedPageView {
        songs: p.songs.iter().map(|(t, _)| rows.song(t)).collect(),
        note: p.note,
        letters: p.letters.into_iter().map(|(l, first)| LetterView { letter: l.to_string(), first: first.map(|i| i as u32) }).collect(),
        fix: p.fix,
        fixing: p.fixing,
    }
}

fn lit_views(v: Vec<(usize, usize)>) -> Vec<LitView> {
    v.into_iter().map(|(line, chars)| LitView { line: line as u32, chars: chars as u32 }).collect()
}

#[uniffi::export]
impl MusicEngine {
    // music videos
    /// The song's other version, from where it is: its music video (`want_video`) or the song.
    pub fn switch_version(&self, want_video: bool) {
        self.inner.switch_version(want_video);
    }
    /// The playing video's DXGI swapchain (an IDXGISwapChain1*, for ISwapChainPanelNative::
    /// SetSwapChain), with a reference of the caller's own: Release it (Marshal.Release) once the
    /// panel holds the swapchain, whatever happens. None: no video playing.
    pub fn video_swapchain(&self) -> Option<u64> {
        self.inner.video_swapchain()
    }
    /// The video's panel is `w`×`h` pixels now (the swapchain follows).
    pub fn resize_video(&self, w: u32, h: u32) {
        self.inner.resize_video(w, h);
    }

    // liking
    pub fn toggle_like(&self, key: String) {
        self.inner.toggle_like(&key);
    }
    pub fn fix_missing(&self) {
        self.inner.fix_missing();
    }
    /// The oldest note not shown yet.
    pub fn toast(&self) -> Option<String> {
        self.inner.toast()
    }

    // the library's pages
    pub fn liked_page(&self, q: String) -> LikedPageView {
        let rows = Rows::of(&self.inner);
        liked_page_view(self.inner.liked_page(&q), &rows)
    }
    pub fn library(&self, kind: GroupKind) -> Vec<TileView> {
        self.inner.library(kind.into()).into_iter().map(|t| TileView { name: t.name, sub: t.sub, art: t.art, round: t.round }).collect()
    }
    pub fn group(&self, kind: GroupKind, name: String) -> Option<GroupPageView> {
        let rows = Rows::of(&self.inner);
        self.inner.group(kind.into(), &name).map(|g| GroupPageView { title: g.title, sub: g.sub, art: g.art, round: g.round, songs: rows.songs(&g.songs) })
    }
    pub fn history(&self) -> Vec<HistoryDayView> {
        let rows = Rows::of(&self.inner);
        self.inner
            .history()
            .into_iter()
            .map(|d| HistoryDayView { label: d.label, plays: d.plays.into_iter().map(|p| PlayView { song: rows.song(&p.track), time: p.time, index: p.index as u32 }).collect() })
            .collect()
    }

    // playing from them
    pub fn play_liked(&self, index: u32, key: String) {
        self.inner.play_from(ListRef::LikedShown, index as usize, &key);
    }
    pub fn play_group(&self, kind: GroupKind, name: String, index: u32, key: String) {
        self.inner.play_from(ListRef::Group(kind.into(), name), index as usize, &key);
    }
    pub fn play_history(&self, index: u32, key: String) {
        self.inner.play_from(ListRef::History, index as usize, &key);
    }
    pub fn play_playlist(&self, id: i64, index: u32, key: String) {
        self.inner.play_from(ListRef::Playlist(id), index as usize, &key);
    }
    pub fn shuffle_liked(&self) {
        self.inner.shuffle_from(ListRef::LikedShown);
    }
    pub fn shuffle_group(&self, kind: GroupKind, name: String) {
        self.inner.shuffle_from(ListRef::Group(kind.into(), name));
    }
    pub fn shuffle_playlist(&self, id: i64) {
        self.inner.shuffle_from(ListRef::Playlist(id));
    }

    // playlists
    pub fn playlists(&self) -> Vec<PlaylistView> {
        self.inner.playlists().into_iter().map(|(id, name, n)| PlaylistView { tip: radiance_core::words::playlist_tip(&name, n), id, name, count: n as u32 }).collect()
    }
    pub fn playlist(&self, id: i64) -> Option<PlaylistPageView> {
        let rows = Rows::of(&self.inner);
        self.inner.playlist(id).map(|p| PlaylistPageView { name: p.name, count: p.count, songs: rows.songs(&p.songs), empty: p.empty })
    }
    pub fn create_playlist(&self, name: String, first: Option<String>) -> Option<i64> {
        self.inner.create_playlist(&name, first.as_deref())
    }
    pub fn rename_playlist(&self, id: i64, name: String) {
        self.inner.rename_playlist(id, &name);
    }
    pub fn delete_playlist(&self, id: i64) {
        self.inner.delete_playlist(id);
    }
    pub fn add_to_playlist(&self, id: i64, key: String) {
        self.inner.add_to_playlist(id, &key);
    }
    pub fn remove_from_playlist(&self, id: i64, key: String) {
        self.inner.remove_from_playlist(id, &key);
    }
    pub fn next_playlist_name(&self) -> String {
        self.inner.next_playlist_name()
    }

    // the queue panel
    pub fn queue_panel(&self) -> QueuePanelView {
        let rows = Rows::of(&self.inner);
        let p = self.inner.queue_panel();
        QueuePanelView { before: rows.songs(&p.before), rows: rows.songs(&p.rows), start: p.start as u32, now: p.now, note: p.note }
    }
    pub fn play_again(&self, key: String) {
        self.inner.play_again(&key);
    }
    pub fn play_next(&self, key: String) {
        self.inner.play_next(&key);
    }
    pub fn enqueue(&self, key: String) {
        self.inner.enqueue(&key);
    }
    pub fn unqueue(&self, index: u32, key: String) {
        self.inner.unqueue(index as usize, &key);
    }
    pub fn move_in_queue(&self, from: u32, to: u32, key: String) {
        self.inner.move_in_queue(from as usize, to as usize, &key);
    }
    pub fn start_radio(&self, key: String) {
        self.inner.start_radio(&key);
    }

    // a song's menu
    pub fn song_menu(&self, key: String, place: MenuPlace) -> Vec<MenuSectionView> {
        self.inner
            .song_menu(&key, place.into())
            .into_iter()
            .map(|s| MenuSectionView { items: s.into_iter().map(|i| MenuItemView { label: i.label, action: i.action.into() }).collect() })
            .collect()
    }
    pub fn do_menu(&self, key: String, index: u32, action: MenuAction) {
        self.inner.do_menu(&key, index as usize, action.into());
    }

    // lyrics word by word
    pub fn sung(&self, at: f64) -> Vec<LitView> {
        lit_views(self.inner.sung(at))
    }
}

/// History when nothing was played yet.
#[uniffi::export]
pub fn history_empty() -> String {
    radiance_core::library::HISTORY_EMPTY.to_string()
}

/// The queue panel's Played before words.
#[uniffi::export]
pub fn played_before_note() -> String {
    radiance_core::library::PLAYED_BEFORE_NOTE.to_string()
}

/// "Add to Playlist" (the submenu the AddToPlaylist items go in).
#[uniffi::export]
pub fn add_to_playlist_label() -> String {
    menu::ADD_TO_PLAYLIST.to_string()
}

/// The engine's clock (NowView::since's), in milliseconds.
#[uniffi::export]
pub fn clock_ms() -> u64 {
    controller::clock_ms()
}

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

/// `--probe-video FILE OUT` (ribs03, the music videos' spike): FILE played for `seconds` the way
/// the app plays videos, silent, with no window; its report, "ok" last or "failed: …".
#[uniffi::export]
pub fn probe_video(file: String, seconds: f64) -> Vec<String> {
    radiance_engine::video_probe::probe(std::path::Path::new(&file), seconds)
}

/// `--video-probe FILE` (ribs03, the music videos' spike): FILE playing, silent, for a test
/// window's SwapChainPanel.
#[derive(uniffi::Object)]
pub struct VideoCheck {
    inner: radiance_engine::video_probe::VideoProbe,
}

#[uniffi::export]
impl VideoCheck {
    #[uniffi::constructor]
    pub fn start(file: String) -> Result<Arc<Self>, EngineError> {
        radiance_engine::video_probe::VideoProbe::start(std::path::Path::new(&file)).map(|inner| Arc::new(VideoCheck { inner })).map_err(EngineError::Failed)
    }
    /// d3d12swapchainsink's swapchain (an IDXGISwapChain1*; 0: none).
    pub fn swapchain(&self) -> u64 {
        self.inner.swapchain()
    }
    pub fn frames(&self) -> u64 {
        self.inner.frames()
    }
    pub fn resize(&self, w: u32, h: u32) {
        self.inner.resize(w, h);
    }
    pub fn report(&self) -> Vec<String> {
        self.inner.report()
    }
    pub fn stop(&self) {
        self.inner.stop();
    }
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

    #[test]
    fn liked_page_view_carries_letters() {
        let page = controller::LikedPage { songs: vec![], note: "n".into(), letters: vec![('A', Some(0)), ('B', None)], fix: Some("Fix missing (2)".into()), fixing: false };
        let v = liked_page_view(page, &Rows::default());
        assert_eq!(v.letters[0], LetterView { letter: "A".into(), first: Some(0) });
        assert_eq!(v.letters[1].first, None);
        assert_eq!(v.fix.as_deref(), Some("Fix missing (2)"));
    }

    #[test]
    fn menu_actions_cross_with_their_data() {
        use radiance_core::menu::Action;
        assert_eq!(Action::from(MenuAction::AddToPlaylist { id: 7 }), Action::AddToPlaylist(7));
        assert_eq!(MenuAction::from(Action::GoToAlbum("Blue Ann".into())), MenuAction::GoToAlbum { q: "Blue Ann".into() });
        assert_eq!(radiance_core::menu::Place::from(MenuPlace::Playlist { id: 3 }), radiance_core::menu::Place::Playlist(3));
    }

    /// Search's rows (and Home's, the queue's) say which songs are liked, as Liked Songs' do: a
    /// liked song's heart filled, so a click doesn't unlike it and trash its file by mistake.
    #[test]
    fn search_rows_say_which_songs_are_liked() {
        use radiance_engine::track::Source;
        let t = Track { source: Source::YouTube, id: "y1".into(), title: "Halo".into(), artist: "Ann".into(), album: String::new(), seconds: 200, art: String::new(), format: String::new(), file: None, video: false, genres: None };
        let mut rows = Rows::default();
        rows.liked.insert(t.key(), radiance_core::library::Saved::Streams);
        let r = controller::Results { query: "halo".into(), mine: vec![t.clone()], lossless: None, songs: Some(vec![t]) };
        let v = results_view(r, false, &rows);
        assert!(v.mine[0].liked, "a liked song found in Search");
        assert!(v.songs.unwrap()[0].liked);
    }

    #[test]
    fn sung_views_are_u32() {
        assert_eq!(lit_views(vec![(1, 5), (2, 0)]), vec![LitView { line: 1, chars: 5 }, LitView { line: 2, chars: 0 }]);
    }
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
        fn download(&self, _url: &str, _to: &std::path::Path) -> Result<(), String> {
            Err("no downloads in this test".into())
        }
        fn genres(&self, _t: &Track) -> Vec<String> {
            Vec::new()
        }
        fn counterpart(&self, _t: &Track) -> Option<Track> {
            None
        }
        fn video_streams(&self, _id: &str) -> Result<(radiance_engine::youtube::Stream, radiance_engine::youtube::Stream), String> {
            Err("no videos in this test".into())
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
        assert_eq!(beta_version(), "0.8.0-beta.2");
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
