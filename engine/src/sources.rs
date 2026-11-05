//! Where the controller finds songs, streams, radios and lyrics (issue #45): the real sources,
//! or a test's own (no network in tests).

use radiance_core::words::Kind;

use crate::lyrics::Lyrics;
use crate::track::Track;
use crate::youtube::Stream;

/// The internet, as the controller asks it things. Called off the engine's thread.
pub trait Sources: Send + Sync + 'static {
    /// Search: the free lossless libraries, YouTube Music's songs, or its videos.
    fn search(&self, kind: Kind, q: &str) -> Vec<Track>;
    /// The internet can be reached (when a search found nothing: is it the song, or the internet?).
    fn reachable(&self) -> bool;
    /// A YouTube song's stream.
    fn stream(&self, id: &str) -> Result<Stream, String>;
    /// Songs like `seed` (Autoplay).
    fn radio(&self, seed: &Track) -> Vec<Track>;
    /// A song's lyrics.
    fn lyrics(&self, t: &Track) -> Lyrics;
    /// A file from the web into `to` (a liked lossless song being saved).
    fn download(&self, url: &str, to: &std::path::Path) -> Result<(), String>;
    /// A song's genres (looked up for liked songs).
    fn genres(&self, t: &Track) -> Vec<String>;
    /// The other version of a song: its official music video (for a song), or the song (for a video).
    fn counterpart(&self, t: &Track) -> Option<Track>;
    /// A YouTube video's picture and its sound, as two streams.
    fn video_streams(&self, id: &str) -> Result<(Stream, Stream), String>;
}

/// The real ones: the Internet Archive, YouTube Music (yt-dlp), LRCLIB.
pub struct Real;

impl Sources for Real {
    fn search(&self, kind: Kind, q: &str) -> Vec<Track> {
        match kind {
            Kind::Lossless => crate::archive::search(q),
            Kind::Songs => crate::youtube::search(q),
            Kind::Videos => crate::youtube::search_videos(q),
        }
    }
    fn reachable(&self) -> bool {
        crate::net::reachable()
    }
    fn stream(&self, id: &str) -> Result<Stream, String> {
        crate::youtube::stream(id)
    }
    fn radio(&self, seed: &Track) -> Vec<Track> {
        crate::mix::radio(seed)
    }
    fn lyrics(&self, t: &Track) -> Lyrics {
        crate::lyrics::find(t)
    }
    fn download(&self, url: &str, to: &std::path::Path) -> Result<(), String> {
        crate::library::download(url, to)
    }
    fn genres(&self, t: &Track) -> Vec<String> {
        crate::genre::lookup(t)
    }
    fn counterpart(&self, t: &Track) -> Option<Track> {
        crate::youtube::counterpart(t)
    }
    fn video_streams(&self, id: &str) -> Result<(Stream, Stream), String> {
        crate::youtube::video_streams(id)
    }
}
