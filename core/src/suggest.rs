//! Search suggestions under (or in) the search box, as the GTK app's update_suggestions (moved
//! out of ui.rs, issue #45): before typing, your artists; while typing, your matching artists and
//! songs, then YouTube Music's.

use crate::track::Track;

/// A suggestion: its words (what gets searched), and whether it comes from your liked songs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chip {
    pub text: String,
    pub mine: bool,
}

/// What shows at once: an empty box, your 10 most-liked artists (ties alphabetical); typing, up
/// to 3 of your artists and 3 of your songs ("{title} {first artist}") holding it. None twice
/// (any case), none the same as what's typed.
pub fn mine(liked: &[Track], typed: &str) -> Vec<Chip> {
    let q = typed.trim().to_lowercase();
    let mut out: Vec<Chip> = vec![];
    let mut add = |text: String| {
        if out.iter().any(|c| c.text.eq_ignore_ascii_case(&text)) || text.eq_ignore_ascii_case(&q) {
            return;
        }
        out.push(Chip { text, mine: true });
    };
    if q.is_empty() {
        // your artists, the most liked first
        let mut count: std::collections::HashMap<String, (String, usize)> = std::collections::HashMap::new();
        for t in liked {
            for a in t.artists() {
                count.entry(a.to_lowercase()).or_insert_with(|| (a.clone(), 0)).1 += 1;
            }
        }
        let mut top: Vec<(String, usize)> = count.into_values().collect();
        top.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        for (name, _) in top.into_iter().take(10) {
            add(name);
        }
        return out;
    }
    for a in liked.iter().flat_map(|t| t.artists()).filter(|a| a.to_lowercase().contains(&q)).take(3) {
        add(a);
    }
    for t in liked.iter().filter(|t| t.title.to_lowercase().contains(&q)).take(3) {
        add(format!("{} {}", t.title, t.artists().first().cloned().unwrap_or_default()));
    }
    out
}

/// YouTube Music's suggestions added after `have`: up to 6 not already shown (any case).
pub fn add_youtube(have: &[Chip], found: &[String]) -> Vec<Chip> {
    let shown: Vec<String> = have.iter().map(|c| c.text.to_lowercase()).collect();
    found.iter().filter(|s| !shown.contains(&s.to_lowercase())).take(6).map(|s| Chip { text: s.clone(), mine: false }).collect()
}

pub const MINE_TIP: &str = "From your liked songs";

#[cfg(test)]
mod tests {
    use super::*;

    fn song(title: &str, artist: &str) -> Track {
        Track { source: crate::track::Source::YouTube, id: title.into(), title: title.into(), artist: artist.into(), album: String::new(), seconds: 0, art: String::new(), format: String::new(), file: None, video: false, genres: None }
    }

    #[test]
    fn suggestions_before_typing_are_your_artists() {
        let liked = vec![song("A", "Rebelution"), song("B", "Nairud"), song("C", "Rebelution"), song("D", "Ben&Ben")];
        let chips = mine(&liked, "");
        assert_eq!(chips.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(), ["Rebelution", "Ben&Ben", "Nairud"]);
        assert!(chips.iter().all(|c| c.mine));
    }

    #[test]
    fn suggestions_merge_mine_then_youtube() {
        let liked = vec![song("Suffering", "Rebelution"), song("Safe and Sound", "Rebelution"), song("Sana", "Natural High")];
        let chips = mine(&liked, "s");
        // artists holding "s": none (Rebelution, Natural High have no s... "Natural High" no); songs: three
        assert_eq!(chips.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(), ["Suffering Rebelution", "Safe and Sound Rebelution", "Sana Natural High"]);
        let found: Vec<String> = ["suffering rebelution", "sana", "s1", "s2", "s3", "s4", "s5", "s6", "s7"].iter().map(|s| s.to_string()).collect();
        let more = add_youtube(&chips, &found);
        assert_eq!(more.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(), ["sana", "s1", "s2", "s3", "s4", "s5"]);
        assert!(more.iter().all(|c| !c.mine));
        assert!(mine(&liked, "rebelution").iter().all(|c| !c.text.eq_ignore_ascii_case("rebelution")), "not the typed words themselves");
    }
}
