#!/bin/bash
# On ribs02 as radmin: the Windows folder of Radiance Music (bin/ with its DLLs, GStreamer's
# plugins, GTK's schemas and icons, yt-dlp, Deno, FFmpeg), from the cross build.
set -uo pipefail
SYS=/usr/x86_64-w64-mingw32/sys-root/mingw
P=/srv/build/win/prefix
EXE=/srv/build/cargo-target/radiance-music-win/x86_64-pc-windows-gnu/release/radiance-music.exe
D=/srv/build/win/dist/RadianceMusic
T=/srv/build/win/tools
export TMPDIR=/srv/build/win/tmp
rm -rf "$D"; mkdir -p "$D/bin" "$D/lib/gstreamer-1.0" "$D/libexec/gstreamer-1.0" "$D/share/glib-2.0/schemas" "$D/share/icons" "$T"
x86_64-w64-mingw32-strip -o "$D/bin/radiance-music.exe" "$EXE"
# the DLLs every .exe/.dll here needs, found in Fedora's mingw sysroot or our own prefix (Windows' own are skipped)
need() { x86_64-w64-mingw32-objdump -p "$1" 2>/dev/null | sed -n 's/^\s*DLL Name: //p'; }
copy_deps() {
  local queue=("$@") f dll src
  while [ ${#queue[@]} -gt 0 ]; do
    f=${queue[0]}; queue=("${queue[@]:1}")
    for dll in $(need "$f"); do
      [ -e "$D/bin/$dll" ] && continue
      src=""
      for dir in "$P/bin" "$SYS/bin"; do [ -e "$dir/$dll" ] && { src="$dir/$dll"; break; }; done
      [ -n "$src" ] || continue
      cp "$src" "$D/bin/"; queue+=("$D/bin/$dll")
    done
  done
}
# GStreamer: its plugins (base, good, bad-free) and the plugin scanner
cp $SYS/lib/gstreamer-1.0/*.dll "$D/lib/gstreamer-1.0/"
cp $P/lib/gstreamer-1.0/*.dll "$D/lib/gstreamer-1.0/"   # opus and flac: Fedora's mingw GStreamer has neither
cp $SYS/libexec/gstreamer-1.0/gst-plugin-scanner.exe "$D/libexec/gstreamer-1.0/" 2>/dev/null || true
copy_deps "$D/bin/radiance-music.exe" "$D"/lib/gstreamer-1.0/*.dll "$D"/libexec/gstreamer-1.0/*.exe
# GIO: its TLS module (https in GStreamer's downloads) and proxy modules
mkdir -p "$D/lib/gio/modules" && cp $SYS/lib/gio/modules/*.dll "$D/lib/gio/modules/" && copy_deps "$D"/lib/gio/modules/*.dll
# GTK: settings schemas (compiled here), icon themes, pixbuf loaders
cp $SYS/share/glib-2.0/schemas/*.xml "$D/share/glib-2.0/schemas/" && glib-compile-schemas "$D/share/glib-2.0/schemas/"
cp -r $SYS/share/icons/Adwaita $SYS/share/icons/hicolor "$D/share/icons/" 2>/dev/null
[ -d $SYS/lib/gdk-pixbuf-2.0 ] && { mkdir -p "$D/lib"; cp -r $SYS/lib/gdk-pixbuf-2.0 "$D/lib/"; copy_deps "$D"/lib/gdk-pixbuf-2.0/*/loaders/*.dll; }
# the tools the app runs: yt-dlp, Deno (YouTube's challenges), FFmpeg (built below)
cd "$T"
[ -f yt-dlp.exe ] || curl -sSLo yt-dlp.exe https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe
[ -f deno.zip ] || curl -sSLo deno.zip https://github.com/denoland/deno/releases/download/v2.9.8/deno-x86_64-pc-windows-msvc.zip
unzip -oq deno.zip -d "$T" && cp "$T/deno.exe" yt-dlp.exe "$D/bin/"
if [ ! -f "$T/ffmpeg.exe" ]; then
  rm -rf ffmpeg-9.0.2 && tar xf /srv/build/tmp/fp-src/ffmpeg-9.0.2.tar.xz && cd ffmpeg-9.0.2
  ./configure --target-os=mingw32 --arch=x86_64 --cross-prefix=x86_64-w64-mingw32- --pkg-config=x86_64-w64-mingw32-pkg-config \
    --enable-static --disable-shared --disable-x86asm --disable-doc --disable-debug --disable-autodetect --disable-everything \
    --enable-libopus --enable-protocol=file,pipe --enable-demuxer=matroska,mov,ogg,aac,mp3,flac,wav \
    --enable-muxer=matroska,webm,mp4,ipod,ogg,opus,flac,mp3,wav --enable-decoder=aac,opus,libopus,vorbis,mp3,flac,pcm_s16le \
    --enable-encoder=libopus,aac,flac,pcm_s16le --enable-parser=aac,opus,vorbis,flac,mpegaudio,h264,hevc,vp8,vp9,av1 \
    --enable-bsf=aac_adtstoasc,extract_extradata,h264_mp4toannexb,vp9_superframe --enable-filter=aresample,aformat,anull,volume,null,format,copy \
    --enable-swresample >/dev/null 2>&1 && make -j4 >/dev/null 2>&1 && cp ffmpeg.exe ffprobe.exe "$T/"
  cd "$T"
fi
cp "$T/ffmpeg.exe" "$T/ffprobe.exe" "$D/bin/" 2>/dev/null
copy_deps "$D/bin/ffmpeg.exe" "$D/bin/ffprobe.exe"
cp /srv/build/src/radiance-music/LICENSE /srv/build/src/radiance-music/README.md /srv/build/src/radiance-music/CHANGELOG.md "$D/"
echo "files: $(find "$D" -type f | wc -l), dlls in bin: $(ls "$D"/bin/*.dll | wc -l), size: $(du -sh "$D" | cut -f1)"
ls "$D/bin" | grep -E "\.exe$"
