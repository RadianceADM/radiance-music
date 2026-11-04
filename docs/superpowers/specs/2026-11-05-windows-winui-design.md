# Radiance Music for Windows, native (WinUI 3): design

Issue #45. Owner's words and choices, 2026-11-05.

## Why

On Windows, GTK 4.24 spends about 30 ms of CPU on every frame of Radiance Music's window
(laying out and preparing the whole window, on the CPU, whatever the renderer). Anything that
moves costs a core: Home's drifting lights did (fixed in 0.7.18), Now Playing's moving
background still costs ~60 % of a core at 20 frames a second (0.7.19) where Linux (GTK 4.22,
Wayland) runs the whole app at ~23 %. The owner: "try native windows C# / WinUI3", then
"no, build the full winui version". WinUI 3 animates with Windows' own compositor on the GPU,
so a moving background costs next to nothing, and the app looks and behaves like a Windows app.

## What the owner decided (dialogs)

- **Approach:** C# + WinUI 3 screens, today's Rust engine as a DLL (the same setup as the
  Radiance POS Windows app, already working on ribs03). Not Rust windows-reactor: no
  programmatic scrolling (the lyrics must scroll to the sung line), no right-click menus, tray
  issues still open in that preview.
- **Look:** Radiance's look, Windows-native: the same dark sunset design, layout and pages as
  today, built with WinUI's own controls (Windows 11 corners, scrollbars, menus, animations).
- **Release:** early, as a **separate beta** beside today's GTK Windows app, growing release by
  release; today's app stays what people get until the beta has everything.
- **Build and test:** on **ribs03** (it's for Windows builds only; the owner's own PC, so builds
  at idle/below-normal priority and test windows only through the RadianceMusicTest task).

## Goals

- Every feature of `docs/FEATURES.md`'s Windows column, eventually (the list stays in step: the
  beta gets its own column while it grows).
- Smooth on a normal PC: moving things animated by the compositor; the app's own CPU near 0 when
  nothing changes, and well under today's while music plays.
- One engine for Linux, Windows (both apps) and, where it fits, the phone's logic: behaviour is
  written once, in Rust.

## Non-goals and anti-goals

- Not a different app: same pages, same words, same behaviour as the Linux app. A failure even if
  it works: anything that behaves differently from the Linux app without a reason.
- No second implementation of the engine in C# (search, playback, parts, effects, lyrics,
  splitting, library, servers stay Rust).
- Not shipped as the main Windows app until nothing on the feature list is missing.
- No change to the Linux app's behaviour while the engine is split out (its tests keep passing).

## How it's built

**The repository:** stays one repository (`RadianceADM/radiance-music`), as a Cargo workspace:

- `engine/` and `core/`: the engine, libraries with no GTK (today's src modules that aren't screens: sources,
  player/GStreamer, parts, effects, lyrics, align, db, library, split, setup, update, servers,
  share, report, feedback, …) **plus the behaviour now inside ui.rs**: the queue and what plays
  next, picking up where you left off, crossfade/DJ mix, autoplay, voice modes, the split queue,
  lyric timing (which line and words are sung), parties. It exposes a controller: commands in
  (play this list, pause, seek, like, split, …) and a state with change events out.
- the GTK app (today's binary) on top of `core`, so Linux keeps working, with its tests.
- `ffi/`: a cdylib for Windows exposing the controller through **uniffi 0.31** (bindings by
  uniffi-bindgen-cs v0.11.1+v0.31.0, as the POS app). Rust errors arrive in C# as exceptions.
- `windows/RadianceMusic.Windows/`: the C# WinUI 3 app (.NET 10, Windows App SDK 2.6.5,
  unpackaged, self-contained; the POS app's csproj settings).

**Threads:** the engine runs its own GLib main loop on a thread of its own (GStreamer's bus and
timers need one); C# calls it off the UI thread and gets events back through a uniffi callback,
posted to the UI thread with `DispatcherQueue.TryEnqueue`.

**GStreamer:** its official Windows (MSVC) build, bundled with the app: the Rust DLL is built
with MSVC, and an MSVC DLL must not share memory ownership with MSYS2's MinGW GStreamer. The
plugin set is the one today's Windows app ships (`packaging/windows/ribs03/gst-plugins.txt`).
yt-dlp, Deno and FFmpeg as today (`tools.sh`).

**The data:** the beta uses the **same library and settings** as today's Windows app (the same
folders), so your songs, likes, splits and lyrics are there. The two shouldn't run at once: the
one that starts second says the other is open (they share the one-copy port file).

**Windows features natively:** tray icon (WinUI/Win32 notify icon), media keys and the volume
flyout (SystemMediaTransportControls), single instance and `radiancemusic://` links (AppInstance
+ protocol registration), toasts, the Windows 11 title bar.

**Distribution:** its own installer, "Radiance Music (beta)", installing beside today's app
(its own folder and Start-menu entry); attached to each computer release as an extra file.
Signing: SignPath later (#31), Microsoft Store possible later.

**Tests:** the engine's Rust tests on Linux (ribs02) and Windows (ribs03, MSVC); the C# app's own
tests; a `--self-check` mode that runs without a window over SSH (POS app's pattern); screens
checked in screenshots at laptop and narrow widths; GUI tests on ribs03 only through the
Interactive scheduled task.

## Stages (each one a beta release)

1. **Engine split + first beta:** the workspace (`core`, GTK app, `ffi`), the controller for
   search, the queue and playback; GStreamer MSVC on ribs03; the WinUI shell with Radiance's look
   (sidebar, pages), Home, Search, Now Playing (art, lyrics lines, the moving background on the
   compositor), the player bar, tray, media keys. Measured: CPU while playing on ribs03.
2. **Library:** Liked Songs (saving, A–Z rail, search), Albums/Artists/Genres, History,
   Playlists, the queue panel, the lyrics column (grows toward the middle, scrolls to the sung
   line, word by word), music videos.
3. **Parts and splitting:** the Mixer (part sliders, low/high cut, reverb), splitting setup and
   queue (first-open download, progress, pause), word timing, lyrics editor.
4. **Everything else:** servers and parties, `radiancemusic://` links, sharing (copy link, phone
   transfer), update pop-up, Report a Problem, rating pop-up, Storage, settings, guide, offline.
5. **Parity:** every row of FEATURES.md's **Windows column** checked on the beta (today's Windows
   features: a port, not new ones; the rows still ❌ on Windows, like story cards, are separate work
   for all platforms under the one-feature-list rule); then the owner decides when it replaces
   today's Windows app.

## Calls made while writing this (the owner can change any of them)

- **The bar for replacing today's app** is the Windows column of FEATURES.md as it is (a port);
  features missing on Windows today are separate, all-platform work.
- **The phone isn't part of this project**, but the split leaves room for it: the pure behaviour
  (queue, what plays next, autoplay, lyric timing, voice modes) goes into a crate with no GLib or
  GStreamer (`core`), and playback, sources and the rest into `engine` on top of it, so the
  Kotlin app could use `core` through uniffi's Kotlin bindings later without re-splitting.
- **The GTK app keeps getting fixes and features** while the beta grows; from now on, new
  behaviour goes into `core`/`engine` first and the GTK screens call it. The behaviour in ui.rs
  moves out in small steps, each merged with the Linux tests green.
- **Versioning:** the beta has its own version line (0.8.0-beta.N), shipped as an extra file in a
  computer release (`RadianceMusic-Beta-Setup.exe`); a finished stage goes out with the next
  computer release (each release still needs the owner's OK); the beta's update pop-up offers only
  beta installers.
- **Stage 1 is a checkpoint:** if the beta's Now Playing while playing isn't clearly under today's
  ~60 % of a core on ribs03 (aim: under 15 %), work stops and the numbers go to the owner before
  stage 2.
- **`radiancemusic://` links** stay with today's app until the beta handles servers and parties
  (stage 4), so an invite never opens an app that can't use it.
- **The POS app's setup is the template:** its csproj, uniffi pipeline and `--self-check` mode
  (`~/Github/radiance-pos-apps`), shared with that session.

## Left to the builder

The exact controller API shape, the C# project layout, how the GTK app is reworked onto `core`
(in steps, tests green each time), the composition effects used for the background (as close to
today's look as possible), and the installer tool (NSIS as today, or Inno/MSIX).
