#!/bin/bash
# MSYS2 UCRT64: Radiance Music's Windows release build (#39). On ribs03 (through run.ps1 at idle
# priority) the source comes from the laptop as C:\rb\src.tgz, unpacked fresh; on GitHub's build
# machine (.github/workflows/windows.yml) RB and SRC point into the checkout.
set -uo pipefail
RB=${RB:-/c/rb}
S=${SRC:-$RB/src/radiance-music}
if [ -f "$RB/src.tgz" ] && [ -z "${SRC:-}" ]; then
  rm -rf "$S" && mkdir -p "$S" && tar -xzf "$RB/src.tgz" -C "$S" && rm "$RB/src.tgz"
fi
cd "$S" || exit 1
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$RB/target}
mkdir -p "$RB"
cargo build --release "$@" > "$RB/build.log" 2>&1
code=$?
grep -E "^(error|warning)" -A8 "$RB/build.log" | head -80
tail -3 "$RB/build.log"
echo "exit=$code"
exit $code
