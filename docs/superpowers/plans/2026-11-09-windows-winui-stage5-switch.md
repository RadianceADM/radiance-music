# Windows stage 5: the switch to the native app — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** an ordinary Windows user on today's Radiance Music (0.7.21) gets the next release's update and, after the
usual pop-up, download and restart, is in the native (WinUI 3) Radiance Music, with their songs, settings and splitter.

**Architecture:** today's app already runs the release's `RadianceMusic-Setup.exe` silently into its own folder and then
opens `RadianceMusic.exe` there if the installer left one (3ffd212, in 0.7.21). The switch release's
`RadianceMusic-Setup.exe` is the native app's installer: same folder, same uninstall entry (`RadianceMusic`), the old
app's program folders removed, `RadianceMusic.exe` at the top. The native app is the beta's code built as the main app
(its name, its own updates on the main release line); data stays in the user's data folders, which both apps share.

**Spec:** docs/superpowers/specs/2026-11-05-windows-winui-design.md (Stages, 5: parity, then the owner decides when it
replaces today's app). Owner, 2026-11-08: "Patch now, delete at switch". Owner, 2026-11-09: "Stage 5 first", and he
wants the switch simulated the way other users would get it ("gusto ko nga ma simulate yung para sa ibang users",
"kung ma siswitch ba into native"); never hand-install it for him.

## Rulings

- **Two builds of one app:** `-Flavor beta` (today: RadianceMusicBeta.exe, its own folder and uninstall entry, beta
  updates) and `-Flavor native` (RadianceMusic.exe, "Radiance Music", today's folder and uninstall entry, the main
  release line's updates and `RadianceMusic-Setup.exe`). The beta stays for testers until the owner retires it.
- **The engine is told which it is** at the start (`set_beta` today; `set_native(version)` new): the native app's
  install is its exe's folder when Uninstall.exe is beside it, so its updates take `RadianceMusic-Setup.exe` and its
  restart opens `RadianceMusic.exe` (the existing non-beta script).
- **The installer removes the old app's program folders only** (bin, lib, share, libexec, python, models, as today's
  installer does on every update), never the data folders (songs, settings, the 842 MB splitter in
  %LOCALAPPDATA%\radiance-music). `radiancemusic://` moves to RadianceMusic.exe; the Start-menu entry keeps its name.
- **The release that switches is the owner's call** (the spec: parity first, then he decides). This plan builds and
  simulates it; nothing is released. Parity gap known: a server song's Save / Add / Split items (beta.11).
- **The simulation is a real user's path** in the ribs02 Windows VM: today's released 0.7.21 installed from
  downloads.radianceph.com into a Windows user with no Radiance Music, used once, then pointed at a test feed
  (`RADIANCE_UPDATE_FEED`, which 0.7.21 honours) offering 0.8.0 with the native installer; the pop-up's buttons pressed
  through the VM's keyboard (QMP), the screen shot at each step.

## Tasks

### Task 1: the engine and ffi know the native app
**Files:** engine/src/update.rs, engine/src/report.rs, engine/src/firewall.rs, ffi/src/lib.rs.
**Produces:** `update::set_native(version)`, `update::is_native()`, `update::native_install(exe) -> Install`
(Windows(root) when root\Uninstall.exe exists, else Other); `version_here()` the native version; ffi `native_start(version)`.
- [ ] Tests: `the_native_app_updates_as_the_main_windows_app` (its install takes RadianceMusic-Setup.exe, the main
  feeds, and its restart opens RadianceMusic.exe), `the_native_app_is_not_the_beta` (report and firewall use the main
  app's names).

### Task 2: the native build of the C# app
**Files:** windows/RadianceMusic.Windows/RadianceMusic.Windows.csproj (Flavor), App.xaml.cs, MainWindow.xaml(.cs),
Crash.cs, VideoProbeWindow.cs, windows/build.ps1 (-Flavor), windows/msys.sh (installer per flavor).
- [ ] `-p:Flavor=native`: AssemblyName RadianceMusic, Product "Radiance Music", NATIVE defined; the title without "Beta",
  crash log `windows-crash.log`, the engine told `native_start`, "already open" words without "(beta)".
- [ ] VM: both flavors build; the native dist's self-check passes.

### Task 3: the native installer
**Files:** windows/installer-native.nsi (new).
- [ ] Today's folder and uninstall key; removes bin, lib, share, libexec, python, models; installs the native dist with
  RadianceMusic.exe at the top; Start menu "Radiance Music.lnk" → RadianceMusic.exe; `radiancemusic://` → RadianceMusic.exe;
  the uninstaller as today's (the app and the downloaded splitter; songs stay).

### Task 4: the simulation in the VM
**Files:** scripts on ribs02 (/srv/data/winvm: sim5.sh, sim5.ps1), qmp.py gains send-key.
- [ ] A Windows user with no Radiance Music; 0.7.21 installed from downloads.radianceph.com; opened once (its data made);
  a test feed on ribs02 offering 0.8.0 (the native installer); the GTK app started with it; its pop-up shot, Update
  pressed, Restart pressed; afterwards: the native app open, the uninstall entry 0.8.0, bin\ gone, the Start-menu entry
  → RadianceMusic.exe, the data folder's files unchanged, `radiancemusic://` → RadianceMusic.exe. Screenshots for the owner.

### Task 5: docs and the owner
- [ ] CHANGELOG (unreleased: the switch), FEATURES note; issue #45 comment; the screenshots and what's left (parity:
  beta.11's items) to the owner; the release itself only on his word.
