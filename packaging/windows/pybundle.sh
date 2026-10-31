#!/bin/bash
# On ribs02 as radmin: the Windows Python for splitting (embeddable 3.14 + the same packages the
# Linux setup installs, as Windows wheels) and the models, under /srv/build/win.
set -uo pipefail
export TMPDIR=/srv/build/win/tmp PIP_CACHE_DIR=/srv/build/win/pip-cache
B=/srv/build/win/pybundle; M=/srv/build/win/models
PYV=3.14.4
cd /srv/build/win/src
[ -f python-$PYV-embed-amd64.zip ] || curl -sSLO https://www.python.org/ftp/python/$PYV/python-$PYV-embed-amd64.zip
rm -rf $B && mkdir -p $B && unzip -q python-$PYV-embed-amd64.zip -d $B || exit 1
# let it find site-packages (the embeddable's ._pth lists its own paths only)
pth=$(ls $B/python*._pth); printf 'Lib\\site-packages\nimport site\n' >> "$pth"; cat "$pth"
[ -x /srv/build/win/pipvenv/bin/pip ] || python3 -m venv /srv/build/win/pipvenv
P=/srv/build/win/pipvenv/bin/pip
$P install -q --upgrade pip 2>&1 | tail -1
# exactly the Linux splitter's packages (pip freeze there) + colorama (Windows-only), with no
# dependency solving: pip would read torch's Linux-only markers against this Linux machine
$P install -q --target $B/Lib/site-packages --platform win_amd64 --python-version 3.14 --implementation cp --abi cp314 --only-binary=:all: \
  --no-deps --index-url https://download.pytorch.org/whl/cpu --extra-index-url https://pypi.org/simple -r /srv/build/win/win-reqs.txt 2>&1 | tail -6
ls $B/Lib/site-packages | grep -iE "^(torch|torchaudio|demucs|soundfile|av|numpy)" | head -12
du -sh $B
# the models, as the Linux setup leaves them: Demucs (Hugging Face cache) and MMS (torch hub)
mkdir -p $M/huggingface/hub $M/torch/hub/checkpoints
cp -r ~/.cache/huggingface/hub/models--adefossez--HTDemucs $M/huggingface/hub/
cp ~/.cache/torch/hub/checkpoints/model.pt $M/torch/hub/checkpoints/
du -sh $M
