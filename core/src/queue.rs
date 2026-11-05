//! The queue: the songs, which one plays, shuffle and repeat, and what plays next. Moved out of
//! the GTK app (ui.rs) so the Windows app plays songs by the same rules (issue #45).

use std::collections::HashSet;

use crate::track::Track;

/// Repeat: off, the whole queue, or this song.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Repeat {
    Off,
    All,
    One,
}

/// What a skip decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Back, more than 3 s into the song: its start again.
    Restart,
    /// That song (the queue's index is on it now).
    Play(usize),
    /// No song there: forward, the list ran out (it stops); back, nothing happens.
    End,
}

/// What removing a song did to the one playing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Removed {
    /// One before it: the index moved up.
    Before,
    /// The song playing itself: `play`, the next one (now at its place) plays; else it stops.
    Current { play: bool },
    /// One after it.
    After,
}

/// The queue: its songs, the one playing (`index`), the order before Shuffle, Shuffle and Repeat.
#[derive(Clone, Debug)]
pub struct Queue {
    tracks: Vec<Track>,
    index: usize,
    unshuffled: Option<Vec<Track>>,
    shuffle: bool,
    repeat: Repeat,
}

impl Default for Queue {
    fn default() -> Self {
        Queue::new()
    }
}

impl Queue {
    pub fn new() -> Queue {
        Queue { tracks: Vec::new(), index: 0, unshuffled: None, shuffle: false, repeat: Repeat::Off }
    }

    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    pub fn index(&self) -> usize {
        self.index
    }

    /// The song at the index (the one playing, or picked to play).
    pub fn at_index(&self) -> Option<&Track> {
        self.tracks.get(self.index)
    }

    pub fn set_index(&mut self, i: usize) {
        self.index = i;
    }

    pub fn get(&self, i: usize) -> Option<&Track> {
        self.tracks.get(i)
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Track> {
        self.tracks.iter()
    }

    /// The song at `i` swapped for another (its other version: song or video). False: none there.
    pub fn set_at(&mut self, i: usize, t: Track) -> bool {
        match self.tracks.get_mut(i) {
            Some(slot) => {
                *slot = t;
                true
            }
            None => false,
        }
    }

    /// Songs set as they are (a radio for the song playing, the queue from before a party):
    /// the order before Shuffle stays as it was.
    pub fn set_tracks(&mut self, tracks: Vec<Track>, index: usize) {
        self.tracks = tracks;
        self.index = index;
    }

    /// The queue as it was (a session picked up again, a party's song): no Shuffle order kept.
    pub fn restore(&mut self, tracks: Vec<Track>, index: usize) {
        self.tracks = tracks;
        self.index = index;
        self.unshuffled = None;
    }

    pub fn repeat(&self) -> Repeat {
        self.repeat
    }

    pub fn shuffle(&self) -> bool {
        self.shuffle
    }

    /// A list picked to play from `i`. With Shuffle on, the song picked first and the rest in
    /// `order` (a random one), its own order kept for when Shuffle goes off.
    pub fn play_list(&mut self, list: Vec<Track>, i: usize, order: impl FnOnce(Vec<Track>) -> Vec<Track>) {
        if self.shuffle && list.len() > 1 {
            self.unshuffled = Some(list.clone());
            let mut rest = list;
            let first = rest.remove(i.min(rest.len() - 1));
            let mut tracks = vec![first];
            tracks.extend(order(rest));
            self.tracks = tracks;
            self.index = 0;
        } else {
            self.unshuffled = None;
            self.tracks = list;
            self.index = i;
        }
    }

    /// Shuffle on: what comes after this song, in `order` (at least 2 songs). Off: the queue's
    /// own order again, carrying on from `current_key`'s song.
    pub fn set_shuffle(&mut self, on: bool, current_key: Option<&str>, order: impl FnOnce(Vec<Track>) -> Vec<Track>) {
        self.shuffle = on;
        let cur = self.index;
        if on {
            if self.tracks.len() > cur + 2 {
                self.unshuffled = Some(self.tracks.clone());
                let mut tracks = self.tracks[..=cur].to_vec();
                tracks.extend(order(self.tracks[cur + 1..].to_vec()));
                self.tracks = tracks;
            }
        } else if let Some(original) = self.unshuffled.take() {
            self.index = current_key.and_then(|k| original.iter().position(|t| t.key() == k)).unwrap_or(0);
            self.tracks = original;
        }
    }

    /// Off → the whole queue → this song → off.
    pub fn cycle_repeat(&mut self) -> Repeat {
        self.repeat = match self.repeat {
            Repeat::Off => Repeat::All,
            Repeat::All => Repeat::One,
            Repeat::One => Repeat::Off,
        };
        self.repeat
    }

    pub fn set_repeat(&mut self, r: Repeat) {
        self.repeat = r;
    }

    /// The song after this one (none with Repeat on this song; the first with the whole queue).
    pub fn next(&self) -> Option<&Track> {
        if self.repeat == Repeat::One {
            return None;
        }
        self.tracks.get(self.index + 1).or_else(|| (self.repeat == Repeat::All && self.tracks.len() > 1).then(|| &self.tracks[0]))
    }

    /// Next can be pressed.
    pub fn can_next(&self) -> bool {
        self.index + 1 < self.tracks.len() || self.repeat == Repeat::All
    }

    /// Next (`by` 1) or back (-1), `at` seconds into the song playing.
    pub fn skip(&mut self, by: i32, at: f64) -> Step {
        if by < 0 && at > 3.0 {
            return Step::Restart;
        }
        let len = self.tracks.len();
        let mut i = self.index as i64 + by as i64;
        if i >= len as i64 && by > 0 && self.repeat == Repeat::All && len > 0 {
            i = 0; // repeat the queue: from the top
        }
        if i < 0 || i >= len as i64 {
            return Step::End;
        }
        self.index = i as usize;
        Step::Play(self.index)
    }

    /// Play next: right after the song playing. Where it went.
    pub fn insert_next(&mut self, t: Track) -> usize {
        let at = (self.index + 1).min(self.tracks.len());
        self.tracks.insert(at, t);
        at
    }

    /// Add to the queue: at its end.
    pub fn push(&mut self, t: Track) {
        self.tracks.push(t);
    }

    /// Autoplay's songs: at the end.
    pub fn extend(&mut self, more: Vec<Track>) {
        self.tracks.extend(more);
    }

    /// The song at `i` out of the queue (None: there's none).
    pub fn remove(&mut self, i: usize) -> Option<Removed> {
        let len = self.tracks.len();
        if i >= len {
            return None;
        }
        let cur = self.index;
        self.tracks.remove(i);
        Some(if i < cur {
            self.index = cur - 1;
            Removed::Before
        } else if i == cur {
            if cur < len - 1 {
                Removed::Current { play: true }
            } else {
                self.index = cur.saturating_sub(1);
                Removed::Current { play: false }
            }
        } else {
            Removed::After
        })
    }

    /// A song dragged from `from` to before `to` (never above the one `playing`). False: it
    /// can't go there, or that's where it was.
    pub fn move_item(&mut self, from: usize, to: usize, playing: bool) -> bool {
        let first = self.index + usize::from(playing);
        let len = self.tracks.len();
        if from < first || from >= len {
            return false;
        }
        let to = to.clamp(first, len);
        if to == from || to == from + 1 {
            return false;
        }
        let t = self.tracks.remove(from);
        self.tracks.insert(if to > from { to - 1 } else { to }, t);
        true
    }
}

/// How much you like a song, from its plays and skips: 0.2 to 8.
pub fn liking(plays: u32, skips: u32) -> f64 {
    ((1.0 + plays as f64) / (1.0 + 2.0 * skips as f64)).clamp(0.2, 8.0)
}

/// Skipped more than played (at least twice): left out of mixes and radios.
pub fn often_skipped(plays: u32, skips: u32) -> bool {
    skips >= 2 && skips > plays
}

/// A shuffle that still listens to you: random, but songs you play a lot tend to come early
/// and songs you skip tend to come late (each song's place is u^(1/w), u random, w how much you
/// like it: weighted random order). `seed`: where the randomness starts (the time, usually).
pub fn shuffled(v: Vec<Track>, liking: impl Fn(&Track) -> f64, seed: u64) -> Vec<Track> {
    let mut x = seed | 1;
    let mut keyed: Vec<(f64, Track)> = v
        .into_iter()
        .map(|t| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let u = ((x >> 11) as f64 / (1u64 << 53) as f64).max(1e-12);
            (u.powf(1.0 / liking(&t)), t)
        })
        .collect();
    keyed.sort_by(|a, b| b.0.total_cmp(&a.0));
    keyed.into_iter().map(|(_, t)| t).collect()
}

/// The song playing, left early (before 70 % and with more than 30 s to go): a skip.
pub fn is_early_skip(at: f64, length: f64) -> bool {
    length > 0.0 && at > 1.0 && at < 0.7 * length && length - at > 30.0
}

/// Autoplay: of the songs found like `seed`, the ones to queue (25 at most): never one already
/// queued, played lately, the seed's own song, or one you keep skipping.
pub fn autoplay_picks(found: Vec<Track>, queued: &HashSet<String>, lately: &HashSet<String>, seed: &Track, skipped: impl Fn(&Track) -> bool) -> Vec<Track> {
    found
        .into_iter()
        .filter(|t| !queued.contains(&t.key()) && !lately.contains(&t.key()) && !t.same_song(seed) && !skipped(t))
        .take(25)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::Source;

    fn song(n: &str) -> Track {
        Track { source: Source::YouTube, id: n.into(), title: format!("Song {n}"), artist: "Someone".into(), album: String::new(), seconds: 200, art: String::new(), format: String::new(), file: None, video: false, genres: None }
    }
    fn songs(names: &str) -> Vec<Track> {
        names.split(',').map(song).collect()
    }
    fn ids(q: &Queue) -> String {
        q.tracks().iter().map(|t| t.id.as_str()).collect::<Vec<_>>().join(",")
    }
    fn rev(mut v: Vec<Track>) -> Vec<Track> {
        v.reverse();
        v
    }
    fn shuffled_queue(list: &str, i: usize) -> Queue {
        let mut q = Queue::new();
        q.set_shuffle(true, None, rev);
        q.play_list(songs(list), i, rev);
        q
    }

    #[test]
    fn play_list_with_shuffle_puts_the_picked_song_first() {
        let q = shuffled_queue("a,b,c,d", 2);
        assert_eq!(ids(&q), "c,d,b,a");
        assert_eq!(q.index(), 0);
    }

    #[test]
    fn play_list_without_shuffle_keeps_the_order() {
        let mut q = Queue::new();
        q.play_list(songs("a,b,c,d"), 2, rev);
        assert_eq!(ids(&q), "a,b,c,d");
        assert_eq!(q.index(), 2);
        assert_eq!(q.at_index().map(|t| t.id.as_str()), Some("c"));
    }

    #[test]
    fn shuffle_off_returns_to_the_queues_own_order_at_the_same_song() {
        let mut q = shuffled_queue("a,b,c,d", 0);
        assert_eq!(ids(&q), "a,d,c,b");
        q.set_shuffle(false, Some(&song("c").key()), rev);
        assert_eq!(ids(&q), "a,b,c,d");
        assert_eq!(q.index(), 2);
        assert!(!q.shuffle());
    }

    #[test]
    fn shuffle_on_keeps_what_has_played() {
        let mut q = Queue::new();
        q.play_list(songs("a,b,c,d,e"), 1, rev);
        q.set_shuffle(true, None, rev);
        assert_eq!(ids(&q), "a,b,e,d,c");
        // fewer than 2 songs after the one playing: nothing to shuffle
        let mut q = Queue::new();
        q.play_list(songs("a,b,c"), 1, rev);
        q.set_shuffle(true, None, rev);
        assert_eq!(ids(&q), "a,b,c");
        assert!(q.shuffle());
    }

    #[test]
    fn next_follows_repeat() {
        let mut q = Queue::new();
        q.play_list(songs("a,b,c"), 2, rev);
        assert!(q.next().is_none());
        assert_eq!(q.cycle_repeat(), Repeat::All);
        assert_eq!(q.next().map(|t| t.id.as_str()), Some("a"));
        assert!(q.can_next());
        assert_eq!(q.cycle_repeat(), Repeat::One);
        assert!(q.next().is_none());
        assert_eq!(q.cycle_repeat(), Repeat::Off);
        assert!(!q.can_next());
        // the whole queue again needs more than one song
        let mut one = Queue::new();
        one.play_list(songs("a"), 0, rev);
        one.cycle_repeat();
        assert!(one.next().is_none());
    }

    #[test]
    fn back_after_three_seconds_restarts() {
        let mut q = Queue::new();
        q.play_list(songs("a,b,c"), 2, rev);
        assert_eq!(q.skip(-1, 3.5), Step::Restart);
        assert_eq!(q.index(), 2);
        assert_eq!(q.skip(-1, 2.0), Step::Play(1));
        assert_eq!(q.index(), 1);
        q.set_index(0);
        assert_eq!(q.skip(-1, 0.5), Step::End);
    }

    #[test]
    fn next_at_the_end() {
        let mut q = Queue::new();
        q.play_list(songs("a,b,c"), 2, rev);
        assert_eq!(q.skip(1, 0.0), Step::End);
        assert_eq!(q.index(), 2);
        q.cycle_repeat();
        assert_eq!(q.skip(1, 0.0), Step::Play(0));
        assert_eq!(q.index(), 0);
    }

    #[test]
    fn songs_go_in_after_the_one_playing_or_at_the_end() {
        let mut q = Queue::new();
        q.play_list(songs("a,b,c"), 0, rev);
        assert_eq!(q.insert_next(song("x")), 1);
        q.push(song("y"));
        q.extend(songs("z1,z2"));
        assert_eq!(ids(&q), "a,x,b,c,y,z1,z2");
    }

    #[test]
    fn removing_before_the_song_moves_the_index() {
        let mut q = Queue::new();
        q.play_list(songs("a,b,c,d"), 2, rev);
        assert_eq!(q.remove(0), Some(Removed::Before));
        assert_eq!(q.index(), 1);
        assert_eq!(q.remove(3), None);
        assert_eq!(q.remove(2), Some(Removed::After));
        assert_eq!(ids(&q), "b,c");
    }

    #[test]
    fn removing_the_song_playing_plays_the_next_or_stops() {
        let mut q = Queue::new();
        q.play_list(songs("a,b,c"), 1, rev);
        assert_eq!(q.remove(1), Some(Removed::Current { play: true }));
        assert_eq!(q.index(), 1);
        assert_eq!(q.at_index().map(|t| t.id.as_str()), Some("c"));
        assert_eq!(q.remove(1), Some(Removed::Current { play: false }));
        assert_eq!(q.index(), 0);
        assert_eq!(ids(&q), "a");
    }

    #[test]
    fn moving_never_goes_above_the_song_playing() {
        let mut q = Queue::new();
        q.play_list(songs("a,b,c,d,e"), 1, rev);
        assert!(!q.move_item(0, 3, true));
        assert!(!q.move_item(3, 4, true)); // where it was
        assert!(q.move_item(4, 2, true));
        assert_eq!(ids(&q), "a,b,e,c,d");
        assert!(q.move_item(2, 5, true)); // to the end
        assert_eq!(ids(&q), "a,b,c,d,e");
    }

    #[test]
    fn a_song_swapped_for_its_other_version_and_lists_set_as_they_are() {
        let mut q = shuffled_queue("a,b,c,d", 0); // a,d,c,b; the queue's own order kept
        assert!(q.set_at(1, song("d2")));
        assert!(!q.set_at(9, song("x")));
        assert_eq!(ids(&q), "a,d2,c,b");
        assert_eq!(q.get(2).map(|t| t.id.as_str()), Some("c"));
        assert_eq!(q.iter().count(), 4);
        // set as they are (a party's queue, a radio): the order before Shuffle stays, as before
        q.set_tracks(songs("r1,r2"), 1);
        assert_eq!((ids(&q), q.index()), ("r1,r2".to_string(), 1));
        q.set_shuffle(false, Some(&song("c").key()), rev);
        assert_eq!(ids(&q), "a,b,c,d");
        // restored (a session, a party song): no order before Shuffle
        let mut q = shuffled_queue("a,b,c,d", 0);
        q.restore(songs("s1,s2,s3"), 2);
        q.set_shuffle(false, None, rev);
        assert_eq!((ids(&q), q.index()), ("s1,s2,s3".to_string(), 2));
    }

    #[test]
    fn liking_and_often_skipped_match_today() {
        assert_eq!(liking(0, 0), 1.0);
        assert_eq!(liking(20, 0), 8.0);
        assert_eq!(liking(0, 10), 0.2);
        assert_eq!(liking(3, 1), 4.0 / 3.0);
        assert!(often_skipped(1, 2));
        assert!(!often_skipped(2, 2));
        assert!(!often_skipped(0, 1));
    }

    #[test]
    fn shuffled_is_steady_for_a_seed() {
        let v = songs("a,b,c,d,e,f,g,h");
        let one = shuffled(v.clone(), |_| 1.0, 42);
        let two = shuffled(v.clone(), |_| 1.0, 42);
        assert_eq!(one.iter().map(|t| &t.id).collect::<Vec<_>>(), two.iter().map(|t| &t.id).collect::<Vec<_>>());
        let mut seen: Vec<_> = one.iter().map(|t| t.id.clone()).collect();
        seen.sort();
        assert_eq!(seen.join(","), "a,b,c,d,e,f,g,h");
    }

    #[test]
    fn a_much_liked_song_tends_to_come_first() {
        let v = songs("a,b,c,d,e,f,g,h");
        let first = (1..200u64).filter(|s| shuffled(v.clone(), |t| if t.id == "h" { 8.0 } else { 0.2 }, *s)[0].id == "h").count();
        assert!(first > 150, "h came first {first} times of 199");
    }

    #[test]
    fn an_early_skip() {
        assert!(is_early_skip(30.0, 200.0));
        assert!(!is_early_skip(150.0, 200.0));
        assert!(!is_early_skip(0.5, 200.0));
        assert!(!is_early_skip(180.0, 200.0));
        assert!(!is_early_skip(30.0, 0.0));
    }

    #[test]
    fn autoplay_never_brings_back_queued_lately_skipped_or_the_seed() {
        let seed = song("seed");
        let mut found: Vec<Track> = (0..40).map(|i| song(&format!("f{i}"))).collect();
        let mut again = song("other-id");
        again.title = seed.title.clone(); // the seed's own song, another upload
        found.push(again);
        let queued: HashSet<String> = ["f0", "f1"].iter().map(|n| song(n).key()).collect();
        let lately: HashSet<String> = ["f2"].iter().map(|n| song(n).key()).collect();
        let picks = autoplay_picks(found, &queued, &lately, &seed, |t| t.id == "f3");
        assert_eq!(picks.len(), 25);
        assert!(picks.iter().all(|t| !["f0", "f1", "f2", "f3", "other-id"].contains(&t.id.as_str())));
        assert_eq!(picks[0].id, "f4");
    }
}
