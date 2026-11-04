# Windows builds on ribs03

Since 2026-11-04 (#39) Radiance Music's Windows app is built on **ribs03**, a real Windows 11 PC,
with [MSYS2](https://www.msys2.org/)'s UCRT64 environment (GTK 4, libadwaita, GStreamer, Rust),
not cross-built on ribs02. These files live in `C:\rb` on ribs03:

| File | What it does |
|---|---|
| `msys2-setup.ps1` | Installs MSYS2 at `C:\msys64` (download checked) and the build tools, once |
| `run.ps1` | Runs a command in the UCRT64 shell at idle priority (music on that PC keeps playing) |
| `build.sh` | Unpacks `C:\rb\src.tgz` (the source, sent from the laptop) and runs `cargo build --release` |
| `win-pack.sh` | The app folder `C:\rb\dist\RadianceMusic`: the exe, its DLLs, GStreamer plugins, schemas, icons, tools |
| `gst-plugins.txt` | The GStreamer plugins the folder carries (the set the cross-built 0.7.15 shipped) |
| `win-installer.sh` | `C:\rb\RadianceMusic-Setup.exe` with NSIS (`../installer.nsi`) |

The tools in `C:\rb\tools` are checked downloads: yt-dlp (its SHA2-256SUMS), Deno 2.9.8, and the
small static FFmpeg built by ribs02's `win-pack.sh`.

```
powershell -File C:\rb\run.ps1 "bash /c/rb/build.sh"
powershell -File C:\rb\run.ps1 "bash /c/rb/win-pack.sh"
powershell -File C:\rb\run.ps1 "bash /c/rb/win-installer.sh"
```
