# Radiance Music for Windows, native (beta)

C# + WinUI 3 screens over the Rust engine (`ffi/`, uniffi), issue #45. Design:
`docs/superpowers/specs/2026-11-05-windows-winui-design.md`; plan:
`docs/superpowers/plans/2026-11-05-windows-winui-stage1.md`.

Built and tested on **ribs03 only** (the owner's PC): `build.ps1` runs at Idle priority, keeps its
test data in `C:\rb\beta-test` (never the owner's library) and makes no sound.

| Step | What |
|---|---|
| `-Step rust` | core/engine/ffi tests and `radiance_ffi.dll`, in MSYS2 UCRT64 (GNU toolchain, MSYS2's GStreamer, as today's Windows app) |
| `-Step app` | the C# binding (uniffi-bindgen-cs v0.11.1+v0.31.0), the WinUI app (.NET 10, Windows App SDK 2.6.5) and its tests |
| `-Step dist` | `C:\rb\beta\dist`: the app, the DLL, GStreamer's runtime and plugins, yt-dlp/Deno/FFmpeg; the self-check |

The DLL is built with MinGW against MSYS2's GStreamer (one C runtime, UCRT, for both); C# calls it
through P/Invoke, and uniffi never hands memory from one runtime to the other.
