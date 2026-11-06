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
    /// How this copy was installed, so how it updates (the tests: a beta in a folder of their own).
    fn install(&self) -> crate::update::Install {
        crate::update::Install::here()
    }
    /// The newest release for this install (worker thread; GitHub).
    fn latest_release(&self, install: &crate::update::Install) -> Result<crate::update::Release, String> {
        crate::update::latest(install)
    }
    /// Its file downloaded and checked (worker thread; minutes), saying how far along; stops when `stop` is set.
    fn download_update(&self, r: &crate::update::Release, install: &crate::update::Install, stop: &std::sync::atomic::AtomicBool, progress: &mut dyn FnMut(u64, u64)) -> Result<std::path::PathBuf, String> {
        crate::update::download(r, install, stop, |have, of| progress(have, of))
    }
    /// The update counted on Radiance Music's site (worker thread).
    fn count_update(&self, r: &crate::update::Release, install: &crate::update::Install) {
        crate::update::counted(r, install)
    }
    /// Is there a network at all (GIO's monitor; read at the start).
    fn network_available(&self) -> bool {
        use gio::prelude::*;
        gio::NetworkMonitor::default().is_network_available()
    }
    /// Whether the engine follows GIO's network monitor (the tests don't: the build server's own
    /// network coming and going would turn their songs' failures into "you're offline").
    fn watches_network(&self) -> bool {
        true
    }
    /// YouTube Music's suggestions for what's typed (worker thread; none on any failure).
    fn suggestions(&self, q: &str) -> Vec<String> {
        crate::youtube::suggestions(q)
    }
    /// The fast split's files fetched if any are missing (worker thread; quietly, as GTK's
    /// get_two_part_ready at the start and back online).
    fn fetch_two_part(&self) {
        if crate::two_part::ready().is_none() {
            if let Err(e) = crate::two_part::fetch(&|_| {}) {
                crate::log::note(&format!("the fast split's files: {e}"));
            }
        }
    }
    /// Report a Problem: sends it (worker thread; the admin site).
    fn send_report(&self, report: &crate::report::Report, note: &str) -> Result<String, crate::feedback::Failure> {
        crate::report::send(report, note)
    }
    /// The rating pop-up: is this free moment the one (the timing, then the roll)?
    fn rating_due(&self) -> bool {
        crate::feedback::time_to_ask()
    }
    /// The account the rating is sent as (none: a sign-in first).
    fn account(&self) -> Option<crate::feedback::Account> {
        crate::feedback::signed_in()
    }
    /// Signs in with Google in the browser (worker thread; minutes): `open` gets the address.
    fn sign_in(&self, stop: &std::sync::atomic::AtomicBool, open: &dyn Fn(&str)) -> Result<crate::feedback::Account, crate::feedback::Failure> {
        crate::feedback::sign_in(stop, open)
    }
    /// Sends the rating (worker thread): whether to offer sharing (4–5 stars).
    fn send_feedback(&self, token: &str, answer: &crate::feedback::Answer) -> Result<bool, crate::feedback::Failure> {
        crate::feedback::send(token, answer)
    }
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
