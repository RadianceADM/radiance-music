#!/bin/bash
# MSYS2 UCRT64 (ribs03, or GitHub's build machine): the tools the Windows app runs, into $TOOLS, each
# checked: yt-dlp (the newest, every time: YouTube changes; its release's SHA2-256SUMS), Deno 2.9.8
# (pinned checksum), and FFmpeg built here from its source (pinned checksum), small and static: only
# what yt-dlp and the app need. Deno and FFmpeg are kept once there.
set -euo pipefail
RB=${RB:-/c/rb}
T=${TOOLS:-$RB/tools}
W=${WORK:-$RB/work}
mkdir -p "$T" "$W"
check() { echo "$2  $1" | sha256sum -c --quiet - || { echo "$1: checksum differs"; exit 1; }; }
get() { curl -sSfL --retry 4 --retry-all-errors --retry-delay 5 -o "$1" "$2"; }

if true; then
  get "$W/yt-dlp.exe" https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe
  get "$W/SHA2-256SUMS" https://github.com/yt-dlp/yt-dlp/releases/latest/download/SHA2-256SUMS
  check "$W/yt-dlp.exe" "$(awk '$2 == "yt-dlp.exe" {print $1}' "$W/SHA2-256SUMS")"
  mv "$W/yt-dlp.exe" "$T/"
fi

if [ ! -f "$T/deno.exe" ]; then
  get "$W/deno.zip" https://github.com/denoland/deno/releases/download/v2.9.8/deno-x86_64-pc-windows-msvc.zip
  check "$W/deno.zip" cba84b26e0df5ae7f74fc66b86a95b866872a86292ebd84fa1e959927edd93bf
  unzip -oq "$W/deno.zip" -d "$T"
fi

if [ ! -f "$T/ffmpeg.exe" ] || [ ! -f "$T/ffprobe.exe" ]; then
  F=ffmpeg-9.0.2
  rm -rf "$W/$F"
  # FFmpeg's own release, else (ffmpeg.org refusing connections, 2026-11-04) the same tag from its
  # GitHub mirror; each with its own pinned checksum
  if get "$W/$F.tar.xz" https://ffmpeg.org/releases/$F.tar.xz; then
    check "$W/$F.tar.xz" 8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e
    tar -xf "$W/$F.tar.xz" -C "$W"
  else
    get "$W/n9.0.2.tar.gz" https://github.com/FFmpeg/FFmpeg/archive/refs/tags/n9.0.2.tar.gz
    check "$W/n9.0.2.tar.gz" 6e374ed621e48faa40639307dff48ba6fe574a509977956d2cce9669b7cc27e9
    tar -xzf "$W/n9.0.2.tar.gz" -C "$W" && mv "$W/FFmpeg-n9.0.2" "$W/$F"
  fi
  cd "$W/$F"
  # the same choice of parts as ribs02's cross build (win-pack.sh), linked statically
  ./configure --enable-static --disable-shared --disable-x86asm --disable-doc --disable-debug --disable-autodetect \
    --disable-everything --enable-libopus --pkg-config-flags=--static --extra-ldflags=-static \
    --enable-protocol=file,pipe --enable-demuxer=matroska,mov,ogg,aac,mp3,flac,wav \
    --enable-muxer=matroska,webm,mp4,ipod,ogg,opus,flac,mp3,wav --enable-decoder=aac,opus,libopus,vorbis,mp3,flac,pcm_s16le \
    --enable-encoder=libopus,aac,flac,pcm_s16le --enable-parser=aac,opus,vorbis,flac,mpegaudio,h264,hevc,vp8,vp9,av1 \
    --enable-bsf=aac_adtstoasc,extract_extradata,h264_mp4toannexb,vp9_superframe --enable-filter=aresample,aformat,anull,volume,null,format,copy \
    --enable-swresample > "$W/ffmpeg-configure.log" 2>&1 || { tail -20 "$W/ffmpeg-configure.log"; exit 1; }
  make -j"$(nproc)" > "$W/ffmpeg-make.log" 2>&1 || { tail -20 "$W/ffmpeg-make.log"; exit 1; }
  cp ffmpeg.exe ffprobe.exe "$T/"
fi
ls -la "$T"
"$T/ffmpeg.exe" -hide_banner -version | head -1
