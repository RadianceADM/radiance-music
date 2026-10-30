#!/bin/sh
# Radiance Music for Linux: puts the app in your home folder (no password needed):
# the app in ~/.local/bin, its entry in your apps and its icon. Run it again to update.
# To remove it: rm ~/.local/bin/radiance-music ~/.local/share/applications/io.github.radianceadm.Music.desktop
#   ~/.local/share/icons/hicolor/scalable/apps/io.github.radianceadm.Music.svg
set -e
here=$(cd "$(dirname "$0")" && pwd)
bin="$HOME/.local/bin/radiance-music"
install -Dm755 "$here/radiance-music" "$bin"
mkdir -p "$HOME/.local/share/applications"
sed "s|^Exec=radiance-music|Exec=$bin|" "$here/io.github.radianceadm.Music.desktop" > "$HOME/.local/share/applications/io.github.radianceadm.Music.desktop"
install -Dm644 "$here/io.github.radianceadm.Music.svg" "$HOME/.local/share/icons/hicolor/scalable/apps/io.github.radianceadm.Music.svg"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q "$HOME/.local/share/applications" || true
echo "Radiance Music is installed: open it from your apps, or run radiance-music."

# what it still needs, said plainly
missing=""
"$bin" --version >/dev/null 2>&1 || missing="$missing gtk4 libadwaita gstreamer"
command -v yt-dlp >/dev/null 2>&1 || missing="$missing yt-dlp"
if command -v gst-inspect-1.0 >/dev/null 2>&1; then
  gst-inspect-1.0 opusdec >/dev/null 2>&1 || missing="$missing gstreamer-plugins"
fi
command -v python3 >/dev/null 2>&1 || missing="$missing python3"
if [ -n "$missing" ]; then
  echo
  echo "It still needs:$missing"
  echo "  Ubuntu, Debian, Mint:  sudo apt install libgtk-4-1 libadwaita-1-0 gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly gstreamer1.0-libav python3 python3-venv yt-dlp"
  echo "  Fedora:                sudo dnf install gtk4 libadwaita gstreamer1-plugins-good gstreamer1-plugins-bad-free gstreamer1-plugins-ugly-free gstreamer1-libav python3 yt-dlp"
fi
