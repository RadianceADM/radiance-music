#!/bin/bash
# The Windows beta's MSYS2 UCRT64 side (called by build.ps1 on ribs03, at Idle priority):
#   msys.sh rust     the core/engine/ffi tests, then radiance_ffi.dll
#   msys.sh deps     what radiance_ffi.dll needs (GStreamer, GLib, the C runtime)
#   msys.sh bindgen  uniffi-bindgen-cs (once) and the C# binding
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
  *) echo "usage: msys.sh rust|deps|bindgen"; exit 2 ;;
esac
