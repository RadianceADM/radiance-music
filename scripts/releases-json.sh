#!/bin/bash
# Every release of a repo, as GitHub's own release JSON array (newest first, all pages), published at
# downloads.radianceph.com/<repo>/releases.json. Each file's browser_download_url points at the copy on
# downloads.radianceph.com when that file is mirrored there (/<repo>/<tag>/<name>), else stays GitHub's.
# For the admin's downloads matrix, which must not depend on GitHub once the app repos are private (radianceph#72).
# Usage: releases-json.sh REPO   (e.g. radiance-music). Uses the laptop's one shared Hostinger session (hs).
set -euo pipefail
repo=$1; base=https://downloads.radianceph.com
hs=$HOME/.local/share/radiance-ops/hs
R=domains/radianceph.com/public_html/downloads
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
gh api --paginate "repos/RadianceADM/$repo/releases?per_page=100" | python3 -I -c 'import json,sys
d=json.JSONDecoder(); s=sys.stdin.read(); i=0; out=[]
while i < len(s):
    while i < len(s) and s[i].isspace(): i += 1
    if i >= len(s): break
    v, i = d.raw_decode(s, i); out += v
json.dump(out, sys.stdout)' > "$work/github.json"
# what downloads.radianceph.com holds for this repo (finished files only)
$hs "cd $R/$repo 2>/dev/null && find . -mindepth 2 -type f ! -name '*.part' ! -name '*.dl' ! -name '.*' -printf '%P\n'" > "$work/have.txt" || true
python3 -I - "$work/github.json" "$work/have.txt" "$base/$repo" > "$work/releases.json" <<'PY'
import json, sys
releases = json.load(open(sys.argv[1]))
have = set(open(sys.argv[2]).read().split())
for r in releases:
    for a in r.get("assets", []):
        if f"{r['tag_name']}/{a['name']}" in have:
            a["browser_download_url"] = f"{sys.argv[3]}/{r['tag_name']}/{a['name']}"
json.dump(releases, sys.stdout, indent=1, ensure_ascii=False)
PY
here=$(python3 -I -c 'import json,sys; r=json.load(open(sys.argv[1])); print(sum(1 for x in r for a in x["assets"] if a["browser_download_url"].startswith("https://downloads.")), "of", sum(len(x["assets"]) for x in r), "files mirrored,", len(r), "releases")' "$work/releases.json")
$hs "cat > $R/$repo/releases.json.new && mv -f $R/$repo/releases.json.new $R/$repo/releases.json" < "$work/releases.json"
echo "$base/$repo/releases.json: $here"
