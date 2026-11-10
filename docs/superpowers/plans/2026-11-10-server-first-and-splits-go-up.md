# Server First, and Joined Splits Go Up (computer apps) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Linux (GTK) and Windows (WinUI) apps follow the owner's server rules 1 and 2, as the Android app 1.3.0 does.

**Architecture:** Nearly all of it is shared engine code, which both apps already call. The lookup goes in `servers.rs`. Saving goes in `library::save_with`. Server parts come first in `splitting::four_parts` / `two_parts`. The upload queue is a new `uploads.rs`. The apps get small hooks only: the stream lookup in `find_stream`, a fallback on a playback error, and the triggers for uploads.

**Tech Stack:** Rust (engine, core, the GTK app), ureq, serde_json; tests with the engine's `stand_in` fake server and `engine/tests/controller.rs`'s `rms_stand_in`.

**Spec:** The owner's rules, 2026-11-09, "applies to all platforms or os", relayed by the Android session (task #74). Rule 1: a joined device asks its server first. If the server has the song, the device takes the server's copy (song, parts, lyrics, cover), doesn't go to YouTube and doesn't split it itself; otherwise it goes to YouTube at once. Rule 2: when a joined device splits a song itself, the song and its parts go up to the server automatically; offline, they wait and go up later. Only joined devices. Android's build, as reported 2026-11-10 (server/ServerFirst.kt, ServerUploads.kt; cdca786), is the reference for every detail below.

## Global Constraints

- Match: `whole(title)` equal AND `plain(artist)` equal. `whole` = accents taken off (NFD, combining marks out), lowercase, letters and digits only, brackets' contents KEPT. `plain` is `core::track::plain` (drops bracketed text).
- Lookup: `GET /api/library?q=<title with ( ) [ ] turned to spaces>`, 3 s timeout. Each song key's answer (found or not) is kept 10 min. A server that didn't answer is skipped for 60 s. Servers are asked in joined order; the first match wins. Server songs (`Source::Server`) are never looked up. Only YouTube songs are looked up (lossless Archive songs keep their own source).
- Uploads: the queue lives in `<config>/radiance-music/server-uploads.json` as `{"waiting":[{"key","server"}],"done":["<first address>|<key>"],"aside":[…]}`. `server` is the server's first address.
- Per song: not on the server → `POST /api/songs` with the song, cover and lyrics, plus `?parts=coming` only when all four parts are here, then `POST /api/songs/{id}/parts`. On the server but not split → its four parts if here, else `POST /api/split`. On the server and split → nothing.
- The two halves (music + vocals) never go up. Songs that aren't saved (no file) never go up.
- Retried later: offline, timeouts, 408/429/5xx. Set aside for good: other 4xx, the file gone, a type servers don't take (.opus/.flac/.m4a only). Needs "Can add songs" (403 = set aside).
- Upload triggers: a split finishing here; app start + 20 s; back online; every 10 min; a backfill of liked songs that have four parts here, 30 s after start.
- Fully silent: logs only (`crate::log::note`), no notice, switch or row.
- Builds and tests on ribs02 only (`rt2` runner, own target dir `/srv/build/server-rules/target`). Never on the laptop.

## Review Focus

1. A server that's away (laptop out of the house): every play must not wait 3 s per song. The 60 s skip and 10 min memo must hold, and `prefetch_next` does the lookup ahead.
2. A server copy whose stream dies mid-song: the song must carry on from YouTube at the same place, not skip.
3. The same song uploaded twice (two triggers at once): one worker at a time, and the `done` list.
4. Titles that differ only in brackets ("Dilaw (Acoustic)" vs "Dilaw") must NOT match; accents and case must.
5. A friend without "Can add songs": no upload loop (403 → aside), and Rule 1 still works (reading needs no switch).

---

### Task 1: The match (core)

**Files:** Modify `core/src/track.rs`, `core/Cargo.toml` (add `unicode-normalization = "0.1"`).

**Interfaces:** Produces `pub fn whole(text: &str) -> String` and `pub fn same_copy(artist: &str, title: &str, t: &Track) -> bool` (`whole(title) == whole(t.title) && plain(artist) == plain(t.artist)`).

- [ ] Test `whole_keeps_brackets_and_drops_accents`: `whole("Dilaw (Acoustic)") == "dilawacoustic"`, `whole("DILAW") == whole("Dílaw")`.
- [ ] Test `same_copy_pairs` (ServerFirstTest.onlyTheSameVersionCounts): "Dilaw (Acoustic)" by Maki ≠ "Dilaw" by Maki; "DILAW" by "Mäki" = "Dilaw" by "Maki"; "Dilaw" by "Maki (PH)" = "Dilaw" by "Maki".
- [ ] RED, implement, GREEN (`cargo test -p radiance-core`), commit.

### Task 2: Rule 1 lookup (engine/servers.rs)

**Interfaces:** Produces:
- `pub fn copy_of(t: &Track) -> Option<(Joined, Song)>` (memo, skip and joined order as above; `None` for non-YouTube songs);
- `pub fn find_on(server: &Joined, t: &Track) -> Result<Option<Song>, Failure>` (one `?q=` call, 3 s);
- `pub fn skip_for_now(address: &str)` (60 s);
- `pub fn copy_stream(server: &Joined, song: &Song) -> String` (absolute + `?t=`);
- `pub fn stream_from(id: &str) -> Option<String>` (the server address a YouTube id's stream came from, set by `note_stream(id, address)`);
- `copy_on` uses `same_copy`.

- [ ] Tests (stand_in): a match returns the song and the request is `/api/library?q=Dilaw%20Acoustic%20`-style (brackets → spaces); a second call within 10 min makes no request; an unreachable server is skipped for 60 s (no second connect); two servers: the first joined that has it wins; Server/Archive songs → None without a request.
- [ ] RED, implement, GREEN (`cargo test -p radiance-engine servers`), commit.

### Task 3: Saving takes the server's copy (engine/servers.rs + library.rs)

**Interfaces:** Produces `pub fn save_copy(server: &Joined, song: &Song, dir: &Path, stem: &str) -> Result<String, String>`; `save()` becomes a call to it with the server's own stem. `library::save_with`: `Source::YouTube` → `copy_of(t)` first → `save_copy(…, &base)`.

- [ ] Test: a YouTube song with a split copy on the stand-in saves "Artist - Title.opus" plus `.drums/.bass/.other/.vocals.ogg`, `.lyrics.yaml` and `.jpg` under the YouTube song's own base name, and never calls yt-dlp (`youtube::save` not reached: the stand-in counts requests, and `split::done(&saved)` is true).
- [ ] RED, implement, GREEN, commit.

### Task 4: Server parts before any split here (engine/splitting.rs)

**Interfaces:** Produces `pub fn bring_parts(t: &Track, server: &Joined, song: &Song) -> Result<usize, String>` in servers.rs (into `split::stems_base(t)`, as a split here would put them). `four_parts`: a split copy → bring, `Ok`. A copy not split → `split(server, …)` (POST /api/split), then check `song(server, id)` every 20 s for up to 45 min (`WAIT_EVERY`, `WAIT_MOST`; tests pass shorter ones through `four_parts_with`), reporting `Phase::Splitting(job progress)`. The job failed or removed, a 403, or the time up → split here as before. `two_parts` never waits: a split copy → bring, else split here.

- [ ] Tests: split copy → parts land at `stems_base` and no splitter runs (the LocalSplitter is None: a local split would fail); copy not split → POST /api/split then parts after the copy turns split; job failed → falls through (error from the missing local splitter proves it tried).
- [ ] RED, implement, GREEN, commit.

### Task 5: Rule 2 queue (engine/uploads.rs, new)

**Interfaces:** Produces `pub fn enqueue(t: &Track)` (every joined server, saved songs only), `pub fn backfill(liked: &[Track])` (saved songs with four parts here, not done), `pub fn kick()` (one worker thread at a time draining `waiting`, needs the liked list: `set_songs(Vec<Track>)` keeps the saved tracks by key), `pub fn state() -> Queue` (tests). `servers::upload` gains `parts_coming: bool` (`?parts=coming`).

- [ ] Tests (stand_in): not there + four parts → POST /api/songs?parts=coming then /parts, then `done`; not there + halves only → POST /api/songs without parts=coming, no /parts; there not split + parts here → /parts only; there not split, no parts → /api/split; there split → nothing; a 503 → still waiting; a 403 → aside; a file gone → aside; the file survives a restart (`load()` after `store()`).
- [ ] RED, implement, GREEN, commit.

### Task 6: Windows engine hooks (engine/src/controller.rs, controller/rms.rs)

- [ ] `find_stream`: in the worker, `servers::copy_of(&t)` first; found → `web_uri(relay, copy_stream)`, `note_stream`, kbps 0. `prefetch_next` gets it too (same function).
- [ ] `Event::Error`: the current song is YouTube and `stream_from(id)` is Some → `skip_for_now`, `streams.forget(id)`, `restore_at` = position, `play_current()` (YouTube now).
- [ ] `split_landed`: `uploads::enqueue(t); uploads::kick()` in place of `rms_share_parts`.
- [ ] Start: `uploads::set_songs(liked)`, kick after 20 s, backfill after 30 s, kick every 10 min. `set_online(true)` → kick.
- [ ] Tests in engine/tests/controller.rs with `rms_stand_in` (its library gains "Maki - Dilaw" with parts): playing YouTube "Dilaw" by Maki loads the server's file; liking it saves the server's copy; a split landing sends the song up.
- [ ] RED, implement, GREEN (`cargo test -p radiance-engine`), commit.

### Task 7: Linux app hooks (src/ui.rs, src/ui/rms.rs)

- [ ] The same five hooks as Task 6 in `find_stream`, the `Event::Error` arm, `split_landed` (in place of `share_parts`), start, and `set_online`.
- [ ] `cargo build` + `cargo test` of the whole workspace on ribs02. `cargo check --target x86_64-pc-windows-gnu -p radiance-engine -p radiance-ffi` for the Windows side.
- [ ] Commit.

### Task 8: End to end against a real server

- [ ] On ribs02: rms from rms-rust (StandIn engine is test-only, so the real one with its models from /srv/build cache, or the apptest server on :18321). Join with the GTK app's `--probe-server` flow, then a probe that plays and likes a song the server has: no yt-dlp call in the log, the files from the server; split a song the server lacks → it appears on the server with its parts and no second split job (`PARTS_CAME`).
- [ ] Ledger the results; the release is a separate dialog.
