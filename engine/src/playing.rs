//! Where a song plays from: its saved copy, the Archive's file, a YouTube stream found lately,
//! a server's file. Shared by the GTK app and the controller (issue #45).

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use gio::prelude::*;

use crate::relay::Relay;
use crate::track::{Source, Track};

/// A YouTube stream's address stays good this long.
pub const STREAM_FRESH: Duration = Duration::from_secs(3600);

/// YouTube streams found (and the ones being looked up).
#[derive(Default)]
pub struct Streams {
    found: HashMap<String, (String, u32, Instant)>,
    finding: HashSet<String>,
}

impl Streams {
    /// A stream found for `id` that's still good: (address, kbps).
    pub fn fresh(&self, id: &str) -> Option<(String, u32)> {
        let (uri, kbps, when) = self.found.get(id)?;
        (when.elapsed() < STREAM_FRESH).then(|| (uri.clone(), *kbps))
    }

    pub fn found(&mut self, id: &str, uri: String, kbps: u32) {
        self.found.insert(id.to_string(), (uri, kbps, Instant::now()));
    }

    /// False: it's being looked up already.
    pub fn start_finding(&mut self, id: &str) -> bool {
        self.finding.insert(id.to_string())
    }

    pub fn done_finding(&mut self, id: &str) {
        self.finding.remove(id);
    }

    /// A stream that broke: found again next time.
    pub fn forget(&mut self, id: &str) {
        self.found.remove(id);
    }
}

/// Where a YouTube song plays from (worker thread): a joined server's copy first (rule 1, owner
/// 2026-11-09: no YouTube when the server has it), else YouTube's stream as `youtube` finds it.
/// (address, kbps: 0 for a server's copy).
pub fn stream_of(t: &Track, relay: &Relay, youtube: impl FnOnce() -> Result<crate::youtube::Stream, String>) -> Result<(String, u32), String> {
    if let Some((server, song)) = crate::servers::copy_of(t) {
        crate::servers::note_stream(&t.id, &server.address);
        crate::log::note(&format!("“{}” plays from {} (its copy there)", t.title, server.name));
        return Ok((web_uri(Some(relay), &crate::servers::copy_stream(&server, &song)), 0));
    }
    youtube().map(|s| (relay.add(s.url, s.size, s.mime), s.kbps))
}

/// An Internet Archive file's (or a Radiance Music Server's) address for the player: as it is,
/// except on Windows, where it goes through the relay (GStreamer's TLS there can't check the Archive).
pub fn web_uri(relay: Option<&Relay>, url: &str) -> String {
    match relay {
        Some(relay) if cfg!(windows) => relay.add_web(url),
        _ => url.to_string(),
    }
}

/// Where the song itself can be played from (a YouTube stream only once found).
pub fn plain_uri(t: &Track, relay: Option<&Relay>, streams: &Streams) -> Option<String> {
    if let Some(f) = &t.file {
        return Some(gio::File::for_path(f).uri().to_string());
    }
    match t.source {
        Source::Archive => Some(web_uri(relay, &t.id)),
        Source::YouTube => streams.fresh(&t.id).map(|s| s.0),
        // with this computer's key (none: the server was left)
        Source::Server => crate::servers::of_track(t).map(|_| web_uri(relay, &crate::servers::signed(&t.id))),
    }
}

/// The saved copy of a liked song (its file still there), else the song itself.
pub fn saved_copy(liked: &[Track], t: &Track) -> Track {
    liked
        .iter()
        .find(|x| x.key() == t.key() && x.file.as_ref().is_some_and(|f| std::path::Path::new(f).exists()))
        .cloned()
        .unwrap_or_else(|| t.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(source: Source, id: &str) -> Track {
        Track { source, id: id.into(), title: "T".into(), artist: "A".into(), album: String::new(), seconds: 1, art: String::new(), format: String::new(), file: None, video: false, genres: None }
    }

    #[test]
    fn where_a_song_plays_from() {
        let mut streams = Streams::default();
        let mut saved = t(Source::YouTube, "s");
        saved.file = Some("/music/A - B.opus".into());
        assert_eq!(plain_uri(&saved, None, &streams).as_deref(), Some("file:///music/A%20-%20B.opus"));
        let archive = t(Source::Archive, "https://archive.org/download/x/y.flac");
        assert_eq!(plain_uri(&archive, None, &streams).as_deref(), Some("https://archive.org/download/x/y.flac"));
        let yt = t(Source::YouTube, "abc");
        assert_eq!(plain_uri(&yt, None, &streams), None);
        streams.found("abc", "http://127.0.0.1:1/s/1".into(), 160);
        assert_eq!(plain_uri(&yt, None, &streams).as_deref(), Some("http://127.0.0.1:1/s/1"));
        assert_eq!(streams.fresh("abc"), Some(("http://127.0.0.1:1/s/1".into(), 160)));
        assert_eq!(plain_uri(&t(Source::Server, "https://h/files/x.opus"), None, &streams), None);
    }

    #[test]
    fn a_stream_is_looked_up_once_at_a_time() {
        let mut streams = Streams::default();
        assert!(streams.start_finding("abc"));
        assert!(!streams.start_finding("abc"));
        streams.done_finding("abc");
        assert!(streams.start_finding("abc"));
    }

    #[test]
    fn the_saved_copy_when_its_file_is_there() {
        let dir = std::env::temp_dir().join(format!("radiance-saved-copy-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.opus");
        std::fs::write(&file, b"x").unwrap();
        let mut liked = t(Source::YouTube, "a");
        liked.file = Some(file.display().to_string());
        let mut gone = t(Source::YouTube, "b");
        gone.file = Some(dir.join("gone.opus").display().to_string());
        assert_eq!(saved_copy(&[liked.clone(), gone], &t(Source::YouTube, "a")).file, liked.file);
        assert_eq!(saved_copy(&[liked], &t(Source::YouTube, "b")).file, None);
        let _ = std::fs::remove_dir_all(dir);
    }
}
