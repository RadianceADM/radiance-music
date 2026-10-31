#!/usr/bin/env bash
# Radiance Music Server: setting it up on a server without Docker
# (Debian, Ubuntu or Fedora; a VPS or a PC at home).
#
#   git clone https://github.com/RadianceADM/radiance-music
#   sudo bash radiance-music/rms/install.sh
#
# With a domain pointed at this server (an A record), for https through Caddy:
#   sudo RMS_DOMAIN=music.example.com bash radiance-music/rms/install.sh
#
# Other settings, the same way: RMS_PORT (8080: where it listens without a
# domain), RMS_DATA (/var/lib/radiance-music-server: the library, the
# database and the models), RMS_SETUP_KEY (the key the setup page asks for:
# it asks you for one, or the server makes one and this shows it at the end),
# RMS_BINARY (an rms program built elsewhere: skips building it here).
#
# Run it again to update (after a `git pull`): your library and people stay.
set -euo pipefail

say() { printf '\n\033[1;33m==>\033[0m %s\n' "$*"; }
die() { printf '\n\033[1;31mStopped:\033[0m %s\n' "$*" >&2; exit 1; }

[ "$(id -u)" = 0 ] || die "Run it as root: sudo bash $0"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(dirname "$here")"
[ -f "$here/Cargo.toml" ] && [ -f "$repo/server/split.py" ] && [ -f "$repo/server/align.py" ] \
  || die "Run it from a copy of the repository (rms/install.sh, with server/split.py beside rms/)."

DOMAIN="${RMS_DOMAIN:-}"
DOMAIN="${DOMAIN#https://}"; DOMAIN="${DOMAIN#http://}"; DOMAIN="${DOMAIN%/}"
PORT="${RMS_PORT:-8080}"
DATA="${RMS_DATA:-/var/lib/radiance-music-server}"
OPT=/opt/rms
UNIT=radiance-music-server
ENV_FILE=/etc/radiance-music-server.env

# shellcheck disable=SC1091
. /etc/os-release
case " ${ID:-} ${ID_LIKE:-} " in
  *" debian "*|*" ubuntu "*) family=debian ;;
  *" fedora "*|*" rhel "*|*" centos "*) family=fedora ;;
  *) die "This script knows Debian, Ubuntu and Fedora. On another system, follow the README's steps by hand." ;;
esac

# the setup key (owner, 2026-11-01: required): typed here, or made by the server at its first start
KEY="${RMS_SETUP_KEY:-}"
if [ -z "$KEY" ] && [ -f "$ENV_FILE" ]; then
  KEY=$(sed -n 's/^RMS_SETUP_KEY=//p' "$ENV_FILE" | head -n 1)
fi
if [ -z "$KEY" ] && [ ! -s "$DATA/server.db" ] && [ -t 0 ]; then
  echo "The setup page asks for a setup key, so nobody else can claim your new server before you open it."
  read -r -p "Type a setup key (letters, digits, dashes), or press Enter and the server makes one: " KEY
fi
case "$KEY" in
  *[!A-Za-z0-9._-]*) die "Use only letters, digits, dashes, dots or underscores in the setup key." ;;
esac

# what splitting needs: about 2.7 GB of memory while it works
mem_gb=$(awk '/MemTotal/ {printf "%d", $2/1024/1024}' /proc/meminfo)
if [ "$mem_gb" -lt 4 ]; then
  echo "Note: this server has about ${mem_gb} GB of memory. Splitting needs about 2.7 GB while it works; 4 GB or more is best."
fi

say "Installing what it needs (Python, ffmpeg, and a C compiler to build the server)"
if [ "$family" = debian ]; then
  export DEBIAN_FRONTEND=noninteractive
  apt-get update -q
  apt-get install -y -q python3 python3-venv ffmpeg ca-certificates curl gcc libc6-dev
else
  # Fedora's own ffmpeg is "ffmpeg-free"
  dnf install -y -q python3 ffmpeg-free ca-certificates curl gcc glibc-devel
fi

# the server program: built here with Rust, kept in /opt/rms (not in root's home)
if [ -n "${RMS_BINARY:-}" ]; then
  [ -x "$RMS_BINARY" ] || die "RMS_BINARY ($RMS_BINARY) isn't a program."
  install -m 755 "$RMS_BINARY" /usr/local/bin/rms
else
  say "Building the server (Rust; 5 to 15 minutes the first time on a small server)"
  export RUSTUP_HOME="$OPT/rustup" CARGO_HOME="$OPT/cargo"
  if [ ! -x "$CARGO_HOME/bin/cargo" ]; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y -q --profile minimal --no-modify-path
  fi
  "$CARGO_HOME/bin/rustup" update -q stable >/dev/null 2>&1 || true
  nice -n 10 "$CARGO_HOME/bin/cargo" build --release --locked -q --manifest-path "$here/Cargo.toml" --target-dir "$OPT/build"
  install -m 755 "$OPT/build/release/rms" /usr/local/bin/rms
fi

# the splitter: the app's own pip list (setup.rs install()), in its own place
say "Setting up the splitter (PyTorch for the CPU and Demucs: about 1 GB to download)"
[ -x "$OPT/venv/bin/python" ] || python3 -m venv "$OPT/venv"
"$OPT/venv/bin/pip" install -q --upgrade pip
# the set the Docker image is tested with, where this Python can have it (3.11 or newer)
if python3 -c 'import sys; sys.exit(0 if sys.version_info >= (3, 11) else 1)'; then
  torch_set="torch==2.15.0+cpu torchaudio==2.11.1+cpu"; demucs_set="demucs==4.1.0"
else
  torch_set="torch torchaudio"; demucs_set="demucs"
  echo "Note: $(python3 -V) is older than 3.11, so it gets the newest PyTorch that suits it (the tested set needs 3.11)."
fi
# shellcheck disable=SC2086
"$OPT/venv/bin/pip" install -q --progress-bar off --only-binary=:all: \
  --index-url https://download.pytorch.org/whl/cpu --extra-index-url https://pypi.org/simple \
  $torch_set soundfile av numpy pyyaml
"$OPT/venv/bin/pip" install -q --only-binary=:all: "$demucs_set"
"$OPT/venv/bin/python" -c "import torch, torchaudio, demucs, soundfile, av; print('The splitter is in: PyTorch', torch.__version__)"
install -d "$OPT/scripts"
install -m 644 "$repo/server/split.py" "$repo/server/align.py" "$OPT/scripts/"

say "Making its user and its data folder ($DATA)"
id rms >/dev/null 2>&1 || useradd --system --home-dir "$DATA" --shell /usr/sbin/nologin rms
install -d -o rms -g rms "$DATA"

# where it listens: behind Caddy with a domain, else on every address at $PORT
if [ -n "$DOMAIN" ]; then bind="127.0.0.1:8080"; else bind="0.0.0.0:$PORT"; fi
{
  echo "# Radiance Music Server's settings (install.sh writes them; see the README)"
  echo "RMS_DATA=$DATA"
  echo "RMS_BIND=$bind"
  echo "RMS_PYTHON=$OPT/venv/bin/python"
  echo "RMS_SCRIPTS=$OPT/scripts"
  [ -n "$DOMAIN" ] && echo "RMS_DOMAIN=$DOMAIN"
  [ -n "$KEY" ] && echo "RMS_SETUP_KEY=$KEY"
  [ -n "${RMS_THREADS:-}" ] && echo "RMS_THREADS=$RMS_THREADS"
  true
} > "$ENV_FILE"
chmod 640 "$ENV_FILE"
chgrp rms "$ENV_FILE"

caps=""
if [ -z "$DOMAIN" ] && [ "$PORT" -lt 1024 ]; then caps="AmbientCapabilities=CAP_NET_BIND_SERVICE"; fi
cat > "/etc/systemd/system/$UNIT.service" <<EOF
[Unit]
Description=Radiance Music Server
After=network-online.target
Wants=network-online.target

[Service]
User=rms
Group=rms
EnvironmentFile=$ENV_FILE
ExecStart=/usr/local/bin/rms
Restart=on-failure
RestartSec=5
$caps
# the splitter runs at the lowest priority by itself (nice 19); the server stays quick
NoNewPrivileges=true
ProtectSystem=full
ProtectHome=true
PrivateTmp=true
ReadWritePaths=$DATA

[Install]
WantedBy=multi-user.target
EOF

if [ -n "$DOMAIN" ]; then
  say "Setting up Caddy for https ($DOMAIN)"
  if ! command -v caddy >/dev/null 2>&1; then
    if [ "$family" = debian ]; then
      if ! apt-get install -y -q caddy; then
        # older releases: Caddy's own package source
        apt-get install -y -q debian-keyring debian-archive-keyring apt-transport-https gnupg
        curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | gpg --dearmor --yes -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
        curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' > /etc/apt/sources.list.d/caddy-stable.list
        apt-get update -q
        apt-get install -y -q caddy
      fi
    else
      dnf install -y -q caddy
    fi
  fi
  printf '%s {\n\treverse_proxy 127.0.0.1:8080\n}\n' "$DOMAIN" > /etc/caddy/radiance-music-server.caddy
  # the Caddyfile imports ours (a Caddyfile that's still the package's sample is replaced, kept as .orig)
  if [ ! -s /etc/caddy/Caddyfile ] || grep -q '/usr/share/caddy' /etc/caddy/Caddyfile; then
    [ -s /etc/caddy/Caddyfile ] && cp -n /etc/caddy/Caddyfile /etc/caddy/Caddyfile.orig
    echo "import /etc/caddy/radiance-music-server.caddy" > /etc/caddy/Caddyfile
  elif ! grep -q 'radiance-music-server.caddy' /etc/caddy/Caddyfile; then
    printf '\nimport /etc/caddy/radiance-music-server.caddy\n' >> /etc/caddy/Caddyfile
  fi
  # Fedora (SELinux): Caddy may pass requests on to the server
  if command -v getenforce >/dev/null 2>&1 && [ "$(getenforce)" = Enforcing ]; then
    setsebool -P httpd_can_network_connect 1 || true
  fi
  systemctl enable -q --now caddy
  systemctl reload caddy || systemctl restart caddy
fi

# the firewall, if one is on: http/https (a domain) or the server's port
if command -v ufw >/dev/null 2>&1 && ufw status | grep -q "Status: active"; then
  if [ -n "$DOMAIN" ]; then ufw allow 80/tcp >/dev/null; ufw allow 443/tcp >/dev/null; else ufw allow "$PORT/tcp" >/dev/null; fi
  echo "Opened the firewall (ufw)."
elif command -v firewall-cmd >/dev/null 2>&1 && firewall-cmd --state >/dev/null 2>&1; then
  if [ -n "$DOMAIN" ]; then firewall-cmd -q --permanent --add-service=http --add-service=https; else firewall-cmd -q --permanent --add-port="$PORT/tcp"; fi
  firewall-cmd -q --reload
  echo "Opened the firewall (firewalld)."
fi

say "Starting it"
systemctl daemon-reload
systemctl enable -q "$UNIT"
systemctl restart "$UNIT"
up=no
for _ in $(seq 1 30); do
  if curl -fsS "http://${bind/0.0.0.0/127.0.0.1}/api/status" >/dev/null 2>&1; then up=yes; break; fi
  sleep 1
done
[ "$up" = yes ] || die "It didn't start. See what it said: journalctl -u $UNIT -n 50"

ip=$(hostname -I 2>/dev/null | awk '{print $1}')
if curl -fsS "http://${bind/0.0.0.0/127.0.0.1}/api/status" 2>/dev/null | grep -q '"needs_setup":false'; then
  key_line="It's set up already: open it to manage it."
elif [ -n "$KEY" ]; then
  key_line="The setup key is the one you typed."
elif [ -s "$DATA/setup-key.txt" ]; then
  key_line="The setup key: $(cat "$DATA/setup-key.txt")  (also in $DATA/setup-key.txt)"
else
  key_line="The setup key is set up already, or see: journalctl -u $UNIT | grep 'setup key'"
fi
if [ -n "$DOMAIN" ]; then address="https://$DOMAIN"; else address="http://${ip:-this-server}:$PORT"; fi
cat <<EOF

Radiance Music Server is running.

  Open $address in a browser, type your name and the setup key: that sets
  it up and gives you the link for your own phone and computer.
  $key_line

  The splitting models download now (about 1.3 GB, once); the page shows how far along.
  Your library, the database and the models: $DATA
  Its settings: $ENV_FILE (then: systemctl restart $UNIT)
  What it says: journalctl -u $UNIT -f
EOF
