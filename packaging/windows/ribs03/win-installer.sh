#!/bin/bash
# MSYS2 UCRT64 (ribs03, or GitHub's build machine): RadianceMusic-Setup.exe from the packed folder (#39).
set -uo pipefail
RB=${RB:-/c/rb}
SRC=${SRC:-$RB/src/radiance-music}
D=${DIST:-$RB/dist/RadianceMusic}
OUT=${SETUP:-$RB/RadianceMusic-Setup.exe}
v=$(grep -m1 '^version' "$SRC/Cargo.toml" | cut -d'"' -f2)
cd "$SRC/packaging/windows" && makensis -V2 -DVERSION=$v "-DDIST=$(cygpath -w "$D")" "-DOUT=$(cygpath -w "$OUT")" installer.nsi | tail -3
ls -la "$OUT" && sha256sum "$OUT"
