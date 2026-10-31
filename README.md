# Radiance Music

A music player for Linux (GTK 4 and libadwaita) that finds any song and plays it, lossless
first. **Like it, save it, split it:** a song you like is saved to your Music folder with its
cover, its lyrics and its music video. Then it's split, on your own computer, into drums, bass,
other and vocals, for a live Mixer, karaoke (No vocals / Vocals only) and lyrics timed word by
word.

Everything happens on your computer. Nothing is uploaded, and there's no account or server.

> **Open beta.** This is Radiance Music's first public version: it's new, some things may not work yet,
> and it changes often. Tell us what breaks or what you'd like: [open an issue](https://github.com/RadianceADM/radiance-music/issues)
> or email hello@radianceph.com.

**Get it:** [music.radianceph.com](https://music.radianceph.com) has the download (and the phone
app, [Radiance Music for Android](https://github.com/RadianceADM/radiance-music-android)). Or
take the newest `RadianceMusic-linux-x86_64.tar.gz` from [Releases](https://github.com/RadianceADM/radiance-music/releases),
unpack it and run `./install.sh`. On an older Linux, or one it doesn't run on, take
`RadianceMusic.flatpak` instead (below).

## What it does

- **Search any song.** Lossless FLAC from the Internet Archive first, YouTube Music when it's
  not there. Search suggestions as you type, with your liked songs first.
- **Liked songs are saved** to `~/Music/Radiance Music`: the song (FLAC when it's lossless),
  its cover, its lyrics file and its official music video (720p). "Fix missing" saves whatever
  a liked song still lacks.
- **Split on your computer:** each liked song is split into drums, bass, other and vocals by
  [Demucs](https://github.com/facebookresearch/demucs). It runs at idle priority, so whatever
  you're doing always comes first, on as many cores as you allow.
- **A seamless Mixer:** a split song plays with its parts at hand, in step with its own file.
  Moving a slider never reloads anything; with every part up you hear the song's own file,
  untouched. Turning parts down keeps the song as loud as before, never louder, with a
  look-ahead limiter so nothing clips.
- **Lyrics, timed word by word:** from [LRCLIB](https://lrclib.net), or YouTube Music's lyrics.
  Every word gets its own time from the song's Vocals part with the MMS aligner (1,100+
  languages, Tagalog included). A lyrics editor sets each line's time and singer by hand, and
  "Find Lyrics Again" fixes another song's lyrics.
- **Now Playing:** the album art's colours flow behind it like a lava lamp and swell on the
  beat. The line being sung is drawn big, and there's a full-screen view and the music video.
- **Queue, radio and Autoplay:** drag songs to reorder the queue and go back to the last 5
  played. Song radio comes from any song, and Autoplay carries on when the queue ends. There's a
  crossfade or a beat-matched DJ mix between songs.
- **Your library:** Liked Songs (A to Z, with search), History, Albums, Artists, Genres and
  playlists, kept in SQLite.
- **Desktop media controls** (MPRIS): GNOME's top bar, the lock screen and the media keys. Close
  the window and the music plays on.

## What you need

### To play music

| | Minimum | Recommended |
|---|---|---|
| System | 64-bit Linux with GTK 4.14+, libadwaita 1.7+ and glibc 2.39+ (Ubuntu 25.04, Fedora 42 or newer) | the latest GNOME |
| Sound | GStreamer 1.22+ with the good, bad and ugly plugins and libav (FLAC, Opus, WebM) | PipeWire |
| YouTube Music | [yt-dlp](https://github.com/yt-dlp/yt-dlp), kept up to date | yt-dlp with [deno](https://deno.com) |
| Network | for searching, streaming and saving | |

### To split songs and time their words (set up by itself, the first time)

| | Minimum: it works | Recommended: a song splits in about the time it plays |
|---|---|---|
| Processor | 2 cores, 64-bit (x86-64 or ARM64) | 4 cores or more |
| Memory | 4 GB (it uses up to 2.7 GB while it works) | 8 GB or more |
| Free space | 4 GB (the install and its two models), plus about 30 MB per split song | 10 GB or more |
| Python | Python 3.9 or newer with `venv` (`python3-venv` on Debian and Ubuntu) | |
| Download | about 2.5 GB, once (PyTorch for the CPU, Demucs, the two models) | |

The first time the app opens, it gets the splitter ready by itself while its sunrise plays, showing
how far along it is (10 to 30 minutes, once); there's nothing to press. On a computer that can't
split songs it goes straight on without it. Splitting never needs a graphics card.

### How long it takes

Measured with the app's own scripts on a 4-minute song: two runs each, on a 6-core Intel Core
i5-9500T (35 W, 2019) with 16 GB of memory and other programs running.

| Cores it may use | Splitting the song | Timing its words | Memory at most |
|---|---|---|---|
| 6 | 3¼ to 4½ min | about 2½ min | 2.7 GB |
| 4 | 3¾ to 4¼ min | about 2½ min | 2.7 GB |
| 2 | 3¼ to 3¾ min | about 2 min | 2.7 GB |
| 1 | about 6 min | about 2 min | 2.7 GB |

On a low-power processor like this one, more cores help little past 2: it's held back by its
power limit, not its core count. Desktop processors gain more from extra cores. Word timing
takes about the same time at any number of cores.

How good it is, from our own measurements:

- **Splitting:** Demucs (htdemucs) gets the vocals out at 10.2 dB SDR over 6 test clips (MUSDB18),
  ahead of the MDX-Net models we tried (9.2 and 9.4 dB).
- **Word timing:** with lines already timed, 92 % of words land within 0.3 s of the singing
  (median 0.04 s); from plain text, 87 % (JamendoLyrics test set).
- **Your computer stays usable:** splitting runs at idle priority. On a 4-core laptop, 2 cores
  this way slowed other apps by about 60 % while it ran, and 4 cores by about 150 %. That's why
  it uses half your cores unless you say otherwise.

## Install the download

Unpack `RadianceMusic-linux-x86_64.tar.gz` and run `./install.sh` in it. It puts the app in your
home folder (`~/.local/bin`, your apps, its icon; no password) and says what it still needs, if
anything. Run it again to update. To remove it, delete `~/.local/bin/radiance-music`,
`~/.local/share/applications/io.github.radianceadm.Music.desktop` and
`~/.local/share/icons/hicolor/scalable/apps/io.github.radianceadm.Music.svg`.

## Windows

`RadianceMusic-Setup.exe` (on [Releases](https://github.com/RadianceADM/radiance-music/releases)) installs
Radiance Music for you alone (no administrator), with a Start menu entry; its uninstaller leaves
your songs where they are. Windows 10 or 11, 64-bit. The installer is about 100 MB; the first
time the app opens it gets splitting and word timing ready by itself (Python, Demucs and MMS: a
one-time download of about 840 MB, `RadianceMusic-Splitter-win64.zip` on the 0.7.12 release,
checked by its SHA-256), as the Linux download does. Built on Linux (Fedora's mingw64 GTK 4 and
GStreamer; `packaging/windows/pack.sh`, `pybundle.sh`, `vcrt.sh`, `mkpack.py` and `installer.nsi`).

## The Flatpak

`RadianceMusic.flatpak` (on [Releases](https://github.com/RadianceADM/radiance-music/releases)) runs on
almost any Linux with Flatpak, with what it needs inside: yt-dlp, Deno for YouTube, FFmpeg. Install it
with your software centre, or:

```sh
flatpak install --user RadianceMusic.flatpak   # it fetches the GNOME runtime from Flathub the first time
flatpak run io.github.radianceadm.Music
```

It's Radiance Music's own, not on Flathub. Inside it, Share Songs with a Phone can't open Ubuntu's
firewall for you: if yours is on, open the ports once with
`sudo ufw allow proto tcp from 192.168.0.0/16 to any port 47800:47809`. Build it yourself with
`packaging/flatpak/build.sh` (flatpak-builder, GNOME 49 runtime and SDK, rust-stable extension).

## Build and install

Install the build tools and libraries. On Fedora:

```sh
sudo dnf install rust cargo gtk4-devel libadwaita-devel gstreamer1-devel gstreamer1-plugins-base-devel \
  gstreamer1-plugins-good gstreamer1-plugins-bad-free gstreamer1-plugins-ugly-free gstreamer1-libav \
  python3 yt-dlp
```

On Debian or Ubuntu:

```sh
sudo apt install cargo libgtk-4-dev libadwaita-1-dev libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
  gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly gstreamer1.0-libav \
  python3 python3-venv yt-dlp
```

Then build it and put it in place:

```sh
cargo build --release
install -Dm755 target/release/radiance-music ~/.local/bin/radiance-music
install -Dm644 data/io.github.radianceadm.Music.desktop ~/.local/share/applications/io.github.radianceadm.Music.desktop
install -Dm644 data/icons/io.github.radianceadm.Music.svg ~/.local/share/icons/hicolor/scalable/apps/io.github.radianceadm.Music.svg
```

A Flatpak is coming.

Optional: Speaker EQ, a separate app (published later), adds Low cut, High cut and Reverb to the
Mixer for the speaker playing. Without it, the Mixer has its four parts only.

## YouTube

YouTube Music is the fallback when a song isn't on the Internet Archive. It plays through
yt-dlp, which keeps up with YouTube's changes, so keep yt-dlp updated. If YouTube asks to
"confirm you're not a bot", updating yt-dlp usually fixes it. You can also let yt-dlp use your
browser's YouTube sign-in.

Saving songs and music videos from YouTube goes against YouTube's terms of service. That's
your choice to make for your own use. To build a player that only streams YouTube songs and
never saves them:

```sh
cargo build --release --no-default-features
```

## Credits and licences

Radiance Music is free software under the [GNU GPL, version 3 or later](LICENSE).

It stands on:
- [Demucs](https://github.com/facebookresearch/demucs) (MIT) for splitting;
- the MMS forced aligner through [torchaudio](https://pytorch.org/audio/main/generated/torchaudio.pipelines.MMS_FA.html)
  for word timing. Its model is Meta's, under CC-BY-NC 4.0: free for personal use, not for
  commercial use. The app downloads it for you when you set splitting up; it isn't part of this
  repository.
- [LRCLIB](https://lrclib.net) and YouTube Music for lyrics;
- [the Internet Archive](https://archive.org) for lossless music;
- [yt-dlp](https://github.com/yt-dlp/yt-dlp);
- GTK, libadwaita and GStreamer.
