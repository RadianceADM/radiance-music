#!/bin/bash
# ribs03 (MSYS2 UCRT64): the Windows folder of Radiance Music (bin\ with its DLLs, GStreamer's plugins,
# GTK's schemas and icons, yt-dlp, Deno, FFmpeg) from the native build (#39). The same layout and
# plugin set as ribs02's cross-build pack (/srv/build/win/win-pack.sh: gst-plugins.txt is its list).
set -uo pipefail
U=/ucrt64
RB=${RB:-/c/rb}
EXE=${EXE:-${CARGO_TARGET_DIR:-$RB/target}/release/radiance-music.exe}
D=${DIST:-$RB/dist/RadianceMusic}
T=${TOOLS:-$RB/tools}
S=${SRC:-$RB/src/radiance-music}
HERE=$(cd "$(dirname "$0")" && pwd)
rm -rf "$D"; mkdir -p "$D/bin" "$D/lib/gstreamer-1.0" "$D/libexec/gstreamer-1.0" "$D/share/glib-2.0/schemas" "$D/share/icons" "$D/lib/gio/modules" "$T"
strip -o "$D/bin/radiance-music.exe" "$EXE" || exit 1
# the DLLs every .exe/.dll here needs, from MSYS2's UCRT64 (Windows' own are skipped); read from
# the files (ntldd), nothing is run
copy_deps() {
  ntldd -R "$@" 2>/dev/null | sed -n 's/.*=> \(.*[\\/]ucrt64[\\/]bin[\\/][^ ]*\.dll\) .*/\1/Ip' | sort -u | while read -r w; do
    u=$(cygpath -u "$w"); b=$(basename "$u")
    [ -e "$D/bin/$b" ] || cp "$u" "$D/bin/"
  done
}
# GStreamer: the plugins ribs02's folder has, and the plugin scanner
missing=""
while read -r p; do
  p=${p%$'\r'}; [ -n "$p" ] || continue   # (a Windows checkout's line endings)
  if [ -e "$U/lib/gstreamer-1.0/$p" ]; then cp "$U/lib/gstreamer-1.0/$p" "$D/lib/gstreamer-1.0/"; else missing="$missing $p"; fi
done < "$HERE/gst-plugins.txt"
[ -n "$missing" ] && echo "plugins MSYS2 doesn't have:$missing"
n=$(ls "$D/lib/gstreamer-1.0" | wc -l)
[ "$n" -ge 150 ] || { echo "only $n GStreamer plugins: the app couldn't play"; exit 1; }
cp $U/libexec/gstreamer-1.0/gst-plugin-scanner.exe "$D/libexec/gstreamer-1.0/"
copy_deps "$D/bin/radiance-music.exe" "$D"/lib/gstreamer-1.0/*.dll "$D"/libexec/gstreamer-1.0/*.exe
# GIO: its TLS module (https in GStreamer's downloads) and proxy modules
cp $U/lib/gio/modules/*.dll "$D/lib/gio/modules/" && copy_deps "$D"/lib/gio/modules/*.dll
# (no gdbus.exe: see one_copy in main.rs)
# GTK: settings schemas (compiled here), icon themes, pixbuf loaders
cp $U/share/glib-2.0/schemas/*.xml "$D/share/glib-2.0/schemas/" && glib-compile-schemas "$D/share/glib-2.0/schemas/"
cp -r $U/share/icons/Adwaita $U/share/icons/hicolor "$D/share/icons/"
if [ -d $U/lib/gdk-pixbuf-2.0 ]; then
  cp -r $U/lib/gdk-pixbuf-2.0 "$D/lib/"
  copy_deps "$D"/lib/gdk-pixbuf-2.0/*/loaders/*.dll
fi
# the tools the app runs: yt-dlp, Deno (YouTube's challenges), FFmpeg (a small static build): tools.sh
for t in yt-dlp.exe deno.exe ffmpeg.exe ffprobe.exe; do
  [ -f "$T/$t" ] || { echo "missing tool $T/$t"; exit 1; }
  cp "$T/$t" "$D/bin/"
done
copy_deps "$D/bin/ffmpeg.exe" "$D/bin/ffprobe.exe"
cp "$S/LICENSE" "$S/README.md" "$S/CHANGELOG.md" "$D/"
echo "files: $(find "$D" -type f | wc -l), dlls in bin: $(ls "$D"/bin/*.dll | wc -l), size: $(du -sh "$D" | cut -f1)"
ls "$D/bin" | grep -E "\.exe$" | tr '\n' ' '; echo
