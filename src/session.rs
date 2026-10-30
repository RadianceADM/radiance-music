//! Where you left off: the queue, the song in it and how far in, and your
//! listening history (every play, with when: the History page and Home's
//! "Recently played"), all in the app's database (db.rs; the history keeps
//! every play, the latest 1000 are shown). Saved every few seconds while
//! playing; restored at start, so play carries on from the same spot.

use gtk::glib;

use crate::track::Track;

const HISTORY: usize = 1000;
const QUEUE: usize = 500;

#[derive(Default)]
pub struct Session {
    pub queue: Vec<Track>,
    pub index: usize,
    pub position: f64,
    /// Newest first: (song, when it started, unix seconds).
    pub history: Vec<(Track, i64)>,
}

pub fn load() -> Session {
    let (queue, index, position) = crate::db::session();
    Session { queue, index, position, history: crate::db::plays(HISTORY) }
}

/// The queue and where you are (the history is kept as each play happens).
pub fn save(s: &Session) {
    // the stretch of the queue around the song playing, if it's very long
    let start = s.index.saturating_sub(QUEUE / 2);
    let queue: Vec<Track> = s.queue.iter().skip(start).take(QUEUE).cloned().collect();
    crate::db::set_session(&queue, s.index - start, s.position);
}

/// A play, at the front of the history (now).
pub fn played(history: &mut Vec<(Track, i64)>, t: &Track) {
    let now = glib::DateTime::now_utc().map(|d| d.to_unix()).unwrap_or(0);
    if history.first().is_some_and(|(x, _)| x.key() == t.key()) {
        return; // the same song again right away (a switch of half, a restart)
    }
    history.insert(0, (t.clone(), now));
    history.truncate(HISTORY);
    crate::db::add_play(t, now);
}

/// The songs played lately, each once, newest first.
pub fn recent(history: &[(Track, i64)]) -> Vec<Track> {
    let mut seen = std::collections::HashSet::new();
    history.iter().filter(|(t, _)| seen.insert(t.key())).map(|(t, _)| t.clone()).collect()
}
