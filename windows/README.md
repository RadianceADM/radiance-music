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

`measure-cpu.ps1 -App beta [-PlayTitle WORDS] [-Page NAME] [-NoPlay] [-NoBeat] [-Video FILE] [-Seconds N] [-Settle N] [-Front]`:
the beta's CPU on a page, then minimized, then in the tray. It runs hidden (`--behind`: past every
screen's edge, no focus; `-Front` only when the owner is away), and the app minimises and trays itself
on time (`--minimize-at`, `--tray-at`): from SSH (session 0) the script can't reach the desktop's windows,
so stage 2's minimized and tray numbers were the page still showing (and `--page` reached only
screenshot runs, so stage 2's "Now Playing" was Home). `-Settle 45` waits out a stream's one-time pulse
decode. Stage 3 (2026-11-07, ribs03, GTX 1060 6GB, a stream playing, % of one core):

| What | Shown | Minimized | Tray |
|---|---|---|---|
| Now Playing, word-by-word lyrics, the background following the beat | 3.5 % | 2.1 % | 2.2 % |
| The same without the beat (`-NoBeat`) | 2.9 % | 2.3 % | 2.3 % |
| Home | 1.2 % | | |
| Now Playing with the Mixer open, a song's four parts playing | 10.8 % | | |
| Now Playing with the Mixer open, a song split in two | 12 % | | |
| A song being split in four (the app's own share; the splitter at Idle priority on its cores) | 10.3 % | | |
| Now Playing, a music video (stage 2) | 7.1 % | | |

The developer runs log their busiest threads by name when over a fifth of a core (`busy:` in
beta-crash.log): with the Mixer open, most of it is the compositor's thread (the pop-up's meters) and
GStreamer's mixing the parts.
