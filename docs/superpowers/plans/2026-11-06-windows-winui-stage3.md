# Radiance Music for Windows, native (WinUI 3): stage 3 implementation plan (Parts and splitting)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The beta's third release. Everything below behaves as the Linux app does:
- **The Mixer:** part sliders (Drums, Bass, Other, Vocals; Music and Vocals for a two-part song) with meters, the loudness kept, Low cut / High cut / Reverb, and the Song / No vocals / Vocals only buttons.
- **Splitting:**
  - the first-open download of the splitter, with its progress on the sunrise;
  - the split queue, with pause;
  - liked songs split by themselves;
  - the fast split in two;
  - "Cores it may use".
- **Lyrics:** every word timed by MMS, the lyrics editor, Show Lyrics, Find Lyrics Again, and lyrics coming back by themselves once you're online again.
- **The beat (#56):** Now Playing's background follows the music. Each kick swells it and strong bass speeds the flow.

**Architecture:**
- The orchestration still in `src/ui.rs` moves out: the split queue, the split job, word timing, Find Lyrics Again, saving edited lyrics, and the parts' volumes. It goes into `core` (pure) and `engine`, and the GTK app calls the moved code.
- The `Controller` grows a split worker thread and the first-open setup. Playback learns parts: the player's `load_parts` / `set_part` already exist, and nothing calls them yet.
- `ffi` exposes all of it. The C# app adds:
  - a Mixer Expander on Now Playing;
  - a Splits tab in the queue panel;
  - the setup's hold on the sunrise;
  - the lyrics dialogs;
  - a beat-driven Backdrop.

**Tech Stack:** As stage 2 (uniffi `=0.31.0`; ribs03 MSYS2 UCRT64 GStreamer 1.28.8; .NET 10; Windows App SDK 2.6.5).

**Spec:** `docs/superpowers/specs/2026-11-05-windows-winui-design.md`, Stages → 3 (issue #45), plus the owner's #56, put inside stage 3 by dialog (2026-11-06). Stage 2's plan (`docs/superpowers/plans/2026-11-05-windows-winui-stage2.md`) has the build pipeline. A map of the GTK code this plan moves, with line numbers as of `main` 4bfe657, is in the session's scratch `stage3-map.md`. Re-find a function by its name if the lines have moved.

## Global Constraints

Stage 2's constraints hold, unchanged except where noted:
- **ribs02:** Rust tests and Linux builds, through the winui copy (`$S/rt`), plus `touch` after rsync, and the lockfile copied back.
- **ribs03:** Windows builds only, through `C:\rb\beta\run-step.ps1 -Step rust|app|dist|installer|all`, at Idle priority.
- **Git:**
  - branch `winui-stage3` from `main` 4bfe657, moved to ribs03 by `git bundle create s3.bundle 4bfe657..winui-stage3`, then `git fetch <bundle> winui-stage3; git reset --hard FETCH_HEAD` in `C:\rb\beta\src`;
  - merging, pushing, releasing and installing each wait for a dialog.
- **ribs03 is the owner's PC:**
  - announce each build or test there;
  - test windows open only through `windows\test-run.ps1` (the `RadianceMusicTest` task), with test data only in `C:\rb\beta-test`;
  - before any test window, `Set-Content D:\rpos\HOLD music` and message RADIANCE PROJECT MANAGER; delete the HOLD file after, and message it again.
  - no media keys;
  - if the owner's own beta is open, ask before closing it.
- **Engine rules:**
  - `core` has no GLib or GStreamer;
  - after every task these pass on ribs02:
    - `cargo test --release -p radiance-core -p radiance-engine -p radiance-ffi -p radiance-music`
    - `cargo build --release --locked`
  - behaviour is written once: what a task moves out of `ui.rs`, the GTK app calls in the same task, its words and behaviour unchanged;
  - commands that name a song carry its `key`, and a stale key does nothing.
- **Splitting in tests:**
  - no test runs Demucs, Spleeter or MMS: they go through `Sources` hooks that the test `Fake` overrides (Task 4);
  - `RADIANCE_SILENT=1`;
  - liked songs split by themselves only when `Settings::auto_split` allows it, and the tests turn that off unless they test it.
- **Words and look:**
  - every string the beta shows comes from `core::words` or a `core` function a task names;
  - the strings quoted here are copied from the GTK code: keep them exactly;
  - Windows' own look (Expander, Slider, ContentDialog, SelectorBar, InfoBar, ProgressBar);
  - nothing sharp, and every button carries a leading icon.
- **Version:** `0.8.0-beta.3` (`ffi/Cargo.toml` and the csproj), set in Task 16.
- **Stage boundaries:**
  - splitting on a Radiance Music Server, hosting a split song, crossfade / DJ mix, Track Info and story cards stay in stage 4;
  - a split server set in the GTK app's settings (`settings.split`) is used as the GTK app uses it, because that comes with the moved split job;
  - six parts stay off (`split::SIX_PARTS_ON = false`); songs already split in six still play, with piano and guitar following Other.

## Review Focus

1. **A song's parts land while it's playing:** it carries on from the same moment with its parts, with no restart and no gap, as GTK's `split_landed` → `start_parts(switch: true)` fades it over. Test: `parts_landing_while_playing_switch_over_in_place` (Task 6).
2. **Pause, or quit, while a song is being split:** pausing lets the current split finish, and then nothing else starts until you resume. Quitting stops the worker without leaving half-written part files beside a liked song. Test: `pausing_finishes_the_song_being_split_then_waits` (Task 4).
3. **Offline at the very first start:** the sunrise doesn't hold for ever. It says "You're offline. Splitting songs needs a one-time download: it starts when you're back online." and offers "Open Radiance Music Now", and the download starts by itself once you're online. Test: `offline_first_start_lets_you_in_and_gets_ready_later` (Task 5).
4. **Saving edited lyrics with blank times, blank lines, or no times at all:**
   - a line without a time sits halfway between its neighbours (or 4 s after the last time);
   - empty lines are dropped;
   - all untimed becomes plain text;
   - a lookup never replaces them afterwards.

   Tests: `from_edited_fills_times_between_neighbours` (Task 2) and `edited_lyrics_are_kept_and_never_replaced` (Task 7).
5. **The Mixer on a song with four parts, with halves only, or not split:**
   - the sliders shown are `split::mix_parts`'s;
   - on four parts the voice buttons are presets;
   - on halves they switch the half file;
   - on a song not split, No vocals asks for the fast split and says so;
   - Reset puts every part back at full.

   Test: `the_mixer_shows_the_parts_the_song_has` (Task 6).

---

### Task 1: core — the Mixer's and splitting's words and rules

**Files:**
- Create: `core/src/mix.rs` and `core/src/split_queue.rs` (register both in `core/src/lib.rs`)
- Modify:
  - `core/src/words.rs`;
  - `core/src/quality.rs` (nothing new: `Heard::Parts` exists);
  - `src/ui.rs`: GTK calls the moved code: `part_volumes` (6930, its test moves too), the voice presets in `voice_changed` (2936), the part titles in the Mixer rows (2030–2069), and the `split_queue`/`queue_split`/`split_now` ordering (3251–3300).
- Test: unit tests in both new files

**Interfaces:**
- Produces:
  - **The parts' volumes:**
    - `core::mix::part_volumes(sliders: &HashMap<String, f64>, six: bool) -> HashMap<String, f64>`, moved verbatim with `every_part_gets_a_volume`;
    - `core::mix::preset(voice: Voice, part: &str) -> f64`: Song gives 1.0 for every part; NoVocals gives vocals 0.0 and the rest 1.0; VocalsOnly gives vocals 1.0 and the rest 0.0.
  - **The voice and the part titles:**
    - `core::mix::Voice { Song, NoVocals, VocalsOnly }`, moved from `engine::split::Voice` with `label()`: "Song", "No vocals", "Vocals only". `engine::split` re-exports it so the existing callers compile.
    - `core::mix::part_title(part: &str) -> &'static str`: "Drums", "Bass", "Other", "Piano", "Guitar", "Music", "Vocals".
  - **The effects' slider words:** `core::mix::EFFECT_WORDS: [(&str, &str); 3]`, the Low cut / High cut / Reverb titles and subtitles copied from ui.rs 2071–2090.
  - **The split queue:** `core::split_queue::SplitQueue`, holding `waiting: Vec<Track>` and `now: Option<Track>`. Its methods:
    - `ask(&mut self, t: Track, first: bool) -> bool`: false if the song is being split now. Otherwise the song is removed from where it waits and put first or last.
    - `take_next(&mut self) -> Option<Track>`: sets `now`.
    - `finish(&mut self) -> Option<Track>`: clears `now`.
    - `place(&self, key: &str) -> Option<usize>`: 0 means being split now; n means n-th waiting.
    - `drop_key(&mut self, key: &str)`.
  - **When liked songs split by themselves:**
    - `core::split_queue::AUTO_AFTER_S: u64 = 20`
    - `core::split_queue::auto_list(liked: &[Track], done: impl Fn(&Track) -> bool) -> Vec<Track>`: saved (`file.is_some()`), not video, not done, in liked order.
  - **The split words in `core::words`, copied from GTK:**
    - the badges: "Waiting to be split" and "Number {n} in line; liked songs are split one after another.";
    - the progress phases from `show_split_progress` (3491);
    - `split_failed(title) = "Couldn't split “{title}” right now"`;
    - the setup steps from the list above ("Getting Radiance Music ready…", "Getting Radiance Music ready: {step}", "Couldn't get splitting ready yet. Trying again in a moment…", "You're offline. Splitting songs needs a one-time download: it starts when you're back online.", "Open Radiance Music Now", "Play music now; splitting gets ready in the background when the internet's back", "How to Fix It");
    - the menu labels "Split: Music + Vocals (Fast)", "Split: Four Parts", "Split in Four Parts", "Show Lyrics", "Edit Lyrics…", "Find Lyrics Again";
    - "Lyrics saved" and "Couldn't save the lyrics".

- [ ] **Step 1: Write the failing tests:**
  - `every_part_gets_a_volume` (moved);
  - `presets_turn_only_the_vocals`, which asserts all seven parts for each `Voice`;
  - `a_song_asked_for_goes_first_and_is_never_twice`: ask a, then b, then `ask(a, true)`; the order is [a, b], and `ask` returns false while a is `now`;
  - `place_counts_from_the_one_being_split`;
  - `auto_list_skips_videos_streams_and_done`.
- [ ] **Step 2: Run them.** Run `$S/rt 'cargo test -q -p radiance-core mix split_queue'`. Expected: they fail to compile, because the modules don't exist.
- [ ] **Step 3: Write the modules and move GTK onto them.** `split_queue: RefCell<Vec<Track>>` and `splitting: RefCell<Option<Track>>` become one `RefCell<SplitQueue>`, and their call sites use `ask`, `take_next` and `finish`. `auto_split_liked` uses `auto_list`.
- [ ] **Step 4: Run the whole suite.** Run `$S/rt 'bash /srv/build/winui/suite.sh'`. Expected: all pass and the locked build is OK.
- [ ] **Step 5: Commit:** "Core: the Mixer's volumes and presets, the split queue and their words, moved from ui.rs (Refs #45)".

### Task 2: engine — word timing, Find Lyrics Again and edited lyrics, moved out of ui.rs

**Files:**
- Create: `engine/src/word_times.rs` (registered in `engine/src/lib.rs`)
- Modify:
  - `core/src/lyrics.rs`: `from_edited`;
  - `src/ui.rs`: delete the moved code and import it from the engine. The moved items are `Aligner` (425), `relook_lyrics` (445), `keep_on_server` (472), `hand_edited` (488), `time_words` (495), `aligned_by_mms` (602), `hosted_lyrics`, and the save body of `lyrics_editor` (6540–6590).
- Test: `core/src/lyrics.rs` unit tests, and `engine/src/word_times.rs` unit tests on files in a temp dir.

**Interfaces:**
- Produces:
  - `core::lyrics::from_edited(picked: &[(Option<f64>, String, Option<String>)]) -> Lyrics`:
    - empty texts are dropped;
    - all untimed gives `Lyrics::Plain`, joined by "\n";
    - otherwise each missing time is `before + (after - before) / 2`, with before defaulting to 0.0 and after to before + 4.0;
    - `end` is the next start, or start + 5.0, and at least start + 0.3;
    - words come from `spread`, and `exact` is false. This is copied from ui.rs 6548–6568.
  - Moved verbatim (unchanged signatures):
    - `engine::word_times::{Aligner, relook_lyrics, keep_on_server, hand_edited, time_words, aligned_by_mms}`.
  - `engine::word_times::save_edited(t: &Track, l: &Lyrics) -> Result<(), String>`. It writes the Lyricsfile with `[HAND_NOTE]` to `lyrics_file(t)`, moves a song's old `.lrc` beside its file to the Trash, and calls `db::set_lyrics_checked`. It returns `Err` when it can't write.

- [ ] **Step 1: Write the failing tests:**
  - `from_edited_fills_times_between_neighbours`: given [(Some 10, "a"), (None, "b"), (Some 20, "c"), (None, "d")], b starts at 15 and d at 24, and each end is the next start;
  - `from_edited_with_no_times_is_plain`;
  - `from_edited_drops_empty_lines`;
  - `saving_edited_lyrics_marks_them_hand_made`: `save_edited` on a temp song file, then `hand_edited` is true and the `.lrc` beside the song is gone.
- [ ] **Step 2: Run them.** Run `$S/rt 'cargo test -q -p radiance-core from_edited; cargo test -q -p radiance-engine word_times'`. Expected: they fail to compile.
- [ ] **Step 3: Implement and move.** The GTK editor's save calls `from_edited` and `save_edited`, and its toasts use the Task 1 words.
- [ ] **Step 4: Run the suite.** Expected: all pass.
- [ ] **Step 5: Commit:** "Engine: word timing, Find Lyrics Again and saving edited lyrics, moved from ui.rs (Refs #45)".

### Task 3: engine — the split jobs, moved out of ui.rs

**Files:**
- Create: `engine/src/splitting.rs`
- Modify: `src/ui.rs`. `run_splits` (3355) and `run_fast` (6181) keep their glib orchestration, and their worker closures call the moved jobs.
- Test: `engine/src/splitting.rs` unit tests

**Interfaces:**
- Consumes: Task 2's `word_times`.
- Produces:
  - `pub enum Phase { Splitting(f64), LibraryCopy, WordTimes }`, the GTK `-1`/`-2` progress codes made explicit.
  - `pub fn after_split(server: bool, local: bool, aligned: bool) -> After`, with `pub enum After { Host, TimeWordsHere, Nothing }`:
    - a server gives `Host`;
    - local and not aligned gives `TimeWordsHere`;
    - anything else gives `Nothing`.
  - `pub fn four_parts(t: &Track, uri: &str, server: Option<&SplitServer>, local: Option<&LocalSplitter>, report: &(dyn Fn(Phase) + Send + Sync)) -> Result<(), String>`: `split::split`, then the `after_split` step. This is run_splits' worker body.
  - `pub fn two_parts(t: &Track, uri: &str, local: Option<&LocalSplitter>, report: &(dyn Fn(Phase) + Send + Sync)) -> Result<(), String>`: `two_part::fetch` when needed, then `split::split_two`. This is run_fast's worker body.

- [ ] **Step 1: Write the failing test** `after_a_split_the_words_are_timed_once`. It checks all 8 combinations of `after_split`.
- [ ] **Step 2: Run it.** Expected: it fails to compile.
- [ ] **Step 3: Move the bodies and make GTK call them.** GTK maps `Phase` back onto its own progress display.
- [ ] **Step 4: Run the suite.** Expected: all pass.
- [ ] **Step 5: Commit:** "Engine: the split jobs (four parts, the fast split in two), moved from ui.rs (Refs #45)".

### Task 4: engine — the split queue in the controller

**Files:**
- Modify:
  - `engine/src/controller.rs`;
  - `engine/src/sources.rs`: the `Sources` trait gets `fn split(&self, t: &Track, two: bool, report: &(dyn Fn(Phase) + Send + Sync)) -> Result<(), String>`, whose default calls Task 3's jobs with the settings' server and local splitter;
  - `engine/src/settings.rs`: `auto_split: bool`, default true;
  - `engine/tests/controller.rs`: the `Fake`'s `split` writes four small WAV parts with `split::stems_base` after an optional delay, or fails on request.
- Test: `engine/tests/controller.rs`

**Interfaces:**
- Consumes: Task 1's `SplitQueue` and `auto_list`; Task 3's `Phase`.
- Produces:
  - **Commands:**
    - `Controller::split(&self, key: &str, two: bool)`: asked by hand, so the song goes first;
    - `Controller::set_split_paused(&self, on: bool)`: the GTK flag file `user_config_dir/radiance-music/split-paused`;
    - `Controller::splits(&self) -> SplitsState`.
  - **The state:** `SplitsState` has:
    - `now: Option<(Track, f64, Phase)>`;
    - `waiting: Vec<Track>`;
    - `two_waiting: Vec<Track>`;
    - `paused: bool`;
    - `can_split: bool`: `settings.local_split` or `settings.split` is set;
    - `can_split_two: bool`, as GTK's `can_split_two` (6116).
  - **The marks and changes:**
    - `Change::Splits`;
    - `Controller::split_mark(&self, key: &str) -> SplitMark`, with `enum SplitMark { None, Waiting(usize), Splitting, Two, Four }`.
  - **The worker:** one thread for four parts and one for the fast lane, as GTK. When a split ends:
    - the song's mark changes;
    - a song split by hand that failed gets `toast(split_failed(title))`;
    - the current song switches to its parts (Task 6).
  - **Liked songs:** they split by themselves `AUTO_AFTER_S` seconds after start when `auto_split` is on, and each newly liked song is asked for too.

- [ ] **Step 1: Write the failing tests:**
  - `a_split_asked_for_goes_first`;
  - `pausing_finishes_the_song_being_split_then_waits`: three asked; pause during the first; it lands, and the second doesn't start within 1 s; on resume, it does;
  - `liked_songs_are_split_by_themselves`, with `auto_split` on and `AUTO_AFTER_S` read through a test-only start delay of 0;
  - `a_failed_split_asked_for_says_so`;
  - `the_fast_split_takes_its_own_lane`: a two-part split runs while a four-part one does.
- [ ] **Step 2: Run them.** Run `$S/rt 'cargo test -q -p radiance-engine --test controller -- split'`. Expected: they fail to compile.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run the suite.** Expected: all pass, and the old tests are unchanged in what they check (they turn `auto_split` off).
- [ ] **Step 5: Commit:** "Engine: the split queue, its pause and liked songs splitting by themselves (Refs #45)".

### Task 5: engine — splitting's first-open setup and the cores it may use

**Files:**
- Modify:
  - `engine/src/controller.rs`;
  - `engine/src/sources.rs`: `fn install_splitter(&self, threads: u32, say: &(dyn Fn(&str, f64) + Send + Sync)) -> Result<LocalSplitter, String>`, whose default is `setup::install`;
  - `engine/tests/controller.rs`.
- Test: `engine/tests/controller.rs`

**Interfaces:**
- Produces:
  - **The state:** `Controller::setup(&self) -> SetupState`, with `enum SetupState { NotNeeded, Getting { step: String, done: f64 }, Offline, Failed { why: String }, Ready }`.
  - **When it's needed:** only when `local_split` is None and `setup::installed(threads)` is None, on Windows. Tests force it with `RADIANCE_SETUP_TEST=1`.
  - **The changes:**
    - `Change::Setup`;
    - success saves `settings.local_split`;
    - a failure tries again after 30 s, or at once when online comes back;
    - while offline the state is `Offline`.
  - **Commands:**
    - `Controller::retry_setup(&self)`;
    - `Controller::set_split_cores(&self, n: u32)`, saved in `local_split.threads`, used from the next split;
    - `Controller::split_cores(&self) -> (u32, u32)` gives the current count and `setup::cores()`.

- [ ] **Step 1: Write the failing tests:**
  - `the_first_start_gets_splitting_ready_with_progress`: the Fake installer says two steps; `Getting` is seen with both texts, then `Ready`, and `local_split` is saved;
  - `offline_first_start_lets_you_in_and_gets_ready_later`: the Fake fails with "you're offline"; the state is `Offline`; `set_online(true)` then leads to `Ready`;
  - `cores_are_kept_for_the_next_split`.
- [ ] **Step 2: Run them.** Expected: they fail to compile.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run the suite.** Expected: all pass.
- [ ] **Step 5: Commit:** "Engine: getting splitting ready at the first start, and the cores it may use (Refs #45)".

### Task 6: engine — playing parts, the Mixer and the effects

**Files:**
- Modify:
  - `engine/src/controller.rs`: `play_current`, `on_player`, and the Task 4 landing;
  - `engine/src/settings.rs`: `effects` is already there;
  - `engine/tests/controller.rs`.
- Test: `engine/tests/controller.rs`

**Interfaces:**
- Consumes: Task 1's `part_volumes` and `preset`; Task 4's landing hook.
- Produces:
  - **Playing parts:**
    - `play_current` plays the parts when `split::mix_parts(t)` is not empty and they're ready, with `player.load_parts(whole, parts, at, gains, rate, false)`; `parts_of` and `file_rate` move here from ui.rs 3138/642.
    - A landing for the current song calls `load_parts(…, switch: true)` from the player's position.
  - **Commands:**
    - `Controller::set_part(&self, part: &str, gain: f64)`: `player.set_part`, and kept in `part_gains` for the session, as GTK keeps them, not saved;
    - `Controller::reset_mix(&self)`: every part at 1.0, and the effects to `Sound::OFF`;
    - `Controller::set_voice(&self, v: Voice)`:
      - with four parts, a preset;
      - with halves, the half file, as GTK's `switch_half`;
      - not split, the fast split is asked for first, with the toast GTK shows (`voice_split`, 2972);
    - `Controller::set_effects(&self, s: Sound)`: `effects::set`, and saved in `settings.effects`;
    - `Controller::part_levels(&self) -> Vec<(String, f64)>`: `player.part_levels()`, polled by the app.
  - **NowState gains** `mix: MixState { parts: Vec<(String, f64)> (mix_parts order, with gains), voice: Voice, effects: Sound, plays_parts: bool }`.
  - **At start:** `effects::set(settings.effects)`, as GTK's 2468.
  - **The quality line:** while parts play it says `Heard::Parts` ("Mixer · its four parts").

- [ ] **Step 1: Write the failing tests:**
  - `a_split_song_plays_its_parts`: WAV parts under `stems_base`; it plays, and `now().mix.plays_parts` is true;
  - `the_mixer_shows_the_parts_the_song_has`: four parts, then halves, then none, checking the `mix.parts` names and the presets;
  - `parts_landing_while_playing_switch_over_in_place`: playing at 1.0 s or later when the Fake split lands; it plays parts and the position doesn't go back below 0.8 s;
  - `effects_are_kept_in_settings`;
  - `reset_puts_every_part_back_at_full`.
- [ ] **Step 2: Run them.** Expected: they fail to compile.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run the suite.** Expected: all pass.
- [ ] **Step 5: Commit:** "Engine: split songs play their parts; the Mixer, its voice buttons and the effects (Refs #45)".

### Task 7: engine — lyrics: MMS after a split, Find Lyrics Again, edited lyrics, back online

**Files:**
- Modify:
  - `engine/src/controller.rs`;
  - `engine/src/sources.rs`: `fn time_words(&self, t: &Track) -> Result<(), String>`, whose default is `word_times::time_words` with `Aligner::of(settings)`;
  - `engine/tests/controller.rs`.
- Test: `engine/tests/controller.rs`

**Interfaces:**
- Consumes: Task 2.
- Produces:
  - **Commands:**
    - `Controller::find_lyrics_again(&self, key: &str)`: `relook_lyrics` in the background, then GTK's toasts (6326) and the lyrics reloaded if it's the current song;
    - `Controller::save_lyrics(&self, key: &str, lines: Vec<(Option<f64>, String, Option<String>)>)`: `from_edited` + `save_edited`, the toast "Lyrics saved" or "Couldn't save the lyrics", the lyrics reloaded, then `time_words` when the song has parts;
    - `Controller::lyrics_text(&self, key: &str) -> Option<(Vec<(Option<f64>, String, Option<String>)>, Vec<String>)>`: the editor's starting rows and its singer names, as GTK's `edit_lyrics` (6348);
    - `Controller::singers(&self, key: &str) -> Vec<String>`, the song's artists.
  - **After splits:** a four-part split that lands with the words not yet aligned by MMS runs `time_words` (`time_words_later`, 6236).
  - **Back online:** lyrics that couldn't be looked up offline are looked up again when `set_online(true)`, as GTK's `lyrics_offline` (1656).

- [ ] **Step 1: Write the failing tests:**
  - `edited_lyrics_are_kept_and_never_replaced`: `save_lyrics` with two lines, then play the song with the Fake's lyrics source giving other lines; the shown lines are the edited ones;
  - `found_again_lyrics_replace_the_kept_ones`;
  - `lyrics_come_back_when_online_again`;
  - `words_are_timed_after_a_split_lands`: the Fake `time_words` is called once.
- [ ] **Step 2: Run them.** Expected: they fail to compile.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run the suite.** Expected: all pass.
- [ ] **Step 5: Commit:** "Engine: lyrics timed after a split, Find Lyrics Again, edited lyrics, and lyrics back online (Refs #45)".

### Task 8: engine — the song's beat for the background (#56)

**Files:**
- Modify: `engine/src/controller.rs`, `engine/tests/controller.rs`
- Test: `engine/tests/controller.rs`

**Interfaces:**
- Produces:
  - `Controller::pulse(&self) -> Option<(String, Arc<beat::Pulse>)>`, the song's key and its pulse;
  - it's worked out once per song, in the background, once the song is 3 s in, so a stream's start isn't slowed: GTK's `load_pulse` (6906) moves here;
  - `Change::Pulse` when the pulse arrives or is cleared, and it's cleared when the song changes.

- [ ] **Step 1: Write the failing test** `a_playing_song_gets_its_pulse`: a 6 s WAV of clicks plays; within 8 s, `pulse()` is Some for its key with `fps > 0` and a non-empty `level`; a new song clears it.
- [ ] **Step 2: Run it.** Expected: it fails to compile.
- [ ] **Step 3: Implement.** The GTK app keeps its own `load_pulse`, because its backdrop is GTK-only, but it calls the same `beat::pulse`.
- [ ] **Step 4: Run the suite.** Expected: all pass.
- [ ] **Step 5: Commit:** "Engine: each song's beat worked out for Now Playing's background (#56, Refs #45)".

### Task 9: core — the song menu's split and lyrics items

**Files:**
- Modify: `core/src/menu.rs`, `engine/src/controller.rs` (`song_menu`), and `src/ui.rs` (the GTK menu, 4085–4134, builds these items from `core::menu`)
- Test: `core/src/menu.rs` unit tests

**Interfaces:**
- Produces:
  - **New actions:** `Action::{SplitTwo, SplitFour, ShowLyrics, EditLyrics, FindLyricsAgain}`.
  - **`menu::split_items(t, online, has_halves, has_four, can_two, can_four, busy_two, busy_four) -> Vec<Item>`.** The rules are GTK's:
    - nothing for a video, or offline with no file;
    - "Split: Music + Vocals (Fast)" when there are no halves, it can split in two, and it isn't busy;
    - "Split: Four Parts", or "Split in Four Parts" when there are halves, when it can split, has fewer than 4 parts, and isn't busy.
  - **The lyrics section:** "Show Lyrics", "Edit Lyrics…" and "Find Lyrics Again".

- [ ] **Step 1: Write the failing test** `split_items_follow_the_linux_menu`. It is a table over the eight inputs: a video, offline without a file, with halves, busy, and the rest.
- [ ] **Step 2: Run it.** Expected: it fails to compile.
- [ ] **Step 3: Implement, and move the GTK menu onto it.**
- [ ] **Step 4: Run the suite.** Expected: all pass.
- [ ] **Step 5: Commit:** "Core: the song menu's split and lyrics items (Refs #45)".

### Task 10: ffi — the stage-3 surface

**Files:**
- Modify: `ffi/src/lib.rs` and its tests
- Test: `ffi/src/lib.rs` tests

**Interfaces:**
- Consumes: Tasks 4–9.
- Produces:
  - **The views:**
    - `MixView { parts: Vec<PartView { name, title, gain }>, voice: VoiceMode, low_cut, high_cut, reverb, plays_parts }`, in `NowView.mix`;
    - `SplitsView { now: Option<SplitNowView { song, done: f64, phase: String (core words) }>, waiting: Vec<SongView>, two_waiting: Vec<SongView>, paused, can_split, can_split_two }`;
    - `SetupView { kind: SetupKind, step: String, done: f64, why: String }`;
    - `PulseView { key, fps, offset, level: Vec<f32>, bass: Vec<f32> }`;
    - `EditedLineView { at: Option<f64>, text: String, singer: Option<String> }`;
    - `SongView.split: SplitMarkView`;
    - `ChangeKind::{Splits, Setup, Pulse}`.
  - **The commands:**
    - the Mixer: `set_part`, `reset_mix`, `set_voice`, `set_effects(low, high, reverb)`, `part_levels() -> Vec<PartLevelView>`;
    - splitting: `split(key, two)`, `set_split_paused`, `splits()`, `split_mark(key)`, `setup()`, `retry_setup()`, `set_split_cores(n)`, `split_cores() -> CoresView { now, max, measured: String, timings: Vec<String> }` (the GTK dialog's words from `src/ui/splitting.rs:29`);
    - lyrics: `find_lyrics_again(key)`, `save_lyrics(key, lines)`, `lyrics_text(key) -> Option<LyricsEditView>`, `show_lyrics(key) -> String`, the plain text the GTK `lyrics_dialog` shows;
    - the beat: `pulse() -> Option<PulseView>`.

- [ ] **Step 1: Write the failing tests:**
  - `the_mix_view_names_the_parts_in_the_linux_words`;
  - `split_marks_reach_the_rows`;
  - `edited_lines_go_through`.
- [ ] **Step 2: Run them.** Expected: they fail to compile.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run the suite on ribs02, then `-Step rust` on ribs03.** Expected: all pass, and `radiance_ffi.dll` and the C# binding are built.
- [ ] **Step 5: Commit:** "ffi: the Mixer, splitting, setup, lyrics editing and the beat (Refs #45)".

### Task 11: C# — the Mixer on Now Playing

**Files:**
- Create: `windows/RadianceMusic.Windows/Controls/Mixer.cs` and `Pure/EffectText.cs`
- Modify: `Pages/NowPlayingPage.xaml(.cs)`: the Mixer goes in a WinUI `Expander` headed "Mixer", under the quality chip in the art column, collapsed by default, as GTK's ExpanderRow.
- Test: `RadianceMusic.Windows.Tests/PureTests.cs`

**Interfaces:**
- Consumes: Task 10's `MixView`, `set_part`, `set_voice`, `set_effects`, `reset_mix` and `part_levels`.
- Produces:
  - **The voice buttons:** a SelectorBar "Song | No vocals | Vocals only".
  - **The part rows:** one row per `MixView.parts` entry, with its title, a Slider from 0 to 100 % and a level meter (a thin ProgressBar fed by `part_levels` every 250 ms, and only while the Expander is open and the page is shown).
  - **The effects:** Low cut, High cut and Reverb Sliders. Their ranges are `effects::LOW_CUT` (20 to 500 Hz), `HIGH_CUT` (1500 to 20000 Hz) and 0 to 100 %. Changes go through the 300 ms `Debounce` before `set_effects`.
  - **Reset**, with a leading icon.
  - `EffectText.Hz(double) -> string`, which gives "500 Hz" and "1.5 kHz", and `EffectText.Percent(double)`. Both copy GTK's slider labels.
  - **When there are no parts,** the part rows are hidden and the voice buttons stay. A song not split asks for the fast split (Task 6).

- [ ] **Step 1: Write the failing tests** `EffectTextTests`: 500 gives "500 Hz", 1500 gives "1.5 kHz", 20000 gives "20 kHz", and 0.42 gives "42 %".
- [ ] **Step 2: Run them.** Run `test-cs.ps1` on ribs03. Expected: they fail to compile.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Build and check on screen.**
  - Run `-Step app`.
  - Then a screenshot run on a test song with parts in `C:\rb\beta-test` (made by Task 13's split, or copied WAV parts): `--page playing --act mixer-open --act part:vocals:0.3 --wait 4`. A new `DevActs` step `mixer-open` expands the Mixer, and `part:NAME:GAIN` sets a slider.
  - Expected: the four sliders, Vocals at 30 %, and the meters moving between two `--again` shots.
- [ ] **Step 5: Commit:** "Beta: the Mixer on Now Playing (Refs #45)".

### Task 12: C# — the Splits tab, the rows' split marks, the menu's split items and "Splitting…"

**Files:**
- Modify:
  - `Controls/QueuePanel.cs`: a third SelectorBar item, "Splits", showing `SplitsView` as GTK's `render_split_queue` (6743) does: the song being split with a ProgressBar and its phase, then the waiting songs, and a Pause/Resume button;
  - `Controls/SongRow.cs`: the split mark at the row's end, a ProgressRing while splitting, and a "waiting" glyph with the Task 1 tooltip words;
  - `Controls/SongMenu.cs`: the split items, and `ShowLyrics`/`EditLyrics`/`FindLyricsAgain`, which Task 14 opens;
  - `MainWindow.xaml(.cs)`: a "Splitting…" entry at the foot of the NavigationView (a settings-style item with a leading icon), opening a ContentDialog with "Cores it may use" (a NumberBox, 1 to max, with the GTK dialog's measured timings) and "Pause Splitting".
- Test: on screen.

**Interfaces:**
- Consumes: Task 10's `splits`, `split`, `set_split_paused`, `split_cores`, `set_split_cores` and `SongView.split`.

- [ ] **Step 1: Build.** Run `-Step app`. Expected: 0 errors.
- [ ] **Step 2: Check on screen,** one window per state:
  - a song's menu with the split items: `--act menu`, then `click:Split: Music + Vocals (Fast)`;
  - the Splits tab with one song splitting and two waiting (`--side splits`);
  - the "Splitting…" dialog.

  Expected: GTK's words, and nothing clipped at 1280 or 900 px.
- [ ] **Step 3: Commit:** "Beta: the Splits tab, split marks, the menu's split items and Splitting… (Refs #45)".

### Task 13: C# — getting splitting ready on the sunrise

**Files:**
- Modify:
  - `Controls/Splash.cs`: `Hold()`, `Release()`, `Progress(string what, double done)` and `Offer(UIElement buttons)`, as GTK's splash.rs `hold`/`release`/`progress`/`offer`. The text sits under the sun, with a 4 px rounded progress track at 36 % of the shorter side;
  - `Pure/SplashTimeline.cs`: `Holds` (the sun keeps playing while held);
  - `App.xaml.cs`: on `ChangeKind.Setup`, hold the sunrise while `Getting`; for `Offline` or `Failed`, offer "Try Again" and "Open Radiance Music Now", the second with its subtitle words.
- Test: `PureTests.cs`

**Interfaces:**
- Consumes: Task 10's `setup`, `retry_setup` and `ChangeKind.Setup`.

- [ ] **Step 1: Write the failing test** `AHeldSunNeverSettles`. While held, `SettleFrom` stays null past `LongestS`. Once released and ready, it settles within the next frame.
- [ ] **Step 2: Run it.** Expected: it fails.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Check on screen.** Run with `RADIANCE_SETUP_TEST=1` (the Task 5 hook) and the test data's network blocked, and `--splash-at 2,6`. Expected: "You're offline…" and the two buttons under the sun, and the app opens on "Open Radiance Music Now" (`click:Open`).
- [ ] **Step 5: Commit:** "Beta: splitting gets ready on the sunrise at the first start (Refs #45)".

### Task 14: C# — the lyrics editor, Show Lyrics and Find Lyrics Again

**Files:**
- Create:
  - `Dialogs/LyricsEditor.xaml(.cs)`: a ContentDialog, as GTK's `lyrics_editor` (6358). Each row has a start time (click it while playing to set "now"), the line's text, and its singer (a ComboBox of `singers` plus added names), and can be removed. The dialog adds "Add Line", "Paste Lyrics" (one line per clipboard line, in place of the rows), "Add a singer", and "Tap Timing" (a ToggleButton: while on, Enter stamps the next line with the playing time). Saving calls `save_lyrics`.
  - `Dialogs/ShowLyrics.cs`, a ContentDialog with a selectable TextBlock;
  - `Pure/TapTiming.cs`.
- Modify: `Controls/SongMenu.cs`, which opens them. Find Lyrics Again calls `find_lyrics_again`.
- Test: `PureTests.cs`

**Interfaces:**
- Produces: `TapTiming.Stamp(IList<double?> times, int next, double at) -> int`. It sets `times[next] = at` and returns the next row without a time, or the end.

- [ ] **Step 1: Write the failing tests** in `TapTimingTests`: stamping walks down the rows, skips rows already timed when going on, and stops at the end.
- [ ] **Step 2: Run them.** Expected: they fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Check on screen.** `--act menu --act "click:Edit Lyrics"`, and the same with "Show Lyrics". Expected: the rows, buttons and words as GTK's. Never paste or quote the lyrics anywhere: they stay in the test data.
- [ ] **Step 5: Commit:** "Beta: the lyrics editor, Show Lyrics and Find Lyrics Again (Refs #45)".

### Task 15: C# — the background follows the beat (#56)

**Files:**
- Create: `Pure/BeatFollower.cs`
- Modify:
  - `Controls/Backdrop.cs`: a `CompositionPropertySet` "Beat" with `Swell`, which the patches' and art's scale expressions read. The flow's keyframe animations run under an `AnimationController` whose `PlaybackRate` is `BeatFollower.Speed`. A `CompositionTarget.Rendering` handler samples the pulse at the playing time (Now's `at` counted on from `since`) and feeds `BeatFollower`. It runs only while Now Playing is shown, the window isn't minimised and the song plays; otherwise it's unhooked, as GTK stops its tick behind other apps.
  - `Pages/NowPlayingPage.xaml.cs`, which hands it `pulse()` on `ChangeKind.Pulse`.
- Test: `PureTests.cs`

**Interfaces:**
- Produces: `BeatFollower`, a port of backdrop.rs's tick (240–270) with its numbers:
  - `PUSH = 25.0`, `PUSH_SETTLES_S = 0.12`, `SWELL = 0.06`, `SWELL_FADES = 3.0`, `BASS_SPEEDS = 3.0`;
  - a kick is `hit > lastHit + 0.2`;
  - bass eases with 0.2 s up and 0.5 s down.

  Its members:
  - `Step(double dt, float hit, float bass)`;
  - `double Swell`, which is `SWELL * swell`, so the art never goes past `MAX_ZOOM` 1.15;
  - `double Speed`, which is `1 + BASS_SPEEDS * bass + push`.

- [ ] **Step 1: Write the failing tests** in `BeatFollowerTests`:
  - a kick swells at once and fades to 0 within 1/3 s;
  - a rise of 0.2 or less isn't a kick;
  - strong bass speeds the flow, within 1 % after 1 s at bass 1;
  - the push settles: under 5 % of itself after 0.36 s.
- [ ] **Step 2: Run them.** Expected: they fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Check on screen and measure.**
  - Take two `--again 0.15` shots on a song with a strong beat. Expected: the art's scale differs between them.
  - Run `measure-cpu.ps1 -Page playing` for 60 s. Expected: under stage 2's 1 % + 2 points, with nothing per frame when minimised.
- [ ] **Step 5: Commit:** "Beta: Now Playing's background follows the beat (#56, Refs #45)".

### Task 16: The checkpoint: a real split on ribs03, features list, version, installer and CPU

**Files:**
- Modify:
  - `docs/FEATURES.md`, the Windows beta column. ✅ only for rows checked on ribs03. The Mixer and splitting rows and the four 🔜 3 lyrics rows, plus a new row "Now Playing's background follows the beat" (Linux ✅, Windows ✅, beta ✅, phone 🔜 android#27);
  - `CHANGELOG.md`, a "Windows beta 0.8.0-beta.3" section;
  - the version, `0.8.0-beta.3`;
  - `windows/README.md`, the CPU numbers.

- [ ] **Step 1: A real split on ribs03,** in the test data with the splitter the owner's PC already has (`%LOCALAPPDATA%\radiance-music\windows-splitter`, read-only). A short liked test song gets split in four and in two. Expected:
  - the parts appear;
  - the Mixer plays them;
  - the word times arrive (MMS);
  - the badge goes from waiting to splitting to split.
- [ ] **Step 2: Run the full suites.** On ribs02 `suite.sh`, and on ribs03 `-Step all`. Expected: all pass.
- [ ] **Step 3: Measure CPU** on ribs03 for 60 s each:
  - parts playing with the Mixer open;
  - the beat background;
  - a split running.

  Expected:
  - the app's own share stays under 15 %;
  - the split itself is the splitter's, at its cores setting;
  - the numbers go in the README.
- [ ] **Step 4: Screenshot every pop-up step** (the owner's rule):
  - the menu's split items;
  - the Splits tab;
  - Splitting…;
  - the lyrics editor, with Tap Timing on;
  - Show Lyrics;
  - Find Lyrics Again's toast;
  - the Mixer;
  - the sunrise holding with progress.
- [ ] **Step 5: Build the installer.** Run `-Step installer`. Expected: `RadianceMusic-Beta-Setup.exe` 0.8.0-beta.3. Installing it on the owner's PC waits for a dialog: install with `/D=D:\Apps\Radiance Music (beta)`.
- [ ] **Step 6: Commit:** "Beta 0.8.0-beta.3: stage 3 checked on ribs03 (Refs #45, #56)".
