//! How to Use It, the first time the app opens (owner, 2026-10-31): five cards to page through,
//! as the phone app's (moved out of src/ui/guide.rs, issue #45).

/// The cards: an icon (the GTK app's icon name), a title, a few words.
pub const CARDS: [(&str, &str, &str); 5] = [
    ("system-search-symbolic", "Search Any Song", "Type a song or an artist at the top. Your own songs and your servers' come first; YouTube Music has the rest."),
    ("heart-filled-symbolic", "Like It, It's Yours", "Click the heart: the song, its cover, its lyrics and its music video are saved in your Music folder and play with no internet."),
    ("edit-cut-symbolic", "Split It, Mix It", "Split a liked song into drums, bass, other and vocals right on this computer, then turn each one up or down in the Mixer. No vocals makes it karaoke."),
    ("format-justify-center-symbolic", "Sing Along", "In Now Playing the lyrics light up word by word as they're sung, big in the middle, with the background moving to the beat."),
    ("phone-symbolic", "On Your Phone Too", "Get Radiance Music for Android at music.radianceph.com, then Share Songs with a Phone (main menu) sends it your songs, with their parts and lyrics."),
];
/// The fifth card's words with where each app keeps Share Songs with a Phone (the Linux app: "main menu"; the beta:
/// "Settings → Help"), stage 4b.
pub fn phone_card(where_share: &str) -> String {
    format!("Get Radiance Music for Android at music.radianceph.com, then Share Songs with a Phone ({where_share}) sends it your songs, with their parts and lyrics.")
}

pub const SKIP: &str = "Skip";
pub const NEXT: &str = "Next";
pub const START: &str = "Start Listening";
pub const TITLE: &str = "How to Use It";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_guide_cards_are_the_linux_apps() {
        assert_eq!(CARDS.len(), 5);
        assert_eq!(CARDS[0].1, "Search Any Song");
        assert_eq!(CARDS[3].2, "In Now Playing the lyrics light up word by word as they're sung, big in the middle, with the background moving to the beat.");
        assert_eq!((SKIP, NEXT, START, TITLE), ("Skip", "Next", "Start Listening", "How to Use It"));
        assert_eq!(phone_card("main menu"), CARDS[4].2, "the Linux app's card 5, word for word");
        assert!(phone_card("Settings → Help").contains("Share Songs with a Phone (Settings → Help)"));
    }
}
