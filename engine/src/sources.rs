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
    /// This computer's splitter (or your split server) is set up: four-part splits can run (stage 3).
    fn can_split(&self) -> bool {
        let s = crate::settings::load();
        s.local_split.is_some() || s.split.is_some()
    }
    /// The fast split's pieces are here (else the internet fetches them when it runs).
    fn can_split_two(&self) -> bool {
        crate::two_part::ready().is_some()
    }
    /// Every word of a split song timed to its Vocals (MMS, worker thread; minutes) where a splitter or
    /// split server is set up; nothing to do without one.
    fn time_words(&self, t: &Track) -> Result<(), String> {
        match crate::word_times::Aligner::of(&crate::settings::load()) {
            Some(a) => crate::word_times::time_words(t, &a),
            None => Ok(()),
        }
    }
    /// Find Lyrics Again (worker thread): the kept ones to the Trash, the ones found in their place.
    /// Ok(true): every word timed too.
    fn find_lyrics_again(&self, t: &Track) -> Result<bool, String> {
        crate::word_times::relook_lyrics(t, crate::word_times::Aligner::of(&crate::settings::load()).as_ref())
    }
    /// Splitting set up on this computer at its first start (worker thread; minutes: the Windows pack's
    /// download), saying each step and how far along (0..1).
    /// What stops splitting being set up on this computer (cores, memory, free disk), in plain words; empty: it can.
    fn setup_problems(&self) -> Vec<String> {
        crate::setup::check().problems()
    }
    fn install_splitter(&self, threads: u32, say: &(dyn Fn(&str, f64) + Send + Sync)) -> Result<crate::settings::LocalSplitter, String> {
        crate::setup::install(threads, say)
    }
    /// A song split (worker thread; minutes): in two, the fast split; else in four with the settings'
    /// splitter, then its words timed (engine::splitting). `uri`: its file, or its stream through the relay.
    fn split(&self, t: &Track, two: bool, uri: &str, report: std::sync::Arc<dyn Fn(crate::splitting::Phase) + Send + Sync>) -> Result<(), String> {
        if uri.is_empty() {
            return Err("couldn't get the song".into());
        }
        let s = crate::settings::load();
        if two {
            crate::splitting::two_parts(t, uri, s.local_split.as_ref(), report)
        } else {
            crate::splitting::four_parts(t, uri, s.split.as_ref(), s.local_split.as_ref(), report)
        }
    }
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
