# Radiance Music Server: split and time words in Rust (no Python) — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** the server (rms/) splits songs and times their words in its own Rust process through ONNX Runtime, with the
models kept loaded and the graphics card used when there's an NVIDIA one (Windows), and needs no Python anywhere.

**Architecture:** a `SplitEngine` seam in rms between the queue (split.rs, unchanged in behaviour) and the work. The real
engine loads ONNX Runtime at run time (`ort` load-dynamic, as the app's engine), runs Demucs (htdemucs ONNX, the app's
prototype engine/src/onnx_split.rs) and MMS (the `models-mms-1` ONNX) and ports align.py's word placing to Rust. Audio
in and out goes through ffmpeg (already in the image and install.sh; fetched on Windows). Everything it needs downloads
at the first start into data/models with its progress on the page, as today.

**Tech stack:** Rust (axum/tokio, as rms), `ort =2.0.0-rc.15` (load-dynamic), reqwest (rustls) for downloads, zip,
flate2+tar; ONNX Runtime 1.31.1; ffmpeg.

**Spec:** issue #72 (owner 2026-11-09: "python talaga??? nababagalan ako sa python eh" → dialog: "Rust, no Python":
"I time one song on ribs03's graphics card first and switch only if it's at least as fast as now"). Measured (one 3:50
song, GTX 1060): today 91 s a song (split.py 23 s on the card + align.py 68 s on the processor); ONNX Runtime CUDA 12:
Demucs 24.9 s, MMS 5.4 s → ~35 s a song with the models loaded. Passes.

## Global constraints
- No Python at run time, in the Dockerfile or in install.sh. (Python may make test fixtures once, on ribs02; never shipped.)
- Pins, each with its SHA-256, verified before use:
  - ONNX Runtime 1.31.1: `onnxruntime-linux-x64-1.31.1.tgz` (c849e36a…), `onnxruntime-win-x64-1.31.1.zip` (5869e26b…),
    `onnxruntime-win-x64-gpu_cuda12-1.31.1.zip` (5cbf65ed…), from github.com/microsoft/onnxruntime releases.
  - Demucs: huggingface.co/StemSplitio/htdemucs-onnx @ d54ed9eb…, `htdemucs_fp16weights.onnx` (d05c269d…, MIT).
  - MMS: RadianceADM/radiance-music release `models-mms-1`, `mms_fa_fp16w.onnx.part00..06` joined (de58e987…, CC-BY-NC 4.0).
  - NVIDIA (Windows, NVIDIA card only): CUDA 12.8 redist cuda_cudart 12.8.90 (4a39058f…), libcublas 12.8.4.1 (57a47011…),
    libcufft 11.3.3.83 (cc6e0ba9…); cuDNN 9.10.2.21 cuda12 (c1a4567d…). Not CUDA 13: it dropped GTX 10xx (Pascal).
  - ffmpeg (Windows, when none on PATH): a versioned GyanD/codexffmpeg essentials build, its SHA-256 pinned.
- The graphics card: Windows with an NVIDIA card (nvidia-smi answers). Used for a job when it has ≥ 2.5 GB free memory
  (a desktop's own use doesn't count against it; split.py's 20 % rule did); else, or if the card fails mid-job, the processor.
- Same outputs as today: the five parts named as now (vocals, music, drums, bass, other; FLAC from a lossless song, Ogg
  Vorbis otherwise), and the Lyricsfile text exactly as align.py writes it (version, the comment line, lines/words/ms).
- The queue, its turns, progress, removal (stop) and the page's words keep working as now; the 24 api tests stay green.
- Builds on ribs02 (Linux tests via `$S/rt`; Windows rms.exe in the ribs02 VM, PM asked first). ribs03: testing only.
- The owner's bassrms01 is swapped to the Rust server only after a dialog (with the merge).

## Review focus
- A song shorter than one Demucs piece (7.8 s), a mono song, and a 10-minute song (memory): parts as long as the song.
- The card runs out of memory or its driver fails mid-song: the job finishes on the processor, not "failed".
- A download cut off halfway, or a file whose SHA-256 is wrong: it resumes / is thrown away and fetched again; the page
  says so; songs wait.
- Lyrics words with no letters (numbers, signs) and lines too short to fit their letters: placed as align.py does.
- Removing the song being split stops the work within one Demucs piece.

---

### Task 1: the SplitEngine seam
**Files:** rms/src/split.rs, rms/src/engine.rs (new), rms/src/lib.rs (Config), rms/tests/api.rs.
**Produces:** `trait SplitEngine: Send + Sync { fn check(&self) -> Result<String, String>; fn fetch(&self, models: &Path,
progress: &dyn Fn(f64, &str)) -> Result<(), String>; fn split(&self, song: &Path, out: &Path, progress: &dyn Fn(f64),
stop: &AtomicBool) -> Result<Vec<String>, String>; fn align(&self, vocals: &Path, given: &serde_json::Value, progress:
&dyn Fn(f64), stop: &AtomicBool) -> Result<String, String>; }` (blocking; split.rs calls it in spawn_blocking);
`Config.engine: Arc<dyn SplitEngine>`; `engine::Fake` (tests: what FAKE_PYTHON did, recording its calls).
- [ ] api tests move from the fake Python to `engine::Fake`; still 24/24. The Python engine stays as the real one until Task 6.

### Task 2: fetching what it needs
**Files:** rms/src/fetch.rs (new), rms/Cargo.toml (reqwest rustls+stream, zip, flate2, tar, sha2).
**Produces:** `struct Item { name, urls: Vec<String> (parts joined in order), size, sha256, unpack: Option<Unpack> }`;
`fn wanted(os, nvidia: bool) -> Vec<Item>`; `async fn fetch_all(dir, items, progress) -> Result<(), String>` (resume with
Range into `name.part`, SHA-256 checked, then unpacked/renamed; a wrong file is deleted and the error says which).
- [ ] Tests (a local axum stand-in): parts joined in order; a wrong SHA-256 → error, nothing kept; a cut-off download
  resumes from its bytes; a zip's chosen files land flat in the folder; progress only rises and ends at 1.

### Task 3: audio through ffmpeg
**Files:** rms/src/audio.rs (new).
**Produces:** `fn ffmpeg() -> Option<PathBuf>` (PATH, else data/models/ffmpeg); `fn decode(path, rate, channels) ->
Result<Vec<f32>, String>`; `fn write_part(path_no_ext, data: &[f32], rate, lossless: bool) -> Result<String, String>`
(FLAC 16-bit or Ogg Vorbis; returns the file name); `fn lossless(path) -> bool` (as split.py's kind).
- [ ] Tests (ribs02 has ffmpeg): a stereo sine round-trips (FLAC within 1e-4, Vorbis within its loudness); mono comes
  back as two equal channels; a missing file errors in words.

### Task 4: Demucs
**Files:** rms/src/demucs.rs (new; from engine/src/onnx_split.rs).
**Produces:** `fn separate(x: &[f32], run: &mut dyn FnMut(&[f32]) -> Result<Vec<f32>, String>, progress, stop) ->
Result<[Vec<f32>; 4], String>` (pieces of N = 343 980 frames, a quarter overlapped, as the prototype); `fn split(session,
song, out, progress, stop)` writing vocals, music (= drums + bass + other), drums, bass, other.
- [ ] Tests: an identity `run` gives each part = the song (within 1e-5) for a 3 s, a 10 s and a 20 s song; `stop` set →
  `Err("removed")` before the next piece; progress ends at 1. Ignored real-model test vs the PyTorch reference (≥ 30 dB).

### Task 5: MMS word timing (align.py in Rust)
**Files:** rms/src/mms.rs (new), rms/tests/fixtures/align/*.json (+ gen_align.py that made them, ribs02 only).
**Produces:** `fn norm(word) -> String`; `fn forced_align(emission: &[f32], frames, classes, tokens: &[usize]) ->
Option<Vec<usize>>` (torchaudio's CTC Viterbi, blank 0); `fn merge_tokens`; `fn place(...)`; `fn lyricsfile(groups) ->
String`; `fn align(emissions_of: &mut dyn FnMut(&[f32]) -> Result<(Vec<f32>, usize), String>, vocals_16k_mono: &[f32],
given: &Value, progress, stop) -> Result<String, String>` (30 s passes, 2 s either side, as align.py).
- [ ] Tests: torchaudio fixtures (emission → frames per token) identical; `norm` as align.py on accents, apostrophes,
  digits; given times → the YAML byte-for-byte as align.py; plain-text lyrics; a line too short → its words take the gap
  as align.py. Ignored real-model test: Rust vs align.py on one song, ≥ 95 % of words within 20 ms.

### Task 6: the ONNX engine, Python gone
**Files:** rms/src/engine.rs (Onnx), rms/src/split.rs, rms/src/lib.rs, rms/src/main.rs.
**Produces:** `engine::Onnx` — `fetch` = fetch::wanted(..) into data/models; `check` = the runtime loads and ffmpeg runs
("Ready (graphics card)" / "Ready (processor)"); sessions made once and kept (Demucs and MMS), CUDA when the GPU runtime
is in and the card has ≥ 2.5 GB free, else the processor (RMS_THREADS); a card error mid-job → that job again on the
processor. split.rs loses python(), MODELS_PY, CHECK_PY, the scripts check; RMS_PYTHON / RMS_SCRIPTS are ignored (a log
line says so).
- [ ] api tests 24/24 (Fake); rms unit tests; the status page's words for each state.

### Task 7: the image, install.sh and the words
**Files:** rms/Dockerfile, rms/install.sh, rms/README.md, docs (FEATURES), CHANGELOG.
- [ ] Dockerfile: debian slim + ffmpeg + ca-certificates + tini + rms (no Python, no torch); install.sh without Python;
  README's "what it installs" and model sizes; the image builds on ribs02 (podman) and a container answers /api/status.

### Task 8: Windows and ribs03
- [ ] rms.exe from the ribs02 VM; on ribs03 a test copy (its own data folder and port, never bassrms01's): first start
  fetches, then one song split + timed on the card: seconds, the parts against Python's (dB), the words against align.py.
- [ ] Results to #72; a dialog: merge, and swap bassrms01 to the Rust server (its data kept).
