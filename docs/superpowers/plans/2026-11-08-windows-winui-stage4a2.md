# Windows beta stage 4a-2: crossfade and DJ mix — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** the beta plays songs into each other as the Linux app does (No Transition / Crossfade / DJ Mix (by the Beat), crossfade length 1–12 s), set in Settings → Playing.

**Architecture:** the sound already exists in the shared engine (`engine/src/player.rs`: two decks, `cue`, `cue_parts`, `mix` with the cos/sin fade and the bass swap; `engine/src/beat.rs`: `analyse`, `plan`). The GTK app drives it from `src/ui.rs` (`transitions` 2657, `begin_mix` 2757, `replan` 2643). This stage ports that driving into `engine/src/controller.rs` (tick → transitions → cue → begin_mix), adds the setting to ffi and the beta's Settings page, and moves the words into core so both apps say the same.

**Tech Stack:** Rust (core, engine, ffi via uniffi 0.31), C# WinUI 3 (windows/RadianceMusic.Windows), GStreamer.

**Spec:** docs/superpowers/specs/2026-11-05-windows-winui-design.md (:55-59 "crossfade/DJ mix" moves into engine/core; :75 the same settings file). Map of the GTK code: the stage 4a-2 map (session notes, 2026-11-08), summarised in each task below.

## Global Constraints

- Settings stay in the shared `settings.json`: keys `"transition"` ("off" | "crossfade" | "dj", default "off") and `"fade"` (seconds, default 6.0, clamped to `FADE_RANGE` = (1.0, 12.0)). Both apps read the same file.
- Words (the Linux app's, verbatim): section "Between Songs"; "No Transition", "Crossfade", "DJ Mix (by the Beat)"; "Crossfade length: {v:.0} s"; toasts "Songs play one after another" / "Songs crossfade" / "DJ mix: beats lined up between songs (a crossfade when the tempos are too far apart)".
- Timing as GTK: no mix for songs under 40 s (`MIX_MIN_S`), a video, Repeat One, the queue's end with Repeat off, or a listening party; cue at most 12 s before the plan's start; start within 0.45 s of it; jumped past half the mix: the song just ends.
- Next / Previous / a click stay hard cuts (`Player::load_from` cancels a mix); a seek drops the cue.
- Every button carries an icon; the animation blueprint for anything that appears (motion only); no sound in tests (RADIANCE_SILENT).
- Builds and tests run on ribs02 (Rust) and the ribs02 Windows VM (C#), never the laptop; ribs03 only after the owner's dialog.

## Review Focus

1. A song added to the queue (or the queue reordered, shuffled, repeat changed) while the next song is cued: the cue must follow the new next song, never mix into a song no longer next.
2. Pause during a crossfade: the fade must hold, not finish silently while paused (GTK bug, fixed in the shared player).
3. A mix into the queue's last song: Autoplay must still add songs (GTK's `begin_mix` never called `autoplay_if_last`; fixed for both).
4. Next pressed during a mix: the new song comes in at full volume, nothing stays silent (`heal_volume` on the tick, as GTK).
5. The DJ plan arriving after the fallback cue: the mix must not ask the cued deck for a bass swap it can't do (the cue is redone when the plan's `beat_matched` differs).

---

### Task 1: The words and the setting in core + engine

**Files:** core/src/words.rs, engine/src/controller.rs, engine/tests/controller.rs, src/ui.rs (words only).

**Interfaces — Produces:**
- `core::words::{BETWEEN_SONGS, NO_TRANSITION, CROSSFADE, DJ_MIX, fn fade_label(seconds: f64) -> String, fn transition_toast(id: &str) -> &'static str}`.
- `Controller::transition() -> Transition`, `Controller::set_transition(Transition)`, `Controller::fade() -> f64`, `Controller::set_fade(f64)` (clamped, saved, toast only for the transition, as GTK).

- [ ] Test `transition_and_fade_are_kept` (engine): set DjMix and 9.4 → `settings::load()` has DjMix and 9.0 (rounded as the GTK slider); the toast is the DJ toast; set_fade(30) keeps 12.
- [ ] Test `between_songs_words_are_the_linux_apps` (core): the five strings above, `fade_label(6.0) == "Crossfade length: 6 s"`.
- [ ] Implement; GTK's literals at ui.rs 2183-2190, 2447-2449, 2592 use the core words.

### Task 2: The engine drives the mix

**Files:** engine/src/controller.rs, engine/tests/controller.rs.

**Interfaces — Consumes:** Task 1's setting. **Produces:** `Controller::mixing() -> bool` (the player's).

Port GTK `transitions` / `begin_mix` / `replan` into `Engine`:
- State: `plan: RefCell<Option<(String, String, beat::Plan)>>`, `analysing: RefCell<Option<(String, String)>>`, `cued: RefCell<Option<(String, bool)>>` (next key, cued beat-matched), `mix_armed: Cell<bool>`.
- `tick`: `heal_volume`, then `transitions(at)` while playing and not in a party.
- `transitions(at)`: GTK's steps 1–7 (map §2). The next song's uri: `uri_for(t) -> Option<String>` (file, Archive relay uri, Server plain uri, a fresh YouTube stream; none → `prefetch_next`). Cue parts when the next song has parts and the plan isn't beat-matched (deck to deck otherwise). A cue whose key isn't the next song's any more, or whose `beat_matched` differs from the plan's, is dropped and redone (Review Focus 1, 5).
- `begin_mix(plan)`: `retune_cued(gains)`, `player.mix(seconds, beat_matched)`; when it starts: the queue moves on (wrap with Repeat All), and the new song becomes current exactly as `play_current` does without loading it (factor that bookkeeping into `fn became_current(&self, t: Track, gen: u64)`): lyrics, note_played, find_counterpart, pulse reset, quality, voice split ahead, publish_queue / publish_now, prefetch_next, **autoplay_if_last** (Review Focus 3).
- Drop the cue on `seek`, on `play_current`, on a voice change, on a transition or fade change.

- [ ] Tests (45 s WAVs, seek near the end; RADIANCE_SILENT): `crossfade_starts_the_next_song_before_the_end` (mixing() seen, the next song current while the first had time left); `no_transition_waits_for_the_end` (never mixing); `repeat_one_never_mixes`; `a_song_added_next_while_cued_is_the_one_mixed_into`; `a_mix_into_the_last_song_still_gets_autoplay`; `next_during_a_mix_comes_in_at_full_volume` (now().playing, the player's volume heals).
- [ ] Implement.

### Task 3: Pause holds a crossfade (shared player)

**Files:** engine/src/player.rs, engine/tests/controller.rs.

- [ ] Test `pausing_holds_a_crossfade` (engine): mixing, pause 3 s, still mixing after a fade's length; resume → done.
- [ ] Implement: the mix's clock leaves out paused time (`Mixing` keeps `held` and `paused_at`; `set_playing` sets/clears it).

### Task 4: DJ plan tests (beat.rs)

**Files:** engine/src/beat.rs.

- [ ] Tests on `plan()` with hand-made `Beats`: tempos within 8 % and strong → beat-matched, `rate` 1.0 within 1.5 %, `seconds` = clamp(16 beats, 5, 16); tempos 20 % apart → crossfade fallback at `end - fade`; one song unanalysed → fallback.
- [ ] `dj_mix_mixes_click_tracks` (engine): two 45 s 120 BPM click tracks (the `clicks()` helper), DJ mode → mixing seen.

### Task 5: ffi + the beta's Settings

**Files:** ffi/src/lib.rs, windows/RadianceMusic.Windows/Pages/SettingsPage.xaml.cs, DevActs.cs.

**Interfaces — Produces (ffi):** `enum TransitionMode { Off, Crossfade, DjMix }`, `MusicEngine::transition()/set_transition(TransitionMode)/fade()/set_fade(f64)`, `HelpWords` gains `between_songs, no_transition, crossfade, dj_mix`, `fn fade_label(seconds: f64) -> String`.

- [ ] ffi test `transition_modes_map_both_ways`.
- [ ] Settings → Playing: a "Between Songs" card under Autoplay: three RadioButtons (icons: none, a crossfade glyph, a beat glyph) and the "Crossfade length: N s" Slider 1–12 (step 1, saved when it rests 120 ms, as the Mixer's effects); the slider dims (not hidden) with No Transition.
- [ ] DevActs `transition:off|crossfade|dj`, `fade:N`; screenshots of Settings → Playing in the VM.

### Task 6: Docs, the full check, the build

- [ ] FEATURES row "Crossfade and DJ mix between songs": beta ✅; CHANGELOG beta 0.8.0-beta.7 + the Linux app's two fixes (autoplay after a mix, pause holds a fade) in "Next release".
- [ ] Full Rust suite on ribs02; the VM build + C# tests; a real run in the VM with two songs (crossfade 6 s: the log's "crossfade over 6 s", the next song current before the first's end).
