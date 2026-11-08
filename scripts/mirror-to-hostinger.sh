#!/bin/bash
# Mirror a repo's newest GitHub release onto downloads.radianceph.com (owner 2026-11-08, #71): its files under
# /<repo>/<tag>/, and latest.json = GitHub's release info with each file's address pointing here.
# Usage: mirror.sh REPO   (e.g. radiance-music)
set -euo pipefail
repo=$1; base=https://downloads.radianceph.com
H="ssh -o BatchMode=yes -o IdentitiesOnly=yes -o ConnectTimeout=15 -i $HOME/.ssh/hostinger_ed25519.pub -p 65002 u302998808@151.106.114.104"
R=domains/radianceph.com/public_html/downloads
work=$HOME/Downloads/rm-mirror/$repo; rm -rf "$work"; mkdir -p "$work"
gh api repos/RadianceADM/$repo/releases/latest > "$work/github.json"
tag=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["tag_name"])' "$work/github.json")
gh release download "$tag" -R RadianceADM/$repo -D "$work/files" --clobber
python3 - "$work/github.json" "$base/$repo/$tag" > "$work/latest.json" <<'PY'
import json, sys
r = json.load(open(sys.argv[1]))
keep = {k: r[k] for k in ("tag_name", "name", "body", "html_url", "published_at", "prerelease") if k in r}
keep["assets"] = [{"name": a["name"], "size": a["size"], "digest": a.get("digest"), "browser_download_url": f"{sys.argv[2]}/{a['name']}"} for a in r["assets"]]
print(json.dumps(keep, indent=1, ensure_ascii=False))
PY
$H "mkdir -p $R/$repo/$tag"
scp -q -o BatchMode=yes -o IdentitiesOnly=yes -i $HOME/.ssh/hostinger_ed25519.pub -P 65002 "$work"/files/* "u302998808@151.106.114.104:$R/$repo/$tag/"
# every file checked against GitHub's fingerprint BEFORE latest.json points at it (a bad upload is never served)
echo "== $repo $tag: GitHub's fingerprints vs Hostinger's copies"
python3 -c 'import json,sys; [print(a["digest"].split(":")[1] + "  " + a["name"]) for a in json.load(open(sys.argv[1]))["assets"]]' "$work/latest.json" | sort > "$work/want.txt"
$H "cd $R/$repo/$tag && sha256sum *" | sort > "$work/have.txt"
if ! diff "$work/want.txt" "$work/have.txt"; then echo "MISMATCH: latest.json not switched"; exit 1; fi
echo "all $(wc -l < "$work/want.txt") files match"
scp -q -o BatchMode=yes -o IdentitiesOnly=yes -i $HOME/.ssh/hostinger_ed25519.pub -P 65002 "$work/latest.json" "u302998808@151.106.114.104:$R/$repo/latest.json.new"
$H "mv -f $R/$repo/latest.json.new $R/$repo/latest.json && ln -sfn $tag $R/$repo/latest" && echo "latest.json and /$repo/latest/ now: $tag"
# Hostinger's CDN may hold the old /latest/ files: cleared (they're kept 5 minutes at most anyway)
hostinger hosting cdn purge-cache u302998808 radianceph.com >/dev/null 2>&1 && echo "CDN cleared" || echo "CDN not cleared (it expires within 5 minutes)"
rm -rf "$work"
