//! The library's pages, as the Linux app shows them (moved out of ui.rs for the Windows beta,
//! #45 stage 2): liked songs grouped by album, artist or genre, the A to Z rail's letters, and
//! the words under Liked Songs, the queue and a row.

use std::collections::BTreeMap;

use crate::track::{letter_of, Track};

/// How liked songs are grouped (the Albums, Artists and Genres pages, and Home).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupKind {
    Albums,
    Artists,
    Genres,
}

/// An album, an artist or a genre, and its liked songs (in liked order).
#[derive(Clone, Debug)]
pub struct Group {
    pub name: String,
    pub songs: Vec<Track>,
}

/// The liked songs by album, by each credited artist, or by genre, A to Z (by the lowercased
/// name; the first spelling met is shown), no song twice in a group.
pub fn groups(liked: &[Track], kind: GroupKind) -> Vec<Group> {
    let mut map: BTreeMap<String, Group> = BTreeMap::new();
    let mut put = |name: &str, t: &Track| {
        let g = map.entry(name.to_lowercase()).or_insert_with(|| Group { name: name.to_string(), songs: vec![] });
        if !g.songs.iter().any(|x| x.key() == t.key()) {
            g.songs.push(t.clone());
        }
    };
    for t in liked {
        match kind {
            GroupKind::Albums => put(if t.album.is_empty() { "Singles" } else { &t.album }, t),
            GroupKind::Artists => t.artists().iter().for_each(|a| put(a, t)),
            GroupKind::Genres => match &t.genres {
                Some(g) if !g.is_empty() => g.iter().for_each(|x| put(x, t)),
                Some(_) => put("Other", t),
                None => put("Finding the genre…", t),
            },
        }
    }
    map.into_values().collect()
}

/// The A to Z rail's letters.
pub const AZ: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ#";

/// Each of the rail's letters with the first shown row starting with it (None: faint, no song).
pub fn letters(shown: &[Track]) -> Vec<(char, Option<usize>)> {
    AZ.chars().map(|l| (l, shown.iter().position(|t| letter_of(&t.title) == l))).collect()
}

/// "1 song", "3 songs".
pub fn songs_count(n: usize) -> String {
    format!("{n} song{}", if n == 1 { "" } else { "s" })
}

/// The words under Liked Songs: `total` liked songs kept in `dir`, `found` of them matching `q`.
pub fn liked_note(total: usize, dir: &str, q: &str, found: usize) -> String {
    if total == 0 {
        "Like a song (the heart) and it's kept here. Lossless songs are saved with their lyrics and album art.".to_string()
    } else if q.is_empty() {
        format!("{total} songs, saved in {dir}")
    } else if found == 0 {
        format!("None of your {total} liked songs match “{q}”. Search finds it online.")
    } else {
        format!("{found} of your {total} liked songs {} “{q}”", if found == 1 { "matches" } else { "match" })
    }
}

/// How a liked song is kept on this computer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Saved {
    /// Its file is being saved now.
    Saving,
    /// It will be saved (it hasn't been yet).
    NotSaved,
    /// It isn't saved here (a YouTube song in a public build): it plays from the internet.
    Streams,
    Saved,
}

/// Downloading (owner, 2026-11-07: "pede ba may downloading den sa sidebar", then "for download not yet saved but
/// liked"): the liked songs not saved on this computer yet, the ones saving now first. A song that streams (a
/// public build's YouTube song) isn't one: it isn't saved at all.
pub fn downloading(liked: &[(Track, Saved)]) -> Vec<(Track, Saved)> {
    let mut out: Vec<(Track, Saved)> = liked.iter().filter(|(_, s)| *s == Saved::Saving).cloned().collect();
    out.extend(liked.iter().filter(|(_, s)| *s == Saved::NotSaved).cloned());
    out
}

/// The sidebar's Downloading, with how many while any wait.
pub fn downloading_title(n: usize) -> String {
    if n == 0 { "Downloading".to_string() } else { format!("Downloading ({n})") }
}

/// Downloading's line under its title: `saving` being saved now, `waiting` not yet.
pub fn downloading_note(saving: usize, waiting: usize) -> String {
    let n = saving + waiting;
    if n == 0 {
        return "Every liked song is saved on this computer.".to_string();
    }
    let (songs, is, plays, it) = if n == 1 { ("song", "isn't", "It plays", "it's") } else { ("songs", "aren't", "They play", "they're") };
    let now = if saving > 0 { format!(": {saving} saving now") } else { String::new() };
    format!("{n} liked {songs} {is} saved on this computer yet{now}. {plays} from the internet until {it} saved.")
}

/// A liked song's row's second line.
pub fn row_subtitle(byline: &str, s: Saved) -> String {
    match s {
        Saved::Saving => "Saving…".to_string(),
        Saved::NotSaved => "Not saved yet".to_string(),
        Saved::Streams => format!("{byline} · streams"),
        Saved::Saved => byline.to_string(),
    }
}

/// "Played 12 times · skipped 3 times" (or less, or nothing).
pub fn counts_text(plays: usize, skips: usize) -> String {
    let times = |n: usize| if n == 1 { "once".to_string() } else { format!("{n} times") };
    match (plays, skips) {
        (0, 0) => String::new(),
        (p, 0) => format!("Played {}", times(p)),
        (p, s) => format!("Played {} · skipped {}", times(p), times(s)),
    }
}

/// The words under the queue: `nothing` (an empty queue, or nothing playing and nothing next),
/// `ahead` songs after the one playing, Autoplay `adding` the next ones.
pub fn queue_note(nothing: bool, ahead: usize, adding: bool) -> String {
    if nothing {
        "Nothing queued. Play a song, or right-click any song and pick Add to Queue.".to_string()
    } else if ahead == 0 {
        if adding {
            "Autoplay is adding songs like this one next.".to_string()
        } else {
            "Nothing after this song. Right-click any song and pick Add to Queue, or start a Song Radio (or turn on Autoplay in the menu).".to_string()
        }
    } else {
        format!("{ahead} songs next")
    }
}

/// A calendar day as a number: two days' numbers differ by the days between them, as the Linux
/// app has always counted them (a month as 31 days: only "Today" / "Yesterday" use it).
pub fn day_number(y: i32, m: u32, d: u32) -> i64 {
    y as i64 * 372 + m as i64 * 31 + d as i64
}

/// History's day heading for a play `days_ago` calendar days and `hours_ago` hours back:
/// "Today", "Yesterday", its weekday within a week (by the hour), else its full date.
pub fn day_label(days_ago: i64, hours_ago: i64, weekday: &str, full_date: &str) -> String {
    match days_ago {
        0 => "Today".to_string(),
        1 => "Yesterday".to_string(),
        _ if hours_ago < 24 * 7 => weekday.to_string(),
        _ => full_date.to_string(),
    }
}

/// History when nothing was played yet, and the most plays it lists.
pub const HISTORY_EMPTY: &str = "Songs you play show up here, with when you played them.";
pub const HISTORY_ROWS: usize = 400;

/// The queue panel's "Played before": this many songs, and its words.
pub const RECENT_ROWS: usize = 5;
pub const PLAYED_BEFORE_NOTE: &str = "Go back to one: it plays now, then the queue carries on";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::{Source, Track};

    fn song(id: &str, title: &str, artist: &str, album: &str, genres: Option<Vec<&str>>) -> Track {
        Track {
            source: Source::YouTube,
            id: id.into(),
            title: title.into(),
            artist: artist.into(),
            album: album.into(),
            seconds: 200,
            art: String::new(),
            format: String::new(),
            file: None,
            video: false,
            genres: genres.map(|g| g.into_iter().map(String::from).collect()),
        }
    }

    /// Downloading (owner, 2026-11-07): the liked songs not saved on this computer yet, the ones saving first.
    #[test]
    fn downloading_lists_the_liked_songs_not_saved_yet() {
        let liked = vec![
            (song("a", "A", "X", "", None), Saved::Saved),
            (song("b", "B", "X", "", None), Saved::NotSaved),
            (song("c", "C", "X", "", None), Saved::Streams),
            (song("d", "D", "X", "", None), Saved::Saving),
        ];
        let list = downloading(&liked);
        let got: Vec<&str> = list.iter().map(|(t, _)| t.id.as_str()).collect();
        assert_eq!(got, ["d", "b"], "saving first, then waiting; saved and streamed ones aren't downloads");
        assert_eq!(downloading_title(0), "Downloading");
        assert_eq!(downloading_title(3), "Downloading (3)");
        assert_eq!(downloading_note(0, 0), "Every liked song is saved on this computer.");
        assert_eq!(downloading_note(1, 2), "3 liked songs aren't saved on this computer yet: 1 saving now. They play from the internet until they're saved.");
        assert_eq!(downloading_note(0, 1), "1 liked song isn't saved on this computer yet. It plays from the internet until it's saved.");
    }

    #[test]
    fn groups_put_singles_other_and_finding_where_the_linux_app_does() {
        let liked = vec![
            song("a", "One", "X", "", Some(vec![])),
            song("b", "Two", "Y", "Blue", None),
            song("c", "Three", "Z", "blue", Some(vec!["Pop", "Rock"])),
        ];
        let albums: Vec<(String, usize)> = groups(&liked, GroupKind::Albums).into_iter().map(|g| (g.name, g.songs.len())).collect();
        assert_eq!(albums, [("Blue".to_string(), 2), ("Singles".to_string(), 1)], "a to z by lowercased name, the first spelling kept");
        let genres: Vec<String> = groups(&liked, GroupKind::Genres).into_iter().map(|g| g.name).collect();
        assert_eq!(genres, ["Finding the genre…", "Other", "Pop", "Rock"]);
    }

    #[test]
    fn an_artist_group_lists_each_credited_artist() {
        let liked = vec![song("a", "Duet", "A & B", "", None), song("b", "Solo", "B", "", None)];
        let artists: Vec<(String, Vec<String>)> = groups(&liked, GroupKind::Artists).into_iter().map(|g| (g.name, g.songs.iter().map(|t| t.title.clone()).collect())).collect();
        assert_eq!(artists, [("A".to_string(), vec!["Duet".to_string()]), ("B".to_string(), vec!["Duet".to_string(), "Solo".to_string()])]);
    }

    #[test]
    fn letters_grey_out_empty_ones_and_hash_takes_the_rest() {
        let shown = vec![song("a", "Ánimo", "", "", None), song("b", "Another", "", "", None), song("c", "Cielo", "", "", None), song("d", "1999", "", "", None)];
        let l = letters(&shown);
        assert_eq!(l.len(), 27);
        assert_eq!(l[0], ('A', Some(0)));
        assert_eq!(l[1], ('B', None));
        assert_eq!(l[2], ('C', Some(2)));
        assert_eq!(l[26], ('#', Some(3)));
    }

    #[test]
    fn the_liked_note_says_what_the_linux_app_says() {
        assert_eq!(liked_note(0, "/m", "", 0), "Like a song (the heart) and it's kept here. Lossless songs are saved with their lyrics and album art.");
        assert_eq!(liked_note(3, "/m", "", 3), "3 songs, saved in /m");
        assert_eq!(liked_note(3, "/m", "zz", 0), "None of your 3 liked songs match “zz”. Search finds it online.");
        assert_eq!(liked_note(3, "/m", "x", 1), "1 of your 3 liked songs matches “x”");
        assert_eq!(liked_note(3, "/m", "x", 2), "2 of your 3 liked songs match “x”");
    }

    #[test]
    fn a_row_says_how_its_song_is_kept() {
        assert_eq!(row_subtitle("Ann", Saved::Saving), "Saving…");
        assert_eq!(row_subtitle("Ann", Saved::NotSaved), "Not saved yet");
        assert_eq!(row_subtitle("Ann", Saved::Streams), "Ann · streams");
        assert_eq!(row_subtitle("Ann", Saved::Saved), "Ann");
        assert_eq!(counts_text(0, 0), "");
        assert_eq!(counts_text(1, 0), "Played once");
        assert_eq!(counts_text(12, 3), "Played 12 times · skipped 3 times");
    }

    #[test]
    fn day_labels_count_back_from_today() {
        assert_eq!(day_label(0, 3, "Friday", "Friday, November 6, 2026"), "Today");
        assert_eq!(day_label(1, 20, "Thursday", "x"), "Yesterday");
        assert_eq!(day_label(3, 70, "Tuesday", "x"), "Tuesday");
        assert_eq!(day_label(7, 160, "Friday", "x"), "Friday", "under a week by the hour, as the Linux app counts");
        assert_eq!(day_label(9, 216, "Wednesday", "Wednesday, October 28, 2026"), "Wednesday, October 28, 2026");
        assert_eq!(day_label(-1, -5, "Saturday", "x"), "Saturday", "a play from the future (a clock moved): its weekday, as the Linux app shows");
        assert_eq!(day_number(2026, 11, 6) - day_number(2026, 10, 31), 6);
        assert_eq!(HISTORY_EMPTY, "Songs you play show up here, with when you played them.");
    }

    #[test]
    fn queue_notes_are_verbatim() {
        assert_eq!(queue_note(true, 0, false), "Nothing queued. Play a song, or right-click any song and pick Add to Queue.");
        assert_eq!(queue_note(false, 0, true), "Autoplay is adding songs like this one next.");
        assert_eq!(queue_note(false, 0, false), "Nothing after this song. Right-click any song and pick Add to Queue, or start a Song Radio (or turn on Autoplay in the menu).");
        assert_eq!(queue_note(false, 4, false), "4 songs next");
        assert_eq!(RECENT_ROWS, 5);
        assert_eq!(PLAYED_BEFORE_NOTE, "Go back to one: it plays now, then the queue carries on");
    }
}
