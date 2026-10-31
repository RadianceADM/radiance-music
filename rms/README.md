# Radiance Music Server

Your own server for Radiance Music. You host it; you and the friends you invite use it.

- **It splits songs for you:** drums, bass, other and vocals, and every word's time in the lyrics,
  on the server's processor instead of your phone or laptop.
- **It keeps your library in one place:** songs, their parts, lyrics, covers and music videos.
- **Your friends play from it and split on it.** Each one joins with an invite you make, and you can
  remove anyone at any time.
- **Listening parties:** everyone in the party hears the same song at the same moment, on the apps or in a
  browser. Everyone adds to the queue. The host plays, pauses and skips (or lets everyone do it), and
  people react.
- **Private:** there's no sign-up page and no public list. Only people you invite can get in, and nothing
  is sent to us.

## How it differs from the app

| | Radiance Music (the app) | Radiance Music Server |
|---|---|---|
| What it is | The player everyone downloads for their phone or computer | A program you run on a server (a VPS or a PC that's always on) |
| Who uses it | You, on your own devices | You and the friends you invite |
| Splitting | On your device | On the server, for everyone, one song at a time |
| Library | On each device | One library on the server, shared |
| You need | A phone or computer | A server, plus the app on your devices |

You still use the app to listen. The server is where the app gets its shared songs, parts, splitting and
parties from.

## What you need

- A 64-bit Linux server that stays on. A **Hostinger VPS** works well:
  - **KVM 2** (2 vCPU, 8 GB of memory) works. A song takes about twice its own length to split.
  - **KVM 4** (4 vCPU, 16 GB) splits a song in about its own length.
  - Splitting uses about **2.7 GB of memory** while it works, so 4 GB is the least that will work and 8 GB is better.
- Disk space: about 2 GB for the program, 1.3 GB for the splitting models (downloaded once, the first
  time it starts), and about 30 MB for each split song.
- A domain is optional (for example `music.example.com`). With one, the server uses https.

## Way 1: Deploy on Hostinger

**[Deploy on Hostinger](https://www.hostinger.com/vps/docker-hosting?compose_url=https://raw.githubusercontent.com/RadianceADM/radiance-music/main/rms/docker-compose.yml)**

1. Click the link above. Hostinger opens its Docker VPS setup with the server already filled in.
2. **Type a setup key** where it asks for `RMS_SETUP_KEY` (any word only you know, such as `sunset-2026`).
   It keeps your new server yours: nobody can set it up without the key. If you leave it empty, the server
   makes one and shows it in its log (Docker Manager → your project → Logs).
3. Pick a plan (KVM 2 or bigger) and finish the order. Hostinger sets up the VPS and starts the server.
4. Open `http://YOUR-VPS-ADDRESS` in a browser (the address is in hPanel → VPS).
5. **Type your name and the setup key.** Your friends will see the server as "*your name*'s server". You
   get your own join link and its QR code for the app. After that, the key isn't needed any more.
6. Leave the page open for a few minutes the first time: it downloads the splitting models (about 1.3 GB,
   only once) and shows how far along it is.

To add a domain later, see [Add a domain (https)](#add-a-domain-https).

## Way 2: Set it up yourself

### With Docker (any Linux server)

```sh
mkdir radiance-music-server && cd radiance-music-server
curl -fsSLO https://raw.githubusercontent.com/RadianceADM/radiance-music/main/rms/docker-compose.yml
docker compose up -d
```

Then open `http://YOUR-SERVER-ADDRESS` and type your name and the setup key. The server made the key at
its first start: it's in `docker compose logs rms` (look for "The setup key is"). To choose your own key
and a domain, put them in a `.env` file next to `docker-compose.yml` first:

```sh
printf 'RMS_SETUP_KEY=sunset-2026\nRMS_DOMAIN=music.example.com\n' > .env
docker compose up -d
```

### Without Docker (Debian, Ubuntu or Fedora)

```sh
git clone https://github.com/RadianceADM/radiance-music
sudo bash radiance-music/rms/install.sh
```

With a domain (Caddy gets the https certificate by itself):

```sh
sudo RMS_DOMAIN=music.example.com bash radiance-music/rms/install.sh
```

The script asks you for a setup key (press Enter and the server makes one). It installs Python, ffmpeg and
the splitter (PyTorch for the processor, plus Demucs), builds the server, and starts it as a service called
`radiance-music-server`. It ends by printing the address to open and the setup key. Without a domain, the
address is `http://YOUR-SERVER-ADDRESS:8080`.

- Settings: `/etc/radiance-music-server.env` (after changing it: `sudo systemctl restart radiance-music-server`)
- Your library, the database and the models: `/var/lib/radiance-music-server`
- What it's doing: `journalctl -u radiance-music-server -f`

## Inviting friends

On your server's page (the address you opened at setup, in the same browser):

1. **Invite a friend:** type their name, then click **Make an invite**. You get a link and a QR code.
2. Send them the link. In Radiance Music they go to **Settings → Servers → Join a server**, then paste
   the link or scan the code. On a device that has the app, the link's page also has an
   **Open in Radiance Music** button that joins straight away. If they don't have the app yet, the
   page lets them listen to parties in a browser.
3. Each link works **once**, on one device, and **expires after 7 days** if it isn't used (the list of
   invites shows how long each has left). For a friend's second device, or if their link expired, click
   **New link** next to their name. For your own phone and computer, click **Add one of my devices**.

Friends can play the library, add songs, split songs (the parts land in the library for everyone), and
join or start listening parties. Under each friend's name, three switches let you turn some of that off for
them: **Can add songs**, **Can split** and **Can host a party** (all on at first). Playing and joining
parties always stay on. To remove a friend, click **Remove** next to their name. Their devices stop working
at once, and the songs they added stay. You can also remove a single device, such as a lost phone.

## Adding songs

- **From the server's page:** **Library → Add a song**. You can add the song file (.opus, .flac or .m4a),
  and also a cover, lyrics and a music video if you have them. If the library already has the song,
  it isn't added twice. Only the extras the library is missing are kept.
- **By copying files:** put your songs in the server's `library` folder, named the way the app saves them
  (`Artist - Title.opus`, with `Artist - Title.lyrics.yaml`, `Artist - Title.jpg` and so on beside them).
  A copy of the app's Music folder works as it is. With Docker:
  `docker cp "Artist - Title.opus" radiance-music-server-rms-1:/data/library/`

Click **Split** on a song to queue it. Its parts appear beside it a few minutes later. If it has lyrics,
every word gets its time too. When several people queue songs, they **take turns**: one song each, in the
order each person asked for their own. The queue shows whose turn is next.

## Listening parties

On the server's page, open **Listening party**, or go to `/party`. Anyone can start one, and there's one at a
time. Add songs from the library: the first one starts right away. In a browser, tap **Listen here**
(browsers need a tap before they play sound). The page keeps your browser in time with everyone else.

## Add a domain (https)

Without a domain, the server works on `http://` but what you send isn't encrypted. To add one:

1. Where your domain is managed, add an **A record** for a name such as `music.example.com`, pointing to
   your server's address.
2. Set `RMS_DOMAIN` to that name:
   - **Hostinger:** Docker Manager → your project → Environment → `RMS_DOMAIN=music.example.com`, then redeploy.
   - **Docker:** `echo "RMS_DOMAIN=music.example.com" > .env && docker compose up -d`
   - **install.sh:** run it again with `sudo RMS_DOMAIN=music.example.com bash radiance-music/rms/install.sh`
3. Open `https://music.example.com`. Caddy gets the certificate by itself, usually within a minute.

No domain? If the server has a public IPv4 address such as `203.0.113.5`, you can use
`203-0-113-5.sslip.io` as `RMS_DOMAIN`. It points to your server for free, and the server's page suggests it.

## Updating and backups

- **Updating:** Hostinger: Docker Manager → your project → Update (or Redeploy). Docker:
  `docker compose pull && docker compose up -d`. install.sh: `git pull` in the repository, then run the
  script again. Your library, people and settings stay.
- **Backups:** back up the data folder: `library/` and `server.db`. You don't need `models/`: if it's
  missing, the models download again. With Docker, the data is in the volume
  `radiance-music-server_rms-data`:
  ```sh
  docker run --rm -v radiance-music-server_rms-data:/data -v "$PWD":/backup debian \
    tar czf /backup/rms-backup.tgz -C /data library server.db
  ```

## Settings

| Setting | What it does | Default |
|---|---|---|
| `RMS_DOMAIN` | Your domain: https through Caddy, and links use it | none (http) |
| `RMS_SETUP_KEY` | The key the setup page asks for, so nobody else can claim your new server. Not needed after setup | made at the first start (log and `setup-key.txt`) |
| `RMS_DATA` | The folder with the library, `server.db` and the models | `/data` (Docker), `/var/lib/radiance-music-server` (install.sh) |
| `RMS_BIND` | Where the server listens | `0.0.0.0:8080` |
| `RMS_PYTHON` | The Python that has the splitter | `/opt/rms/venv/bin/python` |
| `RMS_SCRIPTS` | The folder with `split.py` and `align.py` (the app's own) | `/opt/rms/scripts` |
| `RMS_THREADS` | How many processor cores splitting may use (always at the lowest priority) | all of them |
| `RMS_PUBLIC_URL` | The address links start with, if it's neither the domain nor what the browser used | none |

## If something's wrong

- **"The splitter isn't installed on this server yet":** the server can't find Python with Demucs. The
  Docker image includes it. With install.sh, run the script again. Songs wait in the queue until the
  splitter is installed.
- **"The models' download stopped":** usually the connection. The server tries again a minute later by itself.
- **The page doesn't open:** check that the server is running (Hostinger: Docker Manager), and that the
  firewall lets in ports 80 and 443 (Docker) or 8080 (install.sh without a domain).

---

## For app developers: the API

Every call carries `Authorization: Bearer TOKEN` (the device's token from joining). File addresses and the
party's WebSocket also accept `?t=TOKEN`, because players and browsers can't set headers there. Errors are
`{"error": "what happened, in plain words"}`.

| Call | What it does |
|---|---|
| `GET /api/status` | The server's name and `id` (made at its first start and never changed, so an app knows it's the same server at a new address), whether it's set up, the splitter's and the models' state (no token needed) |
| `POST /api/setup {name, key}` | The setup page's form (the setup key is required): makes the owner, returns this browser's token and the owner's first invite |
| `POST /api/join {code, device_name}` | Joins with an invite's one-time code, and returns the device's `token` |
| `GET /api/me` | Who this device is (person, role, device, server name) and what it may do: `can {add, split, host}` |
| `POST /api/invites {name}` or `{person}` | Owner: an invite for a new friend, or for another device of someone already on the server. Unused invites expire after 7 days (`expires_ms`, `expires_in_days`) |
| `GET /api/invites`, `DELETE /api/invites/CODE` | Owner: invites not used yet (and not expired); take one back |
| `GET /api/people` | Owner: everyone, with their devices, their switches (`can`), and their waiting and expired invites |
| `PATCH /api/people/ID {can_add, can_split, can_host}` | Owner: a friend's switches. With one off, that friend gets a plain 403 |
| `POST /api/people/ID/revoke` | Owner: removes a friend (all their devices) |
| `POST /api/devices/ID/revoke` | Removes a device (the owner any device, a friend only their own) |
| `GET /api/library?q=&offset=&limit=` | The songs (no parameters: all of them; `q`: every word in the artist, title or file name; `total`: how many match). Each song: `artist`, `title`, `duration_ms`, `split`, `parts`, `word_times`, `added_by`, and `files` by kind (song, drums, bass, other, vocals, music, lyrics, cover, video; each with `name`, `size`, `url`) |
| `GET /api/songs/ID` | One song, as one entry of `/api/library` |
| `POST /api/songs/ID/parts` (multipart) | A song split on a phone or computer brings its parts: `drums`, `bass`, `other`, `vocals` (all four, with what the server has), `music`, and optional `lyrics`. Parts the server has are skipped. `replace=1` (query or field; the owner only) puts the new ones in their place. Needs "Can add songs" |
| `GET /files/NAME` | Any library file, with HTTP Range (seeking) |
| `POST /api/songs` (multipart) | `song` file, plus optional `cover`, `lyrics`, `video`, `artist`, `title`, `seconds`. If the song is already there (same artist and title), it's skipped |
| `POST /api/split {song}` | Queues a song for splitting (`song`: its id, or its file name) |
| `GET /api/queue`, `DELETE /api/queue/ID` | The split queue with progress and plain-words notes. Waiting songs are listed in their turns, and `next_turn` says whose turn is next. Take a song out (the owner, or whoever asked) |
| `GET /api/time` | `{now_ms}`: the server's clock, for clock-offset pings (no token needed) |
| `GET /api/party` | The party's state (`on: false` when there's none) |
| `POST /api/party/start {everyone_controls}` | Starts a party (one at a time) |
| `POST /api/party/add {song}` | Adds to the queue (anyone); it starts at once if nothing is playing |
| `POST /api/party/control {action}` | `play`, `pause`, `skip`, `seek {position_ms}`, `reorder {entry, to}`, `remove {entry}`: the host, or everyone if allowed. `allow {everyone}`: the host only |
| `POST /api/party/react {emoji}` | A reaction everyone sees |
| `POST /api/party/end` | Ends the party (the host, or the server's owner) |
| `GET /api/events` | WebSocket (`?t=` works) instead of polling: `{"type":"hello","server":{id,name}}`, then `{"type":"library"}` when songs or parts change, `{"type":"queue", …the queue}` when the split queue changes (at most twice a second while splitting), `{"type":"me"}` when your switches change or you're removed (then it closes) |
| `GET /api/party/ws` | WebSocket: the full state on every change; send `{"type":"time","client_ms":N}` to get `{"type":"time","client_ms":N,"server_ms":M}` back |

**Playing in time.** A playing party song has `started_at_server_ms`: the server time at which its position
was (or will be) 0. A paused one has `paused_at_ms`. Each change starts `lead_ms` (800 ms) after it's made,
so everyone receives it in time. A listener keeps its clock offset to the server
(`offset = server_ms - (sent + received) / 2`, from the quickest of a few pings) and plays from
`position = (now + offset) - started_at_server_ms`. A late joiner uses the same formula. When a song
ends, the server starts the next one by itself.

The image pins PyTorch 2.15.0+cpu, torchaudio 2.11.1+cpu and Demucs 4.1.0: the set that split songs and
timed words in it. It's built from the repository's root:
`docker build -f rms/Dockerfile -t ghcr.io/radianceadm/radiance-music-server:latest .`
