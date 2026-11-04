#!/bin/bash
# ribs03 (MSYS2 UCRT64, through run.ps1 at idle priority): Radiance Music's release build (#39).
# The source comes from the laptop as C:\rb\src.tgz (unpacked fresh into C:\rb\src\radiance-music).
set -uo pipefail
S=/c/rb/src/radiance-music
if [ -f /c/rb/src.tgz ]; then
  rm -rf "$S" && mkdir -p "$S" && tar -xzf /c/rb/src.tgz -C "$S" && rm /c/rb/src.tgz
fi
cd "$S" || exit 1
export CARGO_TARGET_DIR=/c/rb/target
cargo build --release "$@" > /c/rb/build.log 2>&1
code=$?
grep -E "^(error|warning)" -A8 /c/rb/build.log | head -80
tail -3 /c/rb/build.log
echo "exit=$code"
exit $code
