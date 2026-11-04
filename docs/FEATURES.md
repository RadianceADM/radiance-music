# Radiance Music: one feature list, three platforms

The same features on the computer (Linux and Windows) and the phone (Android). The phone's
screens are made for a phone, so they look and work differently; this list is about what each
one **can do**, not how it looks. Kept in step with every change (owner, 2026-11-02).

As of: computer **0.7.17**, phone **0.5.15** (released versions). Built but not released yet:
marked 🔜.

✅ has it · ❌ missing (a gap to close) · ➖ not needed there (the platform does it its own way)

## Finding and playing music

| Feature | Linux | Windows | Phone | Notes |
|---|---|---|---|---|
| Search songs: lossless (Internet Archive) first, YouTube Music after | ✅ | ✅ | ✅ | |
| Search suggestions as you type | ✅ | ✅ | ✅ | |
| Genre tiles before you type | ✅ | ✅ | ❌ | |
| Made for you mix (songs like your most-played likes) | ✅ | ✅ | ❌ | the phone's "Play your mix" shuffles your liked songs |
| Home rows: recently played, liked songs | ✅ | ✅ | ✅ | |
| Albums, Artists, Genres pages | ✅ | ✅ | ❌ | |
| Playlists (make, add, rename, delete) | ✅ | ✅ | ❌ | |
| History | ✅ | ✅ | ✅ | computer keeps 1000, phone 300 |
| Liked Songs: A to Z rail, own search, Play, Shuffle | ✅ | ✅ | ✅ | |
| Music videos: watch in the app | ✅ | ✅ | ❌ | the phone saves them but can't play them |
| Songs from a Radiance Music Server | ✅ | ✅ | ✅ | |

## Playback

| Feature | Linux | Windows | Phone | Notes |
|---|---|---|---|---|
| Plays in the background, media keys, lock screen | ✅ | ✅ | ✅ | MPRIS / Windows media controls + tray icon / Android media notification |
| Queue: see it, move songs, remove | ✅ | ✅ | ✅ | drag on the computer, Move up/down on the phone |
| Played before (go back to a recent song, the queue carries on) | ✅ | ✅ | ❌ | |
| Shuffle, Repeat (all / one) | ✅ | ✅ | ✅ | |
| Crossfade and DJ mix between songs | ✅ | ✅ | ❌ | |
| Autoplay (similar songs when the queue ends) | ✅ | ✅ | ❌ | |
| Song Radio | ✅ | ✅ | ❌ | |
| Often-skipped songs kept out of mixes | ✅ | ✅ | ❌ | the phone has no mixes yet |
| Picks up where you left off | ✅ | ✅ | ✅ | |
| Says why a song won't play, skips, stops after 3 | ✅ | ✅ | ✅ | |
| Starts on the first click (a stuck start is started again) | ✅ | ✅ | ➖ | the computer's sound output could refuse a song; the phone's player doesn't |

## Mixer and splitting

| Feature | Linux | Windows | Phone | Notes |
|---|---|---|---|---|
| Part sliders: Drums, Bass, Other, Vocals | ✅ | ✅ | ✅ | |
| Keeps the loudness when parts are turned down | ✅ | ✅ | ✅ | |
| Low cut, High cut, Reverb (one setting, every song) | ✅ | ✅ | ✅ | built into the computer app since 0.7.14 |
| Split a song on this device | ✅ | ✅ | ✅ | computer: Python Demucs (no-Python version in testing, #31); phone: Demucs as ONNX |
| Liked songs are split by themselves | ✅ | ✅ | ❌ | the phone splits when you ask |
| Split queue, pause / stop splitting | ✅ | ✅ | ✅ | |
| Cores it may use | ✅ | ✅ | ✅ | |
| Pauses for low battery or heat | ➖ | ➖ | ✅ | |
| Split on a Radiance Music Server | ✅ | ✅ | ✅ | |
| Six parts (piano, guitar) | off | off | off | songs already split in six play, piano and guitar follow Other |

## Lyrics

| Feature | Linux | Windows | Phone | Notes |
|---|---|---|---|---|
| Lyrics from LRCLIB, else YouTube Music | ✅ | ✅ | ✅ | |
| Refuses another artist's lyrics | ✅ | ✅ | ✅ | |
| The sung line lit, tap a line to jump there | ✅ | ✅ | ✅ | |
| Word by word (each word lights as it's sung) | ✅ | ✅ | ❌ | the phone lights whole lines |
| Every word timed to the singing (MMS) | ✅ | ✅ | ❌ | |
| Hums light at a steady pace | ✅ | ✅ | ➖ | only matters with word-by-word lyrics |
| Lyrics editor (fix lines, tap timing, singers) | ✅ | ✅ | ❌ | |
| Find Lyrics Again | ✅ | ✅ | ❌ | |
| Comes back by itself when online again | ✅ | ✅ | ✅ | |

## Saving and your library

| Feature | Linux | Windows | Phone | Notes |
|---|---|---|---|---|
| Like = save the song, its cover and lyrics | ✅ | ✅ | ✅ | |
| Likes that couldn't be saved are saved later | ✅ | ✅ | ✅ | |
| Unlike removes the saved files | ✅ | ✅ | ❌ | the phone keeps the files |
| Storage page, move the music folder | ✅ | ✅ | ❌ | |

## Sharing

| Feature | Linux | Windows | Phone | Notes |
|---|---|---|---|---|
| Copy or share a song's link | ✅ | ✅ | ✅ | |
| Story cards and story videos with lyrics (Instagram, Facebook…) | ❌ | ❌ | ✅ | sharing is the main way people find us |
| Share songs with a phone (QR, both ways, only what's missing) | ✅ | ✅ | ✅ | |
| Gentle transfer while Bluetooth audio plays | ✅ | ❌ | ✅ | |
| Works without shared Wi-Fi (Wi-Fi Direct) | ➖ | ➖ | ✅ | |

## Servers and listening parties

| Feature | Linux | Windows | Phone | Notes |
|---|---|---|---|---|
| Join a server, browse, save, add your songs | ✅ | ✅ | ✅ | |
| radiancemusic:// invite links | ✅ | ✅ | ✅ | |
| Listening parties: in sync, reactions, host controls | ✅ | ✅ | ✅ | |

## The app itself

| Feature | Linux | Windows | Phone | Notes |
|---|---|---|---|---|
| Update pop-up: Update now, Remind me later, Skip | ✅ | ✅ | ✅ | |
| Installs its own update | ✅ | ✅ | ✅ | Linux download / Flatpak / Windows installer / Android installer |
| What's New | ✅ | ✅ | ✅ | |
| How to Use It guide | ✅ | ✅ | ✅ | |
| Start screen with first-open setup and progress | ✅ | ✅ | ✅ | |
| Looks: Stone, True Black | ✅ | ✅ | ✅ | |
| Offline sign, things retry when back online | ✅ | ✅ | ✅ | |
| Report a problem: link to GitHub Issues | ✅ | ✅ | ✅ | |
| Email us from the app | ❌ | ❌ | ✅ | the computer shows the address only in the README |
| Report a Problem with the app's log (names removed) | ✅ | ✅ | 🔜 | computer since 0.7.15; the phone's comes with 0.5.16 |
| Rating pop-up ("Are you enjoying…?") | ✅ | ✅ | 🔜 | computer since 0.7.15; the phone's comes with 0.5.16 |

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

