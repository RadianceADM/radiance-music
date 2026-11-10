//! The split jobs (worker thread; minutes), moved out of the GTK app's run_splits and run_fast so the
//! Windows beta splits the same way (issue #45 stage 3): a song in four parts (this computer's splitter
//! or your split server), then its words timed or its copy kept on the server; or the fast split in two.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::relay::Relay;
use crate::settings::{LocalSplitter, SplitServer};
use crate::split;
use crate::track::Track;
use crate::word_times::{aligned_by_mms, keep_on_server, time_words, Aligner};
use crate::youtube;

/// How far a split has got: splitting (0..1), then its copy for the split server's library, then its
/// words' times (the GTK bar's -1 and -2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    Splitting(f64),
    LibraryCopy,
    WordTimes,
}

impl Phase {
    /// The GTK app's progress number: 0..1, or -1 (the library copy), -2 (the word times).
    pub fn code(self) -> f64 {
        match self {
            Phase::Splitting(p) => p,
            Phase::LibraryCopy => -1.0,
            Phase::WordTimes => -2.0,
        }
    }
}

/// What follows a four-part split.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum After {
    /// A copy kept in your split server's library (its words timed there first).
    Host,
    /// Every word timed here (MMS), once.
    TimeWordsHere,
    Nothing,
}

pub fn after_split(server: bool, local: bool, aligned: bool) -> After {
    if server {
        After::Host
    } else if local && !aligned {
        After::TimeWordsHere
    } else {
        After::Nothing
    }
}

/// The song's address for a splitter: its own file's (`plain`), else its stream through the local relay.
pub fn source_uri(t: &Track, plain: Option<String>, relay: Option<Relay>) -> Result<String, String> {
    match plain {
        Some(u) => Ok(u),
        None => {
            let relay = relay.ok_or("no relay")?;
            // a joined server's copy first (rule 1): its sound, not YouTube's
            crate::playing::stream_of(t, &relay, || youtube::stream(&t.id)).map(|(uri, _)| uri)
        }
    }
}

/// Splits asked for by hand (Split, No vocals): they don't wait on a server's own split (rule 1's wait is
/// for the liked songs split in the background).
static ASKED: std::sync::Mutex<Option<std::collections::HashSet<String>>> = std::sync::Mutex::new(None);

/// This song's split was asked for by hand (the apps' split_now / a first-in-line split).
pub fn asked_by_hand(key: &str) {
    if let Ok(mut a) = ASKED.lock() {
        a.get_or_insert_with(Default::default).insert(key.to_string());
    }
}

fn was_asked_by_hand(key: &str) -> bool {
    ASKED.lock().ok().is_some_and(|mut a| a.as_mut().is_some_and(|a| a.remove(key)))
}

/// How many checks in a row a server may leave unanswered while its split is waited on.
const QUIET_MOST: u32 = 3;

/// Rule 1: how often a joined server's own split of a song is checked on, and for how long (the phone's).
const WAIT_EVERY: Duration = Duration::from_secs(20);
const WAIT_MOST: Duration = Duration::from_secs(45 * 60);

/// A song in four parts (the GTK app's run_splits worker), then its words timed or its copy kept.
/// A joined server that has the song gives its parts instead (rule 1).
pub fn four_parts(t: &Track, uri: &str, server: Option<&SplitServer>, local: Option<&LocalSplitter>, report: Arc<dyn Fn(Phase) + Send + Sync>) -> Result<(), String> {
    four_parts_with(t, uri, server, local, report, WAIT_EVERY, WAIT_MOST)
}

/// As [four_parts], checking on a server's own split every `every` for up to `most`.
pub fn four_parts_with(t: &Track, uri: &str, server: Option<&SplitServer>, local: Option<&LocalSplitter>, report: Arc<dyn Fn(Phase) + Send + Sync>, every: Duration, most: Duration) -> Result<(), String> {
    if from_a_server(t, report.as_ref(), Some((every, most))) {
        return Ok(());
    }
    let r = report.clone();
    let done = split::split(t, uri, server, local, Arc::new(move |p| r(Phase::Splitting(p))));
    if done.is_ok() {
        match (after_split(server.is_some(), local.is_some(), aligned_by_mms(t)), server, local) {
            (After::Host, Some(server), _) => {
                report(Phase::LibraryCopy);
                let r = report.clone();
                keep_on_server(t, server, &move || r(Phase::WordTimes)); // for hosting, with its words timed
            }
            (After::TimeWordsHere, _, Some(local)) => {
                // every word timed here too
                report(Phase::WordTimes);
                if let Err(e) = time_words(t, &Aligner::Here(local.clone())) {
                    crate::log::note(&format!("word times for {}: {e}", t.title));
                }
            }
            _ => {}
        }
    }
    done
}

/// The fast split in two (Music + Vocals, inside the app: the GTK app's run_fast worker). A joined
/// server's split copy gives its parts instead (rule 1); one not split yet isn't waited for (it's asked by hand).
pub fn two_parts(t: &Track, uri: &str, local: Option<&LocalSplitter>, report: Arc<dyn Fn(Phase) + Send + Sync>) -> Result<(), String> {
    if from_a_server(t, report.as_ref(), None) {
        return Ok(());
    }
    split::split_two(t, uri, local, Arc::new(move |p| report(Phase::Splitting(p))))
}

/// Rule 1 (owner, 2026-11-09): a joined server's copy of the song gives its parts instead of a split
/// here. Its copy not split yet: with `wait`, the server is asked to split it and checked on every
/// `every` for up to `most`; it failed, or can't (the owner turned splitting off for you): split here.
/// Whether the parts came.
fn from_a_server(t: &Track, report: &(dyn Fn(Phase) + Send + Sync), wait: Option<(Duration, Duration)>) -> bool {
    let Some((server, mut song)) = crate::servers::copy_of(t) else { return false };
    // waited on only for a saved song split in the background: one asked for by hand, or a song only
    // streamed (No vocals as it plays), is split here at once
    let by_hand = was_asked_by_hand(&t.key());
    if !song.split {
        let Some((every, most)) = wait.filter(|_| !by_hand && t.file.is_some()) else { return false };
        if let Err(f) = crate::servers::split_by_id(&server, song.id) {
            crate::log::note(&format!("{} won't split “{}” ({}): split here", server.name, t.title, f.words(&server.name)));
            return false;
        }
        let asked = Instant::now();
        let mut quiet = 0;
        loop {
            if asked.elapsed() >= most {
                crate::log::note(&format!("{} hasn't split “{}” yet: split here", server.name, t.title));
                return false;
            }
            std::thread::sleep(every);
            match crate::servers::song(&server, song.id) {
                Ok(s) if s.split => {
                    song = s;
                    break;
                }
                Ok(_) => quiet = 0,
                Err(f) if f.status == 404 || f.status == 401 => return false,
                Err(f) => {
                    // away a moment: asked again; a few times unanswered: split here
                    quiet += 1;
                    if quiet >= QUIET_MOST {
                        crate::log::note(&format!("{} stopped answering ({}): “{}” is split here", server.name, f.words(&server.name), t.title));
                        crate::servers::skip_for_now(&server.address);
                        return false;
                    }
                    continue;
                }
            }
            if let Ok(q) = crate::servers::queue(&server) {
                match q.jobs.iter().find(|j| j.song == song.id) {
                    Some(j) if j.state == "failed" || j.state == "removed" => {
                        crate::log::note(&format!("{} didn't split “{}” ({}): split here", server.name, t.title, j.note));
                        return false;
                    }
                    Some(j) => report(Phase::Splitting(j.progress)),
                    None => {}
                }
            }
        }
    }
    match crate::servers::bring_parts(t, &server, &song) {
        Ok(n) if n > 0 => {
            crate::log::note(&format!("“{}”: its parts from {} (not split here)", t.title, server.name));
            true
        }
        Ok(_) => false,
        Err(e) => {
            crate::log::note(&format!("“{}”'s parts from {}: {e}: split here", t.title, server.name));
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn after_a_split_the_words_are_timed_once() {
        for server in [false, true] {
            for local in [false, true] {
                for aligned in [false, true] {
                    let want = if server {
                        After::Host
                    } else if local && !aligned {
                        After::TimeWordsHere
                    } else {
                        After::Nothing
                    };
                    assert_eq!(after_split(server, local, aligned), want, "server {server} local {local} aligned {aligned}");
                }
            }
        }
        assert_eq!([Phase::Splitting(0.4).code(), Phase::LibraryCopy.code(), Phase::WordTimes.code()], [0.4, -1.0, -2.0]);
    }
}
