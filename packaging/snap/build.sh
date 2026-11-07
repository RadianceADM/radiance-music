#!/bin/bash
# Builds Radiance Music's snap. On ribs02 as radmin: the source (default
# /srv/build/src/radiance-music) goes into the Ubuntu 26.04 VM (/srv/data/vm/ubuntu26,
# snapcraft installed there), snapcraft packs it there (--destructive-mode: the VM is
# for this only), and the .snap comes back to /srv/build/snap. The recipe is snap/snapcraft.yaml (where
# Snapcraft's own build service finds it too: every push to main builds there and goes to edge, #35).
#
#   packaging/snap/build.sh [source folder]
set -euo pipefail
VM=/srv/data/vm/ubuntu26/vm.sh
SRC=${1:-/srv/build/src/radiance-music}
OUT=/srv/build/snap
mkdir -p "$OUT"
cd /tmp # (runuser keeps root's folder otherwise)
"$VM" start >/dev/null
"$VM" ssh 'sudo rm -rf ~/rm && mkdir -p ~/rm' # (the last pack ran as root)
tar -C "$SRC" --exclude=./target --exclude=./.git --exclude=./packaging/flatpak/out -cf - . | "$VM" ssh 'tar -C ~/rm -xf -'
# destructive mode installs the build packages itself: as root, with fresh package lists
"$VM" ssh 'sudo apt-get update -q >/dev/null && cd ~/rm && sudo snapcraft pack --destructive-mode > ~/pack.log 2>&1; code=$?; tail -15 ~/pack.log; exit $code'
name=$("$VM" ssh 'cd ~/rm && ls *.snap' | tail -1)
"$VM" ssh "cat ~/rm/$name" > "$OUT/$name"
"$VM" ssh "sha256sum ~/rm/$name" | cut -d' ' -f1 > "$OUT/$name.sha256.vm"
echo "$(sha256sum "$OUT/$name" | cut -d' ' -f1)  $OUT/$name ($(du -h "$OUT/$name" | cut -f1))"
cmp -s <(cut -d' ' -f1 "$OUT/$name.sha256.vm") <(sha256sum "$OUT/$name" | cut -d' ' -f1) && echo "copied whole" || { echo "COPY DIFFERS"; exit 1; }
