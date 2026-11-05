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
}
