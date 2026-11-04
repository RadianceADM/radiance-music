# Radiance Music for Windows, native (WinUI 3): stage 1 implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The first beta of the native Windows app: the engine split out of the GTK app (Linux unchanged), a
uniffi DLL, and a C# WinUI 3 window with Home, Search, Now Playing (compositor-animated background), the player bar,
tray and media keys, measured for CPU on ribs03.

**Architecture:** One Cargo workspace. `core` holds pure behaviour (no GLib, no GStreamer). `engine` holds today's
non-screen modules plus a `Controller` that runs its own GLib main loop on a thread. `ffi` exposes the controller to C#
through uniffi 0.31. The GTK app (the root package) calls `core` and `engine` and keeps its own screens. The WinUI app
(`windows/RadianceMusic.Windows/`) is built and tested on ribs03 only.

**Tech Stack:** Rust 2021 (gtk4 0.11.5, glib/gio 0.22.10, gstreamer 0.25.4), uniffi `=0.31.0`, uniffi-bindgen-cs
`v0.11.1+v0.31.0`, .NET 10, Windows App SDK 2.6.5, GStreamer 1.28 (official MSVC build), NSIS 3.13.

**Spec:** `docs/superpowers/specs/2026-11-05-windows-winui-design.md` (issue #45). Read it with this plan.

## Global Constraints

**Where builds run**
- Rust and Linux builds run on **ribs02**:
  - copy at `/srv/build/winui/radiance-music`, rsynced from the laptop without `target/` and `.git/`;
  - `CARGO_TARGET_DIR=/srv/build/winui/target` and `TMPDIR=/srv/build/winui/tmp` (ribs02's `/tmp` is a shared RAM disk).
- Windows builds run on **ribs03 only**:
  - in `C:\rb\beta`, from branch `winui-stage1`, fetched by ribs03 from GitHub (`git fetch origin winui-stage1; git checkout -f FETCH_HEAD`);
  - nothing is built on the laptop.
- ribs03 is the owner's own PC:
  - announce each build or test there first, and run at Idle priority (`build.ps1` sets it);
  - show windows only through the `RadianceMusicTest` scheduled task;
  - never install or update the owner's Radiance Music;
  - install nothing system-wide without a dialog.

**Versions and project settings**
- uniffi `=0.31.0`.
- uniffi-bindgen-cs from `https://github.com/NordSecurity/uniffi-bindgen-cs`, tag `v0.11.1+v0.31.0`.
- .NET target `net10.0-windows10.0.19041.0`, min `10.0.17763.0`, x64, `win-x64`.
- `Microsoft.WindowsAppSDK` `2.6.5`, `Microsoft.Windows.SDK.BuildTools` `10.0.28000.2957`.
- `WindowsPackageType None`, `WindowsAppSDKSelfContained true`, `SelfContained true`.
- The settings above come from `~/Github/radiance-pos-apps/windows/RadiancePos/RadiancePos.csproj`.
- Rust for Windows is built with MSVC (`x86_64-pc-windows-msvc`) against GStreamer's own MSVC build.
  - Never link MSYS2's MinGW GStreamer into the DLL.
  - Keep MSYS2's `bin` off `PATH` during the MSVC build.

**Engine rules**
- `core` depends on neither GLib nor GStreamer: `cargo tree -p radiance-core -e normal` lists no `glib`, `gio` or `gstreamer`.
- The Linux app's behaviour doesn't change. After every task, all of these pass on ribs02:
  - `cargo test --release -p radiance-core -p radiance-engine -p radiance-music`
  - `cargo build --release --locked`
- Behaviour is written once, in Rust. Anything the beta needs that lives in `ui.rs` today moves to `core`/`engine`, and
  the GTK app calls the moved code.
  - GTK's own orchestration of playback (parts, voices, videos, parties, DJ mix) stays in `ui.rs` until the stage
    that ports it.

**Words, look and naming**
- Same words as the Linux app: every text the beta shows comes from `core::words` or is copied from the GTK code line
  cited in the task.
- Look: Radiance's dark "Stone" look, built from WinUI's own controls. Exact values are in Task 9's `Theme.xaml`.
  - Rounded corners and soft gradients, nothing sharp.
  - Every button carries a leading icon.
- The beta's name is "Radiance Music (beta)".
  - Version `0.8.0-beta.1` (the `ffi` crate and the C# assembly). The engine keeps the computer app's version (the
    workspace version).
  - Installer `RadianceMusic-Beta-Setup.exe`.
- Data: the beta opens the same folders as today's app, through GLib's `user_data_dir`, `user_config_dir` and
  `user_cache_dir`.
  - On ribs03, every beta run sets `XDG_DATA_HOME`, `XDG_CONFIG_HOME` and `XDG_CACHE_HOME` to
    `C:\rb\beta-test\{data,config,cache}`. It never uses the owner's folders.

**Sound and Git**
- No sound from any test: every engine or beta test run sets `RADIANCE_SILENT=1`, which makes `player.rs` use `fakesink`.
- Commits are authored "Bassi <villenabassi@gmail.com>", end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`
  and say `Refs #45`.
- Work happens on branch `winui-stage1`. Merging to `main`, and any release, waits for a dialog.

**The checkpoint:** if Now Playing while playing isn't clearly under ~60 % of one core on ribs03 (aim: under 15 %),
stop and take the numbers to the owner before stage 2.

## Review Focus

1. **The owner's own library on ribs03.** The beta shares today's data folders, so a test run that forgets the XDG
   overrides writes plays, history and the session into the owner's real library. Expected: every run refuses to
   start without them. The test is in Task 9: the self-check prints `data <dir>`, and `build.ps1` fails unless that
   dir is under `C:\rb\beta-test`.
2. **A song that can't be played** (offline, or yt-dlp fails): the beta should say why and go on to the next song,
   giving up after 3 in a row, exactly as the GTK app does. The test is in Task 6:
   `a_song_that_wont_play_is_skipped_then_it_stops_after_three`.
3. **Commands faster than the engine**, such as typing on in Search or pressing Next twice while a stream is being
   found: an older answer must never replace a newer one. The tests are in Task 5
   (`an_older_search_never_replaces_a_newer_one`) and Task 6 (`the_song_picked_last_wins`).
4. **Quitting while work is in flight**, such as closing during a search or a stream lookup: no crash and no callback
   into a closed window. The test is in Task 7: `quit_while_searching_is_quiet`.
5. **Long and non-Latin titles** (K-pop, OPM with long featured credits): one line with "…", no overlap, on cards,
   rows, the player bar and the tray tooltip (127 UTF-16 units). Checked in Tasks 10, 11 and 13 by a screenshot of a
   `kpop` search at 1280×800 and 400×780. Unit-tested in Task 14 with `TrayTextTests.cuts_at_127_utf16_units`.

---

## Part A: the workspace (ribs02)

### Task 1: The Cargo workspace and the `engine` crate (a move, no behaviour change)

**Files:**
- Modify: `Cargo.toml` (root: add `[workspace]` and `[workspace.package]`; keep only the GTK app's own dependencies)
- Create: `engine/Cargo.toml`, `engine/src/lib.rs`
- Move (`git mv`) from `src/` to `engine/src/`: `align archive art beat db effects feedback fx genre library limiter log lyrics mix net onnx_split player relay report servers session settings setup share split storage track update youtube` (`.rs` each)
- Modify: `src/main.rs` (replace `mod x;` with `use radiance_engine::x;` for each moved module)
- Modify: `.github/workflows/windows.yml` (its test step)

**Interfaces:**
- Produces: the crate `radiance-engine` (lib `radiance_engine`) with every moved module `pub mod`.
  - `crate::player`, `crate::track` and the other module paths in `src/ui*.rs` keep working, through the `use`s in
    `main.rs`.
- Produces: engine features `gtk` (the video paintable in `player.rs`), `save-youtube` and `onnx-split`.
  - The root package forwards them: `save-youtube = ["radiance-engine/save-youtube"]`,
    `onnx-split = ["radiance-engine/onnx-split"]`. The root always enables `radiance-engine/gtk`.

- [ ] **Step 1: Record the baseline on ribs02.**
  - Run: `cargo test --release 2>&1 | grep -E '^test result'` and save the total passed count.
  - Run: `target/release/radiance-music --version` and save the output.
  - Expected: `0 failed`, and `Radiance Music 0.7.19`.

- [ ] **Step 2: Write the workspace Cargo.toml files.**
  - Root:
    ```toml
    [workspace]
    members = [".", "core", "engine", "ffi"]
    default-members = ["."]
    resolver = "2"

    [workspace.package]
    version = "0.7.19"
    edition = "2021"
    license = "GPL-3.0-or-later"
    ```
    - The root `[package]` uses `version.workspace = true`.
    - `default-members = ["."]` keeps `cargo build --release` (Flatpak, Snap, `ribs03/build.sh`, CI) building only the
      GTK app at `target/release/radiance-music`.
    - Add `core` and `ffi` to `members` in the task that creates each. Until then, list only `[".", "engine"]`.
  - Engine:
    - Dependencies go with the modules that use them: gst, gst-app, realfft, async-channel, serde_json, serde_yaml,
      rusqlite, libc, ureq, image, tungstenite, sha2, getrandom, ort (optional), the Windows `zip` and `keyring`,
      and the Linux `keyring`.
    - `glib = "0.22"` and `gio = "0.22"` directly; these are the versions gtk4 0.11.5 uses.
    - `gtk = { package = "gtk4", version = "0.11", features = ["v4_14"], optional = true }`.
    - `version.workspace = true`, so `env!("CARGO_PKG_VERSION")` in `net.rs`, `update.rs`, `feedback.rs` and
      `report.rs` still reads 0.7.19.

- [ ] **Step 3: Move the modules and fix their paths.**
  - `git mv` each module.
  - In the engine:
    - `gtk::glib` becomes `glib`, and `gtk::gio` becomes `gio`.
    - `library.rs`'s `gtk::prelude` becomes `gio::prelude`.
    - `setup.rs`'s `include_str!("../server/…")` becomes `include_str!("../../server/…")`.
    - `player.rs`'s `load_video` gets `#[cfg(feature = "gtk")]` and uses `gtk::gdk::Paintable` from the optional dep.
  - Items the app uses that were `pub(crate)` become `pub`.
  - Constants that engine modules take from `main.rs` (for example `APP_ID`) move to `engine/src/lib.rs`, and
    `main.rs` uses them from there.
  - The modules that stay in the app are `main ui ui/* mpris smtc tray speaker watch`.
  - `qrcode` stays with the app if only `ui/sharing.rs` uses it.

- [ ] **Step 4: Run the whole suite and the smoke checks on ribs02.**
  - Run: `cargo test --release -p radiance-engine -p radiance-music 2>&1 | grep -E '^test result'`.
    Expected: the sum equals Step 1's count, `0 failed`.
  - Run: `cargo build --release --locked && target/release/radiance-music --version`.
    Expected: `Radiance Music 0.7.19`.
  - Run: `cargo tree -p radiance-engine --no-default-features -e normal | grep -c gtk4`. Expected: `0`.

- [ ] **Step 5: Check the Linux window and the Windows GTK build.**
  - Take a Home screenshot with the existing `RADIANCE_SHOT` mode on ribs02 (`/srv/build/src/updshots.sh`'s setup)
    and compare it with one taken by the 0.7.19 binary. Expected: the same.
  - In `.github/workflows/windows.yml`, change the test step to
    `cargo test --release -p radiance-engine -p radiance-music` (and add `-p radiance-core` in Task 2).
  - Push the branch to `windows-ci`. Expected: the run succeeds, with `folder vs ribs03: only ribs03 0, only GitHub 0`.

- [ ] **Step 6: Commit.** Message: `Workspace: the engine is a crate of its own (a move, nothing changes) Refs #45`.

### Task 2: The `core` crate (`Track` and the lyric types, no GLib)

**Files:**
- Create: `core/Cargo.toml` (deps: `serde_json = "1"` and `sha2 = "0.10"` only), `core/src/lib.rs`
- Move: `engine/src/track.rs` → `core/src/track.rs`
- Create: `core/src/lyrics.rs`. It takes from `engine/src/lyrics.rs` the types (`Word`, `Line`, `Lyrics`,
  `VOCALS_NOTE`, `HAND_NOTE`) and the parsers and writers that need no network (`parse_lyricsfile`, `parse_lrc`,
  `to_file`, `is_hum`, `even_hums`, `spread`), with their tests.
- Modify: `engine/src/lyrics.rs` (keeps `find` and the rest, plus `pub use radiance_core::lyrics::*;`),
  `engine/src/lib.rs` (`pub use radiance_core::track;`)

**Interfaces:**
- Produces: `radiance_core::track::{Track, Source, digest, clock, parse_clock}` and
  `radiance_core::lyrics::{Word, Line, Lyrics, …}`, unchanged.
  - Every `crate::track::…` and `crate::lyrics::…` path in the engine and the app still resolves.

- [ ] **Step 1: Write the failing test.** In `core/src/track.rs`'s tests:
  ```rust
  #[test]
  fn a_digest_is_the_first_16_hex_digits_of_sha256() {
      assert_eq!(digest("radiance"), "ee305956fcdb25b8");
      assert_eq!(digest(""), "e3b0c44298fc1c14");
  }
  ```
  These values are GLib's SHA-256 output, which the existing keys and file names were made with.

- [ ] **Step 2: Run it to make sure it fails.**
  - Run: `cargo test -p radiance-core a_digest`.
  - Expected: it fails to compile, because the crate doesn't exist yet or `digest` still calls GLib.

- [ ] **Step 3: Implement `digest` with `sha2::Sha256`.** Use lowercase hex and the first 16 characters.

- [ ] **Step 4: Run the tests and the dependency check.**
  - Run: `cargo test --release -p radiance-core -p radiance-engine -p radiance-music`. Expected: Task 1's count plus
    1, `0 failed`.
  - Run: `cargo tree -p radiance-core -e normal | grep -E -c 'glib|gio|gstreamer'`. Expected: `0`.

- [ ] **Step 5: Commit.** Message: `core: Track and the lyric types, without GLib Refs #45`.

### Task 3: `core::queue` (the queue, shuffle, repeat and what plays next)

**Files:**
- Create: `core/src/queue.rs`
- Modify:
  - `src/ui.rs`: the fields `queue`, `index`, `unshuffled` and `repeat` become one `queue: RefCell<Queue>`.
  - In `src/ui.rs`, `shuffled`, `liking`, `often_skipped`, `Repeat`, `next_track`, `play_list`, `shuffle_changed`,
    `next_repeat`, `skip`, `play_again`, `play_next`, `enqueue`, `unqueue`, `move_in_queue`, `note_skip_if_early`
    and `autoplay_if_last`'s filter all call `Queue` or the core functions.
  - `src/ui/home.rs` and `src/ui/rms.rs` (their uses).

**Interfaces:**
- Produces (`radiance_core::queue`):
  ```rust
  pub enum Repeat { Off, All, One }
  pub enum Step { Restart, Play(usize), End }                 // what skip() decided
  pub enum Removed { Before, Current { play: bool }, After }  // what remove() did to the song playing
  pub struct Queue { /* tracks, index, unshuffled: Option<Vec<Track>>, shuffle: bool, repeat: Repeat */ }
  impl Queue {
      pub fn new() -> Queue;
      pub fn tracks(&self) -> &[Track];
      pub fn index(&self) -> usize;
      pub fn at_index(&self) -> Option<&Track>;
      pub fn set_index(&mut self, i: usize);
      pub fn repeat(&self) -> Repeat;
      pub fn shuffle(&self) -> bool;
      pub fn play_list(&mut self, list: Vec<Track>, i: usize, order: impl FnOnce(Vec<Track>) -> Vec<Track>);
      pub fn set_shuffle(&mut self, on: bool, current_key: Option<&str>, order: impl FnOnce(Vec<Track>) -> Vec<Track>);
      pub fn cycle_repeat(&mut self) -> Repeat;              // Off → All → One → Off
      pub fn next(&self) -> Option<&Track>;                  // today's next_track()
      pub fn can_next(&self) -> bool;                        // i + 1 < len || repeat == All
      pub fn skip(&mut self, by: i32, at: f64) -> Step;      // today's skip() rules; sets the index for Play
      pub fn insert_next(&mut self, t: Track) -> usize;      // play_next / play_again: at index + 1
      pub fn push(&mut self, t: Track);
      pub fn extend(&mut self, more: Vec<Track>);
      pub fn remove(&mut self, i: usize) -> Option<Removed>;
      pub fn move_item(&mut self, from: usize, to: usize, playing: bool) -> bool; // false: where it was
  }
  pub fn liking(plays: u32, skips: u32) -> f64;              // ((1 + plays) / (1 + 2 skips)).clamp(0.2, 8)
  pub fn often_skipped(plays: u32, skips: u32) -> bool;      // skips >= 2 && skips > plays
  pub fn shuffled(v: Vec<Track>, liking: impl Fn(&Track) -> f64, seed: u64) -> Vec<Track>; // today's xorshift + u^(1/w)
  pub fn is_early_skip(at: f64, length: f64) -> bool;        // length > 0, at > 1, at < 0.7 length, length - at > 30
  pub fn autoplay_picks(found: Vec<Track>, queued: &HashSet<String>, lately: &HashSet<String>,
                        seed: &Track, skipped: impl Fn(&Track) -> bool) -> Vec<Track>; // at most 25
  ```
  - The GTK app keeps `current` (the saved copy playing) apart from `Queue`, as today.

- [ ] **Step 1: Write the failing tests.** In `core/src/queue.rs`. Make tracks with a helper `song(n)` (title and id
  `n`). Each test asserts today's rule exactly:
  - `play_list_with_shuffle_puts_the_picked_song_first`: shuffle on, `play_list([a,b,c,d], 2, rev)` gives tracks
    `[c, d, b, a]`, index 0. Here `rev` reverses its input.
  - `shuffle_off_returns_to_the_queues_own_order_at_the_same_song`: after shuffling `[a,b,c,d]` from a with
    `rev`, `set_shuffle(false, Some(key_of_c), …)` gives tracks `[a,b,c,d]` and index 2.
  - `shuffle_on_keeps_what_has_played`: index 1 of `[a,b,c,d,e]`, `set_shuffle(true, …, rev)` gives `[a,b,e,d,c]`.
    With fewer than 2 songs after the index, nothing changes.
  - `next_follows_repeat`: next of the last song is `None` with `Off`, and the first song with `All` (only when
    len > 1). With `One` it is always `None`.
  - `back_after_three_seconds_restarts`: `skip(-1, 3.5)` gives `Step::Restart`. `skip(-1, 2.0)` at index 2 gives
    `Play(1)`.
  - `next_at_the_end`: `skip(1, 0.0)` at the last song gives `End` with `Off`, and `Play(0)` with `All`.
  - `removing_before_the_song_moves_the_index`: `remove(0)` at index 2 gives `Before`, index 1.
  - `removing_the_last_song_playing_stops`: it gives `Current { play: false }`, and the index steps back one.
  - `moving_never_goes_above_the_song_playing`: `move_item(0, 3, true)` at index 1 returns `false`.
    `move_item(3, 4, true)` returns `false` (where it was). `move_item(4, 2, true)` gives `[a,b,e,c,d]`.
  - `liking_and_often_skipped_match_today`: `liking(0, 0) == 1.0`, `liking(20, 0) == 8.0`, `liking(0, 10) == 0.2`.
    `often_skipped(1, 2)` is true and `often_skipped(2, 2)` is false.
  - `shuffled_is_steady_for_a_seed`: the same seed gives the same order twice. Every track appears once.
  - `an_early_skip`: `is_early_skip(30.0, 200.0)` is true; `is_early_skip(150.0, 200.0)` and
    `is_early_skip(0.5, 200.0)` are false.
  - `autoplay_never_brings_back_queued_lately_skipped_or_the_seed`: 40 found songs, with some queued, some played
    lately, the seed's same song, and ones that `skipped` flags. It returns at most 25, none of those.

- [ ] **Step 2: Run them to make sure they fail.**
  - Run: `cargo test -p radiance-core queue`.
  - Expected: it fails to compile (`Queue` not defined).

- [ ] **Step 3: Implement `core/src/queue.rs` by moving today's code out of `ui.rs`.** The rules are in `src/ui.rs`
  at 287–324, 2929–2937, 4465–4530, 4637–4659, 5220–5262 and 5795–5840. Then switch `ui.rs`, `home.rs` and `rms.rs`
  to `Queue`.
  - The GTK shuffle button's handler calls `set_shuffle`.
  - `liking(t)` and `often_skipped(t)` in `ui.rs` become one-line wrappers that pass `counts_of(t)`.

- [ ] **Step 4: Run the whole suite on ribs02.**
  - Run: `cargo test --release -p radiance-core -p radiance-engine -p radiance-music`.
  - Expected: all pass, with the new tests added to the count.

- [ ] **Step 5: Check the queue by hand on ribs02.** Use `RADIANCE_SHOT` with `RADIANCE_SHOT_PLAY=1` and the panel
  on the queue (`shot_panel("queue")`). Expected: the same queue screenshot as before the change.

- [ ] **Step 6: Commit.** Message: `core: the queue, shuffle, repeat and what plays next (the GTK app uses it) Refs #45`.

### Task 4: `core::sing`, `core::words`, `core::trouble` (lyric timing, the screens' words, why a song won't play)

**Files:**
- Create: `core/src/sing.rs`, `core/src/words.rs`, `core/src/trouble.rs`
- Modify:
  - `src/ui.rs`: `word_spans`, `LyricView::sung_chars`, `sing_now`'s lit lines, `WORD_LEAD`, `LINE_LEAD`,
    `run`/`render_results`' notes, `why_not`, `sound_device_gone` and `unavailable`'s rule.
  - `src/ui/home.rs`: `greeting`, `BROWSE`, `ROW_CARDS` and the hero's sub-line.
- Create: `engine/src/home.rs`. It holds `made_for_you`, the seeds-and-avoid step from `refresh_mix`
  (`src/ui.rs:5189-5212`), which the GTK app then calls.

**Interfaces:**
- Produces (`radiance_core::sing`):
  ```rust
  pub const WORD_LEAD: f64 = 0.08;
  pub const LINE_LEAD: f64 = 0.35;
  pub fn word_spans(text: &str, words: &[Word]) -> Vec<(usize, usize)>;
  pub fn sung_chars(line: &Line, spans: &[(usize, usize)], at: f64) -> usize;
  pub fn lit_lines(lines: &[Line], at: f64) -> Vec<usize>;   // today's sing_now rule
  ```
- Produces (`radiance_core::words`):
  ```rust
  pub fn greeting(hour: u32, real_name: &str) -> String;     // "Good morning, Bassi"; no name → no comma
  pub const BROWSE: [(&str, &str); 12];                      // from home.rs, same order
  pub const ROW_CARDS: usize = 20;
  pub enum Kind { Lossless, Songs, Videos }
  pub fn search_note(kind: Kind, found: Option<usize>, offline: bool, query: &str) -> String; // render_results' texts
  pub const OFFLINE_SEARCH: &str;                            // "You're offline: searching needs the internet. …"
  pub fn mix_note(mix: usize, liked: usize) -> String;       // the hero's sub-line, three cases
  ```
- Produces (`radiance_core::trouble`):
  ```rust
  pub fn why_not(error: &str) -> (&'static str, bool);       // today's Ui::why_not, unchanged
  pub fn sound_device_gone(error: &str) -> bool;
  pub enum After { Next, Stop }
  pub fn after_failure(fails: u32, more: bool, every_song: bool) -> After;  // Next while more && !every_song && fails < 3
  pub fn cant_play_text(title: &str, reason: &str, fails: u32, every_song: bool) -> String;
  ```
- Produces (`radiance_engine::home`):
  `pub fn made_for_you(liked: &[Track], history: &[(Track, i64)]) -> Vec<Track>`. It reads the counts from `db` and
  calls `mix::from_likes`.

- [ ] **Step 1: Write the failing tests.**
  - Move `words_found_in_their_line`, `karaoke_lights_the_sung_part` and `a_lost_sound_device_isnt_a_bad_song` into
    the new modules. Then add:
  - `the_latest_started_line_stays_lit_until_the_next`: lines (0–3), (2.5–4) and (6–8).
    - `lit_lines(&l, 1.0)` is `[0]`.
    - `lit_lines(&l, 2.3)` is `[0, 1]`: line 1 starts within `LINE_LEAD` while line 0 is still on.
    - `lit_lines(&l, 3.5)` is `[1]`: line 0 has ended.
    - `lit_lines(&l, 5.0)` is `[1]`.
    - `lit_lines(&[], 5.0)` is empty.
  - `greetings`:
    - `greeting(6, "Bassi Saba")` is `"Good morning, Bassi"`.
    - `greeting(12, "")` is `"Good afternoon"`.
    - `greeting(23, "Unknown")` is `"Good evening"`.
  - `search_notes_say_what_the_gtk_app_says`: one assert per arm of `src/ui.rs:4027-4046`, with the exact strings.
    For example, `search_note(Kind::Songs, Some(0), true, "x")` is
    `"YouTube Music too, once you're back online."`.
  - `three_failures_in_a_row_stop`: `after_failure(2, true, false)` is `Next`; `after_failure(3, true, false)`,
    `after_failure(1, false, false)` and `after_failure(1, true, true)` are `Stop`.
    - `cant_play_text("A", "you're offline", 3, false)` is `"3 songs in a row wouldn't play: you're offline"`.

- [ ] **Step 2: Run them to make sure they fail.** Run: `cargo test -p radiance-core`. Expected: it fails to compile.

- [ ] **Step 3: Move the code and switch the GTK app.**
  - The rule in `ui.rs` becomes a call to the moved function.
  - `LyricView` keeps its labels and calls `sing::sung_chars`.
  - `greeting()` in `home.rs` passes the hour and `glib::real_name()`.

- [ ] **Step 4: Run the whole suite on ribs02, then take the screenshots.**
  - Run: `cargo test --release -p radiance-core -p radiance-engine -p radiance-music`. Expected: all pass.
  - Take Home, Search (typed `kpop`) and Now Playing screenshots before and after. Expected: the same.

- [ ] **Step 5: Commit.** Message: `core: lyric timing, Home's and Search's words, and why a song won't play Refs #45`.

## Part B: the engine's controller and the DLL (ribs02)

### Task 5: `engine::controller` (the engine thread, search and Home)

**Files:**
- Create: `engine/src/controller.rs`, `engine/src/sources.rs`, `engine/tests/controller.rs`
- Modify: `engine/src/lib.rs`

**Interfaces:**
- Produces (`radiance_engine::sources`):
  ```rust
  pub trait Sources: Send + Sync + 'static {
      fn search(&self, kind: Kind, q: &str) -> Vec<Track>;   // archive::search / youtube::search / youtube::search_videos
      fn reachable(&self) -> bool;                            // net::reachable
      fn stream(&self, id: &str) -> Result<youtube::Stream, String>;
      fn radio(&self, seed: &Track) -> Vec<Track>;            // mix::radio
      fn lyrics(&self, t: &Track) -> Lyrics;                  // the lyrics lookup (Task 6 moves it out of ui.rs)
  }
  pub struct Real;                                            // the real ones
  impl Sources for Real {}
  ```
- Produces (`radiance_engine::controller`):
  ```rust
  pub enum Change { Now, Position, Queue, Results, Home, Lyrics, Online, Problem }
  pub trait Listener: Send + Sync + 'static { fn changed(&self, what: Change); }
  pub enum ListRef { Lossless, Songs, Mix, Recent, Liked, Queue }
  pub struct Results { pub query: String, pub lossless: Option<Vec<Track>>, pub songs: Option<Vec<Track>> } // None: still searching
  pub struct HomeState { pub greeting: String, pub mix_note: String, pub mix: Vec<Track>, pub recent: Vec<Track>, pub liked: Vec<Track> }
  pub struct Controller { /* Arc<Shared>, the engine thread's handle */ }
  impl Controller {
      pub fn start(listener: Arc<dyn Listener>, sources: Arc<dyn Sources>) -> Result<Controller, String>; // Err if one already runs
      pub fn search(&self, q: &str);
      pub fn refresh_mix(&self);
      pub fn results(&self) -> Results;
      pub fn home(&self) -> HomeState;
      pub fn online(&self) -> bool;
      pub fn problem(&self) -> Option<String>;
      pub fn quit(&self);                                     // saves the session, stops the loop, joins the thread
  }
  ```
- Threads:
  - The controller's thread acquires `glib::MainContext::default()` and runs a `MainLoop` on it.
  - In the beta, nothing else uses the default context, so `timeout_add_local`, the player's bus watch and
    `MainContext::default().invoke` all run on that thread.
  - Commands are posted with `MainContext::default().invoke`.
  - Snapshots sit in an `Arc<Mutex<…>>`, which the engine thread writes and any thread reads.
  - Background work goes to `std::thread` and comes back through `invoke`.
- Search follows `run_search`:
  - Fewer than 2 characters clears the results.
  - A generation counter drops stale answers.
  - When Songs finds nothing, it checks whether the internet can be reached, then sets `online`.
  - Videos wait for stage 2: `Kind::Videos` isn't searched.

- [ ] **Step 1: Write the failing tests.** In `engine/tests/controller.rs`. It is its own process.
  - `setup()` runs once (`std::sync::Once`) and sets `XDG_DATA_HOME`, `XDG_CONFIG_HOME` and `XDG_CACHE_HOME` to a
    fresh dir under `TMPDIR`, plus `RADIANCE_SILENT=1`, before anything touches GLib.
  - The tests run one at a time behind a `static LOCK: Mutex<()>`.
  - `FakeSources` returns canned tracks, can be told to wait, and can count calls.
  - `TestListener` records changes into a channel, and `wait_for(Change, secs)` waits for one.
  - Tests:
    - `a_search_fills_both_lists`: `search("binhi")` produces `Change::Results` (twice).
      `results().lossless` and `results().songs` hold the fakes' tracks, and `query == "binhi"`.
    - `an_older_search_never_replaces_a_newer_one`:
      - `FakeSources` makes the query `"bi"` answer after 500 ms and `"binhi"` at once.
      - Call `search("bi")` and then `search("binhi")`.
      - After 1 s, `results().query` is `"binhi"` and its tracks are `"binhi"`'s.
    - `one_letter_clears_the_results`: `search("b")` leaves both lists `Some(empty)` with no source call.
    - `nothing_found_and_no_internet_means_offline`: the fake returns no songs and `reachable() == false`. Expected:
      `Change::Online` and `online() == false`.
    - `home_has_the_greeting_and_the_recent_plays`: seed the test DB with two plays through `db::add_play`.
      `home().recent` holds them, newest first, and `home().greeting` starts with "Good ".
    - `a_second_controller_is_refused`: `Controller::start` while one runs gives `Err`.

- [ ] **Step 2: Run them to make sure they fail.**
  - Run: `cargo test -p radiance-engine --test controller`.
  - Expected: it fails to compile.

- [ ] **Step 3: Implement `sources.rs` and `controller.rs`.** Home fills at start and after `refresh_mix`:
  - `greeting` comes from `words::greeting(hour, real_name)`.
  - `recent` comes from `session::recent`.
  - `liked` comes from `db::liked()`.
  - `mix` comes from `home::made_for_you`, worked out on a background thread.
  - `mix_note` comes from `words::mix_note`.

- [ ] **Step 4: Run the tests on ribs02.**
  - Run: `cargo test --release -p radiance-engine --test controller`, then the whole suite (as in Task 1).
  - Expected: all pass.

- [ ] **Step 5: Commit.** Message: `engine: the controller (its own thread), search and Home Refs #45`.

### Task 6: The controller's playback

**Files:**
- Create: `engine/src/playing.rs`. "Where a song plays from" moves there out of `ui.rs`, and the GTK app calls it too.
- Modify: `engine/src/controller.rs`, `engine/src/lyrics.rs`, `engine/src/art.rs`, `src/ui.rs`, `engine/tests/controller.rs`
  - `engine/src/lyrics.rs`: `lyrics_for`, `lyrics_and_source`, `server_lyrics`, `hosted_lyrics`, `lyrics_file` and
    `lyrics_path` move here from `ui.rs:378-480, 627-642`.
  - `engine/src/art.rs`: `art_source` moves here as `art::source_of`.

**Interfaces:**
- Produces (`radiance_engine::playing`):
  ```rust
  pub const STREAM_FRESH: Duration;                          // today's value from ui.rs
  pub struct Streams { /* id → (uri, kbps, Instant), and the ids being found */ }
  impl Streams {
      pub fn fresh(&self, id: &str) -> Option<(String, u32)>;
      pub fn found(&mut self, id: &str, uri: String, kbps: u32);
      pub fn start_finding(&mut self, id: &str) -> bool;
      pub fn done_finding(&mut self, id: &str);
  }
  pub fn web_uri(relay: Option<&Relay>, url: &str) -> String; // the relay on Windows
  pub fn plain_uri(t: &Track, relay: Option<&Relay>, streams: &Streams) -> Option<String>;
  pub fn saved_copy(liked: &[Track], t: &Track) -> Track;
  ```
- Produces (on `Controller`):
  ```rust
  pub struct NowState { pub song: Option<Track>, pub playing: bool, pub at: f64, pub length: f64,
                        pub can_previous: bool, pub can_next: bool, pub shuffle: bool, pub repeat: Repeat,
                        pub volume: f64, pub lossless: bool, pub kbps: u32, pub lit: Vec<usize> }
  pub enum LyricsState { Looking, None, Plain(String), Lines(Vec<Line>) }
  pub fn play_from(&self, list: ListRef, i: usize);           // play_list() of that list, as GTK's play_from
  pub fn shuffle_from(&self, list: ListRef);                  // "Shuffle": shuffled(list), from 0
  pub fn toggle_play(&self);  pub fn skip(&self, by: i32);  pub fn seek(&self, at: f64);
  pub fn set_volume(&self, level: f64);  pub fn set_shuffle(&self, on: bool);  pub fn cycle_repeat(&self);
  pub fn now(&self) -> NowState;  pub fn queue(&self) -> (Vec<Track>, usize);  pub fn lyrics(&self) -> LyricsState;
  pub fn art_file(&self, source: &str, size: u32) -> Option<PathBuf>;          // PNG in user_cache_dir/radiance-music/beta-art
  pub fn backdrop_files(&self, source: &str) -> Option<(PathBuf, PathBuf)>;    // (art::backdrop, art::blob) as PNGs
  ```
  - `art_file` and `backdrop_files` block, so callers use them off the UI thread.
  - Their file names are `track::digest(source)` plus the size.
- Behaviour, the stage-1 subset of `play_current`, `on_player`, `tick` and `restore_session`, with GTK's rules:
  - It plays the saved copy when there is one.
  - The source decides what happens next:
    - file: its URI;
    - Archive: through `web_uri`;
    - YouTube: a fresh stream, or else a lookup, and the result plays only if its generation is still the current one;
    - Server: "can't play", because servers come in stage 4.
  - Videos play their sound only. The beta's videos come in stage 2.
  - When a song ends, it plays the next one. With `Repeat::One` it plays the same song again.
  - Errors:
    - A sound device that went away gets 3 retries (`carry_on`).
    - Anything else follows `trouble::after_failure`, with `Change::Problem` and the text from `cant_play_text`.
  - Plays and early skips are counted (`count_play`, `count_skip`, `is_early_skip`).
  - Autoplay runs when the last song starts (`autoplay_picks`, if `settings.autoplay` is on and repeat is off).
  - It prefetches the next YouTube stream.
  - `Change::Position` fires 5 times a second while playing, and `lit` comes from `sing::lit_lines`.
  - The session is saved every 5 s while playing and at `quit()`.
  - At start, it restores the last session, paused: the first `toggle_play` plays it from where it was, as
    `restored` and `restore_at` do today.
  - Volume follows `apply_volume` (`src/ui.rs:3797`).

- [ ] **Step 1: Write the failing tests.**
  - The test songs are 3 s WAV files (a quiet sine, 44.1 kHz mono) that the test writes under `TMPDIR`.
  - They are `Track`s with `file: Some(path)`. Playback is silent (`RADIANCE_SILENT`).
  - `FakeSources::stream` can be told to fail.
  - Tests:
    - `a_list_plays_and_moves_on`: `play_from(Songs, 0)` over 2 file tracks. `now().playing` turns true, and
      `Change::Position` comes with `at` rising. After the first ends, `now().song` is the second.
    - `the_song_picked_last_wins`: a YouTube track whose stream answers after 400 ms, then at once
      `play_from(Songs, 1)` (a file track). After 1 s, `now().song` is track 1, and the late stream never loads.
    - `a_song_that_wont_play_is_skipped_then_it_stops_after_three`:
      - 4 YouTube tracks whose streams fail.
      - Expected: `Change::Problem` 3 times, with texts from `cant_play_text`.
      - After the third, playback stops (`playing == false`) on track index 2.
    - `back_restarts_after_three_seconds`: at 3.5 s into a 5 s file, `skip(-1)` keeps the song and `at < 1`.
    - `repeat_one_plays_it_again`: `cycle_repeat()` twice gives `One`. At the end, the same song starts again.
    - `the_session_comes_back_paused`:
      - Play, seek to 2.0, then `quit()`.
      - A new controller's `now()` has the same song with `playing == false`.
      - After `toggle_play()`, `at >= 2.0`.
    - `lyrics_lines_light_with_the_song`: `FakeSources::lyrics` returns lines at 0–1 s and 1.2–2.5 s.
      `now().lit` is `[0]` near 0.5 s and then contains 1.
    - `art_files_are_cached_pngs`: `art_file(path_of_a_test_png, 300)` returns a PNG path. Calling it again returns
      the same path without fetching.
  - Also, in `engine/src/playing.rs`'s unit tests, `plain_uri` of a file, an Archive track (no relay) and a YouTube
    track with a fresh stream and with none. The expected URIs come from today's `uri_plain`.

- [ ] **Step 2: Run them to make sure they fail.**
  - Run: `cargo test -p radiance-engine --test controller`.
  - Expected: it fails to compile (`play_from` not defined).

- [ ] **Step 3: Implement `playing.rs` and the playback in `controller.rs`, then switch the GTK app.** `ui.rs`'s
  `fresh_stream`, `web_uri`, `uri_plain`, `saved_copy`, `lyrics_for` and `art_source` become calls to the moved code.

- [ ] **Step 4: Run all tests on ribs02.**
  - Run: `cargo test --release -p radiance-core -p radiance-engine -p radiance-music`.
  - Expected: all pass, and nothing was heard (the fakesink).

- [ ] **Step 5: Commit.** Message: `engine: the controller plays (queue, streams, lyrics, session), the GTK app shares where songs play from Refs #45`.

### Task 7: The `ffi` crate (uniffi 0.31) and one copy at a time

**Files:**
- Create: `ffi/Cargo.toml`, `ffi/src/lib.rs`, `ffi/uniffi.toml`
- Move: `src/main.rs`'s `one_copy` and `asking` modules → `engine/src/one_copy.rs`
- Modify: `Cargo.toml` (add `ffi` to `members`), `src/main.rs`

**Interfaces:**
- `ffi/Cargo.toml`:
  - `name = "radiance-ffi"`, `version = "0.8.0-beta.1"` (not inherited).
  - `[lib] name = "radiance_ffi"`, `crate-type = ["cdylib", "lib"]`.
  - `uniffi = "=0.31.0"`, and `radiance-engine` with `default-features = false`.
- `ffi/uniffi.toml`: `[bindings.csharp] namespace = "RadianceMusic.Native"`, `global_methods_class_name = "Native"`.
- Produces (C# sees these after Task 9's binding step):
  ```rust
  #[uniffi::export(with_foreign)] pub trait EngineListener: Send + Sync { fn changed(&self, what: ChangeKind); }
  #[uniffi::export(with_foreign)] pub trait ShowListener: Send + Sync { fn show(&self, link: Option<String>); }
  #[derive(uniffi::Enum)] pub enum ChangeKind { Now, Position, Queue, Results, Home, Lyrics, Online, Problem }
  #[derive(uniffi::Enum)] pub enum ListKind { Lossless, Songs, Mix, Recent, Liked, Queue }
  #[derive(uniffi::Enum)] pub enum RepeatMode { Off, All, One }
  #[derive(uniffi::Enum)] pub enum LyricsKind { Looking, None, Plain, Lines }
  #[derive(uniffi::Record)] pub struct SongView { key, title, byline, album, art, length: String, seconds: f64, lossless: bool, video: bool }
  #[derive(uniffi::Record)] pub struct NowView { song: Option<SongView>, playing, at, length, can_previous, can_next, shuffle, repeat: RepeatMode, volume, lossless, kbps: u32, lit: Vec<u32> }
  #[derive(uniffi::Record)] pub struct ResultsView { query: String, lossless: Option<Vec<SongView>>, songs: Option<Vec<SongView>>, lossless_note: String, songs_note: String }
  #[derive(uniffi::Record)] pub struct HomeView { greeting, mix_note: String, mix, recent, liked: Vec<SongView> }
  #[derive(uniffi::Record)] pub struct LineView { text: String, start: f64, end: f64 }
  #[derive(uniffi::Record)] pub struct LyricsView { kind: LyricsKind, plain: String, lines: Vec<LineView> }
  #[derive(uniffi::Record)] pub struct BrowseTile { label: String, query: String }
  #[derive(uniffi::Record)] pub struct BackdropFiles { wash: String, blob: String }
  #[derive(uniffi::Record)] pub struct QueueView { songs: Vec<SongView>, index: u32 }
  #[derive(Debug, thiserror::Error, uniffi::Error)] pub enum EngineError { #[error("{0}")] Failed(String) }
  #[derive(uniffi::Object)] pub struct MusicEngine { /* Controller */ }
  #[uniffi::export] impl MusicEngine {
      #[uniffi::constructor] pub fn start(listener: Arc<dyn EngineListener>) -> Result<Arc<Self>, EngineError>;
      // every Controller command and snapshot of Tasks 5–6, with views in place of engine types:
      // search, refresh_mix, play_from(ListKind, u32), shuffle_from, toggle_play, skip(i32), seek, set_volume,
      // set_shuffle, cycle_repeat, quit, now, results, home, lyrics, online, problem, art_file(source, size) -> Option<String>,
      // backdrop(source) -> Option<BackdropFiles>, queue() -> QueueView
  }
  #[uniffi::export] pub fn beta_version() -> String;          // "0.8.0-beta.1"
  #[uniffi::export] pub fn engine_version() -> String;        // the workspace version
  #[uniffi::export] pub fn browse_tiles() -> Vec<BrowseTile>; // words::BROWSE
  #[uniffi::export] pub fn self_check() -> Vec<String>;       // see Task 9
  #[uniffi::export] pub fn ask_the_other(link: Option<String>) -> bool;
  #[uniffi::export] pub fn listen_for_others(on_show: Arc<dyn ShowListener>);
  ```
  - `ResultsView`'s notes come from `words::search_note` (offline included).
  - `engine::one_copy` is public on every platform. `listen` takes `impl Fn(Option<String>) + Send + Sync + 'static`.
  - The GTK app on Windows calls it exactly as before, so its protocol and port file
    (`user_data_dir/radiance-music/running.port`) stay the same.

- [ ] **Step 1: Write the failing tests.**
  - In `ffi/src/lib.rs`'s tests, with the same `setup()` and `LOCK` as Task 5: a Rust `EngineListener` gets
    `ChangeKind::Results` after `search("binhi")` on a `MusicEngine` built with fake sources. Add a
    `#[cfg(test)] start_with(listener, sources)` constructor for this.
  - `quit_while_searching_is_quiet`:
    - The fake search waits 1 s; `search("binhi")`, then at once `quit()`.
    - `quit()` returns within 2 s, and no `changed` call comes after it returns (the listener records a timestamp).
    - The process doesn't panic.
  - Move `a_second_copy_asks_and_brings_its_link` into `engine/src/one_copy.rs` unchanged. Add
    `the_running_copy_is_asked_to_show`: `listen` under a temp `XDG_DATA_HOME`, then `ask_the_other(None)`.
    Expected: it returns true and the `show` closure ran with `None`.

- [ ] **Step 2: Run them to make sure they fail.**
  - Run: `cargo test -p radiance-ffi -p radiance-engine one_copy`.
  - Expected: it fails to compile.

- [ ] **Step 3: Implement `ffi/src/lib.rs` (`uniffi::setup_scaffolding!()`) and move `one_copy`.** The `quit` order:
  1. Stop the listener: an `AtomicBool` checked before every `changed`.
  2. Post the quit.
  3. Join the thread.

- [ ] **Step 4: Run the tests and generate the C# binding once on ribs02.**
  - Run: `cargo test --release -p radiance-core -p radiance-engine -p radiance-music -p radiance-ffi`.
    Expected: all pass.
  - Install uniffi-bindgen-cs under `CARGO_HOME=/srv/build/winui/cargo`. Then run
    `uniffi-bindgen-cs --library $CARGO_TARGET_DIR/release/libradiance_ffi.so --out-dir /srv/build/winui/cs`.
  - Expected: `radiance_ffi.cs` is written. It contains `class MusicEngine` and `interface EngineListener`.

- [ ] **Step 5: Commit.** Message: `ffi: the engine for C# (uniffi 0.31); one copy at a time lives in the engine Refs #45`.

## Part C: Windows (ribs03)

### Task 8: The MSVC toolchain and GStreamer on ribs03, and `windows/build.ps1`

**Files:**
- Create: `windows/build.ps1`, `windows/gst-msvc.ps1`, `windows/README.md`

**Interfaces:**
- Produces: `powershell -NoProfile -File C:\rb\beta\src\windows\build.ps1 -Step rust|app|dist|all`.
  - `rust` runs the core and engine tests (MSVC), then `cargo build --release -p radiance-ffi`, which gives
    `target\release\radiance_ffi.dll`.
  - `app` generates the binding into `windows\RadianceMusic.Windows\Native\`, then `dotnet build` and `dotnet test`.
  - `dist` assembles `C:\rb\beta\dist`, then runs the self-check.
  - The script always does the following:
    - Sets `(Get-Process -Id $PID).PriorityClass = 'Idle'` first.
    - Puts `$env:USERPROFILE\.cargo\bin`, `C:\Program Files\dotnet`, `C:\Program Files\Git\cmd` and
      GStreamer's `bin` on `PATH`. No MSYS2 directory goes on it.
    - Sets `RADIANCE_SILENT=1` and the three `XDG_*` variables to `C:\rb\beta-test\…`.
- Produces: GStreamer MSVC 1.28 (runtime and devel, the same 1.28.x as MSYS2's) extracted to `C:\rb\gst-msvc`.
  - It uses `msiexec /a <msi> /qn TARGETDIR=C:\rb\gst-msvc`, an extraction: no system install, no registry, no PATH
    change.
  - `windows/gst-msvc.ps1` downloads both MSIs from `gstreamer.freedesktop.org`, checks them against the `.sha256sum`
    files published beside them, and extracts them.

- [ ] **Step 1: Ask the owner in a dialog before touching ribs03.**
  - The question: may ribs03 get GStreamer's MSVC files (about 600 MB) in `C:\rb\gst-msvc`, extracted only (no
    install), and run the MSVC Rust build at Idle priority?
  - Stop here on "no".

- [ ] **Step 2: Check ribs03 without changing anything.**
  - Run: `rustup show`, `dotnet --list-sdks`, and `where cl` after `vcvars64`. These were set up for the POS app.
  - Expected: `stable-x86_64-pc-windows-msvc`, a `10.0.` SDK and `cl.exe`.
  - If anything is missing, stop and ask in a dialog.

- [ ] **Step 3: Run `windows/gst-msvc.ps1` on ribs03.**
  - Expected: `C:\rb\gst-msvc\…\msvc_x86_64\bin\gstreamer-1.0-0.dll` exists.
  - Expected: `gst-inspect-1.0.exe playbin3` prints the element.
  - Expected: `pkg-config --modversion gstreamer-1.0`, using GStreamer's own `pkg-config.exe`, prints `1.28.x`.
  - If GStreamer has no `pkg-config.exe`, set `PKG_CONFIG` to MSYS2's `C:\msys64\ucrt64\bin\pkgconf.exe` with
    `PKG_CONFIG_PATH` at the MSVC `.pc` files, and record that in the README.

- [ ] **Step 4: Run `build.ps1 -Step rust` on ribs03.** Push the branch first, and announce the build to the owner.
  - Expected: `test result: ok` for `radiance-core` and for `radiance-engine` (its unit tests and `--test controller`),
    then `radiance_ffi.dll` built.
  - `dumpbin /dependents target\release\radiance_ffi.dll` lists `gstreamer-1.0-0.dll` and `glib-2.0-0.dll`, and no
    `libgstreamer` / `libglib` (MinGW) names.

- [ ] **Step 5: Commit.** Message: `Windows beta: MSVC build on ribs03 (GStreamer's own MSVC files, extracted) Refs #45`.

### Task 9: The WinUI 3 project: shell, look, engine bridge and self-check

**Files:**
- Create in `windows/RadianceMusic.Windows/`:
  - `RadianceMusic.Windows.csproj`
  - `app.manifest`
  - `Program.cs`
  - `App.xaml(.cs)`
  - `Theme.xaml`
  - `MainWindow.xaml(.cs)`
  - `EngineHub.cs`
  - `Coalescer.cs`
  - `SelfCheck.cs`
  - `Shot.cs`
- Create: `windows/RadianceMusic.Windows.Tests/RadianceMusic.Windows.Tests.csproj` and `CoalescerTests.cs`.
- Generated (not committed, in `.gitignore`): `windows/RadianceMusic.Windows/Native/radiance_ffi.cs`.

**Interfaces:**
- The csproj:
  - The POS csproj's properties (Global Constraints), with `AssemblyName RadianceMusicBeta` and
    `RootNamespace RadianceMusic`.
  - `Version 0.8.0-beta.1`, `Product "Radiance Music (beta)"`, `Company "Radiance Innovation Centers"`.
  - `ApplicationIcon ..\..\packaging\windows\radiance-music.ico`.
  - `DISABLE_XAML_GENERATED_MAIN`.
  - `radiance_ffi.dll` copied to the output.
- Produces `EngineHub`:
  ```csharp
  public sealed class EngineHub : EngineListener {
      public static EngineHub Start(DispatcherQueue ui);   // MusicEngine.Start(this)
      public MusicEngine Engine { get; }
      public event Action<ChangeKind>? Change;            // raised on the UI thread only
      public void Changed(ChangeKind what);                // the Rust thread: Coalescer.Mark, TryEnqueue at most once per pending batch
  }
  public sealed class Coalescer {                         // pure, thread-safe
      public bool Mark(ChangeKind k);                     // true: the first mark since the last Flush (post a flush)
      public ChangeKind[] Flush();                        // each marked kind once, in ChangeKind order
  }
  ```
- Produces `MainWindow`:
  - `ExtendsContentIntoTitleBar` with its own drag area.
  - A sidebar with the brand "Radiance Music" and three entries copied from `home.rs` `NAV`: Home, Search and
    Now Playing, each with its icon (Segoe Fluent Icons equivalents of `go-home`, `system-search` and
    `audio-x-generic`). Library entries come in stage 2.
  - A content card with corner radius 14, hosting a `Frame`.
  - A row for the player bar (Task 13).
  - Narrow widths follow `home.rs`'s phone breakpoint (`src/ui/home.rs:280-296`).
  - `public void Show(string page)` with pages `"home" | "search" | "playing"`.
  - On `Change.Problem`, an in-window notice at the bottom (an `InfoBar`, as GTK's toast) shows
    `Engine.Problem()` for 10 s, with a "Try Again" button.
    - "Try Again" does `PlayFrom(ListKind.Queue, Engine.Queue().Index)`. This matches the toast in `unavailable`
      (`src/ui.rs:5728-5768`).
- Produces `Theme.xaml` brushes and styles (values from `ui.rs` `CSS` and `home.rs` `STONE`):
  - Colours:
    - `ShellBg #0C0A09`, `PageBg #1C1917`, `Fg #F5F5F4`, `CardBg #0DFFFFFF`, `PopoverBg #292524`
    - `Accent #F38A35`, `AccentFg #1A0F05`, `AccentText #FFAE63`, `Liked #E5364F`
  - Text styles:
    - `HeroTitle` 36 px ExtraBold, `CharacterSpacing -42`, white.
    - `HeroSub` 15 px, `#CCFFFFFF`.
    - `NpTitle` 24 px ExtraBold.
    - `Lyric` 29 px ExtraBold, opacity 0.34; its lit state 1.0 with a 350 ms opacity transition.
    - `CardTitle` Bold; `CardSub` opacity 0.62.
  - The browse tiles' twelve 135° gradients `b0`–`b11`, from `home.rs` `CSS`.
- Produces `Shot` (developer runs only): `RadianceMusicBeta.exe --shot <png> --page <home|search|playing>
  [--size 1280x800] [--search <q>] [--wait <s>]`. It draws the window with `RenderTargetBitmap` to a PNG after the
  wait, then quits.
- Produces `SelfCheck`: `RadianceMusicBeta.exe --self-check <file>`, with no window. It writes `Native.SelfCheck()`'s
  lines, then `ok`, or `failed: …` and exit code 1.
  - `self_check()` (Task 7) returns, one per line:
    - `beta <beta_version>`, `engine <engine_version>`
    - `data <glib user_data_dir>/radiance-music`
    - `gstreamer <version>`
    - `missing plugins: <names or none>`: each plugin in `packaging/windows/ribs03/gst-plugins.txt` (minus `lib` and
      `.dll`) checked in the GStreamer registry
    - `yt-dlp <found|missing>` (where `youtube.rs` looks for it)
    - `recent <n>`: the songs in the test data's history

- [ ] **Step 1: Write the failing tests.**
  - `CoalescerTests`:
    - `two_marks_of_one_kind_post_once`: `Mark(Position)` is true, then false. `Flush()` is `[Position]`.
    - `a_flush_returns_every_kind_once_in_order`: mark `Results`, `Now`, `Results`. `Flush()` is `[Now, Results]`.
      After that, `Mark(Now)` is true again.
    - `marks_from_many_threads_lose_nothing`: 8 threads × 1,000 marks of mixed kinds. Every kind marked comes out in
      the union of the flushes.
  - In `build.ps1 -Step dist`, the self-check must print `ok`, and its `data` line must start with
    `C:\rb\beta-test\data`. Otherwise the step fails with "the beta opened a data folder outside C:\rb\beta-test".

- [ ] **Step 2: Run them to make sure they fail.**
  - Run on ribs03: `build.ps1 -Step app`.
  - Expected: the test project fails to build (`Coalescer` missing).

- [ ] **Step 3: Implement the project.**
  - `Program.cs` follows the POS app's: `--self-check` and `--shot` are handled before `Application.Start`.
  - `App.OnLaunched` does the following:
    1. `Native.AskTheOther(null)`. If that is true, show a message box "Radiance Music is already open. Close it to
       use Radiance Music (beta)." and exit.
    2. `Native.ListenForOthers(…)`: brings the window forward on the UI thread.
    3. `EngineHub.Start`, then the window.
  - `build.ps1 -Step dist` copies into `C:\rb\beta\dist`:
    - the app output and `radiance_ffi.dll`;
    - GStreamer's MSVC runtime DLLs (`bin\*.dll`);
    - `lib\gstreamer-1.0\` limited to the plugins in `gst-plugins.txt`;
    - `tools\` (yt-dlp, Deno, FFmpeg from `C:\rb\tools`);
    - and sets `GST_PLUGIN_PATH` and `GST_PLUGIN_SYSTEM_PATH` in code (`Program.cs`, before the engine starts) to the
      app's own `lib\gstreamer-1.0`.
  - The test data `C:\rb\beta-test` is made once as a copy of `C:\rb\test`, the GTK test copy's data (not the
    owner's). Task 15 then compares both apps on the same library.
    - If the self-check says `recent 0`, stop and ask the owner which song the tests may play.

- [ ] **Step 4: Run the tests, the self-check and the shell screenshots on ribs03.**
  - Run: `build.ps1 -Step all`.
  - Expected: `dotnet test` passes, and the self-check prints `missing plugins: none`, `yt-dlp found`, `ok`.
  - Screenshots through the `RadianceMusicTest` task: `--shot` of the empty shell at 1280×800 and 400×780, copied
    back with `scp`.
  - Open them and check the sidebar, the card's corners and the colours against a 0.7.19 screenshot of the same size.

- [ ] **Step 5: Commit.** Message: `Windows beta: the WinUI 3 window, Radiance's look, the engine bridge and self-check Refs #45`.

### Task 10: The Search page

**Files:**
- Create: `windows/RadianceMusic.Windows/Pages/SearchPage.xaml(.cs)`, `Controls/SongRow.xaml(.cs)`, `ArtCache.cs`
- Create: `windows/RadianceMusic.Windows.Tests/DebounceTests.cs`

**Interfaces:**
- Consumes: `MusicEngine.Search`, `Results`, `PlayFrom(ListKind, uint)`, `Native.BrowseTiles()`, and
  `EngineHub.Change` (`Results`, `Online`).
- Produces: `ArtCache.LoadAsync(string source, uint size) -> Task<BitmapImage?>`. It calls `ArtFile` on the thread
  pool, sets `DecodePixelWidth = size`, and keeps an LRU of 200 images. Tasks 11–14 use it.
- Produces: `Debounce` (in `SearchPage.xaml.cs` or `Debounce.cs`): `void Poke(string text)`. It runs its action 300 ms
  after the last poke, which is `SEARCH_DEBOUNCE_MS` (`src/ui.rs:43`).
- The page:
  - A search box. Its placeholder is "Search any song or artist" (`src/ui.rs:1247`).
  - Before typing, the browse tiles from `BrowseTiles()` with the Theme's gradients. Clicking a tile searches its
    query.
  - Then two sections, Lossless and YouTube Music, each with its note from `ResultsView` (the notes are the GTK
    app's words).
  - Rows: art 48 px (radius 6), title, byline, and a lossless pill.
  - Clicking a row calls `PlayFrom(Lossless|Songs, i)`.
  - Offline: the `OFFLINE_SEARCH` text.
  - Long text: one line, `TextTrimming="CharacterEllipsis"`.

- [ ] **Step 1: Write the failing test.** In `DebounceTests.cs`:
  - `pokes_within_300ms_run_once_with_the_last_text`: poke "b", "bi" and "binhi" 100 ms apart. Within 600 ms the
    action runs once, with "binhi".
  - The test uses an injectable clock and timer, so it doesn't sleep.

- [ ] **Step 2: Run it to make sure it fails.** Run on ribs03: `build.ps1 -Step app`. Expected: `Debounce` missing.

- [ ] **Step 3: Implement the page, `ArtCache` and `Debounce`.**

- [ ] **Step 4: Run the tests and the screenshots on ribs03.**
  - Run: `build.ps1 -Step all`. Expected: tests pass, self-check `ok`.
  - Screenshots: `--shot … --page search` with no query, then with `--search kpop`, at 1280×800 and 400×780.
  - Check them against 0.7.19's Search at the same sizes: the same sections, words and order; titles trimmed with "…".

- [ ] **Step 5: Commit.** Message: `Windows beta: Search Refs #45`.

### Task 11: The Home page

**Files:**
- Create: `windows/RadianceMusic.Windows/Pages/HomePage.xaml(.cs)`, `Controls/SongCard.xaml(.cs)`, `Controls/Lights.cs`

**Interfaces:**
- Consumes: `Home()`, `PlayFrom(Mix|Recent|Liked, i)`, `ShuffleFrom(Mix)`, `RefreshMix()`, `Backdrop(source)`,
  `ArtCache`, and `EngineHub.Change` (`Home`).
- The hero (`src/ui/home.rs:689-790`):
  - The greeting and the mix note.
  - Buttons:
    - With a mix: "Play your mix" (accent) and "Shuffle", each with an icon.
    - Without one: "Search for music", which goes to Search and focuses its box.
  - The first cover's wash (`Engine.Backdrop(source)?.Wash`) as the hero's background, radius 26.
  - The scrim gradient: left `#D9000000` → 45 % `#80000000` → right `#0D000000`.
  - The covers fanned out.
  - `Lights`: the two drifting lights, as composition visuals.
    - Each is a `SpriteVisual` with a `CompositionRadialGradientBrush` and an `Offset`/`Scale` keyframe animation
      that repeats forever, period `LEG_S = 18 s` (`src/ui/lights.rs`).
    - There's no per-frame C# code.
- Rows, top to bottom, at most 20 cards each:
  - "Made for you", with the "New picks" refresh button.
  - "Recently played".
  - "Your liked songs". There is no "See all" until Liked Songs comes in stage 2.
  - Albums, Artists and Genres rows come in stage 2.
- Cards are 180 px (136 below the phone breakpoint), art radius 10. Clicking a card plays that row from it.

- [ ] **Step 1: Write the failing test.** In the Tests project, `NoFrameCallbacksTests.no_page_hooks_the_render_loop`:
  - Reflection over the app assembly's IL is overkill. Instead, a test reads every `.cs` file under
    `windows/RadianceMusic.Windows/` and fails if any of them mentions `CompositionTarget.Rendering`.
  - Motion is the compositor's alone.

- [ ] **Step 2: Run it to make sure it passes.** It passes now, and it guards the next steps.
  - Run on ribs03: `build.ps1 -Step app`. Expected: pass.

- [ ] **Step 3: Implement the page, `SongCard` and `Lights`.**

- [ ] **Step 4: Run the tests and the screenshots on ribs03.**
  - Run: `build.ps1 -Step all`. Expected: all pass.
  - Screenshots: `--shot … --page home` at 1280×800 and 400×780. The test data is seeded with the plays and likes
    that Task 15's measurement uses.
  - Check them against 0.7.19's Home: the same rows, words and order; the hero's lights shown.

- [ ] **Step 5: Commit.** Message: `Windows beta: Home Refs #45`.

### Task 12: Now Playing with the compositor's moving background

**Files:**
- Create: `windows/RadianceMusic.Windows/Pages/NowPlayingPage.xaml(.cs)`, `Controls/Backdrop.cs`

**Interfaces:**
- Consumes: `Now()`, `Lyrics()`, `Backdrop(source)`, `ArtCache`, and `EngineHub.Change` (`Now`, `Position`,
  `Lyrics`).
- Produces: `--play-test` (developer runs only, in `Program.cs`): after start, it calls `PlayFrom(Recent, 0)`.
  Tasks 12–15 use it.
- Produces `Backdrop`, a `Grid` whose child visual (`ElementCompositionPreview.SetElementChildVisual`) is a
  `ContainerVisual`:
  - The wash: a `SpriteVisual` with a `CompositionSurfaceBrush` (from
    `LoadedImageSurface.StartLoadFromUri(wash file)`), `Stretch.UniformToFill`, filling the page.
  - Three patches from `backdrop.rs` `PATCHES` (where each wanders, its size, its turn in degrees a second, how much
    it shows):
    - each is a `SpriteVisual` of the blob file, with `CenterPoint` at its middle;
    - a `RotationAngleInDegrees` animation of 360° per (360 / turn) s, linear, forever, the sign giving the
      direction;
    - an `Offset` keyframe loop around its wander point, forever;
    - opacity as listed.
  - It never zooms past `MAX_ZOOM = 1.15`.
  - `void SetSong(string? source)`: the new layer fades in over 900 ms (`FADE_US`) above the old one, which is then
    removed.
  - The beat swell and bass flow wait for after the checkpoint; this is listed in "Left for later".
  - The scrim: a vertical gradient from `#4D000000` to `#8C000000` (`.np-scrim`).
- The page:
  - Cover (max 360 px, radius 16, shadow), title (`NpTitle`), byline, and the lossless/kbps pill.
  - The lyric lines in a `ScrollViewer` (`Lyric` style).
    - The lines in `Now().Lit` are lit.
    - On each `Position` change where the first lit line changed, it scrolls with
      `ChangeView(null, y, null, false)`, placing the line where `glide_to` does (`src/ui.rs:6018`).
  - Plain lyrics are shown as text (`lyric-plain`).
  - "Looking" and "none" show the GTK app's texts (`show_lyrics`, `src/ui.rs:5956`).
  - With nothing playing, the GTK app's empty Now Playing.

- [ ] **Step 1: Write the failing test.** In the Tests project, `LyricScrollTests`:
  - `scrolls_only_when_the_first_lit_line_changes`: `LyricFollower.Next(lit: [0])` returns `0`; `Next([0])` returns
    null; `Next([0,1])` returns null (the first lit line is still 0); `Next([1])` returns `1`.
  - `LyricFollower` is a pure class that the page uses.

- [ ] **Step 2: Run it to make sure it fails.** Run on ribs03: `build.ps1 -Step app`. Expected: `LyricFollower` missing.

- [ ] **Step 3: Implement `Backdrop`, `LyricFollower` and the page.**

- [ ] **Step 4: Run the tests and the screenshots on ribs03.**
  - Run: `build.ps1 -Step all`. Expected: all pass.
  - Screenshots: `--shot … --page playing` with `RADIANCE_SILENT=1`, after `--play-test` (a developer switch that
    plays the first Recent song from the test data). Take them at 1280×800 and 400×780, and a second shot 3 s later.
  - Expected: the patches moved between the two shots, the lit line changed, and the result is close to 0.7.19's Now
    Playing.

- [ ] **Step 5: Commit.** Message: `Windows beta: Now Playing, its background moved by the compositor Refs #45`.

### Task 13: The player bar

**Files:**
- Create: `windows/RadianceMusic.Windows/Controls/PlayerBar.xaml(.cs)`
- Create: `windows/RadianceMusic.Windows.Tests/SeekTests.cs`

**Interfaces:**
- Consumes: `Now()`, `TogglePlay`, `Skip`, `Seek`, `SetVolume`, `SetShuffle`, `CycleRepeat`, and `EngineHub.Change`
  (`Now`, `Position`).
- The layout copies GTK's (`src/ui.rs:2437-2485`):
  - Start: art 56 px, title and byline.
  - Centre: Shuffle, Previous, Play/Pause, Next, Repeat, with times and a seek slider under them.
  - End: mute and the volume slider.
  - The bar's heart and quality pill come in stage 2 with liking.
- Tooltips and their states come from `next_repeat` and `set_controls` ("Repeat: the whole queue", "Shuffle: on", …).
- Below the phone breakpoint it hides what `phone_hides` lists (`src/ui/home.rs:290`).
- The seek slider doesn't jump back while being dragged.

- [ ] **Step 1: Write the failing test.** In `SeekTests.cs`, for `SeekModel` (pure):
  - `position_updates_dont_move_a_dragged_slider`: `BeginDrag()`, `Position(30)`, so `Shown` stays at the drag value.
  - `EndDrag(42)` returns 42 to seek with, and the next `Position(42.2)` moves `Shown`.
  - `Clock(245.3)` is `"4:05"`, the same as `track::clock`.

- [ ] **Step 2: Run it to make sure it fails.** Run on ribs03: `build.ps1 -Step app`. Expected: `SeekModel` missing.

- [ ] **Step 3: Implement `SeekModel` and `PlayerBar`.**

- [ ] **Step 4: Run the tests and the screenshots on ribs03.**
  - Run: `build.ps1 -Step all`. Expected: all pass.
  - Screenshots of the bar while playing, at both sizes, with a long K-pop title.
  - Expected: the title trimmed with "…" and the controls centred.

- [ ] **Step 5: Commit.** Message: `Windows beta: the player bar Refs #45`.

### Task 14: Tray, media keys, closing and one copy

**Files:**
- Create: `windows/RadianceMusic.Windows/Tray.cs`, `MediaKeys.cs`, `TrayText.cs`
- Create: `windows/RadianceMusic.Windows.Tests/TrayTextTests.cs`
- Modify: `MainWindow.xaml.cs`, `App.xaml.cs`

**Interfaces:**
- `Tray`:
  - Uses `Shell_NotifyIconW` through P/Invoke, on its own message-only window (`HWND_MESSAGE`, with a `WndProc`
    delegate kept alive).
  - The icon is the app's `.ico`, at `GetSystemMetrics(SM_CXSMICON)`.
  - The menu (a `TrackPopupMenuEx` Win32 menu, like `tray.rs`) has Play/Pause, Next, Previous, "Show Radiance
    Music" and Quit, in `tray.rs`'s order (`src/tray.rs:46-50`).
  - A left click shows the window.
  - The icon is shown whenever the app is open, and removed at quit.
- `TrayText.Tooltip(string title, string byline) -> string`: "title – byline", cut to 127 UTF-16 units without
  splitting a surrogate pair.
- `MediaKeys`:
  - Uses `SystemMediaTransportControlsInterop.GetForWindow(hwnd)`.
  - Play, Pause, Next and Previous are enabled from `Now()`.
  - `ButtonPressed` calls the engine's commands. These are thread-safe and post to the engine thread.
  - `DisplayUpdater` is `Music`, with title, artist, album, and a thumbnail from `ArtFile(…, 300)`. It is updated on
    `Now`.
  - `PlaybackStatus` is updated, and the timeline at most every 5 s.
- Closing (`AppWindow.Closing`):
  - While playing, it hides the window and keeps playing, with the tray icon staying.
  - Otherwise it quits: `Engine.Quit()`, the tray icon removed, `Application.Current.Exit()`.
  - This is the GTK app's rule (`src/ui.rs:1510-1513`).
- One copy: the start-up checks are in Task 9. A second start of the beta, or of today's app, brings this window
  forward.

- [ ] **Step 1: Write the failing tests.** In `TrayTextTests.cs`:
  - `cuts_at_127_utf16_units`: a 200-character title gives a result of `Length <= 127`.
  - A title of emoji (surrogate pairs) cut at the boundary doesn't end in a lone high surrogate.
  - `joins_title_and_byline`: `Tooltip("Binhi", "Arthur Nery")` is `"Binhi – Arthur Nery"`.

- [ ] **Step 2: Run them to make sure they fail.** Run on ribs03: `build.ps1 -Step app`. Expected: `TrayText` missing.

- [ ] **Step 3: Implement `TrayText`, `Tray`, `MediaKeys` and closing.**

- [ ] **Step 4: Check the tray and one copy on ribs03.** Run through the `RadianceMusicTest` task, silent, with the
  test data:
  - Screenshots of the tray icon, its menu and the volume flyout's media card.
  - Media keys, tested with `keybd_event(VK_MEDIA_PLAY_PAUSE)` from the test script: `Now().Playing` flips.
  - The GTK test copy started second: it quits, and the beta comes forward.
  - The beta started second: the message, then it exits.

- [ ] **Step 5: Commit.** Message: `Windows beta: tray, media keys, close while playing, one copy at a time Refs #45`.

### Task 15: The beta's installer, the feature list and the CPU checkpoint

**Files:**
- Create: `windows/installer-beta.nsi`, `windows/measure-cpu.ps1`
- Modify: `windows/build.ps1` (an `-Step installer`), `docs/FEATURES.md` (a "Windows beta" column)

**Interfaces:**
- `installer-beta.nsi`:
  - Per-user, into `$LOCALAPPDATA\Programs\Radiance Music (beta)`.
  - A Start-menu entry "Radiance Music (beta)".
  - An uninstaller under `HKCU\…\Uninstall\Radiance Music (beta)`.
  - No `radiancemusic://` registration and no file types. Those stay with today's app until stage 4.
  - Built with MSYS2's `makensis` (NSIS 3.13), giving `C:\rb\beta\RadianceMusic-Beta-Setup.exe` and its SHA-256.
- `measure-cpu.ps1 -App beta|gtk -Seconds 60`:
  - Starts the app through the `RadianceMusicTest` task with the test data and `RADIANCE_SILENT=1`, at 1280×800, in
    front.
  - The beta uses its `--play-test` switch, and the GTK test copy uses `RADIANCE_SHOT_PLAY=1`, with the same song.
  - It opens Now Playing and waits 10 s.
  - It prints the process's CPU as a percentage of one core over `Seconds`
    (`TotalProcessorTime` delta / wall time × 100).
  - It measures again with the window minimized. For the beta, it also measures hidden to the tray.

- [ ] **Step 1: Build the installer.**
  - Run on ribs03: `build.ps1 -Step installer`.
  - Expected: `RadianceMusic-Beta-Setup.exe` and its SHA-256 printed.
  - `7z l` of it lists `RadianceMusicBeta.exe`, `radiance_ffi.dll`, `lib\gstreamer-1.0\` and `tools\yt-dlp.exe`.

- [ ] **Step 2: Ask in a dialog before installing it on ribs03.**
  - The question: may the beta be installed for a check, in the test account's folders only, then uninstalled?
  - On "no", Step 1's listing stands as the installer check.

- [ ] **Step 3: Measure, after telling the owner about the ~5 minutes of test windows on ribs03.**
  - Run: `measure-cpu.ps1 -App gtk` (0.7.19's test copy), then `-App beta`.
  - Expected: four numbers each (Now Playing in front, minimized, hidden to tray (beta only), and Home in front).

- [ ] **Step 4: Add the feature-list column.** Add "Windows beta" to `docs/FEATURES.md`:
  - ✅ for the rows stage 1 covers;
  - 🔜 (with the stage number) for the rest.

- [ ] **Step 5: Commit.** Message: `Windows beta: installer, its feature-list column, the CPU measurement Refs #45`.

- [ ] **Step 6: The checkpoint.** Report the numbers to the owner in a dialog, beside 0.7.19's ~60 %.
  - If the beta's Now Playing is clearly under 60 % (aim under 15 %): offer (recommended first) shipping beta 1 as an
    extra file in the next computer release, keeping it for testing, or going on to stage 2.
  - If it isn't: stop, and give the numbers and what used the CPU.
  - Merging `winui-stage1` into `main` is part of that dialog.

---

## Left for later (not in stage 1, on purpose)

- **Stage 2:**
  - Liking (heart) and saving.
  - The Liked Songs, Albums, Artists, Genres and History pages, and those Home rows.
  - The queue panel and song menus.
  - Word-by-word lyrics and the lyrics column.
  - Music videos and the Videos search section.
- **Stage 3:**
  - The beat swell and bass flow on Now Playing's background. The engine will send kick times, and the compositor
    will run each short swell.
  - The Mixer, parts and splitting.
- **Stage 4:** Servers and parties, `radiancemusic://`, sharing, update/report/rating pop-ups, Storage and settings.
- **Ports kept in `ui.rs` until then:** GTK's playback orchestration for parts, voices, videos, parties and the DJ mix.
