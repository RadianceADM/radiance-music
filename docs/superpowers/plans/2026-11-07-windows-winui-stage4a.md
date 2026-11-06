# Radiance Music for Windows, native (WinUI 3): stage 4a implementation plan (the app's own parts)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The beta's fourth release, first part of the spec's stage 4. Everything below behaves as the Linux app does:
- **Updates:** the beta finds a newer beta installer, offers it (Update Now, Remind Me Later 24 h, Skip This Version), downloads it while music plays, and "Restart to Update" puts it on in the folder it runs from.
- **Help:** How to Use It (the first-start guide), What's New, Report a Problem (with the app's log, names taken out), Rate Radiance Music (the pop-up at a free moment, Google sign-in).
- **Storage:** where your songs are kept, the space used, moving the folder, Open the Music Folder.
- **Offline:** an Offline sign with its tooltip; back online, a toast, the search runs again, lyrics and the fast split's files come.
- **Search suggestions** as you type (your artists before you type).
- **Autoplay** on/off.
- Five small fixes from the stage 3 review (#61).

**Architecture:**
- What's still only in `src/ui*.rs` for these features moves into `core` (pure) and `engine` (the update step machine and its gating rules, the guide's cards, release notes, the suggestion merge, the storage move sequence, the back-online actions); the GTK app calls the moved code, its words unchanged.
- The engine modules `update`, `report`, `feedback` and `storage` already exist and compile into the DLL; the `Controller` starts calling them, with `Sources` hooks so tests never touch GitHub, the admin site or the keyring.
- `ffi` exposes all of it. The C# app gets a Settings page (Windows' own: the NavigationView's gear) with three sections, plus the update, guide, report and rating dialogs and the offline sign.

**Tech Stack:** As stage 3 (uniffi `=0.31.0`; ribs03 MSYS2 UCRT64 GStreamer 1.28.8; .NET 10; Windows App SDK 2.6.5).

**Spec:** `docs/superpowers/specs/2026-11-05-windows-winui-design.md`, Stages → 4 (issue #45). A factual map of the GTK code for every feature here, with line numbers as of `main` 07f47ad, is in the session's scratch `stage4a-map.md`; re-find a function by its name if lines moved.

## Global Constraints

Stage 3's constraints hold, with these changes:
- **Git:** branch `winui-stage4a` from `main` 07f47ad in `~/Github/radiance-music-winui`; to ribs03 by `git bundle create s4.bundle 07f47ad..winui-stage4a`; `C:\rb\at.ps1 -Want <hash>` checks the commit before any build.
- **ribs03 is the owner's PC:** every test window is hidden (`--shot` or `--behind`: past every screen's edge, no focus); the HOLD file around test windows; long runs through `C:\rb\detach.ps1` with a log; never install, restart or open the owner's own beta without a dialog.
- **The real update path** (download → close → installer → reopen) opens a visible window at the end: it runs on ribs03 only after a dialog (Task 13), into a test install folder `D:\rb-beta-update-test`, never the owner's.
- **Network in tests:** no test reaches GitHub, YouTube Music or admin.radianceph.com: `Sources` hooks (Tasks 2–5) and the existing `RADIANCE_UPDATE_FEED` / `RADIANCE_FEEDBACK_URL` overrides.
- **Words:** every string the beta shows comes from `core::words` or a function a task names; the strings quoted here are the GTK app's, kept exactly, except where a task says the beta's own words.
- **Version:** `0.8.0-beta.4`, set in Task 13.
- **Stage boundaries:** crossfade / DJ mix get their own plan (4a-2); sharing and `radiancemusic://` links are 4b; servers and parties 4c. The Look (Stone / True Black) stays Windows' light/dark (FEATURES' recorded call).

## Review Focus

1. **An update found while the first-start guide or the sunrise is up, or a dialog is open:** the pop-up waits (every 30 s) and never covers them; Esc on Out or Ready counts as Remind Me Later. Test: `the_update_waits_for_a_free_moment` (Task 2).
2. **The beta updating itself from `D:\Apps\Radiance Music (beta)`:** the installer runs with `/D=` that folder (never the default on C:), the old copy is waited for, the new one opens. Test: `the_beta_puts_its_update_where_it_runs` (Task 2), and Task 13's real run.
3. **A move of the music folder that fails half-way:** the app keeps using the old folder, the database isn't changed, and the toast says why; files already moved stay where they went and are found at the next move. Test: `a_failed_move_keeps_the_old_folder` (Task 4).
4. **Report a Problem on Windows:** the home folder written either way (`C:\Users\Name\…` and `file:///C:/Users/Name/…`) and the user name never leave; the beta's own crash log is included, scrubbed. Test: `windows_paths_are_scrubbed_both_ways` (Task 3).
5. **The rating pop-up never interrupts:** only at a song's end or a pause, never first open, never with a dialog up, never in the 30 days after it was asked. Test: `the_rating_waits_for_a_free_moment` (Task 3).

---

### Task 1: core — the words and pure rules moved out of the GTK app

**Files:**
- Create: `core/src/release_notes.rs` (moved from `ui.rs` `release_notes`), `core/src/guide.rs` (moved `CARDS` from `src/ui/guide.rs`), `core/src/suggest.rs` (the chip merge from `ui.rs` `update_suggestions`), `core/src/update_words.rs` (the pop-up's words from `src/ui/update.rs`, and `mb`, `downloading_words`).
- Modify: `core/src/words.rs` (Back online, Offline pill, storage, report, rating, guide and update words as they are in the GTK code), `src/ui.rs`, `src/ui/guide.rs`, `src/ui/update.rs` (call core).

**Interfaces — produces:**
- `release_notes::html(changelog: &str, sections: usize) -> String` (the GTK function unchanged) and `release_notes::sections(changelog, prefix: &str, n) -> Vec<(String, Vec<String>)>` (heading, bullets) for the beta, which shows only `## Windows beta …` sections.
- `guide::CARDS: [(&str icon, &str title, &str text); 5]`, exact.
- `suggest::chips(liked: &[(String title, Vec<String> artists)], typed: &str, youtube: &[String]) -> Vec<Chip>`, `Chip { text: String, mine: bool }`: box empty → top 10 liked artists by count, ties alphabetical; typing → ≤3 liked artists containing it, ≤3 liked titles (`"{title} {first artist}"`), then ≤6 of YouTube Music's not already shown; case-insensitive dedupe; a chip equal to the typed text skipped.
- `update_words::{out_title, ready_text, downloading_words(have, total, flatpak), mb}` and the button words, exact.

- [ ] **Step 1: Tests** (core): `release_notes_match_the_gtk_html` (the existing ui.rs test moved), `beta_sections_only`, `the_guide_cards_are_the_linux_apps`, `suggestions_before_typing_are_your_artists`, `suggestions_merge_mine_then_youtube` (dedupe, limits 3/3/6), `update_words_say_sizes_as_the_linux_app` (`mb(1_500_000_000) == "1.5 GB"`, `"{have} of {total}"`).
- [ ] **Step 2: Run** on ribs02: they fail. **Step 3: Implement**, the GTK app calling core (its own tests green). **Step 4:** suite green. **Step 5: Commit:** "core: release notes, the guide, suggestions and the update's words, moved out of the GTK app (Refs #45)".

### Task 2: engine — the beta's updates

**Files:** `engine/src/update.rs`, `engine/src/controller.rs`, `engine/src/sources.rs`, `engine/tests/controller.rs`.

**Interfaces:**
- `update::Install::Beta { root: PathBuf }` from `Install::here()` when the running exe is `RadianceMusicBeta.exe` with `Uninstall.exe` beside it; `asset()` = `"RadianceMusic-Beta-Setup.exe"`; uninstall key `RadianceMusicBeta`; reopens `{root}\RadianceMusicBeta.exe`.
- **The beta's version in a release:** the asset's GitHub `label`, `"Radiance Music (beta) {version}"` (set at release time: `gh release upload vX 'RadianceMusic-Beta-Setup.exe#Radiance Music (beta) 0.8.0-beta.4'`, written into `windows/README.md`'s release steps); no label → no offer. `release_from` reads it for `Install::Beta`.
- `version_of(file, asset)`: strips `-{asset}` from the end (a hyphenated version broke the old split at the first `-`); `clean()` uses it.
- `windows_script(pid, root, file, version, Install)`: the beta's key, exe and `/D={root}` (root = the running copy's folder, so `D:\Apps\Radiance Music (beta)` stays there).
- `Sources::latest_release(install) -> Result<Release, String>` and `Sources::download(release, install, progress, stop) -> Result<PathBuf, String>` (defaults: `update::latest`, `update::download`); `put_on` stays outside tests.
- Controller: `UpdateState { step: UpdateStep, release: Option<Release>, asked: bool }`, `UpdateStep::{None, Out, Downloading(f64), Ready, Restarting, Failed(String)}`; `look_for_updates(asked)`, `update_now()`, `update_restart()`, `update_later()`, `update_skip()`, `update_cancel()`; `Change::Update`. At start: 8 s, then a check (old downloads cleaned first); offline → the remembered release. The *gating* (guide, sunrise, dialogs, window seen) is the C# app's (Task 8): the engine only says the step and whether it asks (`update::asks`).
- `Controller::beta_version()` is what's compared (`ffi::beta_version`'s), not the engine's 0.7.x.

- [ ] **Step 1: Tests:** `version_of_reads_hyphenated_versions`, `the_beta_install_is_found_beside_its_uninstaller`, `the_beta_puts_its_update_where_it_runs` (`windows_script` holds `/D=D:\Apps\Radiance Music (beta)` last and unquoted, the `RadianceMusicBeta` key, the beta exe), `a_release_without_the_beta_label_offers_nothing`, `the_update_waits_for_a_free_moment` (Fake release newer → step Out and `asks`; `update_later` → not asked for 24 h; `update_skip` → that version never), `update_now_downloads_then_ready` (Fake download progress → Downloading → Ready), `update_cancel_goes_back_to_out`.
- [ ] **Step 2–4:** fail, implement, suite green. **Step 5: Commit:** "engine: the beta finds and downloads its own updates (Refs #45)".

### Task 3: engine — Report a Problem and the rating pop-up for the beta

**Files:** `engine/src/report.rs`, `engine/src/feedback.rs`, `engine/src/controller.rs`, `engine/src/sources.rs`, tests.

**Interfaces:**
- `report::scrub(text, home, user)`: also replaces the home folder written with forward slashes (`C:/Users/Name`) and URL-encoded; `report::gather_with(version: &str, extra_log: Option<PathBuf>)`: the beta passes `0.8.0-beta.N` and its `beta-crash.log` (scrubbed, its newest lines, under `--- the beta's own log ---`), within the same 200,000 bytes.
- `Sources::send_report(report, note) -> Result<String, Failure>` and `Sources::send_feedback(token, answer) -> Result<bool, Failure>` (defaults: the real ones).
- Controller: `report_gather()` (worker) → `ReportView { version, platform, system, install, log }` via `Change::Report`; `report_send(note)` → `ReportResult::{Sent(code), Failed(words)}`.
- The free moments: on a song's end and a pause, if `feedback::time_to_ask()`, the controller sets `rating_due = true` and tells `Change::Rating` (it calls `asked_now()` when the C# app says it showed it). `feedback_state()`: `{ signed_in: Option<(name, email)>, busy: Option<words> }`; `rate(answer)`: signed in → send; else `sign_in(open: callback)` on a worker: `loopback()`, `sign_in_ready()`, then the C# app opens the URL (the callback), `wait()`, `exchange()`, `keep()`, then send; `cancel_sign_in()`, `sign_out()`, `rating_never()`, `rating_later()`. Platform `"windows"`, app_version the beta's.

- [ ] **Step 1: Tests:** `windows_paths_are_scrubbed_both_ways`, `the_beta_report_carries_its_version_and_its_log`, `a_report_is_sent_with_its_note` (Fake), `the_rating_waits_for_a_free_moment` (`time_to_ask` forced via a `Sources::rating_due` hook; due only after an Ended or a pause, never twice), `a_rating_without_sign_in_asks_for_one_then_sends` (Fake sign-in hook returns a token).
- [ ] **Step 2–4.** **Step 5: Commit:** "engine: Report a Problem and the rating pop-up for the beta (Refs #45)".

### Task 4: engine — storage, Autoplay and the guide's flag

**Files:** `engine/src/storage.rs`, `engine/src/controller.rs`, `src/ui.rs` (calls the moved sequence), tests.

**Interfaces:**
- `storage::move_and_switch(from, to, set_dir: impl Fn(Option<&Path>), relocate: impl Fn(&Path, &Path)) -> Result<(usize moved, usize kept), String>`: the GTK sequence (move → set the folder → relocate the database → move again for files written meanwhile); on failure the folder goes back to `from` and the database is untouched. The `"Radiance Music"` suffix rule as `storage::target(chosen) -> PathBuf`. `move_words(moved, kept, to)` / `failed_words(e)` in core words (the GTK toasts).
- `storage::usage` counts `windows-splitter` and `two-part` under "This computer's splitter" (they counted as App data).
- Controller: `storage()` (worker) → `StorageView { folder, total, rows: Vec<(title, count words, size)>, app_data, splitter, free }` via `Change::Storage`; `move_music(to)` (worker) → toast; `set_autoplay(on)` (the GTK toasts); `guide_seen()` / `set_guide_seen()`.

- [ ] **Step 1: Tests:** `a_move_switches_the_folder_and_the_database`, `a_failed_move_keeps_the_old_folder` (a file that can't be copied), `files_written_during_a_move_follow_it`, `the_windows_splitter_counts_as_the_splitter`, `autoplay_is_kept`, `the_guide_is_seen_once`.
- [ ] **Step 2–4.** **Step 5: Commit:** "engine: storage, Autoplay and the guide for the beta; the folder move moved out of the GTK app (Refs #45)".

### Task 5: engine — offline and search suggestions

**Files:** `engine/src/controller.rs`, `engine/src/sources.rs`, `engine/src/two_part.rs`, tests.

**Interfaces:**
- At start: `online = NetworkMonitor::is_network_available()` (it started at true).
- Back online (a real change): the toast `"Back online"`; the search runs again if there is one; lyrics as now; the setup as now; the fast split's files fetched if missing (`two_part::fetch`, as GTK's `get_two_part_ready`, also once at start).
- `Sources::suggestions(q) -> Vec<String>` (default `youtube::suggestions`); `Controller::suggest(typed)` (worker for YouTube's part; a newer call wins) → `View.suggestions: Vec<Chip>` via `Change::Suggestions`, from `core::suggest::chips`.

- [ ] **Step 1: Tests:** `back_online_says_so_and_searches_again`, `the_start_reads_the_network`, `suggestions_follow_the_typing` (Fake suggestions; an older answer arriving late is dropped).
- [ ] **Step 2–4.** **Step 5: Commit:** "engine: back online and search suggestions for the beta (Refs #45)".

### Task 6: ffi — the 4a surface

**Files:** `ffi/src/lib.rs`, its tests.

**Produces** (records mirror the controller's; enums as uniffi Enums): `ChangeKind::{Update, Report, Rating, Storage, Suggestions}`; `UpdateView`, the update actions; `ReportView`, `report_gather`, `report_send`; `FeedbackView`, `rate(AnswerView)`, `rating_shown()`, `rating_later()`, `rating_never()`, `sign_in(OpenUrl callback)`, `cancel_sign_in()`, `sign_out()`; `StorageView`, `storage()`, `move_music(to)`, `storage_target(chosen)`; `set_autoplay`, `autoplay`, `guide_seen`, `set_guide_seen`; `suggest(typed)`, `suggestions() -> Vec<ChipView>`; `guide_cards()`, `whats_new() -> Vec<NotesSection>` (the beta's CHANGELOG sections, embedded at build time); `beta_words()` grows the 4a words.

- [ ] Tests for each conversion as stage 3's; suite green; **Commit:** "ffi: the stage 4a surface (Refs #45)".

### Task 7: C# — the Settings page

**Files:** `MainWindow.xaml` (`IsSettingsVisible="True"`), Create `Pages/SettingsPage.xaml(.cs)`.

- One page, Windows' own (`SelectorBar`: **Playing** · **Storage** · **Help**; one section at a time, the owner's rule for long pages), in `PageKit.Column`.
- **Playing:** Autoplay When the Queue Ends (a `ToggleSwitch`, the GTK words and toasts).
- **Storage:** Task 11. **Help:** How to Use It, What's New, Look for Updates, Report a Problem…, Rate Radiance Music, and About (version `0.8.0-beta.N`, the Linux app's About words and links: GitHub, Issues, credits).
- Every button carries a leading icon; soft corners.
- [ ] Check hidden on ribs03 at 1400 px and 1000 px; commit "Beta: the Settings page (Refs #45)".

### Task 8: C# — the update pop-up

**Files:** Create `Dialogs/UpdateDialog.cs`; `App.xaml.cs` (gating), `Pure/UpdateGate.cs` (pure: may it show now?).

- `UpdateGate.MayShow(windowSeen, sunriseUp, guideUp, dialogOpen, step, asks)`; retried every 30 s while not; Esc / close on Out or Ready = Remind Me Later.
- The steps as the GTK dialog: Out (title, the first 4 bullets, "Update Now ({size})", "Music goes on playing while it downloads.", Remind Me Later, Skip This Version), Downloading (progress, Hide, Cancel the Download), Ready (Restart to Update), Restarting, Failed (Try Again, Get It from GitHub). Words from core; icons on every button.
- Settings → Help → Look for Updates: asked (`look_for_updates(true)`; "You have the newest version ({v})" toast).
- [ ] **Test:** `UpdateGateTests` (C#): never over the sunrise, the guide or a dialog; waits while hidden. Shots (hidden, `RADIANCE_UPDATE_FEED` pointing at a local file feed with the beta label): Out, Downloading, Ready, Failed. **Commit:** "Beta: the update pop-up (Refs #45)".

### Task 9: C# — How to Use It and What's New

**Files:** Create `Dialogs/GuideDialog.cs`, `Dialogs/WhatsNewDialog.cs`; `App.xaml.cs`.

- The guide: the five cards (a `FlipView` + `PipsPager`), Skip / Next / Start Listening (play icon); first start after the sunrise has gone (and the setup hold), unless `guide_seen`; closing it any way sets `guide_seen`. Card 5 ("On Your Phone Too") says "(main menu)" in the GTK words: the beta shows cards 1–4 until sharing comes (4b) — a ruling to record.
- What's New: the beta's CHANGELOG sections (`whats_new()`), headings and bullets, scrollable, soft corners.
- [ ] Shots (hidden): guide card 1 and the last; What's New. **Commit:** "Beta: How to Use It and What's New (Refs #45)".

### Task 10: C# — Report a Problem and Rate Radiance Music

**Files:** Create `Dialogs/ReportDialog.cs`, `Dialogs/RatingDialog.cs`; `App.xaml.cs` (the rating's free moment: on `ChangeKind.Rating`, shown through the same gate as Task 8, then `rating_shown()`).

- Report: the GTK dialog's words; "What will be sent" (an Expander with the exact text); Send disabled until the log is read; after sending, the code, Copy the Code, Close.
- Rating: Yes! / Not really, five star rows (`RatingControl`), the comment, Send / Cancel Sign-In, the who line with its Privacy link, Sign Out, Not Now, Don't Ask Again; the browser opened by `Launcher.LaunchUriAsync` from the sign-in callback; the thanks page with Share Radiance Music (copies the link, the GTK toast).
- [ ] Shots (hidden, `RADIANCE_FEEDBACK_URL` at a local fake): the report before and after sending; the rating form, filled, the problem line, thanks. **Commit:** "Beta: Report a Problem and Rate Radiance Music (Refs #45)".

### Task 11: C# — Storage

**Files:** `Pages/SettingsPage.xaml.cs` (Storage section).

- "Where your songs are kept": the folder (selectable), Change… (`FolderPicker`, initialised with the window's handle), Open the Music Folder; "Space used" rows as GTK's (Counting… first).
- Change… → `storage_target` → a `ContentDialog` "Move your songs?" with the GTK text, Cancel / Move → "Moving your songs…", then the result toast; the section redraws.
- [ ] Shots (hidden): the section; the confirm dialog (a test folder). A real move in the test data from `C:\rb\beta-test\music` to `D:\rb-beta-move-test` and back (Task 13). **Commit:** "Beta: Storage (Refs #45)".

### Task 12: C# — the Offline sign, back online and search suggestions

**Files:** `MainWindow.xaml(.cs)` (the pill in the title bar's end: `network-offline` glyph `\uF384`, "Offline", the GTK tooltip), `Pages/SearchPage.xaml.cs`.

- Suggestions: the `AutoSuggestBox`'s own list (Windows' way, not GTK's chips): before typing (the box focused) your artists; typing (the 300 ms debounce already there) the merged list, yours with a heart glyph and "From your liked songs"; choosing one searches it.
- [ ] Shots (hidden): offline (a `--offline` dev flag forcing it), the suggestions list. **Commit:** "Beta: the Offline sign and search suggestions (Refs #45)".

### Task 13: Five small fixes from stage 3's review, and the checkpoint

**Fixes (#61):** M1 (GTK's chip knows two parts), M5 (Tap Timing on `LyricClock.At`), M7 (developer runs set `RADIANCE_AUTO_SPLIT_S` off and never run the first-start download), M12 (the no-op assertion), M3's "Splitting paused…" / "Splitting resumed" toasts. Each with its test first.

**Checkpoint:**
- [ ] Suites: ribs02 `suite.sh`; ribs03 builds and the C# tests.
- [ ] Every new pop-up and the Settings page's sections screenshotted hidden (the owner's rule), at 1400 and 1000 px.
- [ ] CPU: Settings open and the app idle, hidden (`measure-cpu.ps1 -Page settings`): no more than Home's.
- [ ] **The real update path, after a dialog:** a test install of 0.8.0-beta.3 in `D:\rb-beta-update-test`, a local feed offering this build's installer with the beta label, Update Now → Ready → Restart to Update → the installer puts it on there (`/D=`), the new copy opens (visible: hence the dialog), its version checked, then closed and the test install removed.
- [ ] FEATURES (beta ✅ only for rows checked on ribs03), CHANGELOG "Windows beta 0.8.0-beta.4", the version `0.8.0-beta.4`, `windows/README.md` (the release step's asset label, the new switches), the installer built. Installing on the owner's PC waits for a dialog.
- [ ] **Commit:** "Beta 0.8.0-beta.4: stage 4a checked on ribs03 (Refs #45)".
