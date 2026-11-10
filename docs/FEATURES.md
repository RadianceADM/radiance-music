# Radiance Music: one feature list, every platform

The same features on the computer (Linux and Windows) and the phone (Android). The phone's
screens are made for a phone, so they look and work differently; this list is about what each
one **can do**, not how it looks. Kept in step with every change (owner, 2026-11-02).

As of: computer **0.7.20**, phone **0.5.15** (released versions). Built but not released yet:
marked 🔜.

✅ has it · ❌ missing (a gap to close) · ➖ not needed there (the platform does it its own way)

**iPhone**: Radiance Music for iPhone and iPad (RadianceADM/radiance-music-ios, Swift and SwiftUI, built on Linux with xtool), 0.1.0 being built: 🔜 built, not released yet (installed on the test iPhone stage by stage). Free-signed for now, so a copy opens for 7 days.

**Windows beta**: the native Windows app (WinUI 3, #45), growing stage by stage beside today's Windows app: ✅ in it now, 🔜 N comes in stage N (2: library, queue, videos, word-by-word lyrics; 3: parts and splitting; 4: servers, sharing, pop-ups, settings). It replaces today's Windows app only once its column matches the Windows one.

## Finding and playing music

| Feature | Linux | Windows | Windows beta | Phone | iPhone | Notes |
|---|---|---|---|---|---|---|
| Search songs: lossless (Internet Archive) first, YouTube Music after | ✅ | ✅ | ✅ | ✅ | 🔜 | |
| A song played from Search plays, then songs like it (its radio) | ✅ | ✅ | ✅ | ✅ | 🔜 | as YouTube Music; before, the other search results played after it (#47) |
| Search suggestions as you type | ✅ | ✅ | ✅ | ✅ | 🔜 | the beta: in the box's own list, yours (a heart) first |
| Genre tiles before you type | ✅ | ✅ | ✅ | ❌ | ❌ | |
| Made for you mix (songs like your most-played likes) | ✅ | ✅ | ✅ | ❌ | ❌ | the phone's "Play your mix" shuffles your liked songs |
| Home rows: recently played, liked songs | ✅ | ✅ | ✅ | ✅ | 🔜 | |
| Albums, Artists, Genres pages | ✅ | ✅ | ✅ | ❌ | 🔜 | |
| Playlists (make, add, rename, delete) | ✅ | ✅ | ✅ | ❌ | ❌ | |
| History | ✅ | ✅ | ✅ | ✅ | 🔜 | computer keeps 1000, phone 300; iPhone: 300 |
| Liked Songs: A to Z rail, own search, Play, Shuffle | ✅ | ✅ | ✅ | ✅ | 🔜 | |
| Downloading: the liked songs not saved yet, with how many, in the sidebar | 🔜 | 🔜 | ✅ | ❌ | ❌ | always there; the ones saving now first, Fix missing on its page (#66) |
| Music videos: watch in the app | ✅ | ✅ | ✅ | ➖ | ➖ | phone: none, by the owner's ruling (2026-11-08, "final na walang music video sa mobile"; 🔜 1.0.5 takes out the Video button and deletes the kept videos, android#32); beta: on the compositor, 16:9, fullscreen; some streamed videos are refused by YouTube (#48); iPhone: none, as on Android |
| Songs from a Radiance Music Server | ✅ | ✅ | 🔜 4 | ✅ | 🔜 | |

## Playback

| Feature | Linux | Windows | Windows beta | Phone | iPhone | Notes |
|---|---|---|---|---|---|---|
| Plays in the background, media keys, lock screen | ✅ | ✅ | ✅ | ✅ | 🔜 | MPRIS / Windows media controls + tray icon / Android media notification; iPhone: lock screen, Control Center, headphone buttons; pauses when headphones come out |
| Queue: see it, move songs, remove | ✅ | ✅ | ✅ | ✅ | 🔜 | drag on the computer, Move up/down on the phone; iPhone: drag to move, swipe to take out |
| Played before (go back to a recent song, the queue carries on) | ✅ | ✅ | ✅ | ❌ | ❌ | |
| Shuffle, Repeat (all / one) | ✅ | ✅ | ✅ | ✅ | 🔜 | |
| Crossfade and DJ mix between songs | ✅ | ✅ | ✅ | ❌ | ❌ | the beta: Settings → Playing → Between Songs (No Transition, Crossfade, DJ Mix (by the Beat), the crossfade length); a pause holds a fade (both computer apps) |
| Autoplay (similar songs when the queue ends) | ✅ | ✅ | ✅ | ❌ | ❌ | |
| Song Radio | ✅ | ✅ | ✅ | ❌ | 🔜 | iPhone: Songs like this |
| Often-skipped songs kept out of mixes | ✅ | ✅ | ✅ | ❌ | ❌ | the phone has no mixes yet |
| Picks up where you left off | ✅ | ✅ | ✅ | ✅ | 🔜 | |
| Says why a song won't play, skips, stops after 3 | ✅ | ✅ | ✅ | ✅ | 🔜 | |
| Starts on the first click (a stuck start is started again) | ✅ | ✅ | ✅ | ➖ | ➖ | the computer's sound output could refuse a song; the phone's player doesn't |
| "Loading…" while a song starts | ✅ | ✅ | ✅ | ✅ | 🔜 | the bar and Now Playing say it; the phone: a ring on the cover and the mini player |
| The bar's song opens Now Playing | ✅ | ✅ | ✅ | ✅ | 🔜 | the phone's mini player |
| Now Playing's background follows the beat (a kick swells it, bass flows it faster) | ✅ | ✅ | ✅ | 🔜 | ❌ | #56; the phone: android#27; the beta's costs about 0.6 % of one core and rests when minimised; iPhone: the cover's colours flow, not to the beat |

## Mixer and splitting

| Feature | Linux | Windows | Windows beta | Phone | iPhone | Notes |
|---|---|---|---|---|---|---|
| Part sliders: Drums, Bass, Other, Vocals | ✅ | ✅ | ✅ | ✅ | ❌ | iPhone: one tap: Song, Karaoke, Vocals only |
| Song / No vocals / Vocals only, carrying on to the next songs (each split as it starts, the next ahead) | 🔜 | 🔜 | ✅ | ✅ | 🔜 | computer: the buttons show Song with "Splitting for No vocals…" and Turn Off till the song is split (#67); Linux and Windows had the buttons built but never on screen; iPhone: the next song isn't split ahead yet |
| Two-part songs: Music and Vocals sliders | ✅ | ✅ | ✅ | 🔜 | ❌ | the phone's split (0.5.17, #43); on the computer, songs split before there were four parts |
| Keeps the loudness when parts are turned down | ✅ | ✅ | ✅ | ✅ | ❌ | |
| Low cut, High cut, Reverb (one setting, every song) | ✅ | ✅ | ✅ | ✅ | 🔜 | built into the computer app since 0.7.14; iPhone: on saved and split songs |
| Fast split in two (Music + Vocals, a few seconds) | ✅ | ✅ | ✅ | 🔜 | 🔜 | computer: inside the app, no Python; the Split button asks "Music + Vocals (fast)" or "Four parts" (#43); phone: its only split (0.5.17); iPhone: Spleeter on ONNX Runtime |
| Split a song on this device | ✅ | ✅ | ✅ | ✅ | 🔜 | computer: Python Demucs, 4 parts (no-Python version in testing, #31); phone: 2 parts (Music + Vocals) with Spleeter, about 16× real time (🔜 0.5.17, #43; Demucs before), 4 parts only from the computer or a server |
| Liked songs are split by themselves | ✅ | ✅ | ✅ | ❌ | ❌ | the phone splits when you ask |
| Split queue, pause / stop splitting | ✅ | ✅ | ✅ | ✅ | 🔜 | |
| Cores it may use | ✅ | ✅ | ✅ | ✅ | 🔜 | |
| Pauses for low battery or heat | ➖ | ➖ | ➖ | ✅ | 🔜 | |
| Split on a Radiance Music Server | ✅ | ✅ | 🔜 4 | ✅ | 🔜 | |
| Six parts (piano, guitar) | off | off | off | off | ❌ | songs already split in six play, piano and guitar follow Other |

## Lyrics

| Feature | Linux | Windows | Windows beta | Phone | iPhone | Notes |
|---|---|---|---|---|---|---|
| Lyrics from LRCLIB, else YouTube Music | ✅ | ✅ | ✅ | ✅ | 🔜 | |
| Refuses another artist's lyrics | ✅ | ✅ | ✅ | ✅ | 🔜 | |
| The sung line lit, tap a line to jump there | ✅ | ✅ | ✅ | ✅ | 🔜 | |
| Word by word (each word lights as it's sung) | ✅ | ✅ | ✅ | ❌ | ❌ | the phone lights whole lines |
| Every word timed to the singing (MMS) | ✅ | ✅ | ✅ | ❌ | ❌ | |
| Hums light at a steady pace | ✅ | ✅ | ✅ | ➖ | ➖ | only matters with word-by-word lyrics |
| Lyrics editor (fix lines, tap timing, singers) | ✅ | ✅ | ✅ | ❌ | ❌ | |
| Find Lyrics Again | ✅ | ✅ | ✅ | ❌ | ❌ | |
| Comes back by itself when online again | ✅ | ✅ | ✅ | ✅ | 🔜 | |

## Saving and your library

| Feature | Linux | Windows | Windows beta | Phone | iPhone | Notes |
|---|---|---|---|---|---|---|
| Like = save the song, its cover and lyrics | ✅ | ✅ | ✅ | ✅ | 🔜 | iPhone: into Files → Radiance Music |
| Likes that couldn't be saved are saved later | ✅ | ✅ | ✅ | ✅ | 🔜 | |
| Unlike removes the saved files | ✅ | ✅ | ✅ | ❌ | ❌ | the phone keeps the files |
| Storage page, move the music folder | ✅ | ✅ | ✅ | ❌ | ➖ | the beta: Settings → Storage; iPhone: the Files app shows the folder |

## Sharing

| Feature | Linux | Windows | Windows beta | Phone | iPhone | Notes |
|---|---|---|---|---|---|---|
| Copy or share a song's link | ✅ | ✅ | ✅ | ✅ | 🔜 | the beta: Copy Link in a song's menu (stage 4b); iPhone: Share link and Copy link in a song's menu |
| Story cards and story videos with lyrics (Instagram, Facebook…) | ❌ | ❌ | ❌ | ✅ | 🔜 | sharing is the main way people find us; iPhone: story cards; no story videos yet |
| Share songs with a phone (QR, both ways, only what's missing) | ✅ | ✅ | ✅ | ✅ | 🔜 | a song's parts go along; the two-part split's Music too (🔜 next computer release, #43) |
| Gentle transfer while Bluetooth audio plays | ✅ | ❌ | ❌ | ✅ | 🔜 | |
| Works without shared Wi-Fi (Wi-Fi Direct) | ➖ | ➖ | ➖ | ✅ | ➖ | iPhone: no Wi-Fi Direct on iPhone; its Personal Hotspot works |

## Servers and listening parties

| Feature | Linux | Windows | Windows beta | Phone | iPhone | Notes |
|---|---|---|---|---|---|---|
| Join a server, browse, save, add your songs | ✅ | ✅ | 🔜 4 | ✅ | 🔜 | iPhone: your songs go up after a split here |
| radiancemusic:// invite links | ✅ | ✅ | 🔜 4c | ✅ | 🔜 | the beta: with servers (an invite opens the Servers dialog); iPhone: an invite link asks to join in Settings, and a share QR read by the Camera starts the share |
| Listening parties: in sync, reactions, host controls | ✅ | ✅ | 🔜 4 | ✅ | 🔜 | iPhone: songs saved as Opus don't play there yet |

## The app itself

| Feature | Linux | Windows | Windows beta | Phone | iPhone | Notes |
|---|---|---|---|---|---|---|
| Update pop-up: Update now, Remind me later, Skip | ✅ | ✅ | ✅ | ✅ | ➖ | the beta: its own installer, its version on the release's asset label; iPhone: free-signed: installed again every 7 days |
| Installs its own update | ✅ | ✅ | ✅ | ✅ | ➖ | Linux download / Flatpak / Windows installer / Android installer; the beta: its own installer where it is (checked on ribs03 from a local feed, 2026-11-07) |
| What's New | ✅ | ✅ | ✅ | ✅ | 🔜 | |
| How to Use It guide | ✅ | ✅ | ✅ | ✅ | 🔜 | |
| Start screen with first-open setup and progress | ✅ | ✅ | ✅ | ✅ | ❌ | the beta: the sunrise holds while splitting gets ready, offline it offers Try Again or Open Radiance Music Now |
| Looks: Stone, True Black | ✅ | ✅ | Windows' light/dark | ✅ | 🔜 | |
| Offline sign, things retry when back online | ✅ | ✅ | ✅ | ✅ | 🔜 | |
| Report a problem: link to GitHub Issues | ✅ | ✅ | ✅ | ✅ | ➖ | iPhone: closed source, so Email us instead |
| Email us from the app | ❌ | ❌ | ❌ | ✅ | 🔜 | the computer shows the address only in the README |
| Report a Problem with the app's log (names removed) | ✅ | ✅ | ✅ | 🔜 | ❌ | computer since 0.7.15; the phone's comes with 0.5.16 |
| Rating pop-up ("Are you enjoying…?") | ✅ | ✅ | ✅ | 🔜 | ❌ | computer since 0.7.15; the phone's comes with 0.5.16 |

## Gaps to close (keeps the three in step)

Phone:
- Albums, Artists and Genres pages; Playlists
- Made for you mix, Song Radio, Autoplay, Crossfade / DJ mix, Played before
- Word-by-word lyrics with every word timed (MMS), the lyrics editor, Find Lyrics Again
- Liked songs split by themselves; Unlike removes the saved files; a Storage page
- Genre tiles in Search

Computer (Linux and Windows):
- Story cards and story videos with lyrics, straight to Instagram / Facebook stories
- Email us from the app

Windows only:
- Gentle transfer while Bluetooth audio plays
- The Splitting window says "about 2.5 GB" and shows a Linux log path; Windows downloads about 840 MB

