#!/bin/bash
# Builds RadianceMusic.flatpak (the app's own Flatpak download; not for Flathub).
# Run from anywhere: needs flatpak-builder and the GNOME 49 runtime and SDK with
# the rust-stable extension. The bundle lands in packaging/flatpak/out/.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
work=${FLATPAK_WORK:-$here/out}
mkdir -p "$work"
flatpak-builder --user --force-clean --disable-rofiles-fuse --state-dir="$work/state" --repo="$work/repo" "$work/app" "$here/io.github.radianceadm.Music.yml"
flatpak build-bundle --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo "$work/repo" "$work/RadianceMusic.flatpak" io.github.radianceadm.Music
ls -la "$work/RadianceMusic.flatpak"
