# Two-part split on the computer (#43) — plan and ledger

**Owner's ruling (dialog 2026-11-05):** "Split button asks" — Split offers **Music + Vocals (fast)**
or **Four parts**. Liked songs still split in four by themselves. A two-part song can be split in
four later. (The phone splits in two only since 0.5.17, Spleeter; android ea8159e.)

**How (my call):** in Rust, inside the app (no Python): Spleeter 2-stems as ONNX through ONNX
Runtime loaded at run time (`ort` 2.0.0-rc.15, load-dynamic, API ≥ 17), STFT with `realfft`.
Runs on this computer always (never on an SSH split server or a Radiance Music Server: those
run split.py, which only knows Demucs). Same on Linux and Windows. Ported from ribs02
`/srv/build/split-bench/spleeter_ort.py` (STFT 4096/1024 periodic Hann, |STFT| bins 0..1023 in
512-frame blocks, ratio masks exponent 2 with EPS 1e-10, upper bins get each mask's mean,
inverse STFT with window-square normalisation). Not sherpa-onnx's Spleeter (its parts don't add
up to the song).

## Pins

| File | From | Size | SHA-256 |
|---|---|---|---|
| vocals.fp16.onnx | huggingface.co/csukuangfj/sherpa-onnx-spleeter-2stems-fp16/resolve/93ba771920ade509f8cbd6825b1a90856c797e08/vocals.fp16.onnx | 19681017 | 24cef84aedcd1fe87c0b743ef3370ad34dc1fabf6c9014d6128a75a538c7b668 |
| accompaniment.fp16.onnx | same, accompaniment.fp16.onnx | 19681024 | d14cea55793cc531a5875f5f4da08207d1c5ab9292e8e0099a104eecb014fcc0 |
| Linux: libonnxruntime.so.1.31.1 | member `onnxruntime-linux-x64-1.31.1/lib/libonnxruntime.so.1.31.1` of github.com/microsoft/onnxruntime/releases/download/v1.31.1/onnxruntime-linux-x64-1.31.1.tgz (11413655 bytes, c849e36a1ef3e22d4ff217b9d1ce4a3cf5b9467b975610913ae959ad5b16bdc9) | 29345384 | a96f132e5eb5ca04e23a7d2fc1078fb95aa591e8d71d0f237702ece3a1e0f4ea |
| Windows: onnxruntime.dll | member `onnxruntime-win-x64-1.31.1/lib/onnxruntime.dll` of …/v1.31.1/onnxruntime-win-x64-1.31.1.zip (82929944 bytes; the member alone, 6178062 compressed, fetched with HTTP ranges) | 17253728 | cb2fa518b18356617f6f9f460c012501da2819ec7e5c1d80bd8f5de20492db0c |

Kept in `<data>/radiance-music/two-part/` (data = glib user data dir). Fetched at the first open
(new installs) or the next open (existing installs), quietly in the background (owner rule:
prerequisites download at first open), and again before a two-part split if missing.

## Tasks

1. `engine/src/spleeter.rs` pure maths: `stft`, `istft`, `masks`; tests: STFT→iSTFT gives the
   song back (> 60 dB), masks sum to 1 (so Vocals + Music = the song).
2. Running the models: `spleeter::split(runtime, models, song, out, threads, report)` →
   `["vocals.ogg", "music.ogg"]`; `decode`/`write_ogg` shared with onnx_split (moved to
   `audio_io.rs`); probe `--probe-two-part`. Check on ribs02 against the Python reference
   (onnxruntime in the bench venv): vocals and music within 30 dB of it; parts vs song > 40 dB.
3. `engine/src/two_part.rs` fetch: pinned files, SHA-256 checked, Linux tgz member (flate2 +
   tar headers), Windows zip member by ranges; `ready()`, `fetch(progress)`. Tests on local data.
4. split.rs + db: `two_parts` table (`wants_two`/`set_two`, as `six_parts`); `Kind`; a two-part
   request runs `spleeter` here; `done`: a two-part request counts as done once Music + Vocals
   are there; liked songs' automatic splits leave a song the person split in two alone.
5. UI: Split becomes a menu (Music + Vocals (fast) / Four parts, with icons); a two-part song
   still offers Four parts; the song menu gets the same two; toasts and Splits-tab words.
6. Check: Linux end to end on ribs02 (a real split from the app), Windows build on ribs03,
   FEATURES.md + CHANGELOG, tell the Android session.

## Ledger

- Pins measured on ribs02 2026-11-05 (files in /srv/build/winui/two-part).
- Task 1: complete (spleeter.rs: separate/shares, 4 tests). Ruling: code and tests written together (not test-first); the tests were proven by breaking the code twice on ribs02 (no 1/N scaling; upper bins unshaped): both times 2 of 4 failed. Cost if wrong: none found.
- Task 2: complete (spleeter::split, ort non-optional, audio_io.rs shared with onnx_split). Real models vs the Python reference on ribs02 (60 s clip, 4 threads): 1.6 s; vocals 79.5 dB, music 81.2 dB from it; parts vs song 137.5 dB. Running block by block (one 512-frame block per model call) equals the reference's all-at-once run. Ruling: two-part parts are Ogg Vorbis q0.85 always (as the Demucs prototype; not FLAC for lossless songs as split.py does) — Spleeter's quality gains nothing from lossless — cost if wrong: bigger files if switched later. Ruling: the probe switch is not needed; the ignored test is the check (RADIANCE_SPLEETER_CHECK).
- Task 3: complete (two_part.rs: pins, fetch, ready, tgz member, zip entry by ranges; 4 tests + 2 ignored real-download tests, all passing on ribs02). Ruling: code and tests written together again; proven by three breaks (tar padding, zip extra field, hash compare), each caught by its test. Ruling: downloads go to memory (largest 29 MB) then .part → rename after the SHA-256 check; no resume (small files) — cost if wrong: a dropped connection restarts that file.
- Task 4: complete (db two_parts + split::wants_two/ask_two/done_as; split() routes a two-part request to split_two: fetch if missing, Spleeter here at nice 19 on Linux, keep). Test a_song_asked_for_in_two_is_done_once_music_and_vocals_are_there RED (no done_as) → GREEN. Ruling: two-part splits never go to an SSH split server or a Radiance Music Server (their split.py only knows Demucs) — cost if wrong: none, it takes seconds here. Ruling: on Windows ONNX Runtime's threads run at normal priority (new threads don't take a lowered thread priority there; a few seconds a song) — cost if wrong: a short busy spell during a split.
- Task 5: complete (ui.rs: can_split_two, split_as, voice_split, split_clicked + split_choices pop-over, queue gates, song menu choices, button label/icon/tooltip, quiet fetch at start). Checked in the real app on ribs02: pieces fetched at start; Split this song → pop-over with both choices (icons + a line each) → Music + Vocals: toast, side card 'Splitting “Clip” on this computer · 54%', clip.music.ogg + clip.vocals.ogg beside the song within 12 s, button now 'Split in four parts', Mixer shows Music and Vocals sliders. Suite: 131 passed, 0 failed, 3 ignored. Ruling: where this computer can't split in four (no Python splitter, no split server), the vocals choices (No vocals / Vocals only) split in two instead of doing nothing — cost if wrong: a fast split the person didn't pick on the button (it's what those choices need). Ruling: liked songs never split in two by themselves (owner: 'Liked songs still split in four by themselves'). Ruling: a song from a Radiance Music Server still splits on its server (no choice there). Ruling: the button keeps its approved sentence-case words ('Split this song', 'Split in four parts').
- Task 6: complete. Linux: real app on ribs02 (above). Windows (ribs03, Idle): engine tests 63+18+1 passed; the real pieces fetched by ranges (ONNX Runtime's DLL from Microsoft's zip, the Visual C++ runtime's four DLLs from the msvc-runtime wheel), all checked; the real split 20 s in 0.8 s, parts vs song 137 dB, and ONNX Runtime bound to the folder's Visual C++ runtime, not the PC's; today's Windows app and radiance_ffi.dll build without warnings. Found on Windows: onnxruntime.dll needs VCRUNTIME140/MSVCP140 (+_1) — fixed by fetching and preloading them (adeff6b). Not run: the Windows GUI (the same ui.rs as Linux; GUI tests there go through the RadianceMusicTest task with a packed build). FEATURES.md row + CHANGELOG.
- Final review (fresh reviewer, Opus): 1 Critical, 7 Important, 12 Minor. One fix pass:
- Final: fixed C1 (whole song held several times in memory, ~170 MB/min, OOM risk) — the split now streams from reading to writing (audio_io Reader/OggWriter with back-pressure; separate() emits block by block) — the_parts_come_out_while_the_song_is_still_being_read RED (API) → GREEN; measured on ribs02: peak 532 MB for 1 min, 529 MB for 10 min, 529 MB for 30 min (flat; was growing); with ONNX Runtime's memory pattern off, 396 MB; reference still 79.5/81.2 dB.
- Final: fixed I1 (a stalled download held the fetch lock for ever) — 30 s per-read timeout; a fetch waiting behind another reports that one's progress.
- Final: fixed I2/I3/I4 (fast split behind a running four-part split; not "done" until word timing; the other kind's request dropped) — the fast split has its own lane (splitting_two / two_queue) beside the four-part queue; word times come after the halves already play (time_words_later); the song menu doesn't offer what's on its way. Checked in the real app on ribs02.
- Final: fixed I5 (the vocals fallback marked songs "two", so liked songs never went to four once set up) — the fallback uses the fast lane unmarked; only a person's pick marks a song.
- Final: fixed I6 (a phone's .m4a halves never played as No vocals / Vocals only) — halves_from_the_phone_play RED → GREEN.
- Final: fixed I7 (Windows can't tell Bluetooth music: never gentle) — always gentle on Windows.
- Final: fixed minors M1 (a failed ONNX Runtime start cached; damaged right-size files never refetched) — only success kept; check_again re-hashes and removes damaged pieces; sync before rename — a_damaged_piece_goes_when_checked_again; M2 (priority lowered before any work; no spinning); M3 (queue stall); M4 (the pop-over splits the song playing when clicked); M5 (four by what plays); M8 (pace kept a second at a time) — a_slow_start_is_no_allowance_for_a_burst; M10 (write wait: with back-pressure at most a few seconds queued; 300 s); M11 (fetched again when back online). NaN → silence when writing. Suite 135 passed, 0 failed.
- Final: minor (deferred): M6 the four-part card's words when a split server is set (the fast lane no longer uses the card).
- Final: minor (deferred): M7 Pause/Resume hidden for fast-only setups (fast splits asked by hand ignore the pause; unasked ones wait).
- Final: minor (deferred): M9 Spleeter's .ogg halves stay beside Demucs' .flac after a later four-part split (unused: flac first).
- Final: minor (deferred): M12 a set-up but unreachable split server: No vocals waits on it instead of the fast split.
- Final: minor (deferred): GStreamer's own threads in the fast split's reading/writing aren't niced (GLib's pool).
- Final: Ruling: liked songs the person split in two stay in two (the owner's "Liked songs still split in four by themselves" read as the unsplit ones) — asked in a dialog.
- Final (Windows rerun): the_real_pieces_come_and_check_out failed on ribs03 when nothing was missing: fetch said 0 % at its start and never 100 % — fixed (says done at the end); one controller test (engine didn't start within 30 s) failed once with ribs03 51 % busy at Idle priority, then 18/18 twice (the deferred 30 s start wait); the_windows_dll_comes_by_ranges failed once on ribs02 (a download hiccup; it fails cleanly), then 5/5.
- Owner (dialog 2026-11-05): liked songs split in two "Leave it in two" (the Final ruling stands, now the owner's); the computer's karaoke button like the phone's: "Not now"; push main: "Push main now".
