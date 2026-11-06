//! The split queue, as the Linux app keeps it (moved from ui.rs, issue #45 stage 3): one song split
//! at a time; a song asked for by hand goes first; a song is never in line twice.

use crate::track::Track;

/// Liked songs start splitting by themselves this long after the app opens.
pub const AUTO_AFTER_S: u64 = 20;

/// The songs waiting to be split, in turn, and the one being split now.
#[derive(Clone, Debug, Default)]
pub struct SplitQueue {
    waiting: Vec<Track>,
    now: Option<Track>,
}

impl SplitQueue {
    /// In line: first (asked for by hand, or it's playing) or last. False: it's being split now.
    pub fn ask(&mut self, t: Track, first: bool) -> bool {
        let key = t.key();
        if self.now.as_ref().is_some_and(|x| x.key() == key) {
            return false;
        }
        self.waiting.retain(|x| x.key() != key);
        if first {
            self.waiting.insert(0, t);
        } else {
            self.waiting.push(t);
        }
        true
    }

    /// The next song to split (none while one is being split: one at a time).
    pub fn take_next(&mut self) -> Option<Track> {
        if self.now.is_some() || self.waiting.is_empty() {
            return None;
        }
        self.now = Some(self.waiting.remove(0));
        self.now.clone()
    }

    /// The song being split is done (split or not).
    pub fn finish(&mut self) -> Option<Track> {
        self.now.take()
    }

    /// Where a song is: 0 being split now, n the n-th in line.
    pub fn place(&self, key: &str) -> Option<usize> {
        if self.now.as_ref().is_some_and(|x| x.key() == key) {
            return Some(0);
        }
        self.waiting.iter().position(|x| x.key() == key).map(|i| i + 1)
    }

    pub fn drop_key(&mut self, key: &str) {
        self.waiting.retain(|x| x.key() != key);
    }

    pub fn now(&self) -> Option<&Track> {
        self.now.as_ref()
    }

    pub fn waiting(&self) -> &[Track] {
        &self.waiting
    }
}

/// The liked songs to split by themselves: saved ones (not streams), not videos, not split yet, in turn.
pub fn auto_list(liked: &[Track], done: impl Fn(&Track) -> bool) -> Vec<Track> {
    liked.iter().filter(|t| t.file.is_some() && !t.video && !done(t)).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::{Source, Track};

    fn song(id: &str) -> Track {
        Track { source: Source::YouTube, id: id.into(), title: id.into(), artist: "Someone".into(), album: String::new(), seconds: 200, art: String::new(), format: String::new(), file: None, video: false, genres: None }
    }

    #[test]
    fn a_song_asked_for_goes_first_and_is_never_twice() {
        let mut q = SplitQueue::default();
        assert!(q.ask(song("a"), false));
        assert!(q.ask(song("b"), false));
        assert!(q.ask(song("a"), true));
        assert_eq!(q.waiting().iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(q.take_next().map(|t| t.id), Some("a".to_string()));
        assert!(!q.ask(song("a"), true), "being split now");
        assert_eq!(q.waiting().len(), 1);
        assert_eq!(q.finish().map(|t| t.id), Some("a".to_string()));
        assert!(q.now().is_none());
    }

    #[test]
    fn place_counts_from_the_one_being_split() {
        let mut q = SplitQueue::default();
        for id in ["a", "b", "c"] {
            q.ask(song(id), false);
        }
        q.take_next();
        assert_eq!(q.place(&song("a").key()), Some(0));
        assert_eq!(q.place(&song("c").key()), Some(2));
        assert_eq!(q.place(&song("x").key()), None);
        q.drop_key(&song("b").key());
        assert_eq!(q.place(&song("c").key()), Some(1));
        assert!(q.take_next().is_none(), "one at a time");
    }

    #[test]
    fn auto_list_skips_videos_streams_and_done() {
        let mut saved = song("saved");
        saved.file = Some("/m/saved.opus".into());
        let mut video = song("video");
        video.file = Some("/m/video.mkv".into());
        video.video = true;
        let stream = song("stream");
        let mut done = song("done");
        done.file = Some("/m/done.opus".into());
        let got = auto_list(&[saved.clone(), video, stream, done], |t| t.id == "done");
        assert_eq!(got.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["saved"]);
        assert_eq!(AUTO_AFTER_S, 20);
    }
}
