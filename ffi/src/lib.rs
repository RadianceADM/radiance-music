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
    Update,
    Report,
    Rating,
    Storage,
    Suggestions,
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
            Change::Update => ChangeKind::Update,
            Change::Report => ChangeKind::Report,
            Change::Rating => ChangeKind::Rating,
            Change::Storage => ChangeKind::Storage,
            Change::Suggestions => ChangeKind::Suggestions,
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
    /// Its split mark (stage 3): waiting (its place in line), being split, split in two or four.
    pub split: SplitMarkView,
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
            split: SplitMarkView::None,
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
    /// The Mixer (stage 3).
    pub mix: MixView,
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
        // this process is the Windows beta: its own updates, version and log (stage 4a)
        radiance_engine::update::set_beta(&beta_version());
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
            mix: mix_view(&n.mix),
        }
    }
    /// The Mixer's split line and choices for the song playing: read again on Now and Splits changes.
    pub fn mixer_split(&self) -> MixerSplitView {
        mixer_split_view(self.inner.mixer_split())
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

/// Downloading (owner, 2026-11-07): the sidebar's title ("Downloading (3)"), the line under the page's title, the
/// liked songs not saved yet (saving first), and Save Them Now's label (Fix missing's) and whether it's at work.
#[derive(Clone, Debug, uniffi::Record)]
pub struct DownloadingView {
    pub title: String,
    pub note: String,
    pub songs: Vec<SongView>,
    pub fix: Option<String>,
    pub fixing: bool,
}

fn downloading_view(list: &[(radiance_core::track::Track, radiance_core::library::Saved)], fix: Option<String>, fixing: bool, rows: &Rows) -> DownloadingView {
    use radiance_core::library as shelf;
    let saving = list.iter().filter(|(_, s)| *s == shelf::Saved::Saving).count();
    DownloadingView {
        title: shelf::downloading_title(list.len()),
        note: shelf::downloading_note(saving, list.len() - saving),
        songs: list.iter().map(|(t, _)| rows.song(t)).collect(),
        // only beside the songs it lists (Fix missing counts music videos too: not beside "Every liked song is saved")
        fix: fix.filter(|_| !list.is_empty()),
        fixing,
    }
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
    /// The engine's split marks now (the queue, and songs split while it runs).
    marks: HashMap<String, controller::SplitMark>,
}

impl Rows {
    fn of(c: &Controller) -> Rows {
        Rows { liked: c.liked().into_iter().map(|(t, s)| (t.key(), s)).collect(), counts: c.counts(), marks: c.split_marks() }
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
        // split before this run: its files say (Vocals, then Drums: in four), saved songs only
        let mark = self.marks.get(&key).copied().or_else(|| {
            t.file.as_ref()?;
            radiance_engine::split::ready_part(t, "vocals")?;
            Some(if radiance_engine::split::ready_part(t, "drums").is_some() { controller::SplitMark::Four } else { controller::SplitMark::Two })
        });
        v.split = mark.map(SplitMarkView::from).unwrap_or(SplitMarkView::None);
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
    /// The sidebar's Downloading: the liked songs not saved on this computer yet.
    pub fn downloading(&self) -> DownloadingView {
        let rows = Rows::of(&self.inner);
        let (fix, fixing) = self.inner.fix_missing_state();
        downloading_view(&self.inner.downloading(), fix, fixing, &rows)
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


// ---------------------------------------------------------------------------------- stage 3: parts, splitting, lyrics, the beat

/// A song's split mark on its row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum SplitMarkView {
    None,
    Waiting { place: u32 },
    Splitting,
    Two,
    Four,
}

impl From<controller::SplitMark> for SplitMarkView {
    fn from(m: controller::SplitMark) -> Self {
        match m {
            controller::SplitMark::None => SplitMarkView::None,
            controller::SplitMark::Waiting(n) => SplitMarkView::Waiting { place: n as u32 },
            controller::SplitMark::Splitting => SplitMarkView::Splitting,
            controller::SplitMark::Two => SplitMarkView::Two,
            controller::SplitMark::Four => SplitMarkView::Four,
        }
    }
}

/// Between songs (stage 4a-2): No Transition, Crossfade, DJ Mix (by the Beat).
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum TransitionMode {
    Off,
    Crossfade,
    DjMix,
}

impl From<radiance_engine::settings::Transition> for TransitionMode {
    fn from(t: radiance_engine::settings::Transition) -> Self {
        use radiance_engine::settings::Transition;
        match t {
            Transition::Off => TransitionMode::Off,
            Transition::Crossfade => TransitionMode::Crossfade,
            Transition::DjMix => TransitionMode::DjMix,
        }
    }
}

impl From<TransitionMode> for radiance_engine::settings::Transition {
    fn from(t: TransitionMode) -> Self {
        use radiance_engine::settings::Transition;
        match t {
            TransitionMode::Off => Transition::Off,
            TransitionMode::Crossfade => Transition::Crossfade,
            TransitionMode::DjMix => Transition::DjMix,
        }
    }
}

/// The crossfade length's label ("Crossfade length: 6 s"), as the slider moves.
#[uniffi::export]
pub fn fade_label(seconds: f64) -> String {
    radiance_core::words::fade_label(seconds)
}

/// The voice buttons: the song, No vocals, Vocals only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum VoiceMode {
    Song,
    NoVocals,
    VocalsOnly,
}

impl From<radiance_core::mix::Voice> for VoiceMode {
    fn from(v: radiance_core::mix::Voice) -> Self {
        match v {
            radiance_core::mix::Voice::Song => VoiceMode::Song,
            radiance_core::mix::Voice::NoVocals => VoiceMode::NoVocals,
            radiance_core::mix::Voice::VocalsOnly => VoiceMode::VocalsOnly,
        }
    }
}

impl From<VoiceMode> for radiance_core::mix::Voice {
    fn from(v: VoiceMode) -> Self {
        match v {
            VoiceMode::Song => radiance_core::mix::Voice::Song,
            VoiceMode::NoVocals => radiance_core::mix::Voice::NoVocals,
            VoiceMode::VocalsOnly => radiance_core::mix::Voice::VocalsOnly,
        }
    }
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PartView {
    pub name: String,
    /// Its title on the Mixer's row ("Drums", "Music"…).
    pub title: String,
    pub gain: f64,
}

/// The Mixer: the song's parts (none: not split), the voice button, the effects, its parts loaded.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MixView {
    pub parts: Vec<PartView>,
    pub voice: VoiceMode,
    pub low_cut: f64,
    pub high_cut: f64,
    pub reverb: f64,
    pub plays_parts: bool,
    /// Under the voice buttons: "Plays as the full song until it's split.", or empty.
    pub note: String,
}

/// The Mixer's split line and choices for the song playing (owner, 2026-11-07/08): what its split is doing ("Splitting
/// for No vocals…", "Splitting into four parts · 40%", "Next in line to be split"; empty: nothing), how far (0..1;
/// -1: not known, a moving bar), Turn Off while it's for the voice picked, and the song menu's split choices.
#[derive(Clone, Debug, uniffi::Record)]
pub struct MixerSplitView {
    pub line: String,
    pub progress: f64,
    pub turn_off: bool,
    pub items: Vec<MenuItemView>,
}

fn mixer_split_view(m: radiance_core::mix::MixerSplit) -> MixerSplitView {
    MixerSplitView {
        line: m.line.unwrap_or_default(),
        progress: m.progress.unwrap_or(-1.0),
        turn_off: m.for_voice,
        items: m.items.into_iter().map(|i| MenuItemView { label: i.label, action: i.action.into() }).collect(),
    }
}

fn mix_view(m: &controller::MixState) -> MixView {
    MixView {
        parts: m.parts.iter().map(|(name, gain)| PartView { name: name.clone(), title: radiance_core::mix::part_title(name).to_string(), gain: *gain }).collect(),
        voice: m.voice.into(),
        low_cut: m.effects.low_cut,
        high_cut: m.effects.high_cut,
        reverb: m.effects.reverb,
        plays_parts: m.plays_parts,
        note: m.note.clone(),
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct PartLevelView {
    pub name: String,
    pub level: f64,
    /// Its meter, 0..1 in dB (core::mix::meter, the Linux app's), and what it says when pointed at.
    pub meter: f64,
    pub tip: String,
}

/// The song being split, how far, and at what (core's words: "Splitting · 40%"…).
#[derive(Clone, Debug, uniffi::Record)]
pub struct SplitNowView {
    pub song: SongView,
    pub done: f64,
    pub phase: String,
}

/// The Splits tab.
#[derive(Clone, Debug, uniffi::Record)]
pub struct SplitsView {
    pub now: Option<SplitNowView>,
    pub waiting: Vec<SongView>,
    pub two_now: Option<SongView>,
    pub two_waiting: Vec<SongView>,
    pub paused: bool,
    pub can_split: bool,
    pub can_split_two: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum SetupKind {
    NotNeeded,
    Getting,
    Offline,
    Failed,
    Ready,
}

/// Getting splitting ready at the first start: what's happening (core's words), how far, why it stopped.
#[derive(Clone, Debug, uniffi::Record)]
pub struct SetupView {
    pub kind: SetupKind,
    pub text: String,
    pub done: f64,
    pub why: String,
}

/// "Cores it may use": now, this computer's, and the dialog's words.
#[derive(Clone, Debug, uniffi::Record)]
pub struct CoresView {
    pub now: u32,
    pub max: u32,
    pub title: String,
    pub note: String,
}

/// A lyrics editor row: its start (none: not set), its words, who sings it.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct EditedLineView {
    pub at: Option<f64>,
    pub text: String,
    pub singer: Option<String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct LyricsEditView {
    pub rows: Vec<EditedLineView>,
    /// Who can sing a line: the song's artists, and anyone already named.
    pub names: Vec<String>,
}

fn edited_rows(v: Vec<EditedLineView>) -> Vec<(Option<f64>, String, Option<String>)> {
    v.into_iter().map(|r| (r.at, r.text, r.singer)).collect()
}

/// The song's beat for the background: how hard each moment hits and how much bass, `fps` a second
/// from `offset` seconds.
#[derive(Clone, Debug, uniffi::Record)]
pub struct PulseView {
    pub key: String,
    pub fps: f64,
    pub offset: f64,
    pub level: Vec<f32>,
    pub bass: Vec<f32>,
}

/// The beta's stage-3 words (core's, the Linux app's), in one call.
#[derive(Clone, Debug, uniffi::Record)]
pub struct BetaWords {
    pub splits_tab: String,
    pub splitting_now: String,
    pub nothing_to_split: String,
    pub splits_note: String,
    pub pause_splitting: String,
    pub splitting_dialog: String,
    pub ready_title: String,
    pub ready_note: String,
    pub try_again: String,
    pub open_now: String,
    pub open_now_note: String,
    pub show_lyrics: String,
    pub edit_lyrics: String,
    pub editor_note: String,
    pub add_line: String,
    pub paste_lyrics: String,
    pub paste_tip: String,
    pub tap_timing: String,
    pub tap_tip: String,
    pub remove_line_tip: String,
    pub start_tip: String,
    pub singer_tip: String,
    pub nobody_set: String,
    pub add_singer: String,
    pub line_placeholder: String,
    pub save: String,
}

#[uniffi::export]
pub fn beta_words() -> BetaWords {
    use radiance_core::words as w;
    BetaWords {
        splits_tab: w::SPLITS_TAB.into(),
        splitting_now: w::SPLITTING_NOW_HEADING.into(),
        nothing_to_split: w::NOTHING_TO_SPLIT.into(),
        splits_note: w::SPLITS_NOTE.into(),
        pause_splitting: w::PAUSE_SPLITTING.into(),
        splitting_dialog: w::SPLITTING_DIALOG.into(),
        ready_title: w::SPLITTING_READY.0.into(),
        ready_note: w::SPLITTING_READY.1.into(),
        try_again: w::TRY_AGAIN.into(),
        open_now: w::OPEN_NOW.into(),
        open_now_note: w::OPEN_NOW_NOTE.into(),
        show_lyrics: w::SHOW_LYRICS.into(),
        edit_lyrics: w::EDIT_LYRICS.into(),
        editor_note: w::EDITOR_NOTE.into(),
        add_line: w::ADD_LINE.into(),
        paste_lyrics: w::PASTE_LYRICS.into(),
        paste_tip: w::PASTE_TIP.into(),
        tap_timing: w::TAP_TIMING.into(),
        tap_tip: w::TAP_TIP.into(),
        remove_line_tip: w::REMOVE_LINE_TIP.into(),
        start_tip: w::START_TIP.into(),
        singer_tip: w::SINGER_TIP.into(),
        nobody_set: w::NOBODY_SET.into(),
        add_singer: w::ADD_SINGER.into(),
        line_placeholder: w::LINE_PLACEHOLDER.into(),
        save: w::SAVE.into(),
    }
}

/// A row's split mark in words (its tooltip), as the Linux app's badges say it.
#[uniffi::export]
pub fn split_mark_tip(mark: SplitMarkView) -> String {
    use radiance_core::words as w;
    match mark {
        SplitMarkView::None => String::new(),
        SplitMarkView::Waiting { place } => format!("{}. {}", w::SPLIT_WAITING, w::split_place(place as usize)),
        SplitMarkView::Splitting => w::SPLITTING_NOW.to_string(),
        SplitMarkView::Two | SplitMarkView::Four => format!("{}. {}", w::SPLIT_DONE.0, w::SPLIT_DONE.1),
    }
}

/// A waiting song's line in the Splits tab: "Number 2 in line · {byline}".
#[uniffi::export]
pub fn in_line(n: u32, byline: String) -> String {
    radiance_core::words::in_line(n as usize, &byline)
}

#[uniffi::export]
impl MusicEngine {
    // the Mixer
    pub fn set_part(&self, name: String, gain: f64) {
        self.inner.set_part(&name, gain);
    }
    pub fn reset_mix(&self) {
        self.inner.reset_mix();
    }
    pub fn set_voice(&self, voice: VoiceMode) {
        self.inner.set_voice(voice.into());
    }
    pub fn set_effects(&self, low_cut: f64, high_cut: f64, reverb: f64) {
        self.inner.set_effects(radiance_engine::effects::Sound { low_cut, high_cut, reverb });
    }
    /// Each part's loudness now (the meters, while the Mixer is open).
    pub fn part_levels(&self) -> Vec<PartLevelView> {
        self.inner.part_levels().into_iter().map(|(name, level)| PartLevelView { meter: radiance_core::mix::meter(level), tip: radiance_core::mix::meter_tip(&name, Some(level)), name, level }).collect()
    }

    // splitting
    pub fn split(&self, key: String, two: bool) {
        self.inner.split(&key, two);
    }
    pub fn set_split_paused(&self, on: bool) {
        self.inner.set_split_paused(on);
    }
    pub fn splits(&self) -> SplitsView {
        let s = self.inner.splits();
        let rows = Rows::of(&self.inner);
        SplitsView {
            now: s.now.map(|(t, p)| SplitNowView { song: rows.song(&t), done: p.code().max(0.0), phase: radiance_core::words::split_progress(p.code()) }),
            waiting: rows.songs(&s.waiting),
            two_now: s.two_now.as_ref().map(|t| rows.song(t)),
            two_waiting: rows.songs(&s.two_waiting),
            paused: s.paused,
            can_split: s.can_split,
            can_split_two: s.can_split_two,
        }
    }
    pub fn split_mark(&self, key: String) -> SplitMarkView {
        self.inner.split_mark(&key).into()
    }
    pub fn setup(&self) -> SetupView {
        use controller::SetupState as S;
        use radiance_core::words;
        match self.inner.setup() {
            S::NotNeeded => SetupView { kind: SetupKind::NotNeeded, text: String::new(), done: 0.0, why: String::new() },
            S::Getting { step, done } => SetupView { kind: SetupKind::Getting, text: if step.is_empty() { words::SETUP_GETTING.to_string() } else { words::setup_step(&step) }, done, why: String::new() },
            S::Offline => SetupView { kind: SetupKind::Offline, text: words::SETUP_OFFLINE.to_string(), done: 0.0, why: String::new() },
            S::Failed { why } => SetupView { kind: SetupKind::Failed, text: words::SETUP_RETRY.to_string(), done: 0.0, why },
            S::Ready => SetupView { kind: SetupKind::Ready, text: String::new(), done: 1.0, why: String::new() },
        }
    }
    pub fn retry_setup(&self) {
        self.inner.retry_setup();
    }
    pub fn set_split_cores(&self, n: u32) {
        self.inner.set_split_cores(n);
    }
    pub fn split_cores(&self) -> CoresView {
        let (now, max) = self.inner.split_cores();
        CoresView { now, max, title: radiance_core::words::CORES_TITLE.to_string(), note: radiance_core::words::cores_note(max) }
    }

    // lyrics
    pub fn find_lyrics_again(&self, key: String) {
        self.inner.find_lyrics_again(&key);
    }
    pub fn save_lyrics(&self, key: String, rows: Vec<EditedLineView>) {
        self.inner.save_lyrics(&key, edited_rows(rows));
    }
    /// The editor's rows and singer names (looked up when needed: call it off the UI thread).
    pub fn lyrics_text(&self, key: String) -> Option<LyricsEditView> {
        self.inner.lyrics_text(&key).map(|(rows, names)| LyricsEditView { rows: rows.into_iter().map(|(at, text, singer)| EditedLineView { at, text, singer }).collect(), names })
    }
    /// Show Lyrics: the lines' words (off the UI thread too).
    pub fn show_lyrics(&self, key: String) -> String {
        self.inner.lyrics_text(&key).map(|(rows, _)| rows.into_iter().map(|r| r.1).collect::<Vec<_>>().join("\n")).unwrap_or_default()
    }

    // the beat (#56)
    pub fn pulse(&self) -> Option<PulseView> {
        self.inner.pulse().map(|(key, p)| PulseView { key, fps: p.fps, offset: p.offset, level: p.level.clone(), bass: p.bass.clone() })
    }
}

// ---------------------------------------------------------------------------------- stage 4a: the app's own parts

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum UpdateStepView {
    None,
    Out,
    Downloading,
    Ready,
    Restarting,
    Failed,
}

/// The update pop-up, its words ready to show (core::update_words, the GTK app's).
#[derive(Clone, Debug, uniffi::Record)]
pub struct UpdateView {
    pub step: UpdateStepView,
    /// How far the download is (0..1).
    pub done: f64,
    pub version: String,
    /// The release's first four points.
    pub bullets: Vec<String>,
    pub asks: bool,
    /// This copy can't put it on: the release's page instead.
    pub page_only: bool,
    pub page: String,
    pub current: String,
    pub title: String,
    pub text: String,
    /// The main button's words (Update Now (149 MB), Restart to Update, Try Again…).
    pub main: String,
    /// Under the download's bar ("75 MB of 150 MB").
    pub progress: String,
    /// Restart to Update arranged it: the app closes now.
    pub quit: bool,
}

fn update_view(u: &controller::UpdateState, playing: bool) -> UpdateView {
    use radiance_core::update_words as w;
    use radiance_engine::controller::UpdateStep as S;
    let r = u.release.clone().unwrap_or_default();
    let (step, done) = match &u.step {
        S::None => (UpdateStepView::None, 0.0),
        S::Out => (UpdateStepView::Out, 0.0),
        S::Downloading(d) => (UpdateStepView::Downloading, *d),
        S::Ready => (UpdateStepView::Ready, 1.0),
        S::Restarting => (UpdateStepView::Restarting, 1.0),
        S::Failed(_) => (UpdateStepView::Failed, 0.0),
    };
    let (title, text, main) = match &u.step {
        S::None => (String::new(), String::new(), String::new()),
        S::Out => (w::out_title(&r.version), w::OUT_NOTE.to_string(), w::update_now_label(r.size, u.page_only, false)),
        S::Downloading(_) => (w::downloading_title(&r.version), w::DOWNLOADING_NOTE.to_string(), w::HIDE.to_string()),
        S::Ready => (w::ready_title(&r.version), w::ready_text(playing, u.installer), w::RESTART.to_string()),
        S::Restarting => (w::restarting_title(&r.version), w::RESTARTING_NOTE.to_string(), String::new()),
        S::Failed(why) => (why.clone(), w::failed_text(&u.current), w::TRY_AGAIN.to_string()),
    };
    UpdateView {
        step,
        done,
        bullets: r.bullets(4),
        asks: u.asks,
        page_only: u.page_only,
        page: if r.page.is_empty() { "https://github.com/RadianceADM/radiance-music/releases/latest".into() } else { r.page.clone() },
        current: u.current.clone(),
        progress: w::downloading_words(r.size, done, false),
        version: r.version,
        title,
        text,
        main,
        quit: u.quit,
    }
}

/// Report a Problem: what will be sent (None until read) and how sending went.
#[derive(Clone, Debug, uniffi::Record)]
pub struct ReportView {
    pub ready: bool,
    /// What will be sent, exactly as the dialog shows it.
    pub what: String,
    pub sending: bool,
    /// Sent: its code ("RM-0042") and the words with it; or why not.
    pub code: Option<String>,
    pub sent_text: String,
    pub failed: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum RateOutcomeView {
    None,
    Sent,
    Failed,
}

/// The rating pop-up's state.
#[derive(Clone, Debug, uniffi::Record)]
pub struct RatingView {
    pub due: bool,
    pub account_name: Option<String>,
    pub account_email: Option<String>,
    /// "Sending as …" / "Send opens your browser…" (the Privacy link goes after it).
    pub who: String,
    pub busy: String,
    /// The browser's sign-in address: the app opens it (once).
    pub sign_in_url: Option<String>,
    pub outcome: RateOutcomeView,
    pub share: bool,
    pub failed: String,
}

/// What the rating form sends.
#[derive(Clone, Debug, uniffi::Record)]
pub struct AnswerView {
    pub enjoying: bool,
    /// 1–5 each, in the five rows' order.
    pub ratings: Vec<u8>,
    pub comment: String,
}

/// A row of Storage's "Space used".
#[derive(Clone, Debug, uniffi::Record)]
pub struct StorageRowView {
    pub title: String,
    pub subtitle: String,
    pub size: String,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct StorageView {
    pub folder: String,
    pub counting: bool,
    pub moving: bool,
    /// "1.2 GB in your songs' folder" (Counting… while counting).
    pub total: String,
    pub rows: Vec<StorageRowView>,
}

fn storage_view(st: &controller::StorageState) -> StorageView {
    use radiance_core::help_words as w;
    use radiance_engine::storage::human;
    let Some(u) = st.usage else {
        return StorageView { folder: st.folder.clone(), counting: true, moving: st.moving, total: w::COUNTING.into(), rows: vec![] };
    };
    let row = |title: &str, sub: String, bytes: u64| StorageRowView { title: title.into(), subtitle: sub, size: human(bytes) };
    let total = u.songs.bytes + u.videos.bytes + u.parts.bytes + u.extras.bytes;
    StorageView {
        folder: st.folder.clone(),
        counting: false,
        moving: st.moving,
        total: w::space_total(&human(total)),
        rows: vec![
            row("Songs", format!("{} files", u.songs.files), u.songs.bytes),
            row("Music videos", format!("{} files", u.videos.files), u.videos.bytes),
            row("Split parts", format!("{} files (drums, bass, other, vocals; no vocals)", u.parts.files), u.parts.bytes),
            row("Lyrics and covers", format!("{} files", u.extras.files), u.extras.bytes),
            row("App data", "the database and cache".into(), u.app),
            row("This computer's splitter", "Python and Demucs".into(), u.splitter),
            row("Free on this drive", String::new(), u.free),
        ],
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ChipView {
    pub text: String,
    pub mine: bool,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct GuideCardView {
    pub icon: String,
    pub title: String,
    pub text: String,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct NotesSectionView {
    pub heading: String,
    pub items: Vec<String>,
}

#[uniffi::export]
impl MusicEngine {
    pub fn update(&self) -> UpdateView {
        update_view(&self.inner.update(), self.inner.now().playing)
    }
    pub fn look_for_updates(&self, asked: bool) {
        self.inner.look_for_updates(asked);
    }
    pub fn update_now(&self) {
        self.inner.update_now();
    }
    pub fn update_restart(&self) {
        self.inner.update_restart();
    }
    pub fn update_later(&self) {
        self.inner.update_later();
    }
    pub fn update_skip(&self) {
        self.inner.update_skip();
    }
    pub fn update_cancel(&self) {
        self.inner.update_cancel();
    }

    pub fn report(&self) -> ReportView {
        use radiance_core::help_words as w;
        let r = self.inner.report();
        let what = r.gathered.as_ref().map(|g| w::report_what(&g.version, g.platform, &g.system, &g.install, &g.log)).unwrap_or_else(|| w::REPORT_READING.into());
        let (code, failed) = match r.outcome {
            Some(Ok(c)) => (Some(c), None),
            Some(Err(e)) => (None, Some(e)),
            None => (None, None),
        };
        ReportView { ready: r.gathered.is_some(), what, sending: r.sending, sent_text: code.as_deref().map(w::report_sent).unwrap_or_default(), code, failed }
    }
    pub fn report_gather(&self) {
        self.inner.report_gather();
    }
    pub fn report_send(&self, note: String) {
        self.inner.report_send(&note);
    }

    pub fn rating(&self) -> RatingView {
        let r = self.inner.rating();
        let (outcome, share, failed) = match &r.outcome {
            Some(controller::RateOutcome::Sent { share }) => (RateOutcomeView::Sent, *share, String::new()),
            Some(controller::RateOutcome::Failed(why)) => (RateOutcomeView::Failed, false, why.clone()),
            None => (RateOutcomeView::None, false, String::new()),
        };
        RatingView {
            due: r.due,
            who: radiance_core::help_words::rate_who(r.account.as_ref().map(|(n, e)| (n.as_str(), e.as_str()))),
            account_name: r.account.as_ref().map(|a| a.0.clone()),
            account_email: r.account.as_ref().map(|a| a.1.clone()),
            busy: r.busy,
            sign_in_url: r.sign_in_url,
            outcome,
            share,
            failed,
        }
    }
    pub fn rating_shown(&self) {
        self.inner.rating_shown();
    }

    /// The rating's free moment passed with the window not free: let go (not counted as asked).
    pub fn rating_missed(&self) {
        self.inner.rating_missed();
    }
    pub fn rating_never(&self) {
        self.inner.rating_never();
    }
    pub fn rate(&self, answer: AnswerView) {
        let mut ratings = [3u8; 5];
        for (i, n) in answer.ratings.iter().take(5).enumerate() {
            ratings[i] = (*n).clamp(1, 5);
        }
        self.inner.rate(radiance_engine::feedback::Answer { enjoying: answer.enjoying, ratings, comment: answer.comment });
    }
    pub fn cancel_sign_in(&self) {
        self.inner.cancel_sign_in();
    }
    pub fn sign_out(&self) {
        self.inner.sign_out();
    }

    pub fn storage(&self) -> StorageView {
        storage_view(&self.inner.storage())
    }
    pub fn storage_count(&self) {
        self.inner.storage_count();
    }
    pub fn move_music(&self, to: String) {
        self.inner.move_music(&to);
    }

    pub fn autoplay(&self) -> bool {
        self.inner.autoplay()
    }
    pub fn set_autoplay(&self, on: bool) {
        self.inner.set_autoplay(on);
    }
    /// Between songs (stage 4a-2): the transition and the crossfade length (whole seconds, 1 to 12).
    pub fn transition(&self) -> TransitionMode {
        self.inner.transition().into()
    }
    pub fn set_transition(&self, t: TransitionMode) {
        self.inner.set_transition(t.into());
    }
    pub fn fade(&self) -> f64 {
        self.inner.fade()
    }
    pub fn set_fade(&self, seconds: f64) {
        self.inner.set_fade(seconds);
    }
    pub fn guide_seen(&self) -> bool {
        self.inner.guide_seen()
    }
    pub fn set_guide_seen(&self) {
        self.inner.set_guide_seen();
    }

    pub fn network_changed(&self, available: bool) {
        self.inner.network_changed(available);
    }
    pub fn suggest(&self, typed: String) {
        self.inner.suggest(&typed);
    }
    pub fn suggestions(&self) -> Vec<ChipView> {
        self.inner.suggestions().into_iter().map(|c| ChipView { text: c.text, mine: c.mine }).collect()
    }
}

/// Storage's Change…: where the songs go for the folder picked, and the confirm's words.
#[uniffi::export]
pub fn storage_target(chosen: String) -> String {
    radiance_engine::storage::target(std::path::Path::new(&chosen)).display().to_string()
}

#[uniffi::export]
pub fn move_text(from: String, to: String) -> String {
    radiance_core::words::move_text(&from, &to)
}

/// How to Use It's cards (the beta shows the first four until sharing comes: stage 4b).
#[uniffi::export]
pub fn guide_cards() -> Vec<GuideCardView> {
    radiance_core::guide::CARDS.iter().map(|(icon, title, text)| GuideCardView { icon: (*icon).into(), title: (*title).into(), text: (*text).into() }).collect()
}

/// What's New: the beta's own CHANGELOG sections (the newest three), as built.
#[uniffi::export]
pub fn whats_new() -> Vec<NotesSectionView> {
    radiance_core::release_notes::sections(include_str!("../../CHANGELOG.md"), "Windows beta", 3).into_iter().map(|(heading, items)| NotesSectionView { heading, items }).collect()
}

/// The stage-4a words the C# app shows (core's help_words, update_words, guide, words), in one call.
#[derive(Clone, Debug, uniffi::Record)]
pub struct HelpWords {
    pub update_title: String,
    pub keeps_playing: String,
    pub remind_later: String,
    pub skip_version: String,
    pub cancel_download: String,
    pub from_github: String,
    pub look: String,
    pub report_title: String,
    pub report_intro: String,
    pub report_heading: String,
    pub report_what: String,
    pub cancel: String,
    pub send: String,
    pub sending: String,
    pub sent_title: String,
    pub copy_code: String,
    pub close: String,
    pub rate_title: String,
    pub rate_head: String,
    pub rate_sub: String,
    pub yes: String,
    pub not_really: String,
    pub rate_areas: Vec<String>,
    pub rate_comment: String,
    pub cancel_sign_in: String,
    pub sign_out: String,
    pub not_now: String,
    pub dont_ask: String,
    pub rate_choose: String,
    pub privacy: String,
    pub privacy_word: String,
    pub thanks: String,
    pub thanks_share: String,
    pub thanks_plain: String,
    pub share_app: String,
    pub share_link: String,
    pub share_toast: String,
    pub done: String,
    pub storage_title: String,
    pub kept_group: String,
    pub folder: String,
    pub change: String,
    pub open_folder: String,
    pub space_group: String,
    pub pick_folder: String,
    pub move_title: String,
    pub move_word: String,
    pub offline: String,
    pub offline_tip: String,
    pub autoplay: String,
    pub between_songs: String,
    pub no_transition: String,
    pub crossfade: String,
    pub dj_mix: String,
    pub mine_tip: String,
    pub guide_title: String,
    pub guide_skip: String,
    pub guide_next: String,
    pub guide_start: String,
    pub about: String,
    pub app_name: String,
    pub developer: String,
    pub website: String,
    pub issues: String,
    pub copyright: String,
    pub about_text: String,
    pub credits_music: String,
    pub credits_built: String,
    pub sources: Vec<String>,
    pub built_with: Vec<String>,
    pub whats_new: String,
    pub report_issue: String,
}

#[uniffi::export]
pub fn help_words() -> HelpWords {
    use radiance_core::{guide as g, help_words as h, update_words as u, words as w};
    let pairs = |list: &[(&str, &str)]| list.iter().map(|(n, url)| format!("{n} {url}")).collect::<Vec<_>>();
    HelpWords {
        update_title: u::TITLE.into(),
        keeps_playing: u::KEEPS_PLAYING.into(),
        remind_later: u::REMIND_LATER.into(),
        skip_version: u::SKIP_VERSION.into(),
        cancel_download: u::CANCEL_DOWNLOAD.into(),
        from_github: u::FROM_GITHUB.into(),
        look: u::LOOK.into(),
        report_title: h::REPORT_TITLE.into(),
        report_intro: h::REPORT_INTRO.into(),
        report_heading: h::REPORT_HEADING.into(),
        report_what: h::REPORT_WHAT.into(),
        cancel: h::CANCEL.into(),
        send: h::SEND.into(),
        sending: w::SENDING.into(),
        sent_title: h::SENT_TITLE.into(),
        copy_code: h::COPY_CODE.into(),
        close: h::CLOSE.into(),
        rate_title: h::RATE_TITLE.into(),
        rate_head: h::RATE_HEAD.into(),
        rate_sub: h::RATE_SUB.into(),
        yes: h::YES.into(),
        not_really: h::NOT_REALLY.into(),
        rate_areas: h::RATE_AREAS.iter().map(|a| a.to_string()).collect(),
        rate_comment: h::RATE_COMMENT.into(),
        cancel_sign_in: h::CANCEL_SIGN_IN.into(),
        sign_out: h::SIGN_OUT.into(),
        not_now: h::NOT_NOW.into(),
        dont_ask: h::DONT_ASK.into(),
        rate_choose: h::RATE_CHOOSE.into(),
        privacy: h::PRIVACY.into(),
        privacy_word: h::PRIVACY_WORD.into(),
        thanks: h::THANKS.into(),
        thanks_share: h::THANKS_SHARE.into(),
        thanks_plain: h::THANKS_PLAIN.into(),
        share_app: h::SHARE_APP.into(),
        share_link: h::SHARE_LINK.into(),
        share_toast: h::SHARE_TOAST.into(),
        done: h::DONE.into(),
        storage_title: h::STORAGE_TITLE.into(),
        kept_group: h::KEPT_GROUP.into(),
        folder: h::FOLDER.into(),
        change: h::CHANGE.into(),
        open_folder: h::OPEN_FOLDER.into(),
        space_group: h::SPACE_GROUP.into(),
        pick_folder: h::PICK_FOLDER.into(),
        move_title: w::MOVE_TITLE.into(),
        move_word: w::MOVE.into(),
        offline: h::OFFLINE.into(),
        offline_tip: h::OFFLINE_TIP.into(),
        autoplay: w::AUTOPLAY.into(),
        between_songs: w::BETWEEN_SONGS.into(),
        no_transition: w::NO_TRANSITION.into(),
        crossfade: w::CROSSFADE.into(),
        dj_mix: w::DJ_MIX.into(),
        mine_tip: radiance_core::suggest::MINE_TIP.into(),
        guide_title: g::TITLE.into(),
        guide_skip: g::SKIP.into(),
        guide_next: g::NEXT.into(),
        guide_start: g::START.into(),
        about: h::ABOUT.into(),
        app_name: h::APP_NAME.into(),
        developer: h::DEVELOPER.into(),
        website: h::WEBSITE.into(),
        issues: h::ISSUES.into(),
        copyright: h::COPYRIGHT.into(),
        about_text: h::ABOUT_TEXT.into(),
        credits_music: h::CREDITS_MUSIC.into(),
        credits_built: h::CREDITS_BUILT.into(),
        sources: pairs(&h::SOURCES),
        built_with: pairs(&h::BUILT_WITH),
        whats_new: h::WHATS_NEW.into(),
        report_issue: h::REPORT_ISSUE.into(),
    }
}

#[cfg(test)]
mod tests {

    // ---- stage 4a

    #[test]
    fn the_update_pop_up_says_each_step_as_the_linux_app() {
        use radiance_engine::controller::{UpdateState, UpdateStep};
        let r = radiance_engine::update::Release { version: "0.8.0-beta.4".into(), notes: "- **One**\n- two\n- three\n- four\n- five".into(), size: 150_000_000, ..Default::default() };
        let mut u = UpdateState { step: UpdateStep::Out, release: Some(r), asks: true, page_only: false, installer: true, current: "0.8.0-beta.3".into(), quit: false };
        let v = update_view(&u, false);
        assert_eq!((v.step, v.title.as_str(), v.main.as_str()), (UpdateStepView::Out, "Radiance Music 0.8.0-beta.4 is out", "Update Now (150 MB)"));
        assert_eq!(v.bullets, ["One", "two", "three", "four"]);
        u.step = UpdateStep::Downloading(0.5);
        assert_eq!(update_view(&u, false).progress, "75 MB of 150 MB");
        u.step = UpdateStep::Ready;
        assert!(update_view(&u, true).text.contains("and stops the music; the installer puts it on"));
        u.step = UpdateStep::Failed("Couldn't update: stopped".into());
        let f = update_view(&u, false);
        assert_eq!((f.title.as_str(), f.text.as_str()), ("Couldn't update: stopped", "Nothing changed: you're still on 0.8.0-beta.3."));
        assert!(update_view(&UpdateState::default(), false).page.contains("releases/latest"));
    }

    #[test]
    fn storage_shows_the_linux_apps_rows() {
        let mut st = radiance_engine::controller::StorageState { folder: "D:/Music/Radiance Music".into(), ..Default::default() };
        let counting = storage_view(&st);
        assert!(counting.counting && counting.total == "Counting…" && counting.rows.is_empty());
        let mut u = radiance_engine::storage::Usage::default();
        u.songs.files = 2;
        u.songs.bytes = 2_000_000;
        st.usage = Some(u);
        let v = storage_view(&st);
        assert_eq!(v.total, "2 MB in your songs' folder");
        assert_eq!(v.rows.len(), 7);
        assert_eq!((v.rows[0].title.as_str(), v.rows[0].subtitle.as_str()), ("Songs", "2 files"));
        assert_eq!(v.rows[5].title, "This computer's splitter");
    }

    #[test]
    fn the_guide_and_whats_new_come_from_core_and_the_changelog() {
        assert_eq!(guide_cards().len(), 5);
        assert_eq!(guide_cards()[0].title, "Search Any Song");
        let notes = whats_new();
        assert!(!notes.is_empty() && notes.iter().all(|n| n.heading.starts_with("Windows beta")), "{notes:?}");
        assert_eq!(storage_target("D:/Songs".into()), std::path::Path::new("D:/Songs").join("Radiance Music").display().to_string());
        let w = help_words();
        assert_eq!((w.report_title.as_str(), w.rate_areas.len(), w.offline.as_str()), ("Report a Problem", 5, "Offline"));
    }

    use super::*;

    #[test]
    fn downloading_says_how_many_and_lists_them() {
        use radiance_core::library::Saved;
        let song = |id: &str| radiance_core::track::Track { source: radiance_core::track::Source::YouTube, id: id.into(), title: id.into(), artist: "X".into(), album: String::new(), seconds: 0, art: String::new(), format: String::new(), file: None, video: false, genres: None };
        let v = downloading_view(&[(song("a"), Saved::Saving), (song("b"), Saved::NotSaved)], Some("Fix missing (2)".into()), true, &Rows::default());
        assert_eq!(v.title, "Downloading (2)");
        assert_eq!(v.note, "2 liked songs aren't saved on this computer yet: 1 saving now. They play from the internet until they're saved.");
        assert_eq!(v.songs.len(), 2);
        assert_eq!((v.fix.as_deref(), v.fixing), (Some("Fix missing (2)"), true));
        let none = downloading_view(&[], Some("Fix missing (3)".into()), false, &Rows::default());
        assert_eq!((none.title.as_str(), none.songs.len()), ("Downloading", 0));
        assert_eq!(none.fix, None, "no songs listed: no Fix missing beside \"Every liked song is saved\" (it counts music videos too)");
    }

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
    fn the_mix_view_names_the_parts_in_the_linux_words() {
        let m = controller::MixState { parts: vec![("drums".into(), 1.0), ("vocals".into(), 0.3)], voice: radiance_core::mix::Voice::NoVocals, effects: radiance_engine::effects::Sound::OFF, plays_parts: true, note: String::new(), splitting_for: None };
        let v = mix_view(&m);
        assert_eq!(v.parts.iter().map(|p| p.title.as_str()).collect::<Vec<_>>(), ["Drums", "Vocals"]);
        assert_eq!(v.parts[1].gain, 0.3);
        assert_eq!(v.voice, VoiceMode::NoVocals);
        assert_eq!((v.low_cut, v.high_cut, v.reverb), (20.0, 20000.0, 0.0));
        assert!(v.plays_parts);
    }

    #[test]
    fn transition_modes_map_both_ways() {
        use radiance_engine::settings::Transition;
        for t in [Transition::Off, Transition::Crossfade, Transition::DjMix] {
            assert_eq!(Transition::from(TransitionMode::from(t)), t);
        }
        assert_eq!(fade_label(6.0), "Crossfade length: 6 s");
        let w = help_words();
        assert_eq!((w.between_songs.as_str(), w.dj_mix.as_str()), ("Between Songs", "DJ Mix (by the Beat)"));
    }

    #[test]
    fn the_mixer_split_view_carries_the_line_its_progress_and_the_choices() {
        use radiance_core::menu::{Action, Item};
        let m = radiance_core::mix::MixerSplit { line: Some("Splitting into four parts · 40%".into()), progress: Some(0.4), for_voice: false, items: vec![Item { label: "Split: Music + Vocals (Fast)".into(), action: Action::SplitTwo }] };
        let v = mixer_split_view(m);
        assert_eq!((v.line.as_str(), v.progress, v.turn_off), ("Splitting into four parts · 40%", 0.4, false));
        assert_eq!(v.items.len(), 1);
        let none = mixer_split_view(radiance_core::mix::MixerSplit { for_voice: true, ..Default::default() });
        assert_eq!((none.line.as_str(), none.progress, none.turn_off), ("", -1.0, true), "no progress known: -1 (a moving bar)");
    }

    #[test]
    fn split_marks_reach_the_rows() {
        use radiance_engine::track::Source;
        let t = Track { source: Source::YouTube, id: "sm".into(), title: "Marked".into(), artist: "Ann".into(), album: String::new(), seconds: 200, art: String::new(), format: String::new(), file: None, video: false, genres: None };
        let mut rows = Rows::default();
        assert_eq!(rows.song(&t).split, SplitMarkView::None);
        rows.marks.insert(t.key(), controller::SplitMark::Waiting(2));
        assert_eq!(rows.song(&t).split, SplitMarkView::Waiting { place: 2 });
        rows.marks.insert(t.key(), controller::SplitMark::Four);
        assert_eq!(rows.song(&t).split, SplitMarkView::Four);
    }

    #[test]
    fn edited_lines_go_through() {
        let rows = edited_rows(vec![EditedLineView { at: Some(1.5), text: "one".into(), singer: Some("Nairud".into()) }, EditedLineView { at: None, text: "two".into(), singer: None }]);
        assert_eq!(rows, vec![(Some(1.5), "one".to_string(), Some("Nairud".to_string())), (None, "two".to_string(), None)]);
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
        assert_eq!(beta_version(), "0.8.0-beta.7");
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
