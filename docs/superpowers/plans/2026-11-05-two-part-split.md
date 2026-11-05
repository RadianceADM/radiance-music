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
