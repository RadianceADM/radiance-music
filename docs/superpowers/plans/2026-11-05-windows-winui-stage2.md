# Radiance Music for Windows, native (WinUI 3): stage 2 implementation plan (Library)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The beta's second release.
- **Library pages:** Liked Songs (A–Z rail, its own search, Play / Shuffle, Fix missing), Albums / Artists / Genres, History and Playlists.
- **Liking and saving:** liking a song saves it.
- **Queue panel:** Played before, drag to reorder, remove.
- **Lyrics column:** grows toward the middle, scrolls to the sung line, lights word by word, shows singer names.
- **Music videos:** drawn by the compositor.

All of it behaves as the Linux app does.

**Architecture:**
- Behaviour first moves out of `src/ui.rs` into `core` (pure) and `engine` (state, saving, db), and the GTK app calls the moved code.
- The engine's `Controller` grows commands and state for each area.
- `ffi` exposes them.
- The C# app adds pages to its NavigationView, a right-hand SplitView for the queue panel, a karaoke lyrics column, and a SwapChainPanel for videos.
- The video goes through GStreamer's `d3d12swapchainsink`. The C# app attaches its DXGI swapchain to the panel, and the compositor draws it.

**Tech Stack:**
- Rust: as stage 1 (uniffi `=0.31.0`).
- Windows: MSYS2 UCRT64 GStreamer 1.28.8 on ribs03, which ships `d3d12swapchainsink`. .NET 10, Windows App SDK 2.6.5.

**Spec:** `docs/superpowers/specs/2026-11-05-windows-winui-design.md`, Stages → 2 (issue #45). Stage 1's plan, `docs/superpowers/plans/2026-11-05-windows-winui-stage1.md`, explains the build pipeline.

## Global Constraints

Stage 1's constraints hold, with what stage 1 settled:

**Where builds run**
- Rust and Linux builds run on **ribs02**:
  - a copy at `/srv/build/winui/radiance-music`, with `CARGO_TARGET_DIR=/srv/build/winui/target` and `TMPDIR=/srv/build/winui/tmp`;
  - `touch` edited sources after the rsync (rsync keeps the laptop's file times, and cargo can skip the rebuild);
  - the lockfile cargo changes there is copied back and committed.
- Windows builds run on **ribs03 only**, through `windows\build.ps1 -Step rust|app|dist|installer|all`:
  - at Idle priority;
  - MSYS2 UCRT64's GNU toolchain and GStreamer;
  - `dotnet build` output, not `dotnet publish`.
- Getting the work to ribs03: branch `winui-stage2` from `main`, moved by `git bundle create x.bundle main..winui-stage2` and `git fetch x.bundle winui-stage2:winui-stage2` in `C:\rb\beta\src`. Pushing the branch waits for a dialog.
- ribs03 is the owner's PC:
  - announce each build or test there;
  - windows open only through `windows\test-run.ps1 -AppArgs …` (the `RadianceMusicTest` task);
  - test data only in `C:\rb\beta-test`;
  - never install or update the owner's app;
  - no media-key presses.

**Engine rules**
- `core` has no GLib or GStreamer: `cargo tree -p radiance-core -e normal` lists neither.
- After every task, all of these pass on ribs02:
  - `cargo test --release -p radiance-core -p radiance-engine -p radiance-ffi -p radiance-music`
  - `cargo build --release --locked`
- Behaviour is written once, in Rust. When a task moves something out of `ui.rs`, the GTK app calls the moved code in the same task, and its words and behaviour don't change.
- Commands that name a song carry its `key` beside any index, and the engine ignores a command whose key no longer matches (stage 1's `play_from` rule).

**Words and look**
- Every text the beta shows comes from `core::words` (or a new `core` function a task names). The strings in this plan are copied from the GTK code; keep them exactly.
- Windows' own look: theme brushes, the type ramp, NavigationView, native ListView/GridView, ContentDialog and MenuFlyout. Nothing sharp, and every button carries a leading icon.
- Version `0.8.0-beta.2`, set in `ffi/Cargo.toml` and the csproj.

**Stage boundaries**
- No splitting here: liking a song doesn't ask for a split in the beta, and the Splits tab, split badges and split menu items wait for stage 3.
- Servers, sharing, Copy Link, Track Info, Show / Edit / Find Lyrics Again and the lyrics editor wait for stages 3–4.
- YouTube likes: the beta builds without `save-youtube`, as every public build does. A liked YouTube song is kept but streams, and its row says "{byline} · streams".

**Sound and Git**
- `RADIANCE_SILENT=1` in every test.
- Commits are authored "Bassi <villenabassi@gmail.com>", end with the Co-Authored-By line, and say `Refs #45`.
- Merging to `main`, pushing, and any release each wait for a dialog.

## Review Focus

1. **A song unliked while it's being saved:** the save finishes and is then thrown away. No file is left behind, and no "Saved" toast appears. Test: `unliking_while_saving_leaves_nothing` (Task 3).
2. **A queue command for a row that moved:** the queue changed between drawing and clicking (autoplay added songs, or the song changed). It acts on the song whose key it names, or does nothing. It never acts on whatever sits at that index now. Test: `queue_commands_follow_the_key` (Task 6).
3. **Liked Songs search with accents or capitals:** "beyonce" finds "Beyoncé", the A–Z rail greys out the letters with no match, and Play plays only what's shown. Test: `the_liked_page_filters_like_the_linux_app` (Task 4).
4. **Lyrics with no word times, overlapping voices, or empty lines:** a line without word times lights whole, two singers can be lit at once, and empty lines show "♪". The column never scrolls to a line that isn't lit. Tests: `sung_follows_core_sing` (Task 7) and `LyricClockTests` (Task 13).
5. **Switching to the video and back mid-song:** the position carries over. A video whose stream can't be found leaves the song playing and shows the problem. The panel never shows a stale frame from the last video. Test: `switching_versions_keeps_the_place` (Task 14).

---

### Task 1: core — the library's words, groups and letters

**Files:**
- Create: `core/src/library.rs` (register in `core/src/lib.rs`)
- Modify: `src/ui.rs`, so the GTK app calls the moved code:
  - `groups_by` (4922);
  - `filter_liked` (4784): its letters;
  - `liked_row` (4911): its subtitle;
  - `counts_text` (568);
  - the description words (4672–4800, 4348–4380);
  - `render_recent` (4518): its words.
- Test: `core/src/library.rs` (unit tests)

**Interfaces:**
- Produces:
  - `pub enum GroupKind { Albums, Artists, Genres }`
  - `pub struct Group { pub name: String, pub songs: Vec<Track> }`
  - `pub fn groups(liked: &[Track], kind: GroupKind) -> Vec<Group>`. A–Z by lowercased name, with liked order inside each group, no song twice. An empty album goes under "Singles". Genres: `Some([])` goes under "Other", `None` under "Finding the genre…". Artists come from `t.artists()`.
  - `pub const AZ: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ#"`
  - `pub fn letters(shown: &[Track]) -> Vec<(char, Option<usize>)>`. One entry per `AZ` letter, holding the index of the first shown row whose title's `track::letter_of` is that letter, or `None` (greyed out).
  - `pub fn liked_note(total: usize, dir: &str, q: &str, found: usize) -> String`:
    - empty library: "Like a song (the heart) and it's kept here. Lossless songs are saved with their lyrics and album art."
    - no search: "{n} songs, saved in {dir}"
    - no match: "None of your {n} liked songs match “{q}”. Search finds it online."
    - else: "{found} of your {n} liked songs match “{q}”" ("matches" when found is 1)
  - `pub enum Saved { Saving, NotSaved, Streams, Saved }` and `pub fn row_subtitle(byline: &str, s: Saved) -> String`. The words are "Saving…", "Not saved yet", "{byline} · streams", and the byline alone.
  - `pub fn counts_text(plays: u32, skips: u32) -> String`, copied from `ui.rs:568` ("Played N times · skipped N times").
  - `pub fn queue_note(len_after: usize, autoplay_adding: bool, nothing_at_all: bool) -> String`, the four `render_queue` descriptions verbatim.
  - `pub const RECENT_ROWS: usize = 5` and `pub const PLAYED_BEFORE_NOTE: &str = "Go back to one: it plays now, then the queue carries on"`.

- [ ] **Step 1: Write the failing tests.** `groups_put_singles_other_and_finding_where_the_linux_app_does` (three songs, one with an empty album, one with genres `Some([])`, one with `None`); `an_artist_group_lists_each_credited_artist` ("A & B" lands in both); `letters_grey_out_empty_ones_and_hash_takes_the_rest` ("1999" → '#', "Ánimo" → 'A'); `the_liked_note_says_what_the_linux_app_says` (all four cases, including "1 of your 3 liked songs matches “x”"); `queue_notes_are_verbatim`.
- [ ] **Step 2:** `cargo test -p radiance-core library::` → FAIL (module missing).
- [ ] **Step 3:** Implement the functions in `core/src/library.rs`, moved from `ui.rs` (the same rules, the same words). Point the GTK code at them.
- [ ] **Step 4:** Run the whole Rust suite on ribs02 (Global Constraints) → PASS. Take Linux screenshots of Liked Songs, Albums and the queue panel (`RADIANCE_SHOT_PAGE=liked|albums`, `RADIANCE_SHOT_PANEL=queue`, test data), each the same as before the move.
- [ ] **Step 5:** Commit "core: the library's words, groups and A–Z letters, out of ui.rs".

### Task 2: History by day

**Files:**
- Create: `engine/src/history.rs` (the days, using `glib::DateTime` local time) and `core::library::day_label`
- Modify: `src/ui/home.rs` `render_history` (965), to call them
- Test: `core/src/library.rs` and `engine/src/history.rs`

**Interfaces:**
- Produces:
  - `pub fn day_label(days_ago: i64, weekday: &str, full_date: &str) -> String` in `core::library`: 0 → "Today", 1 → "Yesterday", 2..=6 → the weekday, otherwise the full date. A `days_ago` below 0 counts as "Today".
  - `pub struct Day { pub label: String, pub plays: Vec<(Track, String /* "%-I:%M %p" */)> }` and `pub fn days(plays: &[(Track, i64)], now_unix: i64) -> Vec<Day>` in `engine::history`:
    - at most 400 plays, newest first;
    - plays without a time (0) go under "Earlier";
    - the full date is formatted "%A, %B %-d, %Y";
    - the empty text constant is `HISTORY_EMPTY = "Songs you play show up here, with when you played them."`.

- [ ] **Step 1: Write the failing tests.**
  - `day_labels_count_back_from_today` (0, 1, 3, 9 days ago, and −1).
  - `plays_are_grouped_by_local_day`: plays at now, now − 26 h and now − 10 days give "Today", "Yesterday" and the date.
  - `untimed_plays_go_under_earlier`.
- [ ] **Step 2:** Run them → FAIL.
- [ ] **Step 3:** Implement, and point `render_history` at `history::days`.
- [ ] **Step 4:** Suite → PASS. A Linux screenshot of History with test plays matches the one before.
- [ ] **Step 5:** Commit "History's days in the engine (the GTK page calls them)".

### Task 3: engine — liking and saving

**Files:**
- Create: `engine/src/liking.rs`
- Modify:
  - `src/ui.rs`: `toggle_like` (4603), `save_liked` (4632), `save_extras` (5822), `keep_video` (360), `keep_lyrics_file` (375), `recheck_lyrics` (390), `missing` (4744), `fix_missing` (4768). The decisions and the file work move to `liking`; GTK keeps its threads and toasts and calls `liking`.
  - `engine/src/controller.rs`: the engine's liked list becomes live; adds `toggle_like`, `fix_missing`, `Change::Liked`, `Change::Toast` and `toast()`.
- Test: `engine/tests/controller.rs`, plus unit tests in `liking.rs`

**Interfaces:**
- Produces:
  - `pub enum Like { Liked { save: bool }, Unliked { trashed: bool } }`
  - `pub fn toggle(liked: &mut Vec<Track>, t: &Track) -> Like`. Liking puts the song first and clears `file`; unliking removes it. It calls `library::store`. `save` = `library::saves(t)`.
  - `pub fn save(t: &Track, sources: &dyn Sources) -> Result<Track, String>`: the file, lyrics and cover, as `library::save`, with the Archive download going through `sources.download`. Returns the track with `file` set. GTK passes `&Real`.
  - `pub fn keep_video(t: &Track) -> Result<Option<String>, String>` and `pub fn keep_lyrics_file(t: &Track)`, moved as they are.
  - `pub fn missing(liked: &[Track]) -> (Vec<Track>, Vec<Track>)`, moved.
  - Words in `core::words` (moved from `ui.rs`, verbatim):
    - `liked_toast(title)` "Liked “X”"
    - `saved_toast` "Saved “X”"
    - `unliked_toast(title, trashed)` "Removed “X” from Liked; its saved copy is in the Trash" / "Removed “X” from Liked"
    - `save_failed` "Couldn't save “X” right now. It tries again next time the app opens."
    - `fix_label(n, fixing)` "Fix missing (N)" / "Fixing… N left"
    - `fixing_toast(songs, videos)`, the three "Saving N … in the background" forms
  - Controller:
    - `pub fn toggle_like(&self, key: &str)` saves in the background one at a time (a `saving` set, as GTK's), and on start saves the likes that weren't saved yet.
    - `pub fn fix_missing(&self)`.
    - `pub fn toast(&self) -> Option<String>` (the latest, taken once) with `Change::Toast`.
    - `Change::Liked` whenever the liked list or a song's saved state changes.
    - `pub fn liked(&self) -> Vec<(Track, core::library::Saved)>`.
    - `NowState.liked: bool`.
- Consumes: `library::{load, store, saves, save, unsave}`, `db::{set_liked, add_video, video_file}`.

- [ ] **Step 1: Write the failing tests** (in `engine/tests/controller.rs`, with `Fake` sources and WAV songs):
  - `liking_puts_the_song_first_and_says_so`: toast "Liked “Song A”" for a YouTube song (no save without `save-youtube`), and `Change::Liked` arrives.
  - `unliking_removes_it_and_says_so`.
  - `unliking_while_saving_leaves_nothing`: an Archive song whose save is slowed by the Fake; unliking during the save leaves no file in the music folder and no "Saved" toast. Add `Sources::download(&self, url: &str, to: &Path) -> Result<(), String>` to the trait (Real = today's download code) so the test controls it.
  - `the_bar_heart_follows_the_song`: `now().liked` flips after `toggle_like`.
- [ ] **Step 2:** Run them → FAIL.
- [ ] **Step 3:** Implement `liking.rs`, move the GTK code onto it (no change in what GTK shows), and add the controller commands. Guard the race from Review Focus 1: after a background save, if the key is no longer liked, unsave the result and say nothing.
- [ ] **Step 4:** Suite → PASS. On Linux: like and unlike from a screenshot copy, compared before and after.
- [ ] **Step 5:** Commit "Liking and saving in the engine (both apps)".

### Task 4: engine — Liked Songs, the library groups and History as controller state

**Files:**
- Modify: `engine/src/controller.rs`, `engine/src/sources.rs` (`fn genres(&self, t: &Track) -> Vec<String>`, Real = `genre::lookup`)
- Test: `engine/tests/controller.rs`

**Interfaces:**
- Produces:
  - `pub struct LikedPage { pub songs: Vec<(Track, Saved)>, pub note: String, pub letters: Vec<(char, Option<usize>)>, pub fix: Option<String>, pub fixing: bool }` and `pub fn liked_page(&self, q: &str) -> LikedPage`: A–Z by `track::file_key`, filtered by `track::matches`.
  - `pub struct Tile { pub name: String, pub sub: String, pub art: String, pub round: bool }`:
    - Albums: `sub` = the first song's `credit()`.
    - Artists: `round`, "N song(s)".
    - Genres: `art` = the first song's art key, for the wash.
  - `pub fn library(&self, kind: GroupKind) -> Vec<Tile>`.
  - `pub struct GroupPage { pub title: String, pub sub: String, pub art: String, pub round: bool, pub songs: Vec<Track> }` and `pub fn group(&self, kind: GroupKind, name: &str) -> Option<GroupPage>`. Subtitles: albums "{credit} · N song(s)", others "N song(s)".
  - `pub fn history(&self) -> Vec<history::Day>`.
  - `ListRef` gains `LikedShown`, `Group(GroupKind, String)`, `History` and `Playlist(i64)`. Lists with data stop being `Copy`: change `ListRef` to `Clone` and adjust the callers.
  - `play_from` and `shuffle_from` play what the page shows: the filtered liked list for the last `liked_page(q)` query, the group's songs, or history's plays.
  - Genre lookups run in the background on the engine's own timer: one song every 250 ms, as GTK's `fill_genres`. Each result is stored and followed by `Change::Library`. Add `Change::{Library, History}`; a play is followed by `Change::History`.
- Consumes: Task 1 (`groups`, `letters`, `liked_note`), Task 2 (`history::days`), Task 3 (the live liked list).

- [ ] **Step 1: Write the failing tests:**
  - `the_liked_page_filters_like_the_linux_app`: "beyonce" finds "Beyoncé", `letters` greys out the rest, and `play_from(LikedShown, 0, key)` plays the first shown.
  - `albums_artists_and_genres_come_from_the_liked_songs`.
  - `genres_arrive_one_by_one`: the Fake answers `genres` and `Change::Library` follows.
  - `history_groups_today`.
- [ ] **Step 2:** Run them → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Suite → PASS.
- [ ] **Step 5:** Commit "The library's pages as engine state".

### Task 5: engine — Playlists

**Files:**
- Modify: `engine/src/controller.rs` and `core/src/words.rs` (the words)
- Test: `engine/tests/controller.rs`

**Interfaces:**
- Produces:
  - `pub fn playlists(&self) -> Vec<(i64, String, usize)>`
  - `pub struct PlaylistPage { pub name: String, pub count: String, pub songs: Vec<Track>, pub empty: Option<String> }` and `pub fn playlist(&self, id: i64) -> Option<PlaylistPage>`
  - `pub fn create_playlist(&self, name: &str, first: Option<&str>) -> Option<i64>`
  - `pub fn rename_playlist(&self, id: i64, name: &str)`
  - `pub fn delete_playlist(&self, id: i64)`
  - `pub fn add_to_playlist(&self, id: i64, key: &str)`, which toasts "Added to “X”" / "Already in “X”"
  - `pub fn remove_from_playlist(&self, id: i64, key: &str)`
  - `pub fn next_playlist_name(&self) -> String` ("My Playlist N")
  - `Change::Playlists`
  - Words, verbatim from `ui.rs`:
    - empty playlist: "No songs in it yet. Add one from any song's menu: Add to Playlist."
    - delete dialog: "Delete “X”?" / "The playlist goes; its songs stay in your library."
    - name dialog: "New Playlist" / "Rename Playlist", buttons "Create" / "Rename", placeholder "Playlist name"
    - nav tooltip: "{name}: N song(s)"
    - the delete button's tooltip: "Delete the playlist"
- Consumes: the existing `db::{playlists, playlist_songs, create_playlist, rename_playlist, delete_playlist, add_to_playlist, remove_from_playlist}` (playlists aren't reordered: parity with Linux).

- [ ] **Step 1: Write the failing tests:**
  - `a_playlist_is_made_named_and_emptied`.
  - `adding_twice_says_already_in`.
  - `the_next_name_counts_up` ("My Playlist 1", then "My Playlist 2").
- [ ] **Step 2:** Run them → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Suite → PASS.
- [ ] **Step 5:** Commit "Playlists through the engine".

### Task 6: engine — the queue panel's state and commands, and the song menu

**Files:**
- Modify: `engine/src/controller.rs`, `core/src/queue.rs` (as needed), and `src/ui.rs` (`render_recent` and `open_menu`'s rules call the moved code)
- Create: `core/src/menu.rs`
- Test: `engine/tests/controller.rs` and `core/src/menu.rs`

**Interfaces:**
- Produces:
  - `pub struct QueuePanel { pub before: Vec<Track> /* ≤ RECENT_ROWS, oldest first */, pub rows: Vec<Track> /* from the song playing, ≤ 100 */, pub note: String }` and `pub fn queue_panel(&self) -> QueuePanel`.
  - Commands:
    - `play_again(&self, key: &str)`: `insert_next` + `set_index`, no skip counted.
    - `play_next(&self, key: &str)`: toast "“X” plays next".
    - `enqueue(&self, key: &str)`: toast "Added “X” to the queue".
    - `unqueue(&self, row: usize, key: &str)`.
    - `move_in_queue(&self, from: usize, to: usize, key: &str)`: never above the song playing (`Queue::move_item`).
    - `start_radio(&self, key: &str)`: "Starting “X” radio…" / "“X” radio: N songs like it" / "YouTube Music has no radio for “X”".
  - `core::menu`:
    - `pub enum Place { Row, Queue, Playlist(i64), History }`
    - `pub enum Action { Play, PlayNext, AddToQueue, RemoveFromQueue, RemoveFromPlaylist(i64), AddToPlaylist(i64), NewPlaylistWith, Like, Unlike, Radio, GoToArtist(String), GoToAlbum, ShowInFiles }`
    - `pub struct Item { pub label: String, pub action: Action }`
    - `pub fn song_menu(t: &Track, place: Place, liked: bool, playlists: &[(i64, String)]) -> Vec<Vec<Item>>`. Stage-2 items in GTK's order and words:
      1. "Remove from Queue" (queue only), "Play", "Play Next", "Add to Queue";
      2. "Remove from This Playlist" (playlist only), "Add to Playlist" (each playlist by name, then "New Playlist…");
      3. "Like" / "Remove from Liked", "Song Radio";
      4. "Go to Artist" or one "Go to {artist}" per credited artist (≤ 8), "Go to Album";
      5. "Show in Files" (only for a saved file).
  - Controller: `pub fn do_menu(&self, key: &str, row: usize, action: Action)`. Go to … starts a search (`search(name)`) and sets `Change::Results`, as GTK does. Show in Files opens Explorer on the file (`explorer /select,` on Windows, the file manager on Linux) through `gio::AppInfo` or `std::process::Command`.
- Consumes: Task 3 (likes), Task 5 (playlists).

- [ ] **Step 1: Write the failing tests:**
  - `queue_commands_follow_the_key`: unqueue or move with a stale key does nothing; with the right key, it acts.
  - `played_before_lists_the_last_five_oldest_first`.
  - `nothing_moves_above_the_song_playing`.
  - `the_menu_has_gtk_s_items_in_gtk_s_order` (core, three places).
- [ ] **Step 2:** Run them → FAIL.
- [ ] **Step 3:** Implement. GTK's `open_menu` builds its stage-2 sections from `core::menu::song_menu` and appends its own later-stage items (split, server, lyrics, info, link) after them, as today.
- [ ] **Step 4:** Suite → PASS. A Linux screenshot of the right-click menu matches the one before.
- [ ] **Step 5:** Commit "The queue panel's commands and the song menu in the engine".

### Task 7: engine — lyrics word by word

**Files:**
- Modify: `engine/src/controller.rs` (`LyricsState::Lines` carries what the beta needs) and `core/src/sing.rs` (if a helper is needed)
- Test: `engine/tests/controller.rs`

**Interfaces:**
- Produces:
  - `pub struct SungLine { pub text: String, pub start: f64, pub end: f64, pub head: Option<String>, pub spans: Vec<(usize, usize)> }`. `text` is "♪" for an empty line. `head` = the singer, only where the voice changes from the line before (GTK's `LyricView`). `spans` = `sing::word_spans`.
  - `LyricsState::Lines(Vec<SungLine>)`, replacing `Vec<Line>` for the controller.
  - `pub fn sung(&self, at: f64) -> Vec<(usize /* line */, usize /* chars lit */)>` = `sing::lit_lines` then `sing::sung_chars` for each lit line. It's read from the shared `View` (no engine-thread hop), so the beta can ask 30 times a second.
  - `NowState.since: u64`: milliseconds on a monotonic clock when `at` was read, so the beta counts on between the 5 Hz readings.
- Consumes: `core::sing::{word_spans, sung_chars, lit_lines, WORD_LEAD, LINE_LEAD}`.

- [ ] **Step 1: Write the failing tests:**
  - `sung_follows_core_sing`: a two-word line with word times lights 0 chars before its lead, the first word at 0.08 s before it, and the whole line after its end. A line without word times lights whole.
  - `two_voices_light_together`.
  - `heads_only_where_the_voice_changes`.
  - `empty_lines_are_notes`.
- [ ] **Step 2:** Run them → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Suite → PASS.
- [ ] **Step 5:** Commit "Lyrics word by word for the beta".

### Task 8: ffi — the stage-2 surface

**Files:**
- Modify: `ffi/src/lib.rs` and `ffi/Cargo.toml` (version `0.8.0-beta.2`)
- Test: `ffi/src/lib.rs` tests

**Interfaces:**
- Produces records and enums mirroring Tasks 3–7:
  - Enums: `GroupKind`, `SavedState`, `MenuPlace` (= `core::menu::Place`: `Row`, `Queue`, `Playlist { id }`, `History`) and `MenuAction` (with data). uniffi enums with fields are fine; C# gets them as classes.
  - Records:
    - `LikedPageView { songs: Vec<SongView>, note, letters: Vec<LetterView { letter: String, first: Option<u32> }>, fix: Option<String>, fixing: bool }`
    - `TileView`, `GroupPageView`, `HistoryDayView { label, plays: Vec<PlayView { song: SongView, time: String }> }`
    - `PlaylistView { id: i64, name, count: u32 }`, `PlaylistPageView`
    - `QueuePanelView { before, rows, note }`
    - `MenuSectionView { items: Vec<MenuItemView { label, action: MenuAction }> }`
    - `SungLineView`, `LitView { line: u32, chars: u32 }`
  - `SongView` gains `liked: bool`, `saved: SavedState`, `subtitle: String` (from `row_subtitle`), `plays: u32` and `counts: String`.
  - `NowView` gains `liked: bool` and `since: u64`.
  - `ChangeKind` gains `Liked`, `Library`, `History`, `Playlists` and `Toast`.
  - Methods on `MusicEngine`:
    - `toggle_like(key)`, `fix_missing()`, `toast() -> Option<String>`
    - `liked_page(q)`, `library(kind)`, `group(kind, name)`, `history()`
    - `play_liked(index, key)`, `play_group(kind, name, index, key)`, `play_history(index, key)`, `play_playlist(id, index, key)`
    - `shuffle_liked()`, `shuffle_group(kind, name)`, `shuffle_playlist(id)`
    - `playlists()`, `playlist(id)`, `create_playlist(name, first)`, `rename_playlist(id, name)`, `delete_playlist(id)`, `add_to_playlist(id, key)`, `remove_from_playlist(id, key)`, `next_playlist_name()`
    - `queue_panel()`, `play_again(key)`, `play_next(key)`, `enqueue(key)`, `unqueue(row, key)`, `move_in_queue(from, to, key)`, `start_radio(key)`
    - `song_menu(key, place: MenuPlace) -> Vec<MenuSectionView>`, `do_menu(key, row, action: MenuAction)`
    - `sung(at) -> Vec<LitView>`
  - Free: `history_empty() -> String`, `played_before_note() -> String`.
  - These are separate `play_*` methods, not data on `ListKind`, so stage 1's C# calls stay as they are.

- [ ] **Step 1: Write the failing tests:**
  - `liked_page_view_carries_letters`.
  - `menu_actions_cross_with_their_data` (AddToPlaylist(7) → `core::menu::Action::AddToPlaylist(7)`).
  - `sung_views_are_u32`.
- [ ] **Step 2:** Run them → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** On ribs02, the suite and `cargo build -p radiance-ffi` → PASS. On ribs03, `build.ps1 -Step rust` → PASS: the DLL, then `-Step app` regenerates the C# binding and builds.
- [ ] **Step 5:** Commit "ffi: stage 2's commands and state".

### Task 9: C# — the Library in the NavigationView, Liked Songs, the song menu and the bar's heart

**Files:**
- Modify:
  - `windows/RadianceMusic.Windows/MainWindow.xaml(.cs)`: a "Library" header with items "Liked Songs" (tag `liked`), "History", "Albums", "Artists" and "Genres", in GTK's `NAV` order (`src/ui/home.rs:9`); a "Playlists" header filled in code (`playlist:{id}`, "New Playlist" `new-playlist`); `Show(tag, param)` re-navigates when the parameter changes.
  - `Controls/PlayerBar.xaml(.cs)`: a heart button (icon `HeartFill`/`Heart`, tooltips "Like: saves the song" / "Liked. Click to remove from Liked").
  - `Controls/SongRow.xaml(.cs)`: the heart; `subtitle`; the plays "N×" with `counts` as its tooltip; the context menu.
  - `EngineHub.cs`: `ChangeKind.Toast` → `MainWindow.Notice` as an informational InfoBar for 4 s.
- Create:
  - `Pages/LikedPage.xaml(.cs)`: search box (placeholder "Search your liked songs", `Debounce` 300 ms); "Play" / "Shuffle"; the "Fix missing (N)" button (disabled while fixing) with GTK's tooltip; the note; a ListView of `SongRow`.
  - `Controls/AzRail.xaml(.cs)`: 27 letters; click or drag jumps, tooltip "Jump to a letter"; greyed out with `first == null`; lights the letter of the top row on `ViewChanged`.
  - `Controls/SongMenu.cs`: builds a `MenuFlyout` from `song_menu` sections (`MenuFlyoutSeparator` between them, `MenuFlyoutSubItem` "Add to Playlist"); each item calls `do_menu`. "New Playlist…" opens Task 11's name dialog.
  - `Pure/AzLight.cs`: picks the lit letter from the rows' tops (within 40 px of the top, as `light_letter_in_view`).
- Test: `windows/RadianceMusic.Windows.Tests/PureTests.cs` (`AzLightTests`)

**Interfaces:**
- Consumes: Task 8's `liked_page`, `toggle_like`, `play_liked`, `shuffle_liked`, `fix_missing`, `song_menu`, `do_menu`, `toast`.
- Produces: `SongMenu.Attach(FrameworkElement row, string key, uint index, MenuPlace place)`, for Tasks 10–12.

- [ ] **Step 1: Write the failing test:** `AzLightTests`: rows at tops `[-300, -10, 30, 400]` with letters `[A, B, C, D]` light "B"; nothing lights when no row is at the top.
- [ ] **Step 2:** `build.ps1 -Step app` on ribs03 → FAIL (no `AzLight`).
- [ ] **Step 3:** Implement the pages and controls.
- [ ] **Step 4:** `-Step app` → PASS. Run `test-run.ps1 -AppArgs "--shot … --page liked --size 1280x800"`, plus 900x700 and 480x800 (the rail stays reachable). Take screenshots with the test data's liked songs, with a search typed, and with the menu open. Every button has its icon. Liking from Search shows the toast and the row's heart.
- [ ] **Step 5:** Commit "Beta: Liked Songs, the song menu and the heart".

### Task 10: C# — Albums, Artists, Genres and History

**Files:**
- Create:
  - `Pages/LibraryPage.xaml(.cs)`: one page for the three kinds (the parameter picks one); a GridView of tiles (square for albums, round for artists; genres 220×120 with the art's wash and a scrim, as GTK's `genre_tile`).
  - `Pages/GroupPage.xaml(.cs)`: a back button carrying the kind's name ("Albums" / "Artists" / "Genres"), the art at 220 px (round for artists), the title and subtitle, "Play" / "Shuffle", and rows.
  - `Pages/HistoryPage.xaml(.cs)`: a ListView grouped by day (a `CollectionViewSource` with `IsSourceGrouped`), the rows' time on the right, and the empty text.
- Test: screenshots (no new pure logic)

**Interfaces:**
- Consumes: Task 8's `library`, `group`, `play_group`, `shuffle_group`, `history`, `play_history`, `history_empty`; Task 9's `SongRow` and `SongMenu`.

- [ ] **Step 1:** Build the pages. Re-render on `ChangeKind.Library` / `History` (key-string compare, as stage 1's pages do).
- [ ] **Step 2:** `-Step app` → PASS. Take screenshots of Albums, an album, Artists, Genres and History at 1280x800 and 480x800. Genres show "Finding the genre…" first and fill in.
- [ ] **Step 3:** Commit "Beta: Albums, Artists, Genres, History".

### Task 11: C# — Playlists

**Files:**
- Create:
  - `Pages/PlaylistPage.xaml(.cs)`: the name, the count, "Play" / "Shuffle", Rename (icon `Rename`, tooltip "Rename"), Delete (icon `Delete`, tooltip "Delete the playlist"), rows (menu place Playlist), and the empty text.
  - `Dialogs/NameDialog.cs`: a ContentDialog with GTK's titles, button, placeholder and default name.
  - The delete confirm: a ContentDialog with Cancel / Delete; after it, go to Liked Songs.
- Modify: `MainWindow`, so the playlist items refresh on `ChangeKind.Playlists`.

**Interfaces:**
- Consumes: Task 8's playlist methods; Task 9's `SongMenu` ("Add to Playlist" → `add_to_playlist`, "New Playlist…" → `NameDialog`, then `create_playlist(name, key)`).

- [ ] **Step 1:** Build it.
- [ ] **Step 2:** `-Step app` → PASS. Screenshots: create "My Playlist 1" from a song's menu → it's in the nav with the song; rename it; delete it (dialog shown).
- [ ] **Step 3:** Commit "Beta: Playlists".

### Task 12: C# — the queue panel

**Files:**
- Modify:
  - `MainWindow.xaml(.cs)`: the page area inside a `SplitView` with `PanePlacement="Right"`, `OpenPaneLength` 360, Inline at a window width of 1300 or more (effective pixels) and Overlay below.
  - `Controls/PlayerBar`: a queue button (icon `List`, tooltip "Queue: what plays next") and a lyrics button (icon `Font`, opens the panel on Lyrics or closes it).
- Create:
  - `Controls/QueuePanel.xaml(.cs)`: a Pivot / SelectorBar with "Queue" | "Lyrics" (no "Splits" until stage 3).
    - Queue: the "Played before" group with its note, rows 0.7 opacity, a click = `play_again`. The "Queue" group: the song playing first ("Now playing · {byline}", accent background, not draggable), then a ListView of the rest with `CanReorderItems`; a drop calls `move_in_queue(from, to, key)`; each row's menu (place Queue).
    - The note under the title.
    - Lyrics: Task 13's column in its small size.
  - `Pure/QueueDrop.cs`: maps a ListView reorder (old index and new index among the rows after the song playing) to the engine's `from`/`to`.
- Test: `QueueDropTests`

**Interfaces:**
- Consumes: Task 8's `queue_panel`, `play_again`, `unqueue`, `move_in_queue`, `played_before_note`.

- [ ] **Step 1: Write the failing test:** `QueueDropTests`: with the song playing at queue index 4, moving the shown row 0 → 2 gives `from = 5`, `to = 7`; nothing maps above index 4.
- [ ] **Step 2:** `-Step app` → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `-Step app` → PASS. Screenshots at 1400x900 (inline) and 1000x700 (overlay); drag a row with `--play-test` data and check the order in a second screenshot.
- [ ] **Step 5:** Commit "Beta: the queue panel".

### Task 13: C# — the lyrics column word by word

**Files:**
- Modify: `Pages/NowPlayingPage.xaml(.cs)` (replacing stage 1's one TextBlock per line)
- Create:
  - `Controls/LyricsColumn.xaml(.cs)`, used by Now Playing (big) and the queue panel (small).
    - Each line is a `RichTextBlock`, or a `TextBlock` with two `Run`s: lit and unlit (40 % opacity, GTK's `fgalpha="40%"`). The singer `head` sits above at 55 % size and 75 % opacity, bold.
    - The sizes are GTK's: 2.05× and weight 800 for big, 1.4× in the panel; plain lyrics at 1.1×.
    - The lit line's opacity is 1; the others 0.34, with a 350 ms implicit composition animation.
    - Grows toward the middle: each line's Visual `Scale` comes from an `ExpressionAnimation` on the ScrollViewer's manipulation property set. The compositor runs it, with no per-frame C#. The scale is smoothstep by distance from the view's middle: down to 0.58 above (sung) and 0.7 below (coming).
    - The sung line (big only): a second TextBlock at 2.5× (at most 60 % of the view's height, at least 1.5×) cross-fades in over 400 ms while the old one shrinks back (`GROW_US`, ease-out cubic).
    - Scroll to the sung line: `ChangeView` to the line's settled middle when the first lit line changes. Top and bottom margins are half the view height minus 40.
    - Clicking a line seeks to its start.
    - Once a second, if the lead line has been out of sight twice, scroll back (GTK's `check_lyrics_shown`).
  - `Pure/LyricClock.cs`: `At(double engineAt, ulong since, ulong nowMs, bool playing) -> double`. It counts on from the 5 Hz reading only while playing, and never runs more than 1 s past the reading.
  - `Pure/LensScale.cs`: the scale formula, mirrored by the expression's text (`LensScale.Expression` builds the string), so the test pins both.
- Test: `LyricClockTests`, `LensScaleTests`

**Interfaces:**
- Consumes: Task 8's `lyrics()` (`SungLineView`s), `sung(at)` and `NowView.since`. A `DispatcherQueueTimer` at 33 ms runs only while the column is visible, the song plays and its lines have word times. No `CompositionTarget.Rendering`.

- [ ] **Step 1: Write the failing tests:**
  - `LyricClockTests`: playing, 400 ms after the reading → +0.4 s; paused → the reading; 3 s after → capped at +1 s.
  - `LensScaleTests`: the middle → 1.0; a line a whole view-half above → 0.58; below → 0.7; and `Expression` contains the same constants.
- [ ] **Step 2:** `-Step app` → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `-Step app` → PASS. With the test data's word-timed song, take screenshots 0.5 s apart while playing (silent): words lit partway along a line, the sung line large and in the middle, the lines above and below smaller. Also take a plain-lyrics song and a two-singer song. Measure: `measure-cpu.ps1 -App beta -Seconds 60` on Now Playing with word lyrics stays under 15 % (stage 1's aim).
- [ ] **Step 5:** Commit "Beta: lyrics word by word, growing toward the middle".

### Task 14: Music videos (a spike first, then the real thing)

**Files:**
- Modify:
  - `engine/src/player.rs`: a non-GTK video path, `#[cfg(not(feature = "gtk"))] pub fn load_video_swapchain(&self, picture: &str, sound: &str) -> Result<VideoOut, String>`, beside GTK's `load_video`. The same two-source or one-file pipeline, with the picture going `videoconvert ! d3d12swapchainsink` (on Windows) and the sound going through today's `fx ! volume ! SOUND_OUT`.
  - `engine/src/controller.rs`: play videos (stage 1 plays only their sound); `switch_version(want_video)`; `NowState.{video: bool, has_counterpart: bool, quality: String /* GTK's update_quality words */}`; `video_swapchain() -> Option<u64>`; `resize_video(w: u32, h: u32)`.
  - `ffi/src/lib.rs`: the same.
  - C#: `Pages/NowPlayingPage`: a `SwapChainPanel` in the art's place while a video plays; the video button on the art (icon `Video` / `MusicNote`, tooltips "Watch the music video" / "Back to the song"); double-click or the Fullscreen button (tooltips "Fullscreen (F11)" / "Leave fullscreen (Esc)") through `AppWindow.SetPresenter(FullScreen)`.
- Create: `windows/RadianceMusic.Windows/Interop/SwapChainPanelNative.cs` (`ISwapChainPanelNative`, IID `63aad0b8-7c24-40ff-85a8-640d944cc325`, `SetSwapChain(IntPtr)`).

**Interfaces:**
- Produces: `pub struct VideoOut { pub swapchain: u64 }`. The engine keeps the sink, which keeps the swapchain alive while it plays. `resize_video` emits the sink's `resize` action signal with the panel's pixel size.

- [ ] **Step 1 (spike, the gate):** on ribs03, an ffi `self_check`-style probe (`--probe-video FILE`) plays a short test video into `d3d12swapchainsink`, and a test window (`--play-test --video`) attaches the swapchain to a SwapChainPanel. Pass when a screenshot shows the frames, the panel follows a window resize, and `measure-cpu.ps1` on a 720p video stays under 15 %. If it fails: stop and bring the findings to the owner in a dialog (the fallback, appsink frames into a WriteableBitmap, costs CPU). Ledger the result.
- [ ] **Step 2: Write the failing test:** `switching_versions_keeps_the_place` (engine, with a Fake counterpart pointing at a local WAV "video" and `RADIANCE_SILENT`, where the sound path is what's checked): at 12 s, `switch_version(true)` plays the counterpart from about 12 s, and `switch_version(false)` comes back near the same place. Also `a_video_that_cant_play_leaves_the_song` (the Fake's stream fails → the song keeps playing and `problem()` says why, with GTK's words).
- [ ] **Step 3:** Run them → FAIL.
- [ ] **Step 4:** Implement (engine, ffi, C#). The SwapChainPanel's `SetSwapChain(IntPtr.Zero)` runs when the video stops or the song changes, so no stale frame stays.
- [ ] **Step 5:** Suite on ribs02 → PASS. On ribs03, `-Step all` → PASS. Take screenshots of a test video playing, after switching back to the song, and fullscreen. CPU stays under 15 % while a video plays.
- [ ] **Step 6:** Commit "Beta: music videos on the compositor".

### Task 15: The checkpoint: features list, version, installer and CPU

**Files:**
- Modify: `docs/FEATURES.md` (the Windows beta column: each stage-2 row from "🔜 2" to ✅, and only those that were checked), `CHANGELOG.md` (a "Windows beta 0.8.0-beta.2" section) and `windows/README.md`

- [ ] **Step 1:** On ribs03, run `build.ps1 -Step all` (Rust, C# tests, dist, self-check, installer; the installer isn't run) → PASS. Write down the installer's sha256 and size.
- [ ] **Step 2:** Run `measure-cpu.ps1 -App beta -Seconds 60` on Now Playing with word lyrics, on a video, and on Liked Songs while idle (each under 15 %, idle near 0). Compare with stage 1's numbers.
- [ ] **Step 3:** Commit "Windows beta 0.8.0-beta.2: stage 2 in the features list". Merging, pushing and releasing wait for the owner's dialogs.
