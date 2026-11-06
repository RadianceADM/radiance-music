# Radiance Music for Windows, native (beta)

C# + WinUI 3 screens over the Rust engine (`ffi/`, uniffi), issue #45. Design:
`docs/superpowers/specs/2026-11-05-windows-winui-design.md`; plans:
`docs/superpowers/plans/2026-11-05-windows-winui-stage1.md` (playing, search, Home) and
`…-stage2.md` (the library, the queue panel, word-by-word lyrics, music videos).

Built and tested on **ribs03 only** (the owner's PC): `build.ps1` runs at Idle priority, keeps its
test data in `C:\rb\beta-test` (never the owner's library) and makes no sound.

| Step | What |
|---|---|
| `-Step rust` | core/engine/ffi tests and `radiance_ffi.dll`, in MSYS2 UCRT64 (GNU toolchain, MSYS2's GStreamer, as today's Windows app) |
| `-Step app` | the C# binding (uniffi-bindgen-cs v0.11.1+v0.31.0), the WinUI app (.NET 10, Windows App SDK 2.6.5) and its tests |
| `-Step dist` | `C:\rb\beta\dist`: the app, the DLL, GStreamer's runtime and plugins, yt-dlp/Deno/FFmpeg; the self-check |
| `-Step all` | the three above |
| `-Step installer` | `C:\rb\beta\RadianceMusic-Beta-Setup.exe` (NSIS, `installer-beta.nsi`); never run on ribs03 |

The DLL is built with MinGW against MSYS2's GStreamer (one C runtime, UCRT, for both); C# calls it
through P/Invoke, and uniffi never hands memory from one runtime to the other.

## Seeing it work (ribs03, the owner's desktop session)

A window shows only through the `RadianceMusicTest` task: `test-run.ps1 -AppArgs '…'` starts the
beta with the test data and no sound, waits until it quits, and puts the task back as it was. Before
any test window, create `D:\rpos\HOLD` (the POS session's tests wait for it) and delete it after.

| Switch | What |
|---|---|
| `--shot FILE` | the window to a PNG after `--wait` seconds, then it quits (`--again S`: a second one) |
| `--page NAME`, `--size WxH`, `--search Q` | where and how big (pages: home, search, playing, liked, history, albums, artists, genres, playlist:ID) |
| `--play-test`, `--play-title WORDS` | Home's first recent song, or the one with WORDS in its title |
| `--side queue\|lyrics` | the side panel open, as the bar's buttons open it |
| `--act STEP` (again for more) | done by the app before the shot, no input sent to the desktop: `like:TITLE`, `menu`, `click:WORDS` (a button or menu item, in a menu or dialog too), `group:albums:NAME`, `switch:video\|song`, `fullscreen`, `liked-search:TEXT`, `queue-move:FROM:TO`, `queue-menu:N`, `lyric-tap:N`, `wait:N`. The log (`beta-crash.log`) says what each did, the queue's order and where the song is |
| `--video-probe FILE` | a video alone in a test window (the music video spike); `--probe-video FILE OUT` the same with no window (fails over SSH: no desktop for a swapchain) |

`measure-cpu.ps1 -App beta [-PlayTitle WORDS] [-Page NAME] [-NoPlay] [-Video FILE] [-Seconds N]`: the
beta's CPU in front, minimized and in the tray. Stage 2 (2026-11-06, ribs03, GTX 1060 6GB):

| What | In front | Minimized | Tray |
|---|---|---|---|
| Now Playing, word-by-word lyrics | 1 % | 0.8 % | 0.8 % |
| Now Playing, a music video (H.264 720p, decoded and drawn by the graphics card) | 7.1 % | | |
| Liked Songs, nothing playing | 2.2 % | 1.2 % | 0.6 % |
| The video check alone (`--video-probe`) | 0.1 % | 0.1 % | |
