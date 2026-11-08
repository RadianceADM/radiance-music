# Windows beta stage 4c: servers, invite links, listening parties — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** the beta does what the Linux app does with Radiance Music Servers: join one by its invite, see its songs, play, save, add and split them there, open `radiancemusic://join` invites, and join or host a listening party in time with everyone.

**Architecture:** the protocol is already shared (`engine/src/servers.rs`: joining, the library, songs, uploads, splits, the party API, both WebSockets, the party clock and `step`). What the Linux app keeps in `src/ui/rms.rs` as screen state (the joined servers' libraries and failures, the events line, the party's state machine and its hold on the player) moves into the engine's controller for the beta, published as `Change::Servers` and `Change::Party`; the words move to `core`. The beta gets a Servers dialog, a page per server, the song menu's server items, the party card and the in-party view.

**Tech Stack:** Rust (core, engine, ffi via uniffi 0.31), C# WinUI 3; a real `rms` server (this repo's `rms/` crate) run on ribs02 for the checks.

**Spec:** docs/superpowers/specs/2026-11-05-windows-winui-design.md (stage 4, line 103; links, lines 128-129). Map of the Linux code: the stage 4c map (session notes 2026-11-08: rms.rs line references, the engine's shared pieces, the gap below).

## Rulings (made while planning)

- **Two betas, two halves:** 4c-1 (beta.9) servers and invites; 4c-2 (beta.10) listening parties. The party's state machine is the biggest piece to move and is checked on its own. Cost if wrong: one more VM pass.
- **The beta reads the same `settings.json` servers** as today's Windows app (they share it), so a server joined in today's app shows in the beta without joining again; leaving in either revokes this computer for both. Cost if wrong: none (it's how the settings file already works).
- **The Linux app keeps its own `rms.rs` session for now**; only the words move to `core` and the Linux app uses them. Moving 2,200 working lines under Linux users is separate work (its own issue). Cost if wrong: two copies of the session flow until then.
- **`radiancemusic://` stays registered to today's Windows app until stage 5** (the switch): two apps can't both own the protocol, and the spec keeps invites with the app that has servers. The beta opens a link it is given (a second start with the link, `Others.Show(link)`) and a pasted invite. Cost if wrong: a clicked invite opens today's app until the switch.
- **Where Servers lives in the beta:** each joined server is a sidebar row under Library (as Linux); "Servers…" is a Settings card (Settings → Servers, as Share Songs with a Phone sits in Settings → Help), since the beta has no main menu. Cost if wrong: a button's place.
- **The controller calls `servers::set_joined`** at start and on every change (the gap the map found: without it, server songs in History and the queue say "you left its server"). Cost if wrong: none.

## Rulings (made while building 4c-2)

- **The party lives in its own engine module** (engine/src/controller/party.rs, beside rms.rs), a straight port of the Linux app's join_party, party_news, party_sync, party_load, party_listen and leave_party; the controls go to it from toggle_play, skip, seek and play_current (a song of your own leaves the party). Cost if wrong: none (same behaviour, one place).
- **Party cards are kept in the shared view** (every joined server's, made on each party or server change), never asked of the engine thread synchronously. Cost if wrong: a little work per change.
- **The stand-in party server in the engine tests** speaks the same API and WebSocket messages as rms/src/party.rs (state on every change, the clock without a key) and serves a WAV with Range support; each stand-in has its own server id (the tests share settings.json). Cost if wrong: a test passing on a server that behaves differently; the VM check against the real rms covers it.
- **Left from 4c-1, moved here as Task 4b:** the song menu's server items other than Add to the Party (Save to Your Library, Add to NAME, Split on NAME and the server's split queue). They weren't in beta.9 (its CHANGELOG lists what was). Cost if wrong: beta.10 waits on them, or they follow in beta.11.

## Global Constraints

- Words are the Linux app's, moved into `core` (`src/ui/rms.rs`: the Servers dialog, the four permission rows "Play its songs and join its parties" / "Add songs to it" / "Split songs on it" / "Host a listening party" with "Yes" / "Off for you: its owner's choice", the server page lines, the party card, the in-party view, `server_reason`).
- The same protocol as the Linux app and the phone (`engine/src/servers.rs`, rms/src/lib.rs routes 223-260); no change to the server.
- Every button carries an icon; the animation blueprint on new views; no sharp visuals (round reaction chips, the reaction icons from data/icons/rms-react-*.svg).
- Builds and tests on ribs02 (Rust) and the ribs02 Windows VM (C#), against a real `rms` on ribs02 (reached from the VM at 10.0.2.2); ribs03 only after a dialog.

## Review Focus

1. A joined server that's down when the beta starts: the app starts as fast as without it; the server's page shows the problem with Try Again; its songs in History say why they won't play.
2. A key revoked on the server (signed out): the page says so with Servers…, nothing keeps retrying in a loop, and the queue moves past its songs.
3. An invite link arriving while the start screen is up opens Servers… after it, the invite filled in; a link that isn't an invite says so in a toast.
4. In a party, the network drops: "Lost the party: joining again…", the music never stutters in a loop, and leaving brings back your own queue at its place.
5. Two computers in one party: both play the same song within the party's step (`servers::IN_STEP`), and a non-host can't play, pause or skip unless the host lets everyone.

---

### Task 1: The words in core

**Files:** core/src/server_words.rs (new), core/src/lib.rs, src/ui/rms.rs, src/ui.rs.

**Produces:** constants and functions for every string in the Global Constraints; `can_words(&Me) -> String`, `off_words(&Me) -> Option<String>`, `and_list(&[&str]) -> String`, `joined_line`, `songs_line(n, me)`, `party_line`, `listening(n)`, `sync_line(ms, clock_ms)`.

- [ ] Tests: `the_permission_rows_say_yes_or_off`, `a_servers_page_lines`, `the_party_card_and_its_lines` (the Linux app's exact words).
- [ ] The Linux app uses them (its strings in rms.rs come from core).

### Task 2: Servers in the engine (4c-1)

**Files:** engine/src/controller.rs, engine/src/servers.rs (only if a helper is missing), engine/tests/controller.rs.

**Produces:** `Controller::servers() -> ServersState { joined: Vec<ServerRow { id, name, address, me_line, can: [bool;4], problem: Option<String>, signed_out }>, joining: Option<String> }`; `server_join(link)`, `server_leave(id)`, `server_refresh(id)`, `server_page(id, filter) -> ServerPage { name, line, can, problem, songs: Vec<(Track, added_by)> }`; `play_server(id, index, shuffle)`; `add_to_server(id, key)`; `split_on_server(key)`; `set_joined` at start and on change; the events line (`servers::events`) keeps each library fresh, `Change::Servers`.

- [ ] Tests (the stand-in HTTP server of `joining_and_the_key_on_every_call`): `joining_by_invite_lists_the_server`, `a_servers_songs_play_and_are_saved_by_liking`, `a_server_song_in_history_plays_after_a_restart` (set_joined), `a_server_thats_down_says_so_and_the_app_goes_on`, `a_revoked_key_says_signed_out_once`, `leaving_forgets_it`.

### Task 3: ffi + the beta's Servers dialog, server pages and song menu (4c-1)

**Files:** ffi/src/lib.rs; windows/RadianceMusic.Windows/Dialogs/ServersDialog.cs (new); Pages/ServerPage.xaml(.cs) (new); MainWindow (sidebar rows); Controls/SongMenu.cs; Pages/SettingsPage.xaml.cs (Servers card); DevActs.cs.

- [ ] ServersDialog: intro, Join a Server (invite box, Join with Enter, Joining… / joined / already / moved / error lines), Your Servers (an Expander each: subtitle, the four permission rows, Open Its Page, Leave with its confirm).
- [ ] ServerPage: name, songs line, can/off lines, the problem card (Try Again, Servers…), Play / Shuffle / Refresh, Search NAME, the songs with "added by".
- [ ] Song menu: Split on NAME, Save to Your Library, Add to NAME; the quality pill "From NAME · format".
- [ ] DevActs `servers`, `server:NAME`, `join:LINK`; VM checks against `rms` on ribs02: join by invite, the page, play one, save one, add one, leave.

### Task 4: Invite links in the beta (4c-1)

**Files:** windows/RadianceMusic.Windows/Program.cs, App.xaml.cs (`AskTheOther(link)`, `Others.Show(link)`), Dialogs/ServersDialog.cs, core (the link's words), windows/installer-beta.nsi (a comment: the protocol stays with today's app until stage 5).

- [ ] A `radiancemusic://join?u=…` argument: a first start opens Servers… with the invite after the start screen; a second start hands it to the running beta, which comes forward and does the same; any other link: the toast "That link isn't a Radiance Music Server invite…".
- [ ] Tests: ffi `an_invite_link_opens_servers` (the link parsed to its invite); VM: a second start with the link.

### Task 5: Listening parties in the engine (4c-2)

**Files:** engine/src/controller.rs (or engine/src/party.rs, new), engine/tests/controller.rs.

**Produces:** `Controller::party() -> PartyState { server, hosting, status, sync_line, now: Option<(Track, added_by)>, controls: bool, hear_here: bool, listening: Vec<String>, up_next: Vec<(Track, added_by, can_move, can_remove)>, reactions: Vec<(String, String)>, everyone_controls, can_end }`; `party_start(server, everyone)`, `party_join(server)`, `party_leave()`, `party_end()`, `party_control(play|pause|skip|seek)`, `party_add(key)`, `party_move(i, later)`, `party_remove(i)`, `party_react(kind)`, `party_hear(on)`, `party_everyone(on)`; joining keeps your own queue and place (Before) and leaving brings them back; the player follows the party with `servers::step` every second (lead, slack, hold), the clock measured again every minute; Previous off, Next/Play/Pause/seek go to the party; picking your own song leaves it ("You left the party to play your own music").

- [ ] Tests (stand-in party API + socket): `joining_a_party_plays_its_song_at_its_place`, `leaving_brings_back_your_own_music`, `only_the_host_controls_unless_everyone_may`, `a_lost_party_joins_again`, `picking_your_own_song_leaves_the_party`, `the_party_follows_the_clock_offset`.

### Task 6: ffi + the beta's party (4c-2)

**Files:** ffi/src/lib.rs; windows/RadianceMusic.Windows/Controls/PartyCard.cs (new), Pages/PartyPage.xaml(.cs) (new), Controls/PlayerBar.cs (party mode), Controls/SongMenu.cs (Add to the Party), Assets (the six reaction icons), DevActs.cs.

- [ ] The party card on a server's page (nobody's party / NAME is hosting / Start a Party with Let everyone play, pause and skip); the in-party view (heading, status pill, sync line, now, controls, Hear It Here, six reactions and recent chips, Listening (N), Up next with Earlier / Later / Take it out, host's control switch, Leave, End with its confirm).
- [ ] The player bar in a party (Previous off; Next, Play/Pause and the seek bar go to the party).
- [ ] VM checks against `rms` on ribs02 with a second listener (a test client joined by the API): start, join, both in step (the sync line), react, skip, leave, end.

### Task 7: Docs, the full check, the builds

- [ ] FEATURES: Join a server… ✅ beta; radiancemusic:// invite links ✅ beta (clicked links open it after stage 5); Listening parties ✅ beta.
- [ ] CHANGELOG beta 0.8.0-beta.9 (servers, invites) and beta.10 (parties); the full Rust suite in a quiet window; the VM build + C# tests; the `rms` test server and its data deleted from ribs02 after each check.
