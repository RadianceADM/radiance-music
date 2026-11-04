#!/bin/bash
# ribs03 (MSYS2 UCRT64): RadianceMusic-Setup.exe from the packed folder (#39).
set -uo pipefail
S=/c/rb/src/radiance-music/packaging/windows
v=$(grep -m1 '^version' /c/rb/src/radiance-music/Cargo.toml | cut -d'"' -f2)
OUT=C:\\rb\\RadianceMusic-Setup.exe
cd "$S" && makensis -V2 -DVERSION=$v "-DDIST=C:\\rb\\dist\\RadianceMusic" "-DOUT=$OUT" installer.nsi | tail -3
ls -la /c/rb/RadianceMusic-Setup.exe && sha256sum /c/rb/RadianceMusic-Setup.exe
