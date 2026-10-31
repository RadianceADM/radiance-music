#!/bin/bash
# Puts the Flatpak build.sh just made into our Flatpak repo (the folder served
# by GitHub Pages from RadianceADM/radiance-music-flatpak), signed with the
# repo's key, so every installed copy can update to it, and makes the release's
# RadianceMusic.flatpak from it: the same signed commit, with the repo's address
# and key (Flatpak won't install a bundle carrying a key unless its commit is
# signed with it). Keeps the newest few versions (GitHub Pages: 1 GB at most).
#
#   FLATPAK_GPG_HOME  the key's GnuPG folder (ribs02, radmin only; never in git)
#   FLATPAK_GPG_KEY   its key id
#   FLATPAK_PAGES     a clone of RadianceADM/radiance-music-flatpak (the repo is its top folder)
#
# It only updates FLATPAK_PAGES; sending it to GitHub is a release step (push.sh).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
work=${FLATPAK_WORK:-$here/out}
pages=${FLATPAK_PAGES:?a clone of RadianceADM/radiance-music-flatpak}
gpg=(--gpg-sign="${FLATPAK_GPG_KEY:?the repo key id}" --gpg-homedir="${FLATPAK_GPG_HOME:?the repo key folder}")
repo_url=${FLATPAK_REPO_URL:-https://radianceadm.github.io/radiance-music-flatpak/}
public_key=${FLATPAK_GPG_PUBLIC:-$here/radiance-music-flatpak.gpg} # (another for tests)
ref=app/io.github.radianceadm.Music/x86_64/master

[ -f "$pages/config" ] || ostree init --mode=archive-z2 --repo="$pages"
# the new build, as a signed commit of its own on top of the ones there
flatpak build-commit-from "${gpg[@]}" --src-repo="$work/repo" "$pages" "$ref"
# the summary (what's there), deltas for quick updates, the old versions pruned
flatpak build-update-repo "${gpg[@]}" --title="Radiance Music" --generate-static-deltas --prune --prune-depth=3 "$pages"

# for people adding the repo by hand: flatpak remote-add radiance-music <url>radiance-music.flatpakrepo
{
  echo "[Flatpak Repo]"
  echo "Title=Radiance Music"
  echo "Url=$repo_url"
  echo "Homepage=https://music.radianceph.com"
  echo "Comment=Radiance Music, an open beta: updates for its own Flatpak"
  echo "GPGKey=$(base64 -w0 "$public_key")"
} > "$pages/radiance-music.flatpakrepo"
touch "$pages/.nojekyll" # served as plain files
cat > "$pages/index.html" <<'HTML'
<!doctype html><meta charset="utf-8"><title>Radiance Music: Flatpak updates</title>
<p>This is where the Radiance Music Flatpak gets its updates. To install the app, go to
<a href="https://music.radianceph.com">music.radianceph.com</a>.</p>
HTML
# the download: installs it and adds our repo (with its key) for its updates
flatpak build-bundle "${gpg[@]}" --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo \
  --repo-url="$repo_url" --gpg-keys="$public_key" \
  "$pages" "$work/RadianceMusic.flatpak" io.github.radianceadm.Music
ls -la "$work/RadianceMusic.flatpak"
du -sh "$pages" --exclude=.git
ostree log --repo="$pages" "$ref" | head -20
