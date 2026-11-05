//! Search's results, arranged as the GTK app shows them: your liked songs that match first (titles
//! before artists), then lossless, YouTube Music and videos, never offering a song twice (#45).

use crate::track::{file_key, matches, Track};

/// Search's lists as shown (None: still being searched).
#[derive(Clone, Debug, Default)]
pub struct Arranged {
    /// Your liked songs that match: titles that match before artists that do.
    pub mine: Vec<Track>,
    pub lossless: Option<Vec<Track>>,
    pub youtube: Option<Vec<Track>>,
    pub videos: Option<Vec<Track>>,
}

/// The lists for `q`: yours first, and a song never twice (yours aren't offered again; a song
/// found lossless isn't offered from YouTube).
pub fn arrange(liked: &[Track], lossless: Option<Vec<Track>>, youtube: Option<Vec<Track>>, videos: Option<Vec<Track>>, q: &str) -> Arranged {
    let mut mine: Vec<Track> = liked.iter().filter(|t| matches(t, q)).cloned().collect();
    let by_title = |t: &Track| matches(&Track { artist: String::new(), album: String::new(), ..t.clone() }, q);
    mine.sort_by_cached_key(|t| (!by_title(t), file_key(&t.title)));
    let yours = |t: &Track| mine.iter().any(|m| m.key() == t.key());
    let lossless = lossless.map(|v| v.into_iter().filter(|t| !yours(t)).collect::<Vec<_>>());
    let youtube = youtube.map(|v| v.into_iter().filter(|t| !yours(t) && !lossless.as_ref().is_some_and(|l| l.iter().any(|x| x.same_song(t)))).collect::<Vec<_>>());
    let videos = videos.map(|v| v.into_iter().filter(|t| !yours(t)).collect::<Vec<_>>());
    Arranged { mine, lossless, youtube, videos }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::{Source, Track};

    fn t(source: Source, id: &str, title: &str, artist: &str) -> Track {
        Track { source, id: id.into(), title: title.into(), artist: artist.into(), album: String::new(), seconds: 200, art: String::new(), format: String::new(), file: None, video: false, genres: None }
    }

    #[test]
    fn your_own_first_and_no_song_twice() {
        let liked = vec![t(Source::YouTube, "l1", "Other", "Binhi Band"), t(Source::YouTube, "l2", "Binhi", "Arthur Nery"), t(Source::YouTube, "l3", "Nope", "Nobody")];
        let lossless = vec![t(Source::Archive, "a1", "Binhi (live)", "Someone"), t(Source::YouTube, "l2", "Binhi", "Arthur Nery")];
        let youtube = vec![t(Source::YouTube, "y1", "Binhi (Live)", "Someone"), t(Source::YouTube, "y2", "Binhi", "Cover Artist"), t(Source::YouTube, "l1", "Other", "Binhi Band")];
        let a = arrange(&liked, Some(lossless), Some(youtube), None, "binhi");
        // titles that match before artists that do
        assert_eq!(a.mine.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["l2", "l1"]);
        // yours aren't offered again; a song found lossless isn't offered from YouTube
        assert_eq!(a.lossless.unwrap().iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["a1"]);
        assert_eq!(a.youtube.unwrap().iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["y2"]);
        assert!(a.videos.is_none()); // still searching
    }
}
