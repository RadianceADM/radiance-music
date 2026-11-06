# Radiance Music: one feature list, three platforms

The same features on the computer (Linux and Windows) and the phone (Android). The phone's
screens are made for a phone, so they look and work differently; this list is about what each
one **can do**, not how it looks. Kept in step with every change (owner, 2026-11-02).

As of: computer **0.7.20**, phone **0.5.15** (released versions). Built but not released yet:
marked 🔜.

✅ has it · ❌ missing (a gap to close) · ➖ not needed there (the platform does it its own way)

**Windows beta**: the native Windows app (WinUI 3, #45), growing stage by stage beside today's Windows app: ✅ in it now, 🔜 N comes in stage N (2: library, queue, videos, word-by-word lyrics; 3: parts and splitting; 4: servers, sharing, pop-ups, settings). It replaces today's Windows app only once its column matches the Windows one.

## Finding and playing music

| Feature | Linux | Windows | Windows beta | Phone | Notes |
|---|---|---|---|---|---|
| Search songs: lossless (Internet Archive) first, YouTube Music after | ✅ | ✅ | ✅ | ✅ | |
| A song played from Search plays, then songs like it (its radio) | ✅ | ✅ | 🔜 2 | ✅ | as YouTube Music; before, the other search results played after it (#47) |
| Search suggestions as you type | ✅ | ✅ | 🔜 2 | ✅ | |
| Genre tiles before you type | ✅ | ✅ | ✅ | ❌ | |
| Made for you mix (songs like your most-played likes) | ✅ | ✅ | ✅ | ❌ | the phone's "Play your mix" shuffles your liked songs |
| Home rows: recently played, liked songs | ✅ | ✅ | ✅ | ✅ | |
| Albums, Artists, Genres pages | ✅ | ✅ | 🔜 2 | ❌ | |
| Playlists (make, add, rename, delete) | ✅ | ✅ | 🔜 2 | ❌ | |
| History | ✅ | ✅ | 🔜 2 | ✅ | computer keeps 1000, phone 300 |
| Liked Songs: A to Z rail, own search, Play, Shuffle | ✅ | ✅ | 🔜 2 | ✅ | |
| Music videos: watch in the app | ✅ | ✅ | 🔜 2 | ❌ | the phone saves them but can't play them |
| Songs from a Radiance Music Server | ✅ | ✅ | 🔜 4 | ✅ | |

## Playback

| Feature | Linux | Windows | Windows beta | Phone | Notes |
|---|---|---|---|---|---|
| Plays in the background, media keys, lock screen | ✅ | ✅ | ✅ | ✅ | MPRIS / Windows media controls + tray icon / Android media notification |
| Queue: see it, move songs, remove | ✅ | ✅ | 🔜 2 | ✅ | drag on the computer, Move up/down on the phone |
| Played before (go back to a recent song, the queue carries on) | ✅ | ✅ | 🔜 2 | ❌ | |
| Shuffle, Repeat (all / one) | ✅ | ✅ | ✅ | ✅ | |
| Crossfade and DJ mix between songs | ✅ | ✅ | 🔜 4 | ❌ | |
| Autoplay (similar songs when the queue ends) | ✅ | ✅ | ✅ | ❌ | |
| Song Radio | ✅ | ✅ | 🔜 2 | ❌ | |
| Often-skipped songs kept out of mixes | ✅ | ✅ | ✅ | ❌ | the phone has no mixes yet |
| Picks up where you left off | ✅ | ✅ | ✅ | ✅ | |
| Says why a song won't play, skips, stops after 3 | ✅ | ✅ | ✅ | ✅ | |
| Starts on the first click (a stuck start is started again) | ✅ | ✅ | ✅ | ➖ | the computer's sound output could refuse a song; the phone's player doesn't |

## Mixer and splitting

| Feature | Linux | Windows | Windows beta | Phone | Notes |
|---|---|---|---|---|---|
| Part sliders: Drums, Bass, Other, Vocals | ✅ | ✅ | 🔜 3 | ✅ | |
| Two-part songs: Music and Vocals sliders | ✅ | ✅ | 🔜 3 | 🔜 | the phone's split (0.5.17, #43); on the computer, songs split before there were four parts |
| Keeps the loudness when parts are turned down | ✅ | ✅ | 🔜 3 | ✅ | |
| Low cut, High cut, Reverb (one setting, every song) | ✅ | ✅ | 🔜 3 | ✅ | built into the computer app since 0.7.14 |
| Fast split in two (Music + Vocals, a few seconds) | ✅ | ✅ | 🔜 3 | 🔜 | computer: inside the app, no Python; the Split button asks "Music + Vocals (fast)" or "Four parts" (#43); phone: its only split (0.5.17) |
| Split a song on this device | ✅ | ✅ | 🔜 3 | ✅ | computer: Python Demucs, 4 parts (no-Python version in testing, #31); phone: 2 parts (Music + Vocals) with Spleeter, about 16× real time (🔜 0.5.17, #43; Demucs before), 4 parts only from the computer or a server |
| Liked songs are split by themselves | ✅ | ✅ | 🔜 3 | ❌ | the phone splits when you ask |
| Split queue, pause / stop splitting | ✅ | ✅ | 🔜 3 | ✅ | |
| Cores it may use | ✅ | ✅ | 🔜 3 | ✅ | |
| Pauses for low battery or heat | ➖ | ➖ | ➖ | ✅ | |
| Split on a Radiance Music Server | ✅ | ✅ | 🔜 4 | ✅ | |
| Six parts (piano, guitar) | off | off | 🔜 3 | off | songs already split in six play, piano and guitar follow Other |

## Lyrics

| Feature | Linux | Windows | Windows beta | Phone | Notes |
|---|---|---|---|---|---|
| Lyrics from LRCLIB, else YouTube Music | ✅ | ✅ | ✅ | ✅ | |
| Refuses another artist's lyrics | ✅ | ✅ | ✅ | ✅ | |
| The sung line lit, tap a line to jump there | ✅ | ✅ | lit ✅, tap 🔜 2 | ✅ | |
| Word by word (each word lights as it's sung) | ✅ | ✅ | 🔜 2 | ❌ | the phone lights whole lines |
| Every word timed to the singing (MMS) | ✅ | ✅ | 🔜 3 | ❌ | |
| Hums light at a steady pace | ✅ | ✅ | 🔜 2 | ➖ | only matters with word-by-word lyrics |
| Lyrics editor (fix lines, tap timing, singers) | ✅ | ✅ | 🔜 3 | ❌ | |
| Find Lyrics Again | ✅ | ✅ | 🔜 2 | ❌ | |
| Comes back by itself when online again | ✅ | ✅ | 🔜 2 | ✅ | |

## Saving and your library

| Feature | Linux | Windows | Windows beta | Phone | Notes |
|---|---|---|---|---|---|
| Like = save the song, its cover and lyrics | ✅ | ✅ | 🔜 2 | ✅ | |
| Likes that couldn't be saved are saved later | ✅ | ✅ | 🔜 2 | ✅ | |
| Unlike removes the saved files | ✅ | ✅ | 🔜 2 | ❌ | the phone keeps the files |
| Storage page, move the music folder | ✅ | ✅ | 🔜 4 | ❌ | |

## Sharing

| Feature | Linux | Windows | Windows beta | Phone | Notes |
|---|---|---|---|---|---|
| Copy or share a song's link | ✅ | ✅ | 🔜 4 | ✅ | |
| Story cards and story videos with lyrics (Instagram, Facebook…) | ❌ | ❌ | ❌ | ✅ | sharing is the main way people find us |
| Share songs with a phone (QR, both ways, only what's missing) | ✅ | ✅ | 🔜 4 | ✅ | a song's parts go along; the two-part split's Music too (🔜 next computer release, #43) |
| Gentle transfer while Bluetooth audio plays | ✅ | ❌ | ❌ | ✅ | |
| Works without shared Wi-Fi (Wi-Fi Direct) | ➖ | ➖ | ➖ | ✅ | |

## Servers and listening parties

| Feature | Linux | Windows | Windows beta | Phone | Notes |
|---|---|---|---|---|---|
| Join a server, browse, save, add your songs | ✅ | ✅ | 🔜 4 | ✅ | |
| radiancemusic:// invite links | ✅ | ✅ | 🔜 4 | ✅ | |
| Listening parties: in sync, reactions, host controls | ✅ | ✅ | 🔜 4 | ✅ | |

## The app itself

| Feature | Linux | Windows | Windows beta | Phone | Notes |
|---|---|---|---|---|---|
| Update pop-up: Update now, Remind me later, Skip | ✅ | ✅ | 🔜 4 | ✅ | |
| Installs its own update | ✅ | ✅ | 🔜 4 | ✅ | Linux download / Flatpak / Windows installer / Android installer |
| What's New | ✅ | ✅ | 🔜 4 | ✅ | |
| How to Use It guide | ✅ | ✅ | 🔜 4 | ✅ | |
| Start screen with first-open setup and progress | ✅ | ✅ | 🔜 3 | ✅ | |
| Looks: Stone, True Black | ✅ | ✅ | Windows' light/dark | ✅ | |
| Offline sign, things retry when back online | ✅ | ✅ | Search's note ✅, retry 🔜 4 | ✅ | |
| Report a problem: link to GitHub Issues | ✅ | ✅ | 🔜 4 | ✅ | |
| Email us from the app | ❌ | ❌ | ❌ | ✅ | the computer shows the address only in the README |
| Report a Problem with the app's log (names removed) | ✅ | ✅ | 🔜 4 | 🔜 | computer since 0.7.15; the phone's comes with 0.5.16 |
| Rating pop-up ("Are you enjoying…?") | ✅ | ✅ | 🔜 4 | 🔜 | computer since 0.7.15; the phone's comes with 0.5.16 |

## Gaps to close (keeps the three in step)

Phone:
- Albums, Artists and Genres pages; Playlists
- Made for you mix, Song Radio, Autoplay, Crossfade / DJ mix, Played before
- Watch music videos in the app
- Word-by-word lyrics with every word timed (MMS), the lyrics editor, Find Lyrics Again
- Liked songs split by themselves; Unlike removes the saved files; a Storage page
- Genre tiles in Search

Computer (Linux and Windows):
- Story cards and story videos with lyrics, straight to Instagram / Facebook stories
- Email us from the app

Windows only:
- Gentle transfer while Bluetooth audio plays
- The Splitting window says "about 2.5 GB" and shows a Linux log path; Windows downloads about 840 MB

