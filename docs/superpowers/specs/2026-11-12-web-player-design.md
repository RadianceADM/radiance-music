# Radiance Music in the browser: the web player on the server's page

Status: design, for the owner's review. Issue: radiance-music#50 (the web version). Date: 2026-11-12.
Checked by a fresh builder read against the code. Its findings are folded in below; my calls are marked
**(my call)** and listed at the end. Two choices are the owner's (marked **(owner)**).

## Why

The owner (2026-11-10, with a screenshot of bassrms01's page): "need naten ng web player." The people on his
Radiance Music Server should be able to listen from any browser. It should have what makes Radiance Music
different: the song in parts, No vocals / Vocals only, the Mixer, and lyrics lit word by word. And it should feel
like the apps, not a file list with a preview button.

His choices, from dialogs on 2026-11-10 (settled; not to be asked again):

| Question | His answer |
|---|---|
| Where | "Both, server page first": inside the server's own page now; the separate website (#50, Google sign-in for allow-listed emails or "request access") later |
| What, first version | Player bar + queue · Now Playing + word lyrics · No vocals / Vocals only · Mixer sliders |
| How | "Yes, Rust core as WASM": the apps' own Rust core runs in the browser |
| Offline | "Yes, Save to this browser": songs kept in the browser and played without the server |

## Goals

1. **Play from the library.** Anyone signed in to the server's page (the owner, or a friend who joined in the
   browser) can play the server's songs there: Play, Play next, Add to queue. A player bar stays at the bottom while
   they browse.
2. **The queue.** Up next, reorder, remove, shuffle, repeat and clear. The queue is kept when the page is reloaded.
3. **Now Playing.** Cover, title and artist, and the lyrics as the apps show them:
   - word times: the words light one by one;
   - line times only: the words are spread over each line (`core::lyrics`) and light one by one, as in the apps;
   - a line with no words lights whole;
   - no timing at all: the text plain;
   - no lyrics: the page says so.
4. **Song / No vocals / Vocals only,** as in the apps. Once picked, the choice carries on to the next songs.
5. **The Mixer.** A slider per part the song has:
   - drums, bass, other, vocals;
   - guitar and piano when a song is split in six;
   - music and vocals for a two-part split.

   Loudness is kept as the apps keep it.
6. **Save to this browser.** A song's file, parts, lyrics and cover are kept in the browser:
   - saved songs play with the server away or the internet off;
   - the space they use is shown;
   - each has Remove from this browser.
7. **Lock screen.** Lock-screen and media-key controls (Media Session), and the music keeps playing with the phone
   locked **(my call: any music player must)**.

## Non-goals (this version)

- No YouTube, no search outside the server, no splitting or word timing in the browser. The web player plays the
  server's library only (rule 3: the server splits every song).
- Low cut, high cut and reverb. They aren't among his four picks; the design leaves room for them (see "Later").
- Listening parties stay on /party as today. The player doesn't join or host one.
- The separate website and its Google sign-in (#50's second half). Nothing here may block it.
- Uploading songs from the player: the page's upload form stays as it is.

## Anti-goals (fails even if it works)

- **The server is felt.** bassrms01 runs on the owner's gaming PC. So there is no transcoding, no mixing and no
  per-listener work on the server. The browser asks for files with Range requests, as the apps do, and does the rest
  itself.
- **Parts out of step.** Parts must be summed sample-aligned. Several `<audio>` elements nudged into step is not
  acceptable: drums and bass a few milliseconds apart sound hollow.
- **The song file and its parts heard together.** Switching between the song file and its parts follows the apps'
  rule for sources that don't play in step (engine/src/player.rs:440-449): fade out 0.6 s, a moment of quiet,
  fade in 1.2 s, never both at once.
- **A slider that answers late.** Once a song plays from its parts, a slider move or a voice pick is heard within
  about 50 ms, as in the apps.
- **Losing the owner's content.** The page today stays as it is: library, search, upload, people, invites, split
  queue, devices, party card and sign out. The player adds. It replaces only the library row's preview Play.
- **Bluetooth stutter on his laptop.** Save to this browser downloads one song at a time.

## Constraints

- **rms/ is a folder in the app repo** (RadianceADM/radiance-music), a crate of its own outside the app workspace.
  The image is built from the repo root.
  - Its pages are plain HTML with inline script, baked into the binary with `include_str!` (rms/src/web.rs), with
    no bundler.
  - `/assets/{name}` serves a fixed list of names.
- **Built files only under rms/.** The player's Rust source is a new workspace crate, `web/`. rms/ holds only its
  built files (`rms/web/player/`), baked in like the other assets.
  - So if the app code goes private later (the owner's plan, on hold: "zero trace of the codes … except RMS"),
    `web/` goes with it, and rms/ keeps working from the built files.
- **Sign-in exists already.**
  - A browser joins with an invite (join.html → POST /api/join) and keeps one device token in
    `localStorage['rms.token']`.
  - API calls use `Authorization: Bearer`; files and the /api/events WebSocket take `?t=` (rms/web/app.js).
  - The owner's page signs in the same way.
- **What the server serves** (rms/src/library.rs):
  - **Library:** GET /api/library gives each song's `files` map (song, drums, bass, other, vocals, music, guitar,
    piano, lyrics, cover), plus `parts`, `lyrics`, `word_times` and `duration_ms`.
  - **`split` is true only with all four parts.** A two-part (music + vocals) song says false, so the player decides
    which parts it has from `files`/`parts`, not from `split`.
  - **Files:** /files/… supports Range, and has no CORS.
  - **Lyrics:** a file, either a Lyricsfile YAML (lines with per-word times) or LRC.
  - **Events:** /api/events tells about library, queue and `me` (a removal) changes.
- **Formats the player must play:**
  - **Songs:** .opus (Ogg Opus, which decodes at 48 kHz), .flac, .m4a.
  - **Parts from the server's own split:** FLAC from a lossless song, else Ogg Vorbis, at 44.1 kHz.
  - **Parts the apps send up (rule 2):** .ogg, .flac or .m4a. The builder lists the exact codecs and rates the computer
    and phone apps write (Vorbis or Opus in .ogg; AAC-LC in .m4a; 44.1 or 48 kHz) from their code, and the player
    plays them all, resampled to the audio context's rate where needed.
- **The core crate (core/) has no native dependencies:** serde_json, serde_yaml, sha2 and unicode-normalization; no
  fs, threads, clocks or glib.
- **Builds run on ribs02** (owner rule). Checks on the owner's devices follow the device rules:
  - tablet: standing OK;
  - iPhone: ask him to leave it unlocked;
  - ribs03: idle check first.
- **Browsers:** current Chrome, Edge, Firefox, Safari on macOS, and Safari on iOS/iPadOS. The owner's iPhone (ip12)
  runs iOS 27.0.1.
- **Secure address:** AudioWorklet, the browser's private storage, service workers and storage persistence need
  https. rms still works over plain http (a fresh install, a LAN address). There, the player plays songs as they are,
  with the queue and lyrics. The Mixer, the voice buttons and Save say "Needs a secure (https) address" and point to
  the page's existing Add a domain banner **(my call)**.
- **Owner UI rules:**
  - every button has a leading icon;
  - nothing is sharp;
  - the page's own look (rms/web/app.css: orange accent, rounded cards);
  - every screen covers empty, loading, error, offline, slow, no results, permission and expired;
  - laptop, tablet and phone sizes are each looked at.

## How it works

### Where it sits

On the server's page (rms/web/index.html), for every signed-in role:
- **The player bar,** fixed to the bottom: cover, title, artist, play/pause, previous/next, a seek bar, the voice
  buttons, a Mixer button, a Queue button and a Now Playing button.
- **A menu on each library row,** which today has only Play and Split, so it's new: Play next, Add to queue, Save to
  this browser, and Split on this server for a person who may split.
- **The Queue and Now Playing panels,** opened from the bar. On a phone each takes the screen, in the page's own
  style, with a back button. The Mixer opens from the bar as a rounded pop-up, as on Windows.

A friend who joins in the browser (join.html) lands on the page with the library and the player, not on /party.
The party stays a card there, as it is for the owner, and /party still works **(my call: a friend should see the
music, not an empty party page)**.

### The Rust side: one WASM module, `radiance-web`

- **Where:** a new workspace crate, `web/`, built for `wasm32-unknown-unknown` with wasm-bindgen.
- **Depends on:** `radiance-core`, and on pure-Rust decoders. It never depends on `engine/`.
- **What it does:**
  - decodes the parts and mixes them with the slider gains;
  - keeps the loudness;
  - parses the lyrics and gives which words are lit at a time (`core::lyrics`, `core::sing`);
  - keeps the queue (`core::queue`).

**What moves from engine/ into `radiance-core`,** so the apps and the browser share one copy (the engine then uses
core's, and the apps' tests pass unchanged):

1. **The whole loudness keeper,** which is three pieces today:
   - each part's loudness averaged over the last 8 s, over everything heard at first, and reset on a seek
     (engine/src/player.rs:563-612);
   - the make-up gain sqrt(Σpower / Σpower·g²), clamped to 1..2, and the look-ahead limiter: 0.97 ceiling, 8 ms
     look-ahead, 250 ms release (engine/src/limiter.rs);
   - the fade of the make-up gain and ceiling toward the song file's own 1.0 as the song file is heard
     (engine/src/player.rs:638-645).
2. **The voice and slider rules:**
   - which parts make the mix: six if there, else four, else music + vocals (engine/src/split.rs:172-182);
   - a voice pick writes its preset into the sliders, and the sliders carry on to later songs until Reset
     (engine/src/controller.rs:2637-2667).

### Sound

Two sources, as in the apps:

- **The song as it is:** the song's own file through one `<audio>` element, streamed with Range requests.
- **The song in parts:**
  - A Worker running `radiance-web` reads each part (Range requests, or the saved copy), decodes it, and hands it a
    little ahead to an AudioWorklet that also runs `radiance-web`.
  - The worklet mixes with the current gains, keeps the loudness and plays.
  - Mixing in the audio thread keeps the parts sample-aligned and makes slider moves heard at once.
  - Seeking works on both sources.

**When a split song uses which source (owner):**
- **Recommended:** it starts from the song file. The first No vocals, Vocals only or slider move on that song fetches
  its parts, shown as "Getting the parts…", and then switches by the rule above. Because a voice pick carries on, the
  next songs then start straight from their parts.
- **The other way:** every split song starts from its parts, so the Mixer is instant always. That costs about four
  times the data from the owner's home internet: about 20 MB a song with Vorbis parts, over 100 MB with FLAC parts.

**A song the browser can't play as it is** (an older browser and an .opus song) plays from its parts at full, since
their sum is the song. A song that's neither playable nor split says so, with Split on this server when the person
may split.

**Memory:** parts are decoded a little ahead, never as whole-song buffers. Four parts of a 4-minute song as whole
buffers would be about 340 MB.

**With the phone locked or the tab in the background,** the music keeps playing, mixed parts included.
- The plan's first task is a proof on the owner's iPhone. One way: the worklet's output played through a
  MediaStream in an `<audio>` element, which also carries the lock-screen controls.
- If iPhone Safari won't keep the mix going, the player falls back to the song as it is while locked, and says so
  when the screen comes back **(my call)**.

### Lyrics

- `files.lyrics` (YAML or LRC) is fetched with the song, parsed by `core::lyrics` and lit by `core::sing`.
- The player's own clock drives the lighting, with the apps' leads: word 0.08 s, line 0.35 s.
- Lines scroll to keep the sung line in view, and a tap on a line seeks there.
- Goal 3 lists what each kind of lyrics shows.

### Queue

- **`core::queue`** holds the queue: shuffle (seeded, as in the apps), repeat, Up next.
- **Kept in localStorage per server,** as track ids, never files. It's restored on reload, paused where it was.
- **A song removed from the server** leaves the queue, with a note.

### Voice and Mixer

- **Song / No vocals / Vocals only** follow the voice rules above, so a pick carries on to the next songs.
- **A song not split yet** shows "Not split yet", with Split on this server (POST /api/split) for a person who may
  split. It plays as it is meanwhile. When its split lands (/api/events), the buttons come on.
- **The Mixer pop-up** has one slider per part the song has (0–100 %), "Keeps loudness" always on, and Reset.

### Save to this browser

- **What a save keeps:** the song's file, parts, lyrics and cover, in the browser's private storage (the Origin
  Private File System). The Worker reads saved songs from there, so they play with no network.
- **One download at a time,** with progress on its row, then "Saved in this browser".
  - A "Saved in this browser" filter on the library lists them.
  - Each has Remove from this browser.
- **Space:** the page shows used and free space (`navigator.storage.estimate()`). It asks the browser to keep saved
  songs (`navigator.storage.persist()`) at the first save.
- **Offline:** a web app manifest and a service worker keep the page itself, so it opens offline. Offline, it shows
  only the saved songs and says the server can't be reached; nothing else pretends to work.
- **A saved song changed on the server** (new parts, new lyrics): its row says "Newer on the server" with Update.
  Nothing re-downloads by itself.
- **Removed by the owner:** when the browser next reaches the server and finds its device or person removed
  (401, or the `me` event), it deletes its saved songs and queue, and shows "You're no longer on SERVER" **(my call:
  the server is invite-only; the owner chooses who has his songs)**.

**On iPhone and iPad (owner):**
- Safari can clear a site's storage after 7 days without a visit, unless the page is on the Home Screen.
- But a Home Screen app on iOS has its own storage, apart from Safari's: no sign-in and no saved songs carry over
  (to be confirmed on iOS 27 in the first proof).
- Invites work once, and only the owner can add devices today (index.html:131). How the Home Screen app signs in is
  his call. Then the first save on an iPhone suggests the Home Screen before it saves.

### Serving it

- **New files in rms/web/player/:** `radiance_web.js`, `radiance_web_bg.wasm`, the worklet script, the Worker
  script, `manifest.webmanifest` and `sw.js`.
- **web.rs** bakes them in and serves `.wasm` as `application/wasm`. The service worker is served at the root scope
  (`/sw.js`).
- **Cache headers:** pages keep today's no-cache. Long caching only for files named by their hash.
- **How they get there:** a script in the app repo builds `web/` on ribs02 and copies the output into
  rms/web/player/.

## Left to the builder

- **The decoders:** Symphonia, or claxon + lewton, plus a way to decode Opus parts (a WASM decoder, or the browser's
  WebCodecs AudioDecoder where it has one), after a size and speed check. Aim for the .wasm under 2 MB gzipped.
- **How decoded audio reaches the worklet:** messages with transferred buffers, or a SharedArrayBuffer ring (which
  needs COOP/COEP headers on the server's pages). Pick what works on iPhone Safari.
- **The exact read-ahead.**
- **The look of each panel** within the page's style, phone layout included.
- **How the WASM build is wired into the repo's checks.**

## Checks a release needs

- **Tests:**
  - core's moved loudness keeper and voice rules: the engine's own tests still pass;
  - the web crate's mixing;
  - sample alignment: the four parts of a FLAC song at full, summed, against the FLAC song (an .opus song decodes at
    48 kHz, so it isn't the reference).
- **Browsers:** Chrome and Firefox on the laptop, Safari on the owner's iPhone (iOS 27; ask him to leave it
  unlocked), and the tablet's Chrome. On each:
  - play, seek, skip, shuffle;
  - No vocals / Vocals only, and every slider moved while playing (heard at once, no click);
  - lyrics lit, and a tap seeks;
  - lock the phone: it keeps playing;
  - save a song, Wi-Fi off, play it, remove it;
  - reload: the queue comes back;
  - removed by the owner: the saved songs go;
  - over http: the song plays and the Mixer says it needs https.
- **Phone memory:** a 6-minute song with FLAC parts plays on the iPhone without the tab reloading.
- **The server's load,** measured on bassrms01 while two browsers play: Range reads only, no CPU worth noting.
- **Screenshots** at laptop, tablet and phone sizes for every panel and state, through the owner-ui-gate before
  "done".

## Calls I made (for the owner to check)

1. A friend who joins in the browser lands on the library and the player; the party is a card there.
2. On a plain-http server, the player plays songs as they are, and the Mixer, voice and Save say they need https.
3. The music keeps playing with the phone locked. If iPhone Safari won't keep the mix going, it falls back to the
   song as it is while locked.
4. When the owner removes a person or device, that browser's saved songs are deleted the next time it reaches the
   server.

## Later (not now, but nothing here may block them)

- **The separate website (#50):** a Radiance-hosted page, signed in with Google for allow-listed emails or a request,
  playing from a server. It needs CORS on rms (allowing that site only), and the player module unchanged.
- **Low cut, high cut and reverb:** core already holds the settings; the worklet would run them after the mix.
- **Joining a listening party** from the player.
