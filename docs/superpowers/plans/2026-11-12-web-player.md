# Web Player (on the server's page) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** People signed in to a Radiance Music Server's page can play its songs in the browser: a player bar and
queue, Now Playing with word lyrics, No vocals / Vocals only, Mixer sliders, and Save to this browser.

**Architecture:**
- **The apps' Rust core,** plus a new `web/` crate, runs as WASM in two places:
  - a Worker, which decodes the song's parts as it reads them from the server or from the browser's saved copy;
  - an AudioWorklet, which mixes them with the slider gains and keeps the loudness, as the apps do.
- **The server (rms/)** gains only the built files, a device-code sign-in for iPhone Home Screen apps, and the
  player's markup in its page.
- **A split song** always plays from its parts. A song not split yet plays from its file in an `<audio>` element.

**Tech Stack:** Rust (core, web, rms/axum), wasm-bindgen, Symphonia (ogg, vorbis, flac, isomp4, aac), plain
JavaScript (no bundler), Web Audio AudioWorklet, Web Workers, OPFS, Service Worker, Media Session, Playwright
(checks on ribs02).

**Spec:** docs/superpowers/specs/2026-11-12-web-player-design.md (branch `web-player`). Read it first; this plan
argues from it.

## Global Constraints

**Builds and tests**
- Every build and test runs on ribs02: `~/.claude/skills/ribs/scripts/ribs-run.sh webplayer -- '<cmd>'` from this
  worktree, with `CARGO_TARGET_DIR=$PWD/target CARGO_BUILD_JOBS=4`. Never build on the laptop.
- `web/` is a workspace member and never depends on `engine/`. rms/ keeps no Rust source of the player, only the
  built files in `rms/web/player/`.

**Values from the spec and the apps (not to change)**
- Lyric leads: word 0.08 s, line 0.35 s (`core::sing::WORD_LEAD`, `LINE_LEAD`).
- Loudness: make-up gain clamped 1..2 (`MOST` = 2.0), ceiling 0.97 (`CEILING`), look-ahead 8 ms, release 0.25 s.
  Each part's loudness is averaged over 8 s (`LOUDNESS_S`).
- Parts that make the mix: six if there, else four (drums, bass, other, vocals), else music + vocals.
- A voice pick writes its preset into every slider (`SIX` + "music"). The sliders carry on to the next songs until
  Reset.
- A slider move or a voice pick is heard within about 50 ms.
- A split song plays from its parts from start to end; the song file and its parts never play at once.
- Parts are FLAC, Ogg Vorbis (server and computer splits), or AAC-LC in .m4a at 44.1 kHz (phone splits). Songs are
  .opus (48 kHz), .flac or .m4a.

**Sign-in and server**
- Sign-in: `localStorage['rms.token']`; `Authorization: Bearer` for /api, `?t=` for /files and WebSockets
  (rms/web/app.js).
- Home Screen sign-in: a one-time code, good for 10 minutes (`DEVICE_CODE_MS` = 600 000). Only its SHA-256 is
  stored. It gives a new device of the same person.

**Page behaviour**
- Save to this browser downloads one song at a time.
- Over plain http, the Mixer, voice buttons and Save show "Needs a secure (https) address" and point to the page's
  Add a domain banner. The song still plays as it is.
- A removed device or person: the saved songs and queue are deleted, and the page shows "You're no longer on
  SERVER".

**Words the page shows (exact)**
- Saving: "Save to this browser", "Saved in this browser", "Remove from this browser", "Newer on the server" with
  Update.
- Splitting: "Not split yet", "Split on this server".
- Home Screen sign-in: "Use on your Home Screen", "Enter your code".
- The voice buttons: "Song", "No vocals", "Vocals only". In the Mixer: "Keeps loudness" and "Reset".

**Looks**
- Every button has a leading icon. Nothing is sharp: rounded, as rms/web/app.css (accent `#F38A35`, cards 22 px,
  pills 999 px).
- Every panel covers empty, loading, error, offline, slow, no results, permission and expired.

**What stays and who checks**
- What the page has today stays: library, search, upload, people, invites, split queue, devices, party card and
  sign out. Only the row's preview Play is replaced.
- Device checks follow the owner's rules:
  - iPhone (ip12, iOS 27.0.1): ask him in a dialog to leave it unlocked;
  - tablet: standing OK;
  - before "done": screenshots at laptop, tablet and phone sizes, then owner-ui-gate.

## Review Focus

1. **Parts of slightly different lengths** (the split writes each part on its own): the mix runs to the longest
   part, the shorter ones end in silence, with no stall and no crash. Pinned in Task 5,
   `parts_of_different_lengths_end_in_silence`.
2. **The network drops mid-song** while parts stream: the player pauses with "Waiting for the server…" and goes on
   when it answers, never with garbled audio. Pinned in Task 4, `a_read_that_fails_is_retried_not_garbled`, and in
   the Task 13 e2e (`offline_mid_song`).
3. **Seeking to the very end, or past it**, moves to the next song, with no hang. Pinned in Task 4,
   `seeking_past_the_end_ends_the_part`.
4. **A slider dragged fast or a voice flipped repeatedly** makes no click: the gain ramps over 10 ms. Pinned in
   Task 5, `a_gain_change_ramps_without_a_jump`.
5. **The token revoked while playing** (the owner removes the device): the next request gets 401, the saved songs
   are wiped and the page shows "You're no longer on SERVER", with no endless retries. Pinned in the Task 12 e2e
   (`removed_device_wipes_saved_songs`).

---

### Task 1: The iPhone proof (throwaway, decides Task 10's output path)

The spec's first task. It answers two questions on the owner's iPhone before any real code depends on them:
1. Does an AudioWorklet keep playing with the phone locked when its output goes through a
   `MediaStreamAudioDestinationNode` into an `<audio>` element?
2. Does a Home Screen app have its own storage (localStorage and OPFS)?

**Files:**
- Create: `web/proof/index.html`, `web/proof/tone-worklet.js`, `web/proof/manifest.webmanifest` (all throwaway,
  deleted at the end of Task 13)

- [ ] **Step 1: Write the proof page.**
  - An AudioWorklet makes a 440 Hz tone at 0.05 gain. The level is low: no loud test sound.
  - Two buttons, "Play direct" (worklet → destination) and "Play via audio element" (worklet →
    MediaStreamAudioDestinationNode → `<audio>.srcObject`). Each sets Media Session metadata.
  - A "Write marker" button stores a timestamp in localStorage and in an OPFS file. A "Read marker" button shows both.
- [ ] **Step 2: Serve it over https in the tailnet.**
  - Ask the owner in one dialog: (a) leave ip12 unlocked, (b) OK to serve the proof from ribs02 with
    `tailscale serve` for 30 minutes.
  - On his OK: `tailscale serve --bg --https=8443 /srv/build/webplayer-proof`. Then `tailscale serve reset` after.
- [ ] **Step 3: Run it on the iPhone.**
  - For each button: play, lock 60 s, unlock, and note whether the tone held.
  - Write the marker in Safari, Add to Home Screen, open the Home Screen app, then Read marker.

  Expected: a written result for each question.
- [ ] **Step 4: Ledger the ruling.** Append to the run's ledger:
  `Ruling: background path = <direct | audio element | neither: fall back to the song file while locked>;
  Home Screen storage = <shared | separate>`.

  "Separate" keeps the spec's code flow. "Shared" means the Task 9 code is still built (other browsers use it), but
  the Home Screen hint in Task 12 shrinks to "Add to Home Screen".

### Task 2: core::loudness, moved from the engine

**Files:**
- Create: `core/src/loudness.rs`
- Modify: `core/src/lib.rs` (`pub mod loudness;`), `engine/src/limiter.rs` (becomes `pub use radiance_core::loudness::*;`
  plus the engine-only probe code, if any), `engine/src/player.rs:563-612,638-645`
- Test: `core/src/loudness.rs` (`#[cfg(test)]`)

**Interfaces:**
- Produces, in `radiance_core::loudness`:
  - `pub const MOST: f32 = 2.0`, `pub const CEILING: f32 = 0.97`, `pub const LOUDNESS_S: f64 = 8.0`
  - `pub fn makeup(parts: &[(f64, f64)]) -> f32`: (power, gain) per part. Moved unchanged.
  - `pub struct Limiter` with `new(channels: usize, rate: u32)`, `fits(&self, channels, rate) -> bool`,
    `take_lowest(&mut self) -> f32` and `process(&mut self, samples: &mut [f32], target: f32, ceiling: f32)`.
    Moved unchanged.
  - `pub fn average(was: f64, mean: f64, dt: f64, heard: f64) -> f64`: the PartLevel step, all of it so far at
    first, then the last LOUDNESS_S, with `k = (dt / heard).max(1 - exp(-dt / LOUDNESS_S))` (player.rs:603-605).
  - `pub fn keep(makeup: f32, whole: f32) -> (f32, f32)`: (target, ceiling) =
    `(1 + (makeup - 1)(1 - w), CEILING + (1 - CEILING) w)` with `w = whole.clamp(0, 1)` (player.rs:640-641).

- [ ] **Step 1: Move the limiter's tests into core, and add the new ones.**
  - `limiter.rs`'s own tests move with it.
  - New, in core/src/loudness.rs:
    - `average_takes_everything_heard_at_first`: `average(0.0, 1.0, 0.5, 0.5) == 1.0`.
    - `average_then_follows_the_last_eight_seconds`: after 30 s of mean 1.0 then 8 s of mean 0.0 in 0.1 s steps, the
      value is within 0.40 ± 0.05 (e^-1 ≈ 0.37).
    - `keep_fades_to_the_song_files_level`: `keep(1.6, 0.0) == (1.6, 0.97)`, `keep(1.6, 1.0) == (1.0, 1.0)`,
      `keep(1.6, 0.5) == (1.3, 0.985)`.
- [ ] **Step 2: Run them.** `cargo test -p radiance-core loudness` fails: there's no module yet.
- [ ] **Step 3: Move the code.** Move `makeup`, `Limiter` and the constants into core/src/loudness.rs, add
  `average` and `keep`, and make engine/src/player.rs call `average` and `keep` instead of its inline arithmetic.
  core stays free of std::time, threads and fs.
- [ ] **Step 4: Run the tests.** On ribs02: `cargo test -p radiance-core && cargo test --release -p radiance-engine --lib`.

  Expected: everything passes, with the engine's counts unchanged (126 lib tests on main + this branch).
- [ ] **Step 5: Commit.** "core: the loudness keeper moved from the engine, for the web player too (#50)".

### Task 3: core::mix, the voice and slider rules moved from the engine

**Files:**
- Modify: `core/src/mix.rs`, `engine/src/split.rs:170-182` (`mix_parts` calls core),
  `engine/src/controller.rs:2649-2655` (`set_voice` uses `voice_sliders`)
- Test: `core/src/mix.rs`

**Interfaces:**
- Produces, in `radiance_core::mix`:
  - `pub const FOUR: [&str; 4] = ["drums", "bass", "other", "vocals"]`
  - `pub fn mix_parts(has: impl Fn(&str) -> bool) -> Vec<&'static str>`: SIX if all six, else FOUR if all four,
    else `["music","vocals"]` if both, else empty.
  - `pub fn voice_sliders(v: Voice) -> Vec<(&'static str, f64)>`: `preset(v, p)` for each p in SIX, then "music".
  - `pub struct Sliders`: the slider gains that carry on between songs.
    - `new() -> Sliders`
    - `set(&mut self, part: &str, gain: f64)` clamps the gain to 0..1.
    - `set_voice(&mut self, v: Voice)` writes `voice_sliders(v)` into the sliders and remembers `v`.
    - `voice(&self) -> Voice`
    - `reset(&mut self)` sets every slider back to 1 and the voice to Song.
    - `gains_for(&self, parts: &[&str]) -> Vec<f64>` goes through `part_volumes` (guitar and piano follow other when
      not split in six; music has its own slider).

- [ ] **Step 1: Write the tests.**
  - `mix_parts_picks_six_then_four_then_two`: covers all four cases, with closures over a set.
  - `a_voice_pick_carries_on_until_reset`:
    1. `set_voice(NoVocals)`, then `gains_for(&FOUR) == [1,1,1,0]`.
    2. A new song with `["music","vocals"]` gives `[1,0]`.
    3. `reset()` gives `[1,1,1,1]` and `voice() == Song`.
  - `a_slider_is_kept_between_0_and_1`: `set("drums", 1.7)` gives 1.0, and -0.2 gives 0.0.
- [ ] **Step 2: Run them.** They fail: the names are undefined.
- [ ] **Step 3: Implement.** Implement the items above. The engine's `split::mix_parts(t)` becomes
  `mix::mix_parts(|p| ready_part(t, p).is_some())`. `set_voice` loops over `voice_sliders(v)`.
- [ ] **Step 4: Run the suites.** On ribs02: `cargo test -p radiance-core && cargo test --release -p radiance-engine`
  (the controller suite included) `&& cargo test --release`.

  Expected: all pass, with the same counts as before.
- [ ] **Step 5: Commit.** "core: which parts make the mix and the voice presets, shared with the web player (#50)".

### Task 4: web/: the crate and the part decoder

**Files:**
- Create: `web/Cargo.toml` (`crate-type = ["cdylib", "rlib"]`; deps: `radiance-core = { path = "../core" }`,
  `symphonia = { version = "0.5", default-features = false, features = ["ogg", "vorbis", "flac", "isomp4", "aac"] }`,
  `wasm-bindgen`, `js-sys`), `web/src/lib.rs`, `web/src/source.rs`, `web/src/decode.rs`, `web/src/resample.rs`
- Create: `web/tests/fixtures/tone.flac`, `tone.ogg` (Vorbis) and `tone.m4a` (AAC-LC). Each is 2 s of a 0.5-amplitude
  440 Hz stereo sine at 44.1 kHz, made on ribs02 with ffmpeg; plus `tone48.flac` at 48 kHz.
- Modify: root `Cargo.toml` members, adding `"web"` (not in default-members)

**Interfaces:**
- Produces:
  - `pub trait ByteSource { fn len(&self) -> u64; fn read_at(&mut self, at: u64, into: &mut [u8]) -> Result<usize, ReadError>; }`
    - `ReadError::Retry` means the network is away for now: the caller tries again later.
    - `ReadError::Gone` means 401 or 404.
  - `pub struct SourceReader<S: ByteSource>`: `std::io::Read + Seek + symphonia MediaSource` over it.
  - `pub struct PartDecoder`:
    - `open(src: Box<dyn ByteSource>, hint: &str /*"flac"|"ogg"|"m4a"*/) -> Result<PartDecoder, String>`
    - `fill(&mut self, out: &mut Vec<f32>, frames: usize) -> Fill`, where
      `enum Fill { Got(usize), End, Wait }`. It appends interleaved stereo f32 at 44 100 Hz, resampled if needed.
    - `seek(&mut self, secs: f64) -> Result<(), String>`
    - `duration(&self) -> Option<f64>`

- [ ] **Step 1: Write the native tests** (`web/src/decode.rs`). Each of these decodes a fixture through an in-memory
  `ByteSource`:
  - `decodes_flac_vorbis_and_aac`: each fixture gives 88 200 ± 2 048 frames, with a peak of 0.5 ± 0.03.
  - `a_48_khz_part_comes_out_at_44_1`: `tone48.flac` gives 88 200 ± 2 048 frames, and the zero crossings show 440 ± 5 Hz.
  - `seeking_past_the_end_ends_the_part`: `seek(5.0)` then `fill` gives `End`.
  - `a_read_that_fails_is_retried_not_garbled`: a source that returns `Retry` on its 3rd read gives `Fill::Wait`,
    then, once reads succeed, the same samples as a clean decode (compared bit for bit).
- [ ] **Step 2: Run them.** `cargo test -p radiance-web decode` fails.
- [ ] **Step 3: Implement.**
  - `PartDecoder` uses Symphonia's probe and decoder.
  - `resample.rs` is linear interpolation to 44 100 Hz. That's enough for parts; an .opus song never goes through it.
  - A `Retry` from the source surfaces as `Fill::Wait`, and the next call resumes from the same packet.
- [ ] **Step 4: Run the tests.** `cargo test -p radiance-web` passes.
- [ ] **Step 5: Commit.** "web: decode a song's parts (FLAC, Vorbis, AAC) from any byte source (#50)".

### Task 5: web/: the mixer

**Files:**
- Create: `web/src/mixer.rs`

**Interfaces:**
- Consumes: `radiance_core::loudness::{makeup, average, Limiter, CEILING}`, `radiance_core::mix::Sliders`.
- Produces: `pub struct Mixer`.
  - `new(parts: Vec<String>, rate: u32) -> Mixer`
  - `set_gains(&mut self, gains: &[f64])`: in the order of `parts`. It ramps to the new value over 10 ms.
  - `process(&mut self, inputs: &[&[f32]], out: &mut [f32])`:
    - each input is an interleaved stereo block of the same length; a short input is padded with silence;
    - it measures each part before its gain with `average`;
    - it applies the gains, sums, then runs `Limiter::process(out, makeup(..), CEILING)`.
  - `seeked(&mut self)`: the loudness is heard afresh, as `PartLevel` does on a flush.

- [ ] **Step 1: Write the tests.**
  - `all_at_full_is_the_song`: four sines at 0.2 amplitude (110, 220, 330, 440 Hz) at gains [1,1,1,1] mix to their
    sample-wise sum within 1e-4. The sum stays under 0.97, and `makeup` with every gain at 1 is 1.
  - `no_vocals_leaves_out_the_vocals`: the vocals input at 0.5 with gain 0, after the 10 ms ramp, gives an output
    that correlates under 0.01 with vocals.
  - `the_ceiling_holds`: four inputs at 0.6 at full never give an output sample over 0.97 + 1e-6.
  - `a_gain_change_ramps_without_a_jump`: going from 1 to 0 on a 0.5 DC input, no step between consecutive output
    samples exceeds 0.5 / 441 + 1e-4 (10 ms at 44.1 kHz).
  - `parts_of_different_lengths_end_in_silence`: an input block shorter than `out` is treated as silence past its
    end, with no panic.
- [ ] **Step 2: Run them.** They fail.
- [ ] **Step 3: Implement** `Mixer`.
- [ ] **Step 4: Run the tests.** `cargo test -p radiance-web` passes.
- [ ] **Step 5: Commit.** "web: the mixer: parts summed in step, loudness kept as the apps keep it (#50)".

### Task 6: web/: lyrics and queue for the page

**Files:**
- Create: `web/src/lyrics.rs`, `web/src/queue.rs`

**Interfaces:**
- Produces:
  - `pub fn lyrics_from(text: &str, kind: &str /*"yaml"|"lrc"*/) -> Lyrics`. It uses `core::lyrics::parse_lyricsfile`
    and `parse_lrc`, and the word spreading the apps use, so lines with only line times get words.
  - `pub struct LitLyrics`:
    - `new(lyrics: Lyrics) -> LitLyrics` (uses `core::sing::prepare`)
    - `lines(&self) -> Vec<(String, f64)>` (text, start)
    - `lit(&self, at: f64) -> Vec<(usize, usize)>` (`core::sing::sung`: line index and characters lit)
  - `pub struct PageQueue`: `core::queue::Queue` over `Track`s. Each is made from a library song as
    `Track { source: Source::Server, id: "<server id>/<song id>", title, artist, seconds, .. }`.
    - `set(list_json, index)`, `insert_next(song_json)`, `push(song_json)`
    - `remove(i)`, `move_item(from, to)`
    - `skip(by, at) -> Option<String>` (the id to play), `set_shuffle(on, seed: u64)`, `cycle_repeat() -> String`
    - `to_json() -> String` and `from_json(text)`: the ids and index for localStorage. Never files.

- [ ] **Step 1: Write the tests.**
  - `word_lyrics_light_word_by_word`: a 2-line Lyricsfile with word times gives, at 0.08 s before word 2's start,
    word 2 lit (the 0.08 lead).
  - `line_only_lyrics_light_spread_words`: LRC `[00:01.00]one two` then `[00:03.00]three` gives "one" lit at 1.0 and
    "two" lit before 3.0.
  - `the_queue_survives_its_json`: `set` + `skip(1)` + `to_json` → `from_json` gives the same index and ids.
  - `shuffle_is_seeded`: the same seed gives the same order twice.
- [ ] **Step 2: Run them.** They fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run the tests.** They pass.
- [ ] **Step 5: Commit.** "web: lyrics lit and the queue, from the apps' core (#50)".

### Task 7: web/: the WASM build and its bindings

**Files:**
- Create: `web/src/bindings.rs` (`#[cfg(target_arch = "wasm32")]`), `web/build.sh`
- Create (built output, committed): `rms/web/player/radiance_web.js`, `rms/web/player/radiance_web_bg.wasm`

**Interfaces:**
- Produces, through `wasm_bindgen`, the classes the page's scripts use:
  - `WebDecoder`: constructed with a JS `{ len(): number, read(at: number, into: Uint8Array): number }`, where
    returning -1 means retry and -2 means gone. Methods: `fill(frames): Float32Array | "end" | "wait"`,
    `seek(secs)`, `duration()`.
  - `WebMixer`: `new(parts: string[], rate)`, `setGains(Float64Array)`, `process(inputs: Float32Array[], out: Float32Array)`,
    `seeked()`.
  - `WebLyrics`: `new(text, kind)`, `lines(): Array<[string, number]>`, `lit(at): Array<[number, number]>`.
  - `WebQueue`, mirroring `PageQueue`.
  - `WebSliders`, mirroring `core::mix::Sliders` (with `setVoice("song"|"novocals"|"vocals")`), plus
    `mixParts(has: string[]): string[]`.
- `web/build.sh` (run on ribs02):
  1. `cargo build -p radiance-web --release --target wasm32-unknown-unknown`, then
     `wasm-bindgen --target web --out-dir rms/web/player`, then `wasm-opt -Oz` if present.
  2. Print `gzip -c radiance_web_bg.wasm | wc -c`.

- [ ] **Step 1: Build.** Run `bash web/build.sh` via ribs-run.sh, installing the wasm32 target and a
  `wasm-bindgen-cli` matching the crate's wasm-bindgen version first if missing.

  Expected: the two files exist, and the gzipped size is under 2 097 152 bytes.
- [ ] **Step 2: A smoke test in headless Chromium on ribs02** (Playwright, `web/e2e/smoke.mjs`):
  1. The page loads `radiance_web.js`.
  2. `new WebDecoder` over the FLAC fixture bytes gives 88 200 ± 2 048 frames.
  3. `WebMixer` over 4 inputs returns a block.

  Expected: it prints `ok`.
- [ ] **Step 3: Commit.** Commit the bindings, the build script, the smoke test and the built files:
  "web: the WASM build for the server's page (#50)".

### Task 8: rms: serving the player's files

**Files:**
- Modify: `rms/src/web.rs` (assets), `rms/src/lib.rs` (routes)
- Create: `rms/web/player/worker.js`, `rms/web/player/worklet.js`, `rms/web/player/player.js` (empty stubs here;
  filled in Tasks 10–12), `rms/web/manifest.webmanifest`, `rms/web/sw.js`
- Test: `rms/tests/api.rs`

**Interfaces:**
- **Routes:**
  - `GET /assets/player/{name}` serves the baked-in files: `radiance_web.js`, `worker.js`, `worklet.js` and
    `player.js` as `text/javascript; charset=utf-8`, and `radiance_web_bg.wasm` as `application/wasm`, the latter
    via `include_bytes!`. An unknown name gives 404.
  - `GET /sw.js` serves `text/javascript` with `Cache-Control: no-cache`.
  - `GET /manifest.webmanifest` serves `application/manifest+json`: name "Radiance Music", `start_url` "/",
    display "standalone", the logo as its icon.
- **Cache:** `Cache-Control: no-cache` on all of them, as today's assets.

- [ ] **Step 1: Write the test.** `the_player_files_are_served` covers each route's status and Content-Type, and a
  404 for `/assets/player/nope.js`.
- [ ] **Step 2: Run it.** `cargo test --release --test api the_player_files` in rms/ fails.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run the suite.** All of rms's tests pass: `cargo test --release` in rms/.
- [ ] **Step 5: Commit.** "Server: serves the web player's files, its manifest and service worker (#50)".

### Task 9: rms: the Home Screen sign-in code

**Files:**
- Modify: `rms/src/db.rs` (schema), `rms/src/people.rs` (`device_code`, `join`), `rms/src/lib.rs` (route)
- Test: `rms/tests/api.rs`

**Interfaces:**
- **The table:**
  `CREATE TABLE IF NOT EXISTS device_codes (code_sha256 TEXT PRIMARY KEY, person_id INTEGER NOT NULL REFERENCES people(id), created_ms INTEGER NOT NULL, used_ms INTEGER, device_id INTEGER)`.
- **`POST /api/me/code`** (`Who`): returns `201 {"code": "ABCD-EFGH", "expires_ms": <created+600000>}`.
  - The code is 8 letters from `auth::new_code`'s alphabet, shown with `auth::show_code`. Only `auth::hash(plain)` is
    stored.
  - Earlier unused codes of that person are deleted.
- **`POST /api/join`** with `{code, device_name}`: when no invite matches, it looks up `device_codes` by
  `auth::hash(auth::plain_code(code))`.
  - It refuses a code that's used, past `DEVICE_CODE_MS`, or whose person is revoked, with the invite's error
    words and status (410).
  - Otherwise it makes a device for that person, exactly as an invite join does, and the same 400 ms wait applies
    after a wrong guess.
- **`pub const DEVICE_CODE_MS: i64 = 600_000`** in people.rs.

- [ ] **Step 1: Write the tests.**
  - `a_device_code_signs_in_the_same_person_once`:
    1. A friend's device asks for a code; a join with it gives a token whose /api/me has the same person id.
    2. A second join with the same code gives 410.
    3. The owner's /api/people lists the new device under that friend.
  - `a_device_code_expires_after_ten_minutes`: a row with `created_ms` set 600 001 ms back, through the test's
    direct DB handle (as other rms tests do), gives 410.
  - `a_removed_persons_code_is_refused`: revoke the person, then join gives 410.
- [ ] **Step 2: Run them.** They fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run the suite.** All of rms's tests pass.
- [ ] **Step 5: Commit.** "Server: a one-time code signs in another device of the same person, for iPhone Home Screen apps (#50)".

### Task 10: the page: the sound engine

**Files:**
- Fill: `rms/web/player/worker.js`, `rms/web/player/worklet.js`, `rms/web/player/player.js`
- Modify: `rms/web/index.html` (load player.js after app.js; replace `preview()`'s use in library rows)
- Test: `web/e2e/player.mjs` (Playwright on ribs02 against a test rms with one split FLAC-parts song and one unsplit
  .opus song, as /srv/data/winvm/r23-server.sh + r23-copy.sh make)

**Interfaces:**
- **player.js** exposes `window.RMSPlayer` with:
  - `play(song)`: a library song JSON;
  - `pause()`, `resume()`, `seek(secs)`;
  - `setVoice(v)`, `setPart(part, gain)`, `resetMix()`;
  - `onTime(cb)`: about every 50 ms, with `{at, duration}`;
  - `onState(cb)`: one of `"loading" | "playing" | "paused" | "waiting" | "ended" | "error:<words>" | "gone"`.
  - `canMix()` is false off https (`window.isSecureContext`).
- **Source per song:**
  - **A split song** (by `WebSliders.mixParts(Object.keys(song.files))`): the Worker opens a `WebDecoder` per part,
    reading through a ByteSource. That's a synchronous `XMLHttpRequest` with a `Range` header and a 1 MiB block
    cache for the server, or an OPFS `FileSystemSyncAccessHandle` for a saved song.
  - **A song not split yet:** an `<audio>` element with `RMS.fileUrl(song.url)`.
- **Hand-off and mixing:**
  - The Worker posts transferred `Float32Array` blocks per part, about 2 s ahead, to the worklet's port.
  - The worklet holds a `WebMixer` (its WASM module is sent through the port) and outputs per Task 1's ruling:
    straight to the destination, or through `MediaStreamAudioDestinationNode` into an `<audio>`.
  - The worklet posts the frames played every 50 ms; that's the clock.
- **Media Session:** metadata and the actions play, pause, previoustrack, nexttrack and seekto.
- **Errors:**
  - A 401 from any read or API call gives the state "gone".
  - A network error gives "waiting", with a retry every 3 s; it never gives up on its own.

- [ ] **Step 1: Write the e2e.** `web/e2e/player.mjs`, in headless Chromium with fake audio:
  - `split_song_plays_from_parts`: `play(split)`, then within 5 s the state is "playing" and the clock advances.
  - `no_vocals_is_heard_at_once`: an AnalyserNode on the worklet's output during a vocals-only test song shows RMS
    falling more than 90 % within 100 ms of `setVoice("novocals")`.
  - `unsplit_song_plays_its_file`: `play(unsplit)` uses the `<audio>` element.
  - `offline_mid_song`: routing the server away for 4 s gives "waiting", then "playing", with the clock not skipping
    more than 0.5 s.
- [ ] **Step 2: Run it.** It fails against the stubs.
- [ ] **Step 3: Implement** the three scripts.
- [ ] **Step 4: Run the e2e.** It passes, and Task 7's smoke test still passes.
- [ ] **Step 5: Commit.** "Server page: the web player's sound: parts mixed in the browser, in step (#50)".

### Task 11: the page: player bar, queue, Now Playing, voice and Mixer

**Files:**
- Modify: `rms/web/index.html` (markup and `drawLibrary()` rows), `rms/web/app.css`, `rms/web/player/player.js` (UI
  half), `rms/web/join.html` (after joining, go to `/`, not `/party`)
- Test: `web/e2e/ui.mjs`, plus screenshots at 1440×900, 820×1180 and 390×844

**Interfaces:**
- **Consumes:** `RMSPlayer` (Task 10), and `WebQueue`, `WebLyrics` and `WebSliders` (Task 7).
- **The bar:** `#player-bar`.
- **The row menu:** a "⋯" button with an icon menu: Play next, Add to queue, Save to this browser, and Split on this
  server (only with `can.split` from /api/me).
- **The panels:**
  - `#queue-panel`;
  - `#now-panel` (cover, title, artist, the lit lyrics; a tap on a line seeks);
  - `#mixer-pop`, a rounded pop-up anchored to the bar's Mixer button, with one slider per part present,
    "Keeps loudness" and Reset.
- **The voice buttons** and the Mixer are disabled with "Not split yet" on an unsplit song. When the song's split
  lands (/api/events `library`), they come on at the next play.
- **Kept:** the queue in `localStorage['rms.queue.' + server id]`, and the sliders and voice in
  `localStorage['rms.mix']`.

- [ ] **Step 1: Write the e2e.** `web/e2e/ui.mjs`:
  - `queue_survives_a_reload`: add 3 songs, play the 2nd, reload, and the same queue and current song are back,
    paused.
  - `lyrics_light_and_a_tap_seeks`: Now Playing on a song with word times shows exactly one `.lit` word near the
    clock, and tapping line 3 makes the clock jump to that line's start ± 0.3 s.
  - `the_voice_carries_on`: No vocals, then Next, and the new song plays with vocals at 0.
  - `http_shows_needs_https`: over http (not localhost), the Mixer button shows "Needs a secure (https) address".
  - `a_removed_song_leaves_the_queue`: the owner deletes a queued song on the server (an /api/events `library` event),
    and it leaves the queue with a note naming it; the current song keeps playing.
- [ ] **Step 2: Run it.** It fails.
- [ ] **Step 3: Implement.** Keep the page's look and the owner's UI rules (Global Constraints).
- [ ] **Step 4: Run the e2e.** It passes. Take the screenshots of every panel and state at the three sizes, and read
  them.
- [ ] **Step 5: Commit.** "Server page: the player bar, queue, Now Playing with word lyrics, voice and Mixer (#50)".

### Task 12: the page: Save to this browser, offline, removal, Home Screen code

**Files:**
- Create: `rms/web/player/saved.js` (OPFS store; served like the others: add it to Task 8's list and test)
- Modify: `rms/web/sw.js`, `rms/web/index.html`, `rms/web/player/player.js`
- Test: `web/e2e/saved.mjs`

**Interfaces:**
- **saved.js** exposes `window.RMSSaved` with:
  - `save(song)`: queued, one download at a time;
  - `remove(id)`, `list()`, `has(id)`;
  - `newer(song) -> bool`: the server's `files` sizes or names differ from the saved ones;
  - `usage()`: `navigator.storage.estimate()`;
  - `wipe()`.
- **What a save keeps:** OPFS `/<server id>/<song id>/` holds `song.<ext>`, `<part>.<ext>`, `lyrics.<ext>` and
  `cover.<ext>`, plus `meta.json` (the song JSON at save time).
- **At the first save:** `navigator.storage.persist()`. On iOS (`/iPhone|iPad/` and not standalone), the
  "Use on your Home Screen" sheet:
  - the Add to Home Screen steps;
  - a code from `POST /api/me/code`, with the 10-minute countdown.
- **Signed-out start:** a Home Screen start with no token shows "Enter your code", which goes to `POST /api/join`
  with that code.
- **The service worker** caches the page, app.css, app.js, logo.svg and every `/assets/player/*` file, by version.
  - Offline, the page shows only saved songs, with "Can't reach SERVER".
- **A 401 anywhere:** `RMSSaved.wipe()`, the queue is cleared, and the page shows "You're no longer on SERVER".

- [ ] **Step 1: Write the e2e.** `web/e2e/saved.mjs`:
  - `save_then_play_offline`: save the split song, set the context offline, reload, play it from parts, and the
    clock advances.
  - `one_download_at_a_time`: saving 3 songs at once never has more than one `/files/` download in flight.
  - `removed_device_wipes_saved_songs`: the owner revokes this device and the page reloads; OPFS for the server is
    empty and the message shows.
  - `newer_on_the_server`: upload new parts for a saved song, and the row shows "Newer on the server".
  - `home_screen_code_signs_in`: the code from the sheet, in a fresh context's "Enter your code", gives a working
    library.
- [ ] **Step 2: Run it.** It fails.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run the e2e.** It passes, and the earlier e2e files still pass.
- [ ] **Step 5: Commit.** "Server page: Save to this browser, the page offline, and the Home Screen code (#50)".

### Task 13: checks on the owner's devices, and the server's load

**Files:**
- Delete: `web/proof/` (Task 1)
- Modify: `docs/FEATURES.md` (a "Web player (server page)" row: 🔜 until released)

- [ ] **Step 1: Run a server for the device checks.** A test rms on ribs02 served over Tailscale https (as in Task 1,
  with a fresh dialog), holding a split FLAC song, a split Vorbis song, a phone-split AAC song and an unsplit .opus
  song.
- [ ] **Step 2: Run the spec's browser list on each device.**
  - Chrome and Firefox on the laptop, at `nice`, one at a time.
  - The tablet's Chrome (standing OK).
  - Safari on the iPhone: ask him in a dialog to leave it unlocked, and run the foreground check before each tap.

  Expected: every line of the spec's "Checks a release needs → Browsers" passes, with a screenshot for each.
- [ ] **Step 3: Phone memory.** A 6-minute song with FLAC parts plays to its end on the iPhone, and the tab doesn't
  reload.
- [ ] **Step 4: The server's load.** While two browsers play, `pidstat -p <rms pid> 5 12` on ribs02 averages under
  5 % CPU.
- [ ] **Step 5: The UI review.** owner-ui-gate on the screenshots, then fix whatever FAILs.
- [ ] **Step 6: Commit.** Commit FEATURES.md and the proof's removal: "Web player: checked on the laptop, tablet and
  iPhone (#50)".
- [ ] **Step 7: Finish.** Run the whole-branch review (the executing skill's final review). Then the owner's one
  release dialog: merge, the server image, and putting it on bassrms01. Merging into main also rebuilds the public
  server image.
