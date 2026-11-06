//! What the screens say, written once: the GTK app and the Windows app show the same words
//! (issue #45). Moved out of ui.rs and ui/home.rs.

/// Search's tiles before anything is typed: (what it says, what it searches).
pub const BROWSE: [(&str, &str); 12] = [
    ("Pinoy hip-hop", "pinoy hip hop"),
    ("OPM", "OPM hits"),
    ("R&B", "r&b"),
    ("Pop", "pop hits"),
    ("K-pop", "kpop"),
    ("Lo-fi", "lofi beats"),
    ("Live concerts", "live concert"),
    ("Jazz", "jazz"),
    ("Classical", "classical"),
    ("Rock", "rock"),
    ("Acoustic", "acoustic"),
    ("Reggae", "reggae"),
];

/// Cards in a row on Home.
pub const ROW_CARDS: usize = 20;

/// Home's greeting at `hour` (0-23), with the first of `real_name`'s names ("Unknown": none).
pub fn greeting(hour: u32, real_name: &str) -> String {
    let part = match hour {
        5..=11 => "Good morning",
        12..=17 => "Good afternoon",
        _ => "Good evening",
    };
    let name = real_name.split_whitespace().next().unwrap_or_default();
    if name.is_empty() || name.eq_ignore_ascii_case("unknown") {
        part.to_string()
    } else {
        format!("{part}, {name}")
    }
}

/// The hero's line under the greeting: the mix ready, being found, or how to get one.
pub fn mix_note(mix: usize, liked: usize) -> String {
    if mix > 0 {
        format!("Your mix is ready: {mix} songs like the ones you liked.")
    } else if liked > 0 {
        "Finding songs like the ones you liked…".to_string()
    } else {
        "Like a few songs and a mix made from them shows up here.".to_string()
    }
}

/// Search's lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The free lossless libraries (Internet Archive).
    Lossless,
    /// YouTube Music's songs.
    Songs,
    /// Music videos.
    Videos,
}

/// Offline, nothing is found anywhere: said, not "no songs" (2026-10-31 audit).
pub const OFFLINE_SEARCH: &str = "You're offline: searching needs the internet. Your liked songs still play; it searches again when you're back online.";

/// The note under a Search list: still searching (`found` None), nothing found (offline or
/// not), or what the list is.
pub fn search_note(kind: Kind, found: Option<usize>, offline: bool, query: &str) -> String {
    match (kind, found) {
        (Kind::Lossless, None) => "Searching the free lossless libraries (Internet Archive)…".to_string(),
        (Kind::Lossless, Some(0)) if offline => OFFLINE_SEARCH.to_string(),
        (Kind::Lossless, Some(0)) => format!("The free libraries have no lossless recording of “{query}”."),
        (Kind::Lossless, Some(_)) => "Free FLAC from the Internet Archive. Played first.".to_string(),
        (Kind::Songs, None) => "Searching YouTube Music…".to_string(),
        (Kind::Songs, Some(0)) if offline => "YouTube Music too, once you're back online.".to_string(),
        (Kind::Songs, Some(0)) => format!("YouTube Music found no other songs for “{query}”."),
        (Kind::Songs, Some(_)) => "Compressed (Opus). The fallback when there's no lossless copy.".to_string(),
        (Kind::Videos, None) => "Searching music videos…".to_string(),
        (Kind::Videos, Some(0)) if offline => "Music videos too.".to_string(),
        (Kind::Videos, Some(0)) => format!("No music videos for “{query}”."),
        (Kind::Videos, Some(_)) => "Music videos from YouTube: they play with their picture in Now playing.".to_string(),
    }
}

/// The note over your liked songs that match a search.
pub fn liked_matches(n: usize) -> String {
    format!("{n} of your liked songs {}", if n == 1 { "matches" } else { "match" })
}

/// Liking and saving: the toasts and the Fix missing button (both apps, as the Linux app says them).
pub fn liked_toast(title: &str) -> String {
    format!("Liked “{title}”")
}

pub fn saved_toast(title: &str) -> String {
    format!("Saved “{title}”")
}

pub fn unliked_toast(title: &str, trashed: bool) -> String {
    if trashed {
        format!("Removed “{title}” from Liked; its saved copy is in the Trash")
    } else {
        format!("Removed “{title}” from Liked")
    }
}

pub fn save_failed(title: &str) -> String {
    format!("Couldn't save “{title}” right now. It tries again next time the app opens.")
}

/// The Fix missing button: `n` songs (or videos) missing, `fixing` while it works on them.
pub fn fix_label(n: usize, fixing: bool) -> String {
    if fixing { format!("Fixing… {n} left") } else { format!("Fix missing ({n})") }
}

pub fn fixing_toast(songs: usize, videos: usize) -> String {
    match (songs, videos) {
        (0, v) => format!("Saving {v} music videos in the background"),
        (s, 0) => format!("Saving {s} songs in the background"),
        (s, v) => format!("Saving {s} songs and {v} music videos in the background"),
    }
}

/// Playlists (both apps, as the Linux app says them).
pub const PLAYLIST_EMPTY: &str = "No songs in it yet. Add one from any song's menu: Add to Playlist.";
pub const PLAYLIST_DELETE_BODY: &str = "The playlist goes; its songs stay in your library.";
pub const PLAYLIST_DELETE_TIP: &str = "Delete the playlist";
pub const PLAYLIST_NAME_HINT: &str = "Playlist name";

pub fn added_to(playlist: &str) -> String {
    format!("Added to “{playlist}”")
}

pub fn already_in(playlist: &str) -> String {
    format!("Already in “{playlist}”")
}

pub fn delete_playlist_title(name: &str) -> String {
    format!("Delete “{name}”?")
}

/// The sidebar's tooltip for a playlist.
pub fn playlist_tip(name: &str, songs: usize) -> String {
    format!("{name}: {}", crate::library::songs_count(songs))
}

/// A new playlist's suggested name, with `existing` playlists already.
pub fn next_playlist_name(existing: usize) -> String {
    format!("My Playlist {}", existing + 1)
}

// ----------------------------------------------------------------- splitting (stage 3, the Linux app's words)

/// A song's split mark while it waits, and its place in line.
pub const SPLIT_WAITING: &str = "Waiting to be split";
pub fn split_place(n: usize) -> String {
    format!("Number {n} in line; liked songs are split one after another.")
}
pub const SPLITTING_NOW: &str = "Being split into the voice and the music now";
pub const SPLIT_DONE: (&str, &str) = ("Split", "No vocals and Vocals only are ready: switch on Now Playing, instantly.");

/// The split's bar: how far, in words (sending, splitting, bringing back); after the split, its
/// copy for the split server's library (-1) and its words' times (-2).
pub fn split_progress(p: f64) -> String {
    match p {
        p if p <= -2.0 => "Timing every word (MMS on the split server)…".to_string(),
        p if p < 0.0 => "Keeping a copy in the split server's library…".to_string(),
        p if p < 0.05 => format!("Sending the song · {:.0}%", p * 100.0),
        p if p < 0.93 => format!("Splitting · {:.0}%", p * 100.0),
        p => format!("Bringing the parts back · {:.0}%", p * 100.0),
    }
}

pub fn split_failed(title: &str) -> String {
    format!("Couldn't split “{title}” right now")
}

/// Getting splitting ready at the first start (under the sunrise).
pub const SETUP_GETTING: &str = "Getting Radiance Music ready…";
pub fn setup_step(step: &str) -> String {
    // (its first letter small, after the colon: as the GTK app's say)
    let mut c = step.chars();
    let step = c.next().map(|f| f.to_lowercase().collect::<String>() + c.as_str()).unwrap_or_default();
    format!("Getting Radiance Music ready: {step}")
}
pub const SETUP_RETRY: &str = "Couldn't get splitting ready yet. Trying again in a moment…";
pub const SETUP_OFFLINE: &str = "You're offline. Splitting songs needs a one-time download: it starts when you're back online.";
pub const OPEN_NOW: &str = "Open Radiance Music Now";
pub const OPEN_NOW_NOTE: &str = "Play music now; splitting gets ready in the background when the internet's back";
pub const HOW_TO_FIX: &str = "How to Fix It";

/// A song's menu: splitting and lyrics.
pub const SPLIT_TWO: &str = "Split: Music + Vocals (Fast)";
pub const SPLIT_FOUR: &str = "Split: Four Parts";
pub const SPLIT_FOUR_AFTER_TWO: &str = "Split in Four Parts";
pub const SHOW_LYRICS: &str = "Show Lyrics";
pub const EDIT_LYRICS: &str = "Edit Lyrics…";
pub const FIND_LYRICS_AGAIN: &str = "Find Lyrics Again";
pub const LYRICS_SAVED: &str = "Lyrics saved";
pub const LYRICS_NOT_SAVED: &str = "Couldn't save the lyrics";

/// Splitting… (the cores it may use), as the Linux app's dialog says it.
pub const CORES_TITLE: &str = "Cores it may use";
pub fn cores_note(total: u32) -> String {
    format!("Of the {total} this computer has. More is quicker; fewer leaves more for everything else.")
}
pub const SPLITTING_READY: (&str, &str) = ("Ready", "Demucs splits, MMS times the words. It runs at idle priority: whatever you're doing always goes first.");

/// Find Lyrics Again (the song's menu): looking, then what was found.
pub fn finding_lyrics_again(title: &str, timed: bool) -> String {
    if timed {
        format!("Finding “{title}”'s lyrics again, then timing every word (a few minutes)…")
    } else {
        format!("Finding “{title}”'s lyrics again…")
    }
}
pub fn found_lyrics_again(timed: bool) -> &'static str {
    if timed { "New lyrics, every word timed to the singing" } else { "New lyrics found" }
}

/// The Splits tab (the Linux app's split queue).
pub const SPLITS_TAB: &str = "Splits";
pub const SPLITTING_NOW_HEADING: &str = "Splitting now";
pub const NOTHING_TO_SPLIT: &str = "Nothing to split";
pub const SPLITS_NOTE: &str = "Liked songs are split one after another, by themselves.";
pub fn in_line(n: usize, byline: &str) -> String {
    format!("Number {n} in line · {byline}")
}
pub const PAUSE_SPLITTING: &str = "Pause Splitting";
pub const SPLITTING_DIALOG: &str = "Splitting…";
pub const TRY_AGAIN: &str = "Try Again";

/// The lyrics editor (the Linux app's).
pub const EDITOR_NOTE: &str = "Each line: when it starts, its words, who sings it. Saved as yours: never replaced by a lookup. Once the song is split, every word gets its own time.";
pub const ADD_LINE: &str = "Add Line";
pub const PASTE_LYRICS: &str = "Paste Lyrics";
pub const PASTE_TIP: &str = "Lines from the clipboard (one per line), in place of these";
pub const TAP_TIMING: &str = "Tap Timing";
pub const TAP_TIP: &str = "Play the song and press Enter as each line starts: it gets the time, and the next line is chosen";
pub const REMOVE_LINE_TIP: &str = "Remove this line";
pub const START_TIP: &str = "When the line starts. Click while the song plays to set it to now.";
pub const SINGER_TIP: &str = "Who sings this line";
pub const NOBODY_SET: &str = "Nobody set";
pub const ADD_SINGER: &str = "Add a singer";
pub const LINE_PLACEHOLDER: &str = "The words of this line";
pub const SAVE: &str = "Save";

/// Split by hand (the menu): what's happening, and how long it takes.
pub fn split_two_toast(title: &str) -> String {
    format!("Splitting “{title}” into Music and Vocals: a few seconds")
}
pub fn split_four_toast(title: &str) -> String {
    format!("Splitting “{title}” into four parts: about 3 minutes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greetings() {
        assert_eq!(greeting(6, "Bassi Saba"), "Good morning, Bassi");
        assert_eq!(greeting(12, ""), "Good afternoon");
        assert_eq!(greeting(17, "x"), "Good afternoon, x");
        assert_eq!(greeting(23, "Unknown"), "Good evening");
        assert_eq!(greeting(3, "  "), "Good evening");
    }

    #[test]
    fn search_notes_say_what_the_gtk_app_says() {
        assert_eq!(search_note(Kind::Lossless, None, false, "x"), "Searching the free lossless libraries (Internet Archive)…");
        assert_eq!(search_note(Kind::Lossless, Some(0), true, "x"), OFFLINE_SEARCH);
        assert_eq!(search_note(Kind::Lossless, Some(0), false, "binhi"), "The free libraries have no lossless recording of “binhi”.");
        assert_eq!(search_note(Kind::Lossless, Some(3), false, "x"), "Free FLAC from the Internet Archive. Played first.");
        assert_eq!(search_note(Kind::Songs, None, false, "x"), "Searching YouTube Music…");
        assert_eq!(search_note(Kind::Songs, Some(0), true, "x"), "YouTube Music too, once you're back online.");
        assert_eq!(search_note(Kind::Songs, Some(0), false, "binhi"), "YouTube Music found no other songs for “binhi”.");
        assert_eq!(search_note(Kind::Songs, Some(9), false, "x"), "Compressed (Opus). The fallback when there's no lossless copy.");
        assert_eq!(search_note(Kind::Videos, None, false, "x"), "Searching music videos…");
        assert_eq!(search_note(Kind::Videos, Some(0), true, "x"), "Music videos too.");
        assert_eq!(search_note(Kind::Videos, Some(0), false, "binhi"), "No music videos for “binhi”.");
        assert_eq!(search_note(Kind::Videos, Some(2), false, "x"), "Music videos from YouTube: they play with their picture in Now playing.");
        assert_eq!(OFFLINE_SEARCH, "You're offline: searching needs the internet. Your liked songs still play; it searches again when you're back online.");
        assert_eq!(liked_matches(1), "1 of your liked songs matches");
        assert_eq!(liked_matches(4), "4 of your liked songs match");
    }

    #[test]
    fn the_hero_says_where_your_mix_is() {
        assert_eq!(mix_note(25, 40), "Your mix is ready: 25 songs like the ones you liked.");
        assert_eq!(mix_note(0, 40), "Finding songs like the ones you liked…");
        assert_eq!(mix_note(0, 0), "Like a few songs and a mix made from them shows up here.");
    }

    #[test]
    fn browse_tiles_as_home_has_them() {
        assert_eq!(BROWSE.len(), 12);
        assert_eq!(BROWSE[0], ("Pinoy hip-hop", "pinoy hip hop"));
        assert_eq!(BROWSE[11], ("Reggae", "reggae"));
        assert_eq!(ROW_CARDS, 20);
    }

    #[test]
    fn the_splits_tab_and_editor_say_it_as_the_linux_app() {
        assert_eq!(in_line(2, "Ann · Blue"), "Number 2 in line · Ann · Blue");
        assert_eq!((SPLITTING_NOW_HEADING, NOTHING_TO_SPLIT, PAUSE_SPLITTING), ("Splitting now", "Nothing to split", "Pause Splitting"));
        assert_eq!(SPLITS_NOTE, "Liked songs are split one after another, by themselves.");
        assert_eq!((ADD_LINE, PASTE_LYRICS, TAP_TIMING, NOBODY_SET, ADD_SINGER), ("Add Line", "Paste Lyrics", "Tap Timing", "Nobody set", "Add a singer"));
        assert_eq!(TAP_TIP, "Play the song and press Enter as each line starts: it gets the time, and the next line is chosen");
        assert_eq!(TRY_AGAIN, "Try Again");
    }

    #[test]
    fn splitting_says_it_as_the_linux_app() {
        assert_eq!(split_progress(-2.0), "Timing every word (MMS on the split server)…");
        assert_eq!(split_progress(-1.0), "Keeping a copy in the split server's library…");
        assert_eq!(split_progress(0.02), "Sending the song · 2%");
        assert_eq!(split_progress(0.5), "Splitting · 50%");
        assert_eq!(split_progress(0.95), "Bringing the parts back · 95%");
        assert_eq!(split_place(3), "Number 3 in line; liked songs are split one after another.");
        assert_eq!(split_failed("Tarot"), "Couldn't split “Tarot” right now");
        assert_eq!(setup_step("Downloading Demucs…"), "Getting Radiance Music ready: downloading Demucs…"); // (as the GTK app's say: its first letter small)
        assert_eq!(SPLIT_WAITING, "Waiting to be split");
        assert_eq!(SETUP_OFFLINE, "You're offline. Splitting songs needs a one-time download: it starts when you're back online.");
        assert_eq!((SPLIT_TWO, SPLIT_FOUR, SPLIT_FOUR_AFTER_TWO), ("Split: Music + Vocals (Fast)", "Split: Four Parts", "Split in Four Parts"));
        assert_eq!((LYRICS_SAVED, LYRICS_NOT_SAVED), ("Lyrics saved", "Couldn't save the lyrics"));
    }
}
