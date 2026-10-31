#!/bin/bash
# Sends the Flatpak repo (FLATPAK_PAGES, made by publish.sh) to GitHub Pages:
# one commit with only what's there now (old versions don't pile up in git).
# A release step: run it only with the owner's OK for the release.
set -euo pipefail
pages=${FLATPAK_PAGES:?a clone of RadianceADM/radiance-music-flatpak}
cd "$pages"
git checkout -q --orphan next
git add -A
git -c user.name=Bassi -c user.email=villenabassi@gmail.com commit -q -m "Radiance Music Flatpak repo: $(ostree log --repo=. app/io.github.radianceadm.Music/x86_64/master | grep -m1 -o 'Version: .*' || date -u +%F)"
git branch -M next gh-pages
git push -q --force origin gh-pages
git log --oneline -1
