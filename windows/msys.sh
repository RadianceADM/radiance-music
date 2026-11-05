#!/bin/bash
# The Windows beta's MSYS2 UCRT64 side (called by build.ps1 on ribs03, at Idle priority):
#   msys.sh rust     the core/engine/ffi tests, then radiance_ffi.dll
#   msys.sh deps     what radiance_ffi.dll needs (GStreamer, GLib, the C runtime)
#   msys.sh bindgen  uniffi-bindgen-cs (once) and the C# binding
#   msys.sh installer  RadianceMusic-Beta-Setup.exe from C:\\rb\\beta\\dist (NSIS)
#   msys.sh pack     C:\rb\beta\dist's native part: bin\ (the DLL and what it needs, yt-dlp, Deno, FFmpeg),
#                    lib\gstreamer-1.0 (today's Windows app's plugins), the plugin scanner, GIO's modules
set -uo pipefail
cd /c/rb/beta/src || exit 1
export CARGO_TARGET_DIR=/c/rb/beta/target
case "${1:-}" in
  rust)
    cargo test --release -p radiance-core -p radiance-engine -p radiance-ffi > /c/rb/beta/test.log 2>&1
    code=$?
    grep -E '^(test result|error|failures:)|panicked|^    [a-z_:]+$' /c/rb/beta/test.log | head -60
    [ $code -eq 0 ] || exit $code
    cargo build --release -p radiance-ffi > /c/rb/beta/build.log 2>&1
    code=$?
    grep -E '^(error|warning)' -A6 /c/rb/beta/build.log | head -40
    ls -la /c/rb/beta/target/release/radiance_ffi.dll
    exit $code
    ;;
  deps)
    ntldd /c/rb/beta/target/release/radiance_ffi.dll | grep -E -i 'gst|glib|ucrt|msvcr|vcruntime' | sed 's/ (0x.*//'
    ;;
  bindgen)
    if [ ! -x /c/rb/beta/tools/bin/uniffi-bindgen-cs.exe ]; then
      cargo install uniffi-bindgen-cs --git https://github.com/NordSecurity/uniffi-bindgen-cs --tag 'v0.11.1+v0.31.0' --root /c/rb/beta/tools --quiet || exit 1
    fi
    rm -rf windows/RadianceMusic.Windows/Native
    /c/rb/beta/tools/bin/uniffi-bindgen-cs.exe --library /c/rb/beta/target/release/radiance_ffi.dll --config ffi/uniffi.toml --out-dir windows/RadianceMusic.Windows/Native 2>&1 | grep -v CSharpier
    ls windows/RadianceMusic.Windows/Native
    ;;
  pack)
    U=/ucrt64; D=/c/rb/beta/dist; T=/c/rb/tools
    mkdir -p "$D"; rm -rf "$D/bin" "$D/lib" "$D/libexec" "$D/share"
    mkdir -p "$D/bin" "$D/lib/gstreamer-1.0" "$D/libexec/gstreamer-1.0" "$D/lib/gio/modules" "$D/share/glib-2.0/schemas"
    strip -o "$D/bin/radiance_ffi.dll" /c/rb/beta/target/release/radiance_ffi.dll || exit 1
    # the DLLs each file here needs, from MSYS2's UCRT64 (Windows' own are skipped); read, never run
    copy_deps() {
      ntldd -R "$@" 2>/dev/null | sed -n 's/.*=> \(.*[\\/]ucrt64[\\/]bin[\\/][^ ]*\.dll\) .*/\1/Ip' | sort -u | while read -r w; do
        u=$(cygpath -u "$w"); b=$(basename "$u")
        [ -e "$D/bin/$b" ] || cp "$u" "$D/bin/"
      done
    }
    missing=""
    while read -r p; do
      p=${p%$'\r'}; [ -n "$p" ] || continue
      if [ -e "$U/lib/gstreamer-1.0/$p" ]; then cp "$U/lib/gstreamer-1.0/$p" "$D/lib/gstreamer-1.0/"; else missing="$missing $p"; fi
    done < packaging/windows/ribs03/gst-plugins.txt
    [ -n "$missing" ] && echo "plugins MSYS2 doesn't have:$missing"
    n=$(ls "$D/lib/gstreamer-1.0" | wc -l)
    [ "$n" -ge 150 ] || { echo "only $n GStreamer plugins: the app couldn't play"; exit 1; }
    cp $U/libexec/gstreamer-1.0/gst-plugin-scanner.exe "$D/libexec/gstreamer-1.0/"
    copy_deps "$D/bin/radiance_ffi.dll" "$D"/lib/gstreamer-1.0/*.dll "$D"/libexec/gstreamer-1.0/*.exe
    cp $U/lib/gio/modules/*.dll "$D/lib/gio/modules/" && copy_deps "$D"/lib/gio/modules/*.dll
    cp $U/share/glib-2.0/schemas/*.xml "$D/share/glib-2.0/schemas/" && glib-compile-schemas "$D/share/glib-2.0/schemas/"
    for t in yt-dlp.exe deno.exe ffmpeg.exe ffprobe.exe; do
      [ -f "$T/$t" ] || { echo "missing tool $T/$t"; exit 1; }
      cp "$T/$t" "$D/bin/"
    done
    copy_deps "$D/bin/ffmpeg.exe" "$D/bin/ffprobe.exe"
    echo "native part: $(find "$D/bin" "$D/lib" "$D/libexec" "$D/share" -type f | wc -l) files, $n plugins, $(ls "$D"/bin/*.dll | wc -l) DLLs in bin"
    ;;
  installer)
    v=$(grep -o -E '<Version>[^<]+' windows/RadianceMusic.Windows/RadianceMusic.Windows.csproj | head -1 | cut -d'>' -f2)
    makensis -V2 -DVERSION="$v" -DDIST="$(cygpath -w /c/rb/beta/dist)" -DOUT="$(cygpath -w /c/rb/beta/RadianceMusic-Beta-Setup.exe)" -DICON="$(cygpath -w packaging/windows/radiance-music.ico)" windows/installer-beta.nsi
    ;;
  *) echo "usage: msys.sh rust|deps|bindgen|pack|installer"; exit 2 ;;
esac
