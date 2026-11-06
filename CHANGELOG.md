# Changelog

## v0.7.20 — 2026-11-06

- **A song played from Search plays, then songs like it:** as YouTube Music does, the song you pick from Search plays at once and its radio (songs like it) fills Up next. Before, the queue was the other search results: covers and same-named songs by other artists, so unrelated songs played after it. Songs you add to the queue meanwhile stay, before the radio (#47).
- **Split a song in two in seconds:** Split this song now asks: **Music + Vocals (fast)**, a few seconds a song, or **Four parts** (Drums, Bass, Other, Vocals for the Mixer, about 3 minutes). The fast split runs inside the app, on Linux and Windows, with nothing to install (what it needs, about 50 MB, comes at the first start; gently while Bluetooth music plays). A song split in two can be split in four later. Liked songs still split in four by themselves. Where this computer can't split in four, No vocals and Vocals only use the fast split (#43).
- **Sharing with the phone carries its two-part split:** a song the phone split in two (Music + Vocals, its new fast split) comes to the computer with both parts, and the computer sends a song's Music part along too. The computer's mixer plays two-part songs with two sliders (#43).
- **Next right after opening starts the next song from its start:** with last time's song shown (picked up at its saved place), pressing Next, picking another song in the queue, Play again, or removing that song from the queue started the next one at the old song's place, and the first Pause then started it over (#46).

## v0.7.19 — 2026-11-05

- **Now Playing doesn't hold the processor any more (Windows lag, continued):** its moving background (the art flowing with the beat) drew itself 60 times a second, about a whole core on Windows. It draws 20 times a second now, still following every beat, and stands still while Radiance Music is behind another window or minimized. Home's lights already do the same since 0.7.18.

## v0.7.18 — 2026-11-04

- **No more lag on Windows:** 0.7.16 and 0.7.17 kept a PC's processor busy all the time (about one and a half cores, even with nothing playing), so other windows lagged. It was Home's drifting lights: they're drawn a lighter way now, move only while Radiance Music is the window in front, and stand still while you work in another app or it's minimized. They look the same.
- Windows: the program file now carries its own details and icon (Radiance Music, Radiance Innovation Centers, its version), as Windows expects of an app.
- **The update pop-up offers the newest version:** it offered what it found the day before (and could download a version two releases old); it looks again every time the app opens now. Its round icon no longer stretches into a tall bar.
- Windows: moving your library to another folder (Storage) now takes your saved songs' places with it; songs saved before the move could still point at the old folder.

## v0.7.17 — 2026-11-04

- **Splitting works on Windows again for songs from the internet:** No vocals, Vocals only and the Mixer's parts said "Couldn't split … right now" for every song that wasn't saved on the PC, because the work file's folder was written the wrong way. They split now (a song took about 2 minutes on a 6-core PC).
- **Only offered where it can work:** while a music video plays, No vocals and Vocals only are greyed out (music videos play as they are), and offline, a song from the internet isn't offered for splitting.

## v0.7.16 — 2026-11-04

- **A tray icon on Windows:** Radiance Music sits in the notification area by the clock while it's open (new icons start behind the ^ arrow: drag it onto the taskbar to keep it in sight). Click it to bring the window back; right-click for Play/Pause, Next, Previous, Show Radiance Music and Quit. Closing the window while music plays leaves it playing there; with nothing playing, closing still quits.
- **The song carries on when the speaker changes (Windows):** a monitor with speakers switching off, headphones unplugged or another output picked used to skip to the next song ("its source didn't answer"). Now the same song carries on from where it was, and the music follows the speaker Windows plays to, coming back to the monitor's speakers when it wakes.
- Windows: the splitter pack's unzipping uses a newer, fixed library (security alerts).
- The Windows app is now built on a Windows PC.

## v0.7.15 — 2026-11-03

- **Report a Problem:** Menu → Report a Problem… sends us the app's recent log with a note from you, so we can see what went wrong. You see everything that goes before you press Send, with your name, your home folder and email addresses taken out, and you get a code (like RM-0123) to quote if you write to us.
- **Tell us how it's going:** after a week and 30 songs, at the end of a song or a pause, Radiance Music may ask "Are you enjoying Radiance Music?", at most once a month. Rate it in five quick parts and add a comment; sending asks you to sign in with Google. Not Now and Don't Ask Again are always there, and Menu → Rate Radiance Music opens it any time.
- Home's banner moves gently now: two soft lights drift under the sunset.

## v0.7.14 — 2026-11-02

- **Low cut, high cut and reverb, built in:** the Mixer has them for every song now (split or not, music videos too), done by Radiance Music itself: no other app needed, one setting for every speaker, on Windows too. The part sliders still answer at once with them on.
- **Songs start on the first click:** a click starts the new song before the old one stops (it goes quiet at once), so a sound setup that closes for a moment between songs no longer refuses it; and a song on your computer that still hasn't started after a few seconds is started again by itself.
- **Hums in step:** in word-timed lyrics, hums like "mmmh", "hmm" and "ooh" now light one after another at a steady pace, instead of bunching up or leaving gaps. Lyrics you edited yourself keep your times.
- **A roomier Queue:** Queue and Played before show each song's title, artists and length only, so long titles fit.
- **No second Lyrics tab:** the side panel hides its Lyrics tab while Now Playing shows the lyrics itself (it stays when the window is too narrow for them).
- The sidebar no longer shows a Speaker EQ entry (the effects are in the Mixer now), and splitting writes a song's parts a little faster.

## v0.7.13 — 2026-11-01

- **Updates tell you they're out:** a pop-up when a new version is out, with Update Now,
  Remind Me Later (a day) and Skip This Version, and Look for Updates in the menu. Update Now
  downloads while the music keeps playing, checks the file, then shows Restart to Update; it
  never restarts by itself. Windows and the Linux download put the new version on for you; the
  Flatpak updates through Flatpak itself, from Radiance Music's own Flatpak repository.
- **Join a Radiance Music Server:** your own server, or a friend's. Menu → Servers, then paste
  the invite link (or open it: radiancemusic:// links open the app). Each server you joined
  shows in the sidebar under Library: play its songs, save them with their parts, add your liked
  songs to it, and split songs there instead of on your computer, taking turns with everyone
  else. Changes on the server show right away.
- **Listening parties:** start or join one on a server, and everyone in it hears the same song
  at the same moment, wherever they are. Up next, who's listening, reactions, and the host plays,
  pauses and skips. Leaving brings back your own queue.
- The quality pill under the song no longer runs out of the window at phone width.

## v0.7.12 — 2026-10-31

- **Sound on Windows:** 0.7.11's player asked for Linux's sound output, so on Windows it
  couldn't play a song at all. It plays through Windows' own (WASAPI) now (checked on a real
  Windows 11).
- **Windows splits songs too**, as Linux does: drums, bass, other and vocals, made on your PC,
  and every word of the lyrics timed from the vocals (MMS). As the Linux download does, the
  installer stays small (about 100 MB) and the app gets splitting ready by itself the first
  time it opens: a one-time download of about 840 MB (Python, Demucs, the word-timing model)
  while its sunrise shows how far along it is, picking up where it stopped if the internet
  drops, checked before it's used. Settings' Splitting has the same cores setting.
- On Windows, the programs the app runs (yt-dlp, the splitter) open no console windows, and
  splitting runs at idle priority, so the music and the rest of the PC go first.
- **Lossless songs play on Windows:** the Internet Archive's FLAC files failed there with a
  certificate error (Windows' own player couldn't check the Archive's servers). They now come
  through the app's own relay, as YouTube songs already did.
- A song split right after installing, before anything else, failed ("can't open split.py"):
  the app's data folder wasn't made yet. It is now, first.
- **The window shows on every Windows PC:** on one without a working graphics driver (a
  virtual machine, Remote Desktop, an old driver) 0.7.11's window never appeared. It now draws
  in software there; a PC with a graphics card still draws with it.
- The window is never taller or wider than the screen (on a 1366×768 laptop the player bar was
  under Windows' taskbar).
- **Media controls on Windows:** the keyboard's media keys, and the player in Windows' volume
  flyout and on the lock screen, show the song and its cover, and play, pause and skip, as
  GNOME's top bar does on Linux.
- **One Radiance Music on Windows:** opening it again (Start menu, desktop) brings back the
  window that's already open, as on Linux, instead of starting a second one. So closing the
  window while music plays keeps it playing there too.

- **The Mixer after Reset:** lowering Vocals could leave only the vocals playing, until Drums,
  Bass and Other were each moved again: Reset put the sliders back at full but not the parts
  behind them. Every change now sends every part's volume.
- The "How to use it" cards wait until the first start's setup is done (they came up over the
  sunrise while it was still getting splitting ready).

Linux and Windows alike, what the app says when something's wrong:

- **Offline shows:** an "Offline" pill in the top bar while there's no internet, and "Back
  online" when it returns. Search says you're offline (your liked songs still play) instead
  of "no songs found", and searches again when you're back; lyrics say they'll come when
  you're back online, not "no lyrics".
- **A song that won't play says why** (you're offline, YouTube wants to check you're not a
  bot, the song isn't on YouTube anymore, …) with **Try Again**. It goes on to the next song,
  but not when every song would fail the same way, and not after three in a row: it stopped
  saying only "isn't available" and skipping song after song.
- **Missing GStreamer parts or yt-dlp:** a banner says so and stays, with **How to Fix It**,
  instead of a 4-second note and then "Loading…" for ever.
- **The first start, offline:** the sunrise says why getting splitting ready can't go on, with
  **Try Again** and **Open Radiance Music Now** (it waited with no way past it before). It goes
  on by itself when the internet's back.
- **Sharing with a phone on Windows:** if Windows' firewall question was answered no, the
  phone couldn't reach the computer and nothing said so. The share window now offers **Let It
  In** (one Windows prompt): the sharing ports, from your home network only.

## v0.7.11 — 2026-10-31

- **Always the dark look** (Stone or True Black), even on a light desktop: it followed the
  system's light or dark setting before.
- **Radiance Music for Windows (open beta):** `RadianceMusic-Setup.exe` installs it for you
  alone (no administrator), with a Start menu entry and an uninstaller that leaves your songs
  where they are. Searching, playing lossless and YouTube songs, lyrics, liking and saving work;
  splitting songs on Windows comes later (it needs Python there).

## v0.7.10 — 2026-10-31

- **A Flatpak download:** `RadianceMusic.flatpak` installs on almost any Linux, older ones
  too, with everything it needs inside (yt-dlp, Deno for YouTube, FFmpeg). Radiance Music's
  own: it isn't on Flathub.
- Share Songs with a Phone draws its QR code in the Stone look's darkest, not navy.

## v0.7.9 — 2026-10-31

- **How to Use It, the first time the app opens:** five cards to page through (search, like it
  and it's saved, split and mix, sing along, on your phone too), a few plain words each, as
  the phone app's. Skip any time; the main menu's How to Use It shows them again.

## v0.7.8 — 2026-10-31

- Share Songs with a Phone says where the phone's side is now: Radiance Music for Android
  0.5.9 keeps sharing songs in its Settings (Get songs), no longer behind a QR button on Library.

## v0.7.7 — 2026-10-30

- **The Linux download runs on more systems:** it needs glibc 2.39 or newer now (2.43 before),
  so Ubuntu 25.04, Fedora 42 and newer run it, with GTK 4.14+ and libadwaita 1.7+.
- A Linux download of its own: an archive with the app and `install.sh`, which puts it in your
  home folder (no password) and says plainly what it still needs. `radiance-music --version`
  says which version it is.

## v0.7.6 — 2026-10-30

- **Piano and Guitar (6 parts) are put away for a later release:** the "6 parts" tick is gone
  and songs are split in four. A song already split in six still plays whole: its Piano and
  Guitar move with the Other slider.
- Setting splitting up no longer gets the six-part model.

## v0.7.5 — 2026-10-29

- **Splitting gets itself ready on the first start:** on a computer where it isn't set up yet,
  the app stays on its sunrise and gets the splitter (PyTorch, Demucs, its four- and six-part
  models, the word-timing model) by itself, showing how far along it is, then goes on into the
  app. Nothing to press, nothing asked; with no internet it tries again by itself. A computer
  that can't split songs goes straight on without it.

## v0.7.4 — 2026-10-29

- **The start, in the app's own look:** the sunrise is drawn in Stone or True Black, whichever
  you use (no more navy), the sun's bars cut out of it in the same colour.
- **Then the logo writes itself:** once the app is ready, the bars come to rest, the sun sets
  again, and the R writes itself, its leg rolling on into the m and the wave fading out, with
  "Radiance Music" coming up under it. Then the dark opens from the middle out into Home (not
  all at once), the logo staying a moment over the opening. A click goes straight in.
- **Smoother start:** covers are loaded by two quiet helpers, newest first, instead of a few
  hundred at once (that took every core, and the animation stuttered); the steps that can't be
  split run before the sun moves; the window behind stays hidden until the logo is written.
  The slowest moving frame went from 283 ms to 83 ms, and the app is ready in about 1.2 s. How
  smooth each start was goes in the app log.

## v0.7.3 — 2026-10-29

- **A loading animation when the app starts:** the icon's sun rises out of the sea and its four
  bars play like an equalizer while the app gets ready, then it fades into the app. The window
  gets ready behind it a step at a time (Home a section at a time), so it keeps moving. Not
  shown when animations are off in your system's settings.
- Starting builds Home once. It used to be built twice, the second time just after the window
  showed, and the window froze for about 0.7 s.
- **Home changes only where something changed:** your mix arriving updates the greeting and
  "Made for you" (in 28 ms), a song played updates "Recently played". It used to build all of
  Home again each time, and the window froze for about 0.7 s (often right after starting, or
  when going back to Home).
- **Fixed: "6 parts" gave four on most songs.** A song a minute or longer is split on the server
  and this computer together, and this computer's copy of the splitter was an old one without
  the six-part model, so the song came back in four parts. The app now brings its copy up to
  date when it starts, and a split that comes back missing parts is done again by the server
  alone. Songs already ticked are split in six again on their own.

## v0.7.2 — 2026-10-29

- **New look: the sunset with four parts**, the app's new icon (window, About, the app grid).
- **A meter on every Mixer slider:** how loud that part is in the song right now. Some songs
  hardly have one (in many beats the splitter puts the melody in Drums and Vocals, and Other
  is nearly empty); the meter shows it, and its tooltip says so.
- **6 parts, when you tick it** (Mixer → "6 parts: Piano and Guitar too"): that song is split
  again with Demucs' six-part model, and Piano and Guitar get sliders of their own. Song by
  song; untick to go back to four (Other then moves Piano and Guitar with it, so the song
  stays whole).

## v0.7.1 — 2026-10-29

- **Fixed: a song could play without its vocals (or with a part you'd turned up still
  down) after a crossfade.** The next song gets ready 12 seconds before the crossfade, with
  the Mixer as it was then; moving a slider (or No vocals → Song) in those last seconds
  didn't reach it, so it played with the old volumes while the sliders showed the new ones.
  Now the song being readied follows every slider move, and takes the sliders as they are
  the moment the crossfade starts.
- `--probe-mixer SONG OUT.wav …` plays a song through the Mixer into a file, never to the
  speakers (with `next=SONG2` for a crossfade into another song's parts), to check the
  Mixer by measuring.

## v0.7.0 — 2026-10-29

- **Share Songs with a Phone** (Menu): a QR code that Radiance Music for Android scans
  (Library → the QR button → Scan a QR). **To the Phone:** the phone takes your liked songs
  with their four parts, lyrics and covers (and music videos, if you switch them on), so it
  doesn't have to download or split anything. **From the Phone:** the phone's songs come here
  the same way and join Liked Songs.
- Nothing is sent twice: the receiving side says what it has first (songs saved, parts split,
  lyrics, covers, videos) and only what's missing goes. Straight over your home network, with
  no server in between.
- On Wi-Fi while music plays to Bluetooth, songs go slowly (256 KB/s), because on many laptops
  Wi-Fi and Bluetooth share a radio and the music would stutter. On a cable: full speed.
- If Ubuntu's firewall is on, the window offers to let the phone in on your home network
  (ports 47800 to 47809; it asks for your password).
- Parts split on the phone (.m4a) play in the Mixer here too.

## v0.6.1 — 2026-10-29

- **Fixed: no sound at all (the JBL or the laptop's speakers).** Fading a song out (a crossfade,
  or a switch to its parts) used to lower the sound server's own volume for Radiance Music, all
  the way to 0. The sound server remembers that per app, so the next song started at 0 % and
  played silently. Each player now fades with its own volume, inside the app; the sound
  server's volume for Radiance Music (Settings → Sound → Volume Levels) is never touched.

## v0.6.0 — 2026-10-28

The first public release: everything happens on your own computer.

- **Splitting on this computer:** Menu → Splitting… sets it up. It installs PyTorch for the CPU,
  Demucs and the MMS aligner into the app's own folder (about 2.5 GB, once, in the
  background). Then every liked song is split here, at idle priority (whatever you're doing
  comes first), and every word of its lyrics is timed here too. No server needed, and nothing
  is uploaded.
- **Cores it may use:** you choose (Splitting…); half of them unless you say. Splitting… also
  shows what it needs against this computer (processor, memory, free space, Python) and how
  long it takes on each number of cores, as measured.
- **The Mixer's effects come from Speaker EQ**, a separate app it now talks to through its
  command line (`speaker-eq --effects`). Radiance Music builds on its own, and without Speaker
  EQ the Mixer has its four parts only.
- Saving liked songs, their music videos and lyrics is on in every build: like it, save it,
  split it. `--no-default-features` builds a player that only streams YouTube songs.
- A split server stays possible (set in settings.json): songs split there first, with this
  computer helping.
- **Fits any window, down to a phone:** at phone width (under 600 px) the sidebar and the side
  panel slide over, Now Playing stands its art above, cards come two to a row, and the player
  bar keeps the song, back / play / next, the seek bar, Like and the queue. Song rows keep their
  title, length and heart. Nothing asks for more room than the window has any more: only one
  window-size layout applies at a time, so each narrower one now carries everything the wider
  ones do. Pages are only as tall as the one showing, and Now Playing scrolls when the window
  is short.
- **A mini player:** make the window short (under 240 px) and it becomes just the song and its
  controls, with the window's own minimise and close buttons. It goes down to the bar's own
  height; make it taller and the pages come back.
- README (what it does, what it needs, how long it takes, build and install), and the GPL-3.0
  licence.

## v0.5.25 — 2026-10-28

- **The Mixer is seamless:** a split, saved song now plays with its parts at hand. Its own file
  and its four parts play together, in step (they line up to the sample; the file minus its
  parts leaves only a trace, 28 dB under the song). So moving a slider never loads anything,
  pauses or jumps. With every part up you hear the song's own file, untouched (full quality, no
  limiter). Turning one down blends over to the parts in 0.3 s, and putting them all back up
  blends back. Stem players do it the same way: everything plays, only volumes change.
- **Crossfades carry the parts along:** the next split song is cued with its parts at hand, and
  the crossfade goes from parts to parts, parts to a plain song, or a plain song to parts.
  (A beat-matched DJ mix still goes deck to deck.)
- **Where a song has no saved copy** (streamed), or arrived in a DJ mix, the first slider move
  still switches it over once. That switch fades now: the song fades out (0.6 s), then its
  parts fade in (1.2 s) from where it's got to. Never both at once, so no beat doubles. Split
  halves (Vocals only, No vocals) switch the same way.
- **Gentler limiter on bass-heavy mixes:** 8 ms look-ahead and a 0.25 s release (was 5 ms and
  0.08 s), so 808s don't pump when they're raised.

## v0.5.24 — 2026-10-28

- **The Mixer keeps the song as loud:** turning a part down (drums, bass, other or vocals) no
  longer makes the whole song quieter. What's left is raised to make up for it, never louder
  than the whole song and at most +6 dB, and a look-ahead limiter keeps every peak just under
  full scale, so nothing clips or crackles. Each part's loudness is measured as it plays (over
  the last 8 s), so the make-up follows the song.
- **Switching between the song and its parts plays onward, never back:** going from the full
  song to the 4 parts (or back, or to a split half) starts the new one where the song has got to
  by the time it's ready, so there's a moment's quiet instead of a jump back.

## v0.5.23 — 2026-10-28

- **The line being sung is 2.5 times bigger** in Now Playing's lyrics. It's wrapped again to fit
  the column at that size, and its room swells as the last line shrinks back (0.4 s), so nothing
  jumps. A long line on a small screen grows a little less (down to 1.5 times), so it never
  fills more than 60% of the view.
- **Drag songs in the Queue:** pick up any song after the one playing (a handle shows on each)
  and drop it above or below another; an orange line shows where it goes. Near the top or bottom
  of the list it scrolls.
- **Find Lyrics Again** (a song's menu, and the Show Lyrics window): looks the lyrics up again.
  The old ones go to the Trash, the ones found take their place and, for a split song, every
  word is timed to the singing on the split server. Show Lyrics also has **Edit…**, which opens
  the lyrics editor (each line's time, its words, its singer). Saving there times every word
  the same way.
- **Lyrics that are another song's are turned away:** someone had filed Young Greatness's
  "Moolah" under SCOOP DOGG & daarth's on LRCLIB, and the lyrics' own tag said whose they were.
  Lyrics whose "[ar: …]" tag names another artist are skipped now, and the right ones come from
  YouTube Music. A lyrics re-check that finds nothing (offline, say) no longer moves the kept
  ones to the Trash.

## v0.5.22 — 2026-10-28

- **Fullscreen:** a button on the art (or F11, or a double-click on the art): Now Playing fills
  the screen; the sidebar, the side panel and the header step aside, and the player bar fades
  away until you move the pointer. A music video fills the whole screen, on black. Esc, F11, the
  button or a double-click leave it.
- **Fix missing:** Liked Songs shows "Fix missing (N)" while any liked song's file or music video
  isn't on this computer; it saves them in the background (with their lyrics and covers) and
  counts down.
- Your own build saves liked songs, their music videos (720p) and their lyrics files again: the
  builds from v0.5.15 to v0.5.21 were made without that option by mistake, so 7 songs liked since
  then only streamed. They're saved now, one at a time in the background.

## v0.5.21 — 2026-10-28

- **Autoplay when the queue ends:** when the queue's last song plays, about 25 songs like it (its
  radio) are added after it, so the music carries on (and Next works). It leaves out songs played
  lately, songs you keep skipping and songs already queued; it stays out of the way with Repeat
  on. On by default: Autoplay When the Queue Ends, in the menu (Between Songs).

## v0.5.20 — 2026-10-28

- **The window never freezes on the sound server again:** stopping a song (to start the next, to
  skip, or for the mixer) used to wait on the sound server right on the window's thread; when the
  JBL's EQ chain wouldn't start, that wait lasted its whole 30 s ("Not Responding"). The song
  before now stops on a worker thread and a fresh player takes its place at once.

## v0.5.19 — 2026-10-28

- **The effects are in the Mixer now, one of each:** Low cut, High cut and Reverb sit under the
  parts, for every song (split or not). They're the Speaker EQ's own, for the speaker playing:
  the same room you liked, heard at once, and they move only the effects (Bass, Mid, Treble and
  the presets stay in Speaker EQ). Each is off at its left; the Mixer's reset turns them off too.
- **No more delay on the parts:** the Mixer's own reverb (which made Drums, Bass, Other and
  Vocals take seconds to be heard) is gone; the reverb is the Speaker EQ's, after the player.
- **Speaker EQ is its own app:** the sidebar's Speaker EQ opens it (bass, mid, treble and presets
  for each speaker, with its live graph).
- **Closing the window keeps the music playing:** the window goes, the song plays on. GNOME's top
  bar, the lock screen and the keyboard's media keys show the song and play, pause and skip it
  (and bring the window back); opening the app again does too. Quit (in the menu, or Ctrl+Q)
  stops the music. Closed with nothing playing, it quits as before.
- The song you left off on shows its lyrics as soon as the app opens (they waited for Play).
- Now Playing's lyrics check themselves once a second: if they're ever left invisible, or the
  line being sung is out of sight, they're put back (and it's written in the app log).
## v0.5.18 — 2026-10-28

- **The Mixer's reverb is the EQ's room now:** the same room and blend as the EQ's, instead of
  a brighter, metallic-sounding synthetic reverb. (Replaced in v0.5.19 by the EQ's own reverb.)
- **Played before, above the queue:** the Queue panel shows the last 5 songs you played (the
  newest just above the one playing, a little faded), so a song stuck in your head is one tap
  away. Tapping it plays it now and the queue carries on after it; the song it interrupts isn't
  counted as skipped. (Five covers most going-back: in your own history, nearly half of the songs
  played again within an hour came back right after the next song.)

## v0.5.17 — 2026-10-28

- **Reverb in the Mixer:** a Reverb slider under the parts puts the whole song in a room, from
  none to a big hall. It's blended in (at most 40 % room, the rest the song itself), so it's
  never louder; it starts off, carries on to the next songs, and the Mixer's reset turns it off.
- **Fits the laptop screen again:** with the side panel open on a laptop (1536 px at 125 %),
  the window grew wider than the screen (the big cover forced 540 px a half). The cover now
  grows to 540 only when its half has the room.

## v0.5.16 — 2026-10-28

- **The lava lamp feels the beat:** each kick pushes the flow ahead for a moment and swells the
  picture at once, fading evenly over a third of a second (the way the keyboard's beat lights
  follow a song); strong bass still makes it all flow faster. Even swollen, it never zooms in
  past 1.15 times the page.

## v0.5.15 — 2026-10-28

- **Search your liked songs:** Liked Songs has its own search box on top. Only the songs that
  match stay listed (title, artists or album; accents, case and spaces aside, so "loki" finds
  "Lo ki"), the A–Z rail follows them, and Play / Shuffle play just those.
- **Search shows your liked songs first:** the ones that match, before the lossless and YouTube
  Music results.
- **The background flows like Apple Music's:** the album art, blurred, with soft round patches of
  it on top, each slowly turning and wandering, so its colours flow into each other. Calm, always
  moving, never jumping; strong bass makes it flow faster (up to 4 times), easing in and out.
  No more spin or zoom jumps on the beat.
- **The background never zooms in too far:** at most 1.15 times the page, so the art's own
  shapes always show (it used to fill the page with one colour).
- Now Playing doesn't offer "Split this song" for a song already waiting in the split queue.
- The app log writes a line once, not a dozen times a second when the mixer reloads.

## v0.5.14 — 2026-10-28

- **Lyrics light word by word, each word whole** the moment it's sung (no sweep across the letters).
- **Lyrics like Apple Music's:** Now Playing is two true halves, the art centred in one and the
  lyrics in the other, left-aligned in a block in the middle of their half (lines grow from
  their left edge, so they don't slide). The side panel's lyrics too.
- The sidebar and the player bar keep their colour when the window isn't focused (they turned
  a different shade from the page).
- **The app keeps a log** (~/.cache/radiance-music/app.log): each song loaded, played, ended or
  failed, crossfades, the mixer, and which speaker the sound went to.
- **Play and skip counts:** each song shows how often it was played ("12×" on its row; Now
  Playing and Track Info say "Played 12 times · skipped 3 times"). A skip is moving on before
  70 % of the song with more than 30 s left.
- **What you play shapes what plays:** every shuffle (Liked, albums, artists, mixes, the queue)
  still mixes at random but brings songs you play a lot earlier and songs you skip later; For You
  grows from the likes you play most; For You and song radios leave out songs you keep skipping.
- **A song never plays silently after a crossfade:** if the song's volume was left below yours
  outside a fade, it's put back at once (and written in the log).

## v0.5.13 — 2026-10-28

- Liked Songs: the A–Z rail is the scroll bar (no second one), and the letter it lights follows
  the songs at the top of the list as you scroll.

## v0.5.12 — 2026-10-28

- **The beat moves the background one way at a time:** each beat either spins it (sometimes the
  other way round) or zooms it in or out, picked at random, never both; the zooms go deeper.
- Fixed: a lyric line or title with a very long word could make the window too wide for the screen
  (its right side cut off); long words now break when they must.

## v0.5.11 — 2026-10-28

- Now Playing is two equal halves: the album art centred in one, the lyrics in the other (the art
  centred on the page when the window is too narrow for both).
- Fixed: in a window under about 1060 px the lyrics column stayed and the right side was cut off
  again (only one window-size rule applies at a time; each now carries the wider ones' changes).

## v0.5.10 — 2026-10-28

- No more lines between the sidebar, the page and the side panel, and no square box around the
  side panel: it takes the card's colour and sits inside its soft corners.

## v0.5.9 — 2026-10-28

- In a window under about 1300 px wide, the side panel (Queue, Lyrics, Splits) slides over the
  page instead of squeezing beside it, so nothing is cut off on the right.
- **The song's title, artists and quality sit on the album art**, over a soft shade so they
  read on any cover; the video button moved to the art's top corner.

## v0.5.8 — 2026-10-28

- Fixed: with the Mixer in the sidebar, a smaller window lost its right side (the volume and the
  window's buttons were cut off): the Mixer now fits the sidebar's width.
- **The Mixer works for every split song:** songs split before there were four parts get
  Music and Vocals sliders (they offered "Split this song" instead).
- **Song | Music video is gone:** a small video button on the album art plays the song's
  official video (it shows when there is one); the same button brings the song back.
- **Bigger album art** on Now Playing (up to 480 px; smaller windows still fit).
- In a narrower window (under about 1060 px) Now Playing shows the art without the big lyrics
  column, so nothing is cut off; the lyrics are in the side panel's Lyrics tab.
- **Bigger moves on the beat:** each beat turns the background about twice as far, and its zooms
  in and out go deeper.

## v0.5.7 — 2026-10-28

- The frame around the page (top, right, and the player bar at the bottom) is the sidebar's
  own colour, in the light look too: the page card has a hairline edge instead of a shadow
  that greyed its surroundings, and the player bar no longer has its own background and line.
- The split card at the bottom left only shows when there's something to say.
- In the light look, Now Playing's switches and Mixer read on its dark background (they were
  white on white).
- What's New (About) shows this changelog's latest versions, updated with every build.
- The Mixer sits at the bottom left too, above the split card, on every page (for a split song).

## v0.5.6 — 2026-10-28

- **Edit and create lyrics** (a song's menu → Edit Lyrics…): every line with when it starts,
  its words and **who sings it** (the song's artists, or any name you add). Paste Lyrics
  fills it from the clipboard; **Tap Timing** sets each line's time as you press Enter while
  the song plays. Your lyrics are kept as yours (never replaced by a lookup); once the song is
  split, every word is timed by the split server, your singers kept.
- **Who sings shows in the lyrics:** the singer's name above a line when the voice changes.
- **What's being split sits at the bottom left**, under the sidebar, on every page: the song,
  its progress, and a click opens the Splits tab. The Splits tab's top row says what's
  happening now, including the copy to the server's library and the word timing after a split.
- The page inside the card has soft corners too.
- **One Mixer instead of Song / No vocals / Vocals only:** a row that opens (closed at first)
  with a slider per part, and a reset button that puts them all back at full. No vocals is the
  Vocals slider at zero; your mix carries on to the next songs.
- **The background turns either way:** now and then a beat reverses its spin, or zooms it in
  or out for a moment.

## v0.5.5 — 2026-10-28

- **Playlists:** make your own (sidebar → New Playlist, or a song's menu → Add to Playlist →
  New Playlist…). Each has Play, Shuffle, Rename and Delete; remove a song from its menu.
- **Liked Songs has Play and Shuffle.**
- **Search suggestions:** before you type, your artists (the most liked first); while you
  type, your matching liked songs and artists, then what YouTube Music suggests.
- **Now Playing:** the art sits at the left with the song's details, the column (and the
  mixer) no wider than 420 px, and what's being split shows above the art, with a button to
  the new **Splits** tab in the side panel: the song being split (how far), then the queue.
- **Storage and Folder** (main menu): how much room your songs, videos, split parts, lyrics
  and the app take, the space left; and **Change…** to keep your songs in another folder.
  Everything moves there (on another drive each file is copied, checked, then taken away)
  and the app follows.
- Fixed: a setting saved could drop the split server's library folder.

## v0.5.4 — 2026-10-28

- **Lyrics keep up:** a word lights as it starts (not letter by letter across the whole word),
  and the next line comes to the middle just before it's sung. No line across the top of the
  lyrics any more.
- **Right lyrics for the right song:** a lyrics match must be by the same artist (a song that
  only shares the title, like another "Moolah", no longer counts). Lyrics kept before are
  checked once; another song's go to the Trash and the right ones take their place.
- **The background jumps with the beat** instead of flashing: each beat fast-forwards its spin
  and sway for a moment.
- **A lyrics mark on every song:** orange, word by word; white, line by line; faint, text only.
  Point at it for what it means.
- **The page is a card with soft corners** on the sidebar's colour (an inset layout).
- The freeze recorder also notes shorter hitches (0.6 s or more).

## v0.5.3 — 2026-10-28

- **Real word-by-word timing.** After a song is split, the split server times every word
  against the song's Vocals part with Meta's MMS aligner (one open model for 1100+ languages,
  Tagalog included). Measured on hand-marked songs: 92 % of words within 0.3 s (median
  0.04 s) when the lyrics are timed by line, 87 % from plain text alone (before: about half).
  The Lyricsfile comes back next to the song and into the server's library; songs split
  earlier get theirs in the background.
- **More songs have lyrics:** when LRCLIB has none, the app takes the lyrics YouTube Music
  shows (7 of the 14 liked songs that had none), and the aligner times them.
- **Your own build keeps every liked song's music video** (720p, H.264, about 30 MB each)
  next to the song, one at a time in the background at most 1.5 MB/s, and each new like
  after that; Music video then plays from the file, offline. Every liked song keeps its
  lyrics file too (a Lyricsfile, or the plain text as .lrc). Unliking moves the video to the
  Trash with the rest.

## v0.5.2 — 2026-10-28

- **The background spins with the beat.** Now Playing's blurred art keeps turning, faster
  when the music is busy, and every beat gives it a twist; each hit also swells it more
  (12 %) and lights it more than before.
- **Your data is in a database now** (SQLite, ~/.local/share/radiance-music/radiance.db):
  liked songs, every play (the whole history is kept now, not just the last 1000), where you
  left off, and the server library's list. It can't be left half-written. The JSON files
  from before are read in once and left as they were, as a backup. Settings stay in
  settings.json.
- The freeze recorder now also writes every thread's stack (through gdb) when the window is
  stuck 4 s, with function names and lines, so the freeze can be pinned down.

## v0.5.1 — 2026-10-28

- **The split server keeps a library of every split**, ready to host: one folder per song in
  its library folder with its four parts, the No vocals mix, its Lyricsfile (words
  timed from the vocals) or plain lyrics, its cover and info.json. Songs split earlier are
  copied once, a minute after the app starts. Copies go over the home network, never the
  internet line; files already there aren't sent again.
- A freshly split song gets its words timed right away, not only when it's played.
- **Freezes get written down:** if the window ever stops answering for 2 s, what it was doing
  goes into ~/.cache/radiance-music/stalls.log at once (even if you force quit), so it can be
  fixed.
- The word-by-word lyrics no longer ask the player for the time 30 times a second (a call that
  can wait on the sound system); they count on from the regular reading.
- Fixed: a long song title (all the featured names) squeezed the lyrics into a sliver.

## v0.5.0 — 2026-10-28

- **Liked Songs are always A to Z** (by title; accents and leading quotes don't count, songs
  starting with a number go under #), with an **A–Z rail** beside the list: click a letter, or
  drag along the rail, to jump there. Letters with no songs stay faint.
- **Every artist on a song shows**, the featured ones too ("Tamaraw (feat. KKRAM)" reads
  "Uncle Dags & KKRAM"); pointing at a song shows all the names when the line is cut short,
  featured artists get their own Artists page, and a song's menu goes to each of them.
- **Words timed from the singing.** LRCLIB times only lines for almost every song (none of 49
  liked songs had real word times), so once a song is split, each word is placed where the
  voice actually sings it, from its Vocals part, and kept in the song's Lyricsfile. Measured
  on hand-marked songs: words land 0.50 s off on average instead of 0.69 s, and no longer
  light up during instrumental gaps.

## v0.4.0 — 2026-10-28

- **Lyrics, word by word.** The line being sung sits in the middle, full size, and its words
  light up as they're sung; lines grow as they rise from below and shrink as they move up
  into the ones already sung. Uses LRCLIB's Lyricsfile when it has each word's timing (also
  two voices at once); otherwise the words follow the line's timing. Saved songs keep their
  lyrics as a Lyricsfile next to the song; songs saved earlier fetch theirs once more.
- **The background follows the beat.** Each kick and hit swells and lights Now Playing's
  blurred art, busy parts of a song move it faster, and it drifts more than before.
- **The side panel has Queue and Lyrics tabs**; the bar's lyrics button opens the Lyrics tab.
- **Split songs come in four parts** (Drums, Bass, Other, Vocals) with a **mixer**: a slider
  per part, live while the song plays. No vocals and Vocals only are presets of it.
- **Song | Music video** on Now Playing, when the song has an official video (or the video
  has its song); it carries on from the same spot.
- **Splitting:** a progress bar with the time left, a **pause / resume** button for the
  splitting queue, and **this computer helps the server** (it splits part of each song, two
  threads at idle priority, so whatever you're doing always comes first).
- Quality and split marks are small icons that explain themselves when you point at them.
- Fixed: a song's cover could vanish when songs changed quickly; lyrics and covers of saved
  songs with a dot in the name ("Mood (feat. Muric)") weren't found again.

## v0.3.0 — 2026-10-27

- **A new look, like Apple Music.** A sidebar with Home, Search and Now Playing, your
  library (Liked Songs, History, Albums, Artists, Genres) and the EQ. Pages use the whole
  screen.
  - **Home** greets you over your mix's own colours, its covers fanned out, with Play and
    Shuffle, then rows of Made for you, Recently played, Your liked songs, albums, artists
    and genres.
  - **Albums, Artists and Genres** are grids; each opens its own page with Play and Shuffle.
  - **Search** opens on tiles to browse by (Pinoy hip-hop, OPM, R&B, K-pop…).
  - Two dark looks in the menu: **Stone** (warm greys) and **True Black**.
- **History:** every song you play, with the time, by day (Today, Yesterday, …).
- **Last played is kept.** The queue, the song and how far in are saved; open the app, press
  play and it carries on from the same spot.
- **Splitting never wastes the server.** A song's No vocals / Vocals only halves are found
  wherever they were kept, halves finished on the server while the app was closed are fetched
  instead of split again, and a song being split there is waited for, not split twice.
  - A **Split** pill on songs whose halves are ready (**Splitting…** while it runs).
  - **Split this song** on Now Playing and **Split Vocals** in a song's menu.
  - **Liked songs are split by themselves**, one after another, kept next to the song.
- Clicking the song in the player bar opens Now Playing.
- Now Playing's moving background no longer lags or flickers; album art is kept on disk, so it
  shows at once next time.

## v0.2.0 — 2026-10-27

- A **menu on every song** (right-click, long-press or the Menu key): Play next, Add to queue,
  Like, Song radio, Go to artist or album, Show lyrics, Track info, Copy link, Show in Files.
- **The queue** in a side panel; **shuffle** and **repeat**.
- **For you:** songs like the ones you liked, from YouTube Music radio.
- **Now Playing** glows with the album art's colours; synced lyrics fade between songs.
- **Between songs** (menu): no transition, **Crossfade**, or **DJ mix**: the beats lined up,
  the tempo matched and the bass swapped halfway (a crossfade when the tempos are too far apart).
- **No vocals / Vocals only**, split by Demucs on a split server.
- **Volume** and mute (only Radiance Music's own sound; never louder than the system).
- **Music videos** in Search, played in Now Playing (720p).
- Liked songs by album, artist or genre (genres from Deezer).
- A song YouTube won't give is skipped with a short note; YouTube can use your browser's
  sign-in, so it stops asking to prove you're not a bot.
- YouTube songs start in a fraction of a second (IPv4 only on networks with broken IPv6).

## v0.1.0 — 2026-10-27

- Search any song: lossless recordings from the free libraries (Internet Archive FLAC) first,
  YouTube Music as the fallback.
- A liked song is saved to Music › Radiance Music with its synced lyrics (LRCLIB) and album art.
- Now Playing with the album art and the lyrics line by line; the EQ is your Speaker EQ.
