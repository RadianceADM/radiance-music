//! Radiance Music Servers, invite links and listening parties (stage 4c, #45): the Linux app's words
//! (src/ui/rms.rs, and the song menu's, the quality pill's and the bar's in src/ui.rs), now core's,
//! for the Windows beta to say the same.

// ---- the Servers dialog (rms.rs servers_dialog_with) and invite links
pub const SERVERS_TITLE: &str = "Servers";
/// The menu item, and the button on a server's page when its key was signed out.
pub const SERVERS_MENU: &str = "Servers…";
pub const SERVERS_INTRO: &str = "A Radiance Music Server is your own, or a friend's: its songs play here, it splits songs for everyone on it, and it holds listening parties. Its owner sends you an invite link.";
pub const JOIN_TITLE: &str = "Join a Server";
pub const JOIN_DESCRIPTION: &str = "Paste the invite link its owner sent you. Each link works once, on one device.";
pub const INVITE_LINK: &str = "Invite link (https://…/join/…)";
pub const JOIN: &str = "Join";
pub const JOINING: &str = "Joining…";
pub const YOUR_SERVERS: &str = "Your Servers";
pub const NO_SERVERS: &str = "No servers yet";
pub const NO_SERVERS_SUB: &str = "Paste an invite link above to join one.";
/// A radiancemusic:// link that isn't an invite (a toast).
pub const NOT_AN_INVITE: &str = "That link isn't a Radiance Music Server invite. Ask the server's owner for a new one.";

/// An invite to a server this computer is on already.
pub fn already_on(name: &str) -> String {
    format!("This computer is on {name} already. Its songs are under Library in the sidebar.")
}

/// The dialog's line after a join.
pub fn joined_line(name: &str) -> String {
    format!("Joined {name}. Its songs are under Library in the sidebar.")
}

pub fn joined_toast(name: &str) -> String {
    format!("Joined {name}")
}

/// The dialog's line when the invite was a joined server's, at a new address.
pub fn moved_line(name: &str, address: &str) -> String {
    format!("{name} moved to {address}: the same server, your songs from it as they were.")
}

/// The toast when a joined server is found at a new address.
pub fn moved_toast(name: &str, address: &str) -> String {
    format!("{name} is at {address} now: the same server, your songs from it as they were")
}

/// A server's row in the sidebar, its tooltip.
pub fn nav_tooltip(name: &str, address: &str) -> String {
    format!("{name}: its songs, its splitting and its listening party ({address})")
}

// ---- a server's row in Servers…: its state, what you can do there, Open and Leave
pub const OPEN_PAGE: &str = "Open Its Page";
pub const OPEN_PAGE_TIP: &str = "Its songs and its listening party";
pub const LEAVE: &str = "Leave";
pub const LEAVE_TIP: &str = "This computer forgets its key to this server";
pub const CANCEL: &str = "Cancel";
pub const CAN_PLAY_ROW: &str = "Play its songs and join its parties";
pub const CAN_ADD_ROW: &str = "Add songs to it";
pub const CAN_SPLIT_ROW: &str = "Split songs on it";
pub const CAN_HOST_ROW: &str = "Host a listening party";
pub const YES: &str = "Yes";
pub const OFF_FOR_YOU: &str = "Off for you: its owner's choice";
pub const ASKING_WHAT_YOU_CAN: &str = "Asking the server what you can do there…";

/// The row's subtitle once the server said who you are there.
pub fn row_subtitle(address: &str, owner: bool, name: &str) -> String {
    format!("{address} · you're {}", if owner { "its owner" } else { name })
}

/// The row's subtitle while the server is asked.
pub fn row_checking(address: &str) -> String {
    format!("{address} · checking…")
}

/// The four rows of what you can do on a server: (allowed, its title), in the dialog's order.
pub fn permission_rows(can_add: bool, can_split: bool, can_host: bool) -> [(bool, &'static str); 4] {
    [(true, CAN_PLAY_ROW), (can_add, CAN_ADD_ROW), (can_split, CAN_SPLIT_ROW), (can_host, CAN_HOST_ROW)]
}

/// A permission row's subtitle.
pub fn permission_answer(yes: bool) -> &'static str {
    if yes {
        YES
    } else {
        OFF_FOR_YOU
    }
}

pub fn leave_heading(name: &str) -> String {
    format!("Leave {name}?")
}

pub fn leave_body(name: &str) -> String {
    format!("This computer forgets its key to {name}: its songs, splitting and parties go from here. Songs you saved from it stay in your library. To come back, ask its owner for a new invite.")
}

/// After Leave (`told`: the server took it; else it's forgotten here only).
pub fn left_toast(name: &str, told: bool) -> String {
    if told {
        format!("Left {name}")
    } else {
        format!("Left {name} on this computer (the server didn't take it: its owner can remove this computer under People)")
    }
}

/// "a", "a and b", "a, b and c".
pub fn and_list(items: &[&str]) -> String {
    match items.len() {
        0 => String::new(),
        1 => items[0].to_string(),
        n => format!("{} and {}", items[..n - 1].join(", "), items[n - 1]),
    }
}

/// What you can do on a server, in plain words.
pub fn can_words(can_add: bool, can_split: bool, can_host: bool) -> String {
    let mut can = vec!["play its songs"];
    if can_add {
        can.push("add songs");
    }
    if can_split {
        can.push("split songs");
    }
    can.push(if can_host { "join and host listening parties" } else { "join listening parties" });
    format!("You can {}.", and_list(&can))
}

/// What the owner turned off for you there ("" when nothing is, and for its owner).
pub fn off_words(owner: bool, can_add: bool, can_split: bool, can_host: bool) -> String {
    let mut off = vec![];
    if !can_add {
        off.push("adding songs");
    }
    if !can_split {
        off.push("splitting");
    }
    if !can_host {
        off.push("hosting parties");
    }
    if off.is_empty() || owner {
        String::new()
    } else {
        format!("Its owner turned off {} for you.", and_list(&off))
    }
}

/// A server page's line under its name: what you can do, then what's off for you.
pub fn permissions_line(owner: bool, can_add: bool, can_split: bool, can_host: bool) -> String {
    [can_words(can_add, can_split, can_host), off_words(owner, can_add, can_split, can_host)].iter().filter(|x| !x.is_empty()).cloned().collect::<Vec<_>>().join(" ")
}

// ---- a server's page (rms.rs render_server, filter_server)
pub const TRY_AGAIN: &str = "Try Again";
pub const PLAY: &str = "Play";
pub const SHUFFLE: &str = "Shuffle";
pub const REFRESH: &str = "Refresh";
pub const REFRESH_TIP: &str = "Ask the server again for its songs and its party";
pub const ADD_TO_PARTY_TIP: &str = "Add to the party's queue";
/// The songs' line when they play and nothing else needs saying.
pub const SONGS_HINT: &str = "Click one to play it. Like it (the heart) to save it, with its parts, lyrics and cover, into your own library.";

pub fn song_count(n: usize) -> String {
    format!("{n} song{}", if n == 1 { "" } else { "s" })
}

/// Who you are on it (`name`: your name there).
pub fn who_you_are(owner: bool, name: &str) -> String {
    if owner {
        "Your own server".to_string()
    } else {
        format!("You're on it as {name}")
    }
}

/// The line under a server's name: its songs (once they came) · who you are there (once it said; owner, your name).
pub fn page_sub(songs: Option<usize>, me: Option<(bool, &str)>) -> String {
    let mut sub = vec![];
    if let Some(n) = songs {
        sub.push(song_count(n));
    }
    if let Some((owner, name)) = me {
        sub.push(who_you_are(owner, name));
    }
    sub.join(" · ")
}

pub fn search_placeholder(name: &str) -> String {
    format!("Search {name}")
}

/// A song's subtitle with who added it (`before`: its byline or artist, maybe empty).
pub fn added_by(before: &str, by: &str) -> String {
    if before.is_empty() {
        format!("Added by {by}")
    } else {
        format!("{before} · added by {by}")
    }
}

pub fn getting_songs(name: &str) -> String {
    format!("Getting the songs on {name}…")
}

/// No songs there, and you may add one.
pub fn no_songs_add(name: &str) -> String {
    format!("No songs on {name} yet. Add one of yours: right-click a liked song, then “Add to {name}”.")
}

pub fn no_songs(name: &str) -> String {
    format!("No songs on {name} yet.")
}

pub fn no_match(name: &str, q: &str) -> String {
    format!("No songs on {name} match “{q}”.")
}

/// The songs' line when you're in the server's party (a click adds one to it).
pub fn party_songs(n: usize) -> String {
    format!("{n} songs. Click one to add it to the party.")
}

pub fn some_match(shown: usize, n: usize) -> String {
    format!("{shown} of {n} songs match.")
}

/// The songs as last sent, when the server isn't answering now.
pub fn as_last_sent(name: &str) -> String {
    format!("As {name} last sent them.")
}

// ---- why a server's song won't play (rms.rs server_reason, ui.rs)
pub const LEFT_ITS_SERVER: &str = "you left its server";

pub fn not_joined_anymore(name: &str) -> String {
    format!("this computer isn't joined to {name} anymore")
}

pub fn not_on_it_anymore(name: &str) -> String {
    format!("it isn't on {name} anymore")
}

pub fn not_answering(name: &str) -> String {
    format!("{name} isn't answering")
}

// ---- a server's song in the song menu, its pill and its details (ui.rs)
pub const MENU_ADD_TO_PARTY: &str = "Add to the Party";
pub const SAVE_TO_LIBRARY: &str = "Save to Your Library";
/// A server's name when its song's server isn't known (left).
pub const A_SERVER: &str = "a Radiance Music Server";
pub const SERVER_SONG_BODY: &str = "Streams from the server you joined. Like it to save it, with its parts, lyrics and cover, into your own library.";
pub const SPLIT_ON_ITS_SERVER: &str = "Split on its server";
pub const SPLIT_ON_ITS_SERVER_BODY: &str = "Its parts are on the server: the Mixer, No vocals and Vocals only play them from there.";

pub fn split_on(name: &str) -> String {
    format!("Split on {name}")
}

pub fn add_to(name: &str) -> String {
    format!("Add to {name}")
}

/// The quality pill's title for a server's song.
pub fn from_server(name: &str, format: &str) -> String {
    format!("From {name} · {format}")
}

// ---- your songs split on a server (the toasts, the split card, the Splits tab)
pub fn parts_didnt_come(title: &str, why: &str) -> String {
    format!("“{title}”'s parts didn't come from its server: {why}")
}

pub fn split_already(title: &str, name: &str) -> String {
    format!("“{title}” is split on {name} already: its parts are there for everyone")
}

/// Asked to split (`note`: the server's word on its place in line).
pub fn splits_now(name: &str, title: &str, note: &str) -> String {
    format!("{name} splits “{title}”: {note}")
}

pub fn split_done(title: &str, name: &str) -> String {
    format!("“{title}” is split on {name}: its parts are there for everyone")
}

pub fn split_failed(name: &str, note: &str) -> String {
    format!("{name}: {note}")
}

pub fn taken_out(title: &str, name: &str) -> String {
    format!("“{title}” was taken out of {name}'s split queue")
}

/// The split card's line for your first song on a server (`state`: its job's, "splitting", "timing" or
/// waiting; `others`: your other songs on servers).
pub fn away_line(state: &str, title: &str, server: &str, note: &str, next_turn: Option<&str>, others: usize) -> String {
    let more = if others > 0 { format!(" ({others} more on servers)") } else { String::new() };
    let turn = next_turn.map(|n| format!(" Next turn: {n}.")).unwrap_or_default();
    match state {
        "splitting" => format!("Splitting “{title}” on {server}{more}"),
        "timing" => format!("Timing “{title}”'s words on {server}{more}"),
        _ => format!("“{title}” on {server}: {note}{turn}{more}"),
    }
}

/// The split card's bar while a server works on your song (`progress`: 0 to 1).
pub fn away_bar(state: &str, progress: f64) -> String {
    format!("{} · {:.0}%", if state == "timing" { "Timing the words" } else { "Splitting" }, progress * 100.0)
}

/// A Splits tab row for your song on a server: where it is (`ahead`: songs before it in line).
pub fn away_row(state: &str, ahead: Option<usize>, server: &str, note: &str, next_turn: Option<&str>) -> String {
    match (state, ahead) {
        ("splitting" | "timing", _) => format!("On {server} · {note}"),
        (_, Some(n)) => format!("On {server} · number {} in line{}", n + 1, next_turn.map(|t| format!(" · next turn: {t}")).unwrap_or_default()),
        _ => format!("On {server} · {note}"),
    }
}

// ---- your songs added to a server
pub fn adding(title: &str, name: &str) -> String {
    format!("Adding “{title}” to {name}…")
}

pub fn has_already(name: &str, title: &str) -> String {
    format!("{name} has “{title}” already")
}

/// It had the song; what was missing went (`went`: what_went's words).
pub fn had_already(name: &str, title: &str, went: &str) -> String {
    format!("{name} had “{title}” already; its {went} went with it")
}

/// What went with a song: its extras (as the server names them), and its parts.
pub fn what_went(extras: &[String], parts: bool) -> String {
    let mut w: Vec<&str> = extras.iter().map(String::as_str).collect();
    if parts {
        w.push("parts");
    }
    and_list(&w)
}

pub fn on_it_now(title: &str, name: &str, with_parts: bool) -> String {
    if with_parts {
        format!("“{title}” is on {name} now with its parts, for everyone there")
    } else {
        format!("“{title}” is on {name} now, for everyone there")
    }
}

/// A song split here: its parts went to the server's copy.
pub fn parts_on_it_now(title: &str, name: &str) -> String {
    format!("“{title}”'s parts are on {name} now, for everyone there")
}

// ---- the party card (a server's party, not joined)
pub const PARTY_HEADING: &str = "Listening party";
pub const NO_PARTY: &str = "Nobody's having a party right now. In one, everyone hears the same song at the same moment and anyone adds songs.";
pub const LET_EVERYONE: &str = "Let everyone play, pause and skip";
pub const START_PARTY: &str = "Start a Party";
pub const NOTHING_YET: &str = "Nothing's playing yet: add a song and it starts.";
pub const JOIN_YOUR_PARTY: &str = "Join Your Party";
pub const JOIN_THE_PARTY: &str = "Join the Party";

pub fn asking_party(name: &str) -> String {
    format!("Asking {name} about its party…")
}

pub fn hosting_off(name: &str) -> String {
    format!("Hosting a party is off for you on {name}: its owner's choice. You can join one when someone starts it.")
}

/// The card's heading when a party is on (`yours`: you host it).
pub fn party_heading(yours: bool, host: &str) -> String {
    if yours {
        "Your listening party is on".to_string()
    } else {
        format!("{host} is hosting a listening party")
    }
}

/// What the party plays (`artist` may be empty).
pub fn playing(title: &str, artist: &str) -> String {
    format!("Playing “{title}”{}.", if artist.is_empty() { String::new() } else { format!(" by {artist}") })
}

/// How many are in the party.
pub fn listening(n: usize) -> String {
    match n {
        0 => "Nobody's listening right now.".to_string(),
        1 => "1 person listening.".to_string(),
        n => format!("{n} people listening."),
    }
}

/// The card's line: what plays, who's listening, and joining.
pub fn party_line(now: &str, listening: &str) -> String {
    format!("{now} {listening} Join, and you hear it here in step with everyone.")
}

/// Under Join when you're in another server's party.
pub fn joining_leaves(host: &str) -> String {
    format!("Joining leaves {host}'s party.")
}

// ---- the party you're in (rms.rs render_in_party), its toasts and the bar's play button
pub const PILL_PLAYING: &str = "Playing";
pub const PILL_PAUSED: &str = "Paused";
pub const PILL_QUIET: &str = "Quiet";
pub const LOST_PARTY: &str = "Lost the party: joining again…";
pub const NOTHING_PLAYING: &str = "Nothing playing";
pub const ADD_A_SONG: &str = "Add a song from the list below: it starts at once.";
pub const PAUSE: &str = "Pause";
pub const SKIP: &str = "Skip";
pub const HEAR_IT_HERE: &str = "Hear It Here";
pub const STOP_HEARING: &str = "Stop Hearing It Here";
pub const HEAR_TIP: &str = "The party plays on either way; back on, it's in step again at once";
pub const UP_NEXT: &str = "Up next";
pub const NOTHING_UP_NEXT: &str = "Nothing up next";
pub const NOTHING_UP_NEXT_SUB: &str = "Click a song in the list below to add it: anyone in the party can.";
pub const EARLIER: &str = "Earlier";
pub const LATER: &str = "Later";
pub const TAKE_OUT: &str = "Take it out of the queue";
pub const ONLY_I_CONTROL: &str = "Only I Play, Pause and Skip";
pub const LET_EVERYONE_CONTROL: &str = "Let Everyone Play, Pause and Skip";
pub const LEAVE_PARTY: &str = "Leave the Party";
pub const YOU_LEFT_PARTY: &str = "You left the party: back to your own music";
/// A song of your own picked while in a party.
pub const LEFT_FOR_YOUR_OWN: &str = "You left the party to play your own music";
pub const END_PARTY: &str = "End the Party";
pub const END_PARTY_HEADING: &str = "End the party for everyone?";
pub const END_PARTY_BODY: &str = "Everyone in it goes back to their own music.";
pub const PARTY_ON_TOAST: &str = "The party is on. Add a song: it starts at once.";

/// The in-party view's heading (`host`: you host it; `host_name`: who does).
pub fn in_party_heading(host: bool, host_name: &str) -> String {
    if host {
        "You're hosting the party".to_string()
    } else {
        format!("You're in {host_name}'s party")
    }
}

/// The party's pill: playing, paused (a song in it) or quiet.
pub fn party_pill(playing: bool, has_song: bool) -> &'static str {
    if playing {
        PILL_PLAYING
    } else if has_song {
        PILL_PAUSED
    } else {
        PILL_QUIET
    }
}

/// Under the controls when only the host has them.
pub fn host_controls(host: &str) -> String {
    format!("{host} (the host) plays, pauses and skips.")
}

/// A reaction chip's tooltip (`name`: the reaction's).
pub fn reaction_tip(by: &str, name: &str) -> String {
    format!("{by}: {name}")
}

/// A listener's chip.
pub fn member(name: &str, host: bool) -> String {
    if host {
        format!("{name} · host")
    } else {
        name.to_string()
    }
}

pub fn listening_heading(n: usize) -> String {
    format!("Listening ({n})")
}

/// The bar's play button in a party (`control`: you play and pause it; `playing`: heard here now).
pub fn bar_tooltip(control: bool, playing: bool) -> &'static str {
    match (control, playing) {
        (true, true) => "Pause the party",
        (true, false) => "Play the party",
        (false, true) => "Stop hearing the party here",
        (false, false) => "Hear the party here",
    }
}

pub fn not_answering_try_again(name: &str) -> String {
    format!("{name} isn't answering. Try joining again in a moment.")
}

/// The party ended (`by`: its host, "" when that was you).
pub fn party_ended(by: &str) -> String {
    if by.is_empty() {
        "The party is over: back to your own music".to_string()
    } else {
        format!("{by} ended the party: back to your own music")
    }
}

/// The server took you out of the party (`why`: its words).
pub fn removed_from_party(why: &str) -> String {
    format!("{why} You're out of the party: back to your own music.")
}

/// Next pressed in a party only the host controls.
pub fn host_skips(host: &str) -> String {
    format!("{host} (the host) skips songs in this party")
}

pub fn party_song_cant_play(reason: &str) -> String {
    format!("The party's song can't play here: {reason}")
}

pub fn added_to_party(title: &str) -> String {
    format!("Added “{title}” to the party")
}

// ---- the party's sync line (rms.rs show_sync)
pub const NOT_HEARD_HERE: &str = "Not heard here: the party plays on. Hear It Here to be back in step.";
pub const WAITING_FOR_SONG: &str = "Waiting for a song.";
pub const STARTING: &str = "Starting…";
pub const GETTING_IN_STEP: &str = "Getting in step…";

/// Paused (`by_you`: you host it; else `host` did).
pub fn paused_by(by_you: bool, host: &str) -> String {
    format!("Paused by {}", if by_you { "you" } else { host })
}

/// Within the party's step (`drift_ms`: + ahead, − behind).
pub fn in_time(drift_ms: f64) -> String {
    format!("In time with the party ({:+.0} ms)", drift_ms)
}

pub fn catching_up(drift_ms: f64) -> String {
    format!("Catching up ({:+.0} ms)", drift_ms)
}

/// The line as shown: its words, then how well the server's clock is known (± ms).
pub fn sync_line(text: &str, clock_ms: i64) -> String {
    format!("{text} · clock ±{clock_ms} ms")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stage 4c: what you can do on a server, as the Linux app said it (Servers… and the server's page).
    #[test]
    fn the_permission_rows_say_yes_or_off() {
        // its owner: everything, nothing off
        assert_eq!(can_words(true, true, true), "You can play its songs, add songs, split songs and join and host listening parties.");
        assert_eq!(off_words(true, true, true, true), "");
        assert_eq!(off_words(true, false, false, false), "");
        assert_eq!(permissions_line(true, true, true, true), "You can play its songs, add songs, split songs and join and host listening parties.");
        // a member with everything on
        assert_eq!(off_words(false, true, true, true), "");
        // a member with everything off
        assert_eq!(can_words(false, false, false), "You can play its songs and join listening parties.");
        assert_eq!(off_words(false, false, false, false), "Its owner turned off adding songs, splitting and hosting parties for you.");
        assert_eq!(
            permissions_line(false, false, false, false),
            "You can play its songs and join listening parties. Its owner turned off adding songs, splitting and hosting parties for you."
        );
        // one or two off
        assert_eq!(can_words(true, false, true), "You can play its songs, add songs and join and host listening parties.");
        assert_eq!(off_words(false, true, false, true), "Its owner turned off splitting for you.");
        assert_eq!(off_words(false, false, true, false), "Its owner turned off adding songs and hosting parties for you.");
        assert_eq!((and_list(&[]), and_list(&["a"]), and_list(&["a", "b"]), and_list(&["a", "b", "c"])), (String::new(), "a".to_string(), "a and b".to_string(), "a, b and c".to_string()));
        // the four rows, Yes or off
        assert_eq!(
            permission_rows(false, true, false),
            [(true, "Play its songs and join its parties"), (false, "Add songs to it"), (true, "Split songs on it"), (false, "Host a listening party")]
        );
        assert_eq!((permission_answer(true), permission_answer(false)), ("Yes", "Off for you: its owner's choice"));
        assert_eq!(row_subtitle("music.example.com", true, "Bassi"), "music.example.com · you're its owner");
        assert_eq!(row_subtitle("music.example.com", false, "Ana"), "music.example.com · you're Ana");
        assert_eq!(row_checking("music.example.com"), "music.example.com · checking…");
        assert_eq!(ASKING_WHAT_YOU_CAN, "Asking the server what you can do there…");
        assert_eq!(left_toast("Home", true), "Left Home");
        assert_eq!(left_toast("Home", false), "Left Home on this computer (the server didn't take it: its owner can remove this computer under People)");
        assert_eq!(leave_heading("Home"), "Leave Home?");
        assert!(leave_body("Home").starts_with("This computer forgets its key to Home: its songs, splitting and parties go from here."));
    }

    /// Stage 4c: a server's page, its songs' lines as the Linux app said them.
    #[test]
    fn a_servers_page_lines() {
        let name = "Bassi's Server";
        assert_eq!((song_count(1), song_count(0), song_count(12)), ("1 song".to_string(), "0 songs".to_string(), "12 songs".to_string()));
        assert_eq!(page_sub(Some(12), Some((false, "Ana"))), "12 songs · You're on it as Ana");
        assert_eq!(page_sub(Some(1), Some((true, "Bassi"))), "1 song · Your own server");
        assert_eq!(page_sub(None, None), "");
        assert_eq!(getting_songs(name), "Getting the songs on Bassi's Server…");
        assert_eq!(no_songs_add(name), "No songs on Bassi's Server yet. Add one of yours: right-click a liked song, then “Add to Bassi's Server”.");
        assert_eq!(no_songs(name), "No songs on Bassi's Server yet.");
        assert_eq!(no_match(name, "yellow"), "No songs on Bassi's Server match “yellow”.");
        assert_eq!(party_songs(12), "12 songs. Click one to add it to the party.");
        assert_eq!(some_match(2, 12), "2 of 12 songs match.");
        assert_eq!(as_last_sent(name), "As Bassi's Server last sent them.");
        assert_eq!(SONGS_HINT, "Click one to play it. Like it (the heart) to save it, with its parts, lyrics and cover, into your own library.");
        assert_eq!(search_placeholder(name), "Search Bassi's Server");
        assert_eq!(added_by("", "Ana"), "Added by Ana");
        assert_eq!(added_by("Coldplay · Parachutes", "Ana"), "Coldplay · Parachutes · added by Ana");
        assert_eq!(joined_line(name), "Joined Bassi's Server. Its songs are under Library in the sidebar.");
        assert_eq!(already_on(name), "This computer is on Bassi's Server already. Its songs are under Library in the sidebar.");
        assert_eq!(moved_line("Home", "home.example.com"), "Home moved to home.example.com: the same server, your songs from it as they were.");
        assert_eq!(nav_tooltip("Home", "10.0.0.2:8080"), "Home: its songs, its splitting and its listening party (10.0.0.2:8080)");
        assert_eq!(not_joined_anymore("Home"), "this computer isn't joined to Home anymore");
        assert_eq!((split_on("Home"), add_to("Home"), from_server("Home", "FLAC")), ("Split on Home".to_string(), "Add to Home".to_string(), "From Home · FLAC".to_string()));
        // your songs split there, and added
        assert_eq!(away_line("splitting", "Yellow", "Home", "", None, 2), "Splitting “Yellow” on Home (2 more on servers)");
        assert_eq!(away_line("timing", "Yellow", "Home", "", None, 0), "Timing “Yellow”'s words on Home");
        assert_eq!(away_line("waiting", "Yellow", "Home", "Number 3 in line.", Some("Ana"), 0), "“Yellow” on Home: Number 3 in line. Next turn: Ana.");
        assert_eq!((away_bar("splitting", 0.426), away_bar("timing", 1.0)), ("Splitting · 43%".to_string(), "Timing the words · 100%".to_string()));
        assert_eq!(away_row("waiting", Some(2), "Home", "", Some("Ana")), "On Home · number 3 in line · next turn: Ana");
        assert_eq!(away_row("splitting", Some(0), "Home", "Splitting", None), "On Home · Splitting");
        assert_eq!(what_went(&["lyrics".to_string(), "cover".to_string()], true), "lyrics, cover and parts");
        assert_eq!(had_already("Home", "Yellow", "lyrics and parts"), "Home had “Yellow” already; its lyrics and parts went with it");
        assert_eq!(on_it_now("Yellow", "Home", true), "“Yellow” is on Home now with its parts, for everyone there");
        assert_eq!(on_it_now("Yellow", "Home", false), "“Yellow” is on Home now, for everyone there");
    }

    /// Stage 4c: the party card, the in-party view and its sync line, as the Linux app said them.
    #[test]
    fn the_party_card_and_its_lines() {
        assert_eq!(PARTY_HEADING, "Listening party");
        assert_eq!(asking_party("Home"), "Asking Home about its party…");
        assert_eq!(NO_PARTY, "Nobody's having a party right now. In one, everyone hears the same song at the same moment and anyone adds songs.");
        assert_eq!(hosting_off("Home"), "Hosting a party is off for you on Home: its owner's choice. You can join one when someone starts it.");
        assert_eq!((party_heading(true, "Ana"), party_heading(false, "Ana")), ("Your listening party is on".to_string(), "Ana is hosting a listening party".to_string()));
        assert_eq!(playing("Yellow", "Coldplay"), "Playing “Yellow” by Coldplay.");
        assert_eq!(playing("Yellow", ""), "Playing “Yellow”.");
        assert_eq!((listening(0), listening(1), listening(4)), ("Nobody's listening right now.".to_string(), "1 person listening.".to_string(), "4 people listening.".to_string()));
        assert_eq!(party_line(&playing("Yellow", "Coldplay"), &listening(1)), "Playing “Yellow” by Coldplay. 1 person listening. Join, and you hear it here in step with everyone.");
        assert_eq!(party_line(NOTHING_YET, &listening(0)), "Nothing's playing yet: add a song and it starts. Nobody's listening right now. Join, and you hear it here in step with everyone.");
        assert_eq!((JOIN_YOUR_PARTY, JOIN_THE_PARTY, START_PARTY), ("Join Your Party", "Join the Party", "Start a Party"));
        assert_eq!(joining_leaves("Ana"), "Joining leaves Ana's party.");
        // in it
        assert_eq!((in_party_heading(true, "Ana"), in_party_heading(false, "Ana")), ("You're hosting the party".to_string(), "You're in Ana's party".to_string()));
        assert_eq!((party_pill(true, true), party_pill(false, true), party_pill(false, false)), ("Playing", "Paused", "Quiet"));
        assert_eq!(host_controls("Ana"), "Ana (the host) plays, pauses and skips.");
        assert_eq!((member("Ana", true), member("Ben", false)), ("Ana · host".to_string(), "Ben".to_string()));
        assert_eq!(listening_heading(3), "Listening (3)");
        assert_eq!(LOST_PARTY, "Lost the party: joining again…");
        assert_eq!(party_ended(""), "The party is over: back to your own music");
        assert_eq!(party_ended("Ana"), "Ana ended the party: back to your own music");
        assert_eq!(host_skips("Ana"), "Ana (the host) skips songs in this party");
        assert_eq!(bar_tooltip(false, true), "Stop hearing the party here");
        // its sync line
        assert_eq!(sync_line(&in_time(12.4), 18), "In time with the party (+12 ms) · clock ±18 ms");
        assert_eq!(sync_line(&catching_up(-340.0), 1), "Catching up (-340 ms) · clock ±1 ms");
        assert_eq!(sync_line(GETTING_IN_STEP, 18), "Getting in step… · clock ±18 ms");
        assert_eq!(sync_line(STARTING, 18), "Starting… · clock ±18 ms");
        assert_eq!(sync_line(&paused_by(false, "Ana"), 7), "Paused by Ana · clock ±7 ms");
        assert_eq!(paused_by(true, "Ana"), "Paused by you");
        assert_eq!(sync_line(NOT_HEARD_HERE, 7), "Not heard here: the party plays on. Hear It Here to be back in step. · clock ±7 ms");
        assert_eq!(sync_line(WAITING_FOR_SONG, 7), "Waiting for a song. · clock ±7 ms");
    }
}
