#!/bin/bash
# Builds Radiance Music's own Flatpak (not for Flathub) into packaging/flatpak/out/repo.
# Run from anywhere: needs flatpak-builder and the GNOME 49 runtime and SDK with
# the rust-stable extension.
#
# The download people install, RadianceMusic.flatpak, comes from publish.sh, not from here:
# it is made from our signed Flatpak repo, so it carries the repo's address and key and the
# exact signed commit (owner, 2026-10-31: the Flatpak updates the "official" way). What this
# leaves in out/ is a local test bundle (RadianceMusic-local.flatpak) that never updates.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
work=${FLATPAK_WORK:-$here/out}
mkdir -p "$work"
flatpak-builder --user --force-clean --disable-rofiles-fuse --state-dir="$work/state" --repo="$work/repo" "$work/app" "$here/io.github.radianceadm.Music.yml"
flatpak build-bundle --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo "$work/repo" "$work/RadianceMusic-local.flatpak" io.github.radianceadm.Music
ls -la "$work/RadianceMusic-local.flatpak"
echo "For a release: publish.sh (the signed repo and RadianceMusic.flatpak), then push.sh."
