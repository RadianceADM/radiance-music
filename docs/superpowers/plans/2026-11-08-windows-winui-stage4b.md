# Windows beta stage 4b: sharing — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** the beta shares as the Linux app does: Copy Link on a song, Share Songs with a Phone (QR, both ways, only what's missing), and the guide's fifth card.

**Architecture:** the phone protocol is already shared (`engine/src/share.rs`: `Share::send/receive`, `ticket`, `Store`, the HTTP server). This stage moves the rest of the Linux app's sharing out of `src/` into core/engine (a song's link, the QR's modules, adding received songs to Liked, the firewall rule) so both apps use one copy, and gives the beta its dialog.

**Tech Stack:** Rust (core, engine, ffi via uniffi 0.31), C# WinUI 3, the `qrcode` crate (moves from the GTK crate to the engine).

**Spec:** docs/superpowers/specs/2026-11-05-windows-winui-design.md (stage 4 lines 103-104; links 128-129). Map of the GTK code: session notes 2026-11-08 (stage 4b map).

## Rulings (made while planning)

- `radiancemusic://` links move to stage 4c with servers: the only link the computer opens is a server invite (`radiancemusic://join?u=…`, opening the Servers dialog), and the spec (128-129) keeps links with today's app until the beta has servers. Cost if wrong: links a stage later.
- The beta's firewall rule has its own name, "Radiance Music (beta) (songs to and from a phone)", tied to its own exe, and is checked by name each time the dialog opens; the shared `share_firewall_open` flag stays the GTK app's (it would tell the beta it's open when only GTK's exe is let in). Cost if wrong: one more Windows prompt.
- The share key comes from `getrandom` on every platform (Windows had the clock: guessable).
- Gentle transfer while Bluetooth plays stays Linux-only (FEATURES records it ❌ on Windows already; separate work).
- Guide card 5's words for the beta: "Share Songs with a Phone (Settings → Help)" where the Linux app says "(main menu)": `core::guide` gets a `where_share` parameter. Cost if wrong: one string.

## Global Constraints

- Words are the Linux app's (`src/ui.rs` 4109, 4136-4139; `src/ui/sharing.rs`), moved into core: "Copy Link", "Link copied", "Share Songs with a Phone", "To the Phone", "From the Phone", "With music videos" / "Bigger: each about 30 to 60 MB", the two captions, "Done: N song(s) here", "{songs} of {of} songs · {mb} of {mb}", the no-network and couldn't-get-ready lines, the firewall title/subtitle and "Let It In", "The phone can reach this computer now".
- The same protocol as the phone (Wire.kt): no change to `share.rs`'s wire format, ports 47800-47809, `radiancemusic://share?…` QR text.
- Every button carries an icon; the animation blueprint for the dialog's steps; no sharp visuals (the QR's dots round, as GTK's `draw_qr`).
- Builds and tests on ribs02 (Rust) and the ribs02 Windows VM (C#); ribs03 only after a dialog.

## Review Focus

1. The dialog closed mid-transfer: the share stops (its listener and threads end), nothing half-written stays (`.part` files removed or finished).
2. A song received that's already liked (same key or same title and artist): its file is updated, never a second liked row.
3. No network: the QR area says how to connect, never a QR that can't work.
4. The firewall row on a computer where only the GTK app's rule exists: the beta still offers Let It In.
5. Copy Link on a song from a server the beta hasn't joined (no servers yet): the server's address from the song's own address, never a key.

---

### Task 1: Words, the link and the QR in core/engine

**Files:** core/src/words.rs (or core/src/share_words.rs), core/src/track.rs, core/src/guide.rs, engine/src/share.rs, engine/Cargo.toml, src/ui.rs, src/ui/sharing.rs.

**Produces:** `core::track::Track::link(&self) -> String` (GTK's `link()`, `src/ui.rs:281-294`); `core::words` share words (above); `engine::share::qr_modules(text: &str) -> Option<Vec<Vec<bool>>>` (EC level M, as `draw_qr`); `share::new_key()` from `getrandom`; `core::guide::cards(where_share: &str)`.

- [ ] Tests: `a_songs_link_is_its_page_on_the_web` (YouTube, Archive item, server address without a key); `the_qr_reads_back` (modules square, the version's size); `share_keys_are_random` (two keys differ, 32 hex); the guide's card 5 with each app's words.
- [ ] GTK uses them (its `link()`, `draw_qr`'s matrix and words come from core/engine).

### Task 2: Received songs and the firewall in the engine

**Files:** engine/src/controller.rs, engine/src/share.rs (or engine/src/firewall.rs), engine/tests/controller.rs, src/ui/sharing.rs.

**Produces:** `Controller::share_start(to_phone: bool, videos: bool)`, `share_stop()`, a `ShareState { qr_text, caption, progress, done, note, gentle, firewall: Option<FirewallRow> }` published as `Change::Share`; received songs added to Liked as GTK's `received_from_phone` (update the matching liked song's file, else insert first; store; liked_changed); `firewall::{is_open(rule) -> bool, let_in(rule, exe) -> Result}` with each app's rule name.

- [ ] Tests (engine, in-process HTTP against the real `Share`): `a_phone_puts_a_song_and_it_is_liked` (POST /plan → want, PUT /file sizes, POST /done → in Liked with its file); `a_song_already_liked_gets_its_file_not_a_second_row`; `closing_the_share_stops_it` (the port closes); `the_plan_asks_only_for_whats_missing`.

### Task 3: ffi + the beta's dialog

**Files:** ffi/src/lib.rs; windows/RadianceMusic.Windows/Dialogs/ShareDialog.cs; Controls/SongMenu.cs; Pages/SettingsPage.xaml.cs (Help: Share Songs with a Phone…); Dialogs/GuideDialog.cs (five cards); DevActs.cs.

- [ ] Song menu: Copy Link (all four places the menu opens) → clipboard + "Link copied".
- [ ] ShareDialog: the To/From SelectorBar, With music videos, the QR drawn from `qr_modules` with round dots on white, the caption, a ProgressBar, the note; the firewall InfoBar with Let It In (shield icon); closing stops the share; the animation blueprint (crossfade between the QR and done).
- [ ] Guide: all five cards with the beta's card 5 words.
- [ ] DevActs `share:to|from`, `copy-link`; screenshots in the VM; a real transfer check from the VM to a test client (curl against /list and /file with the QR's key).

### Task 4: Docs, the full check, the build

- [ ] FEATURES: Copy or share a song's link ✅ beta; Share songs with a phone ✅ beta; radiancemusic:// links stay 🔜 (4c).
- [ ] CHANGELOG beta 0.8.0-beta.8; the full Rust suite on ribs02 (a quiet window); the VM build + C# tests.
