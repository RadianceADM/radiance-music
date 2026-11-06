//! The split jobs (worker thread; minutes), moved out of the GTK app's run_splits and run_fast so the
//! Windows beta splits the same way (issue #45 stage 3): a song in four parts (this computer's splitter
//! or your split server), then its words timed or its copy kept on the server; or the fast split in two.

use std::sync::Arc;

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
            let s = youtube::stream(&t.id)?;
            Ok(relay.add(s.url, s.size, s.mime))
        }
    }
}

/// A song in four parts (the GTK app's run_splits worker), then its words timed or its copy kept.
pub fn four_parts(t: &Track, uri: &str, server: Option<&SplitServer>, local: Option<&LocalSplitter>, report: Arc<dyn Fn(Phase) + Send + Sync>) -> Result<(), String> {
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

/// The fast split in two (Music + Vocals, inside the app: the GTK app's run_fast worker).
pub fn two_parts(t: &Track, uri: &str, local: Option<&LocalSplitter>, report: Arc<dyn Fn(Phase) + Send + Sync>) -> Result<(), String> {
    split::split_two(t, uri, local, Arc::new(move |p| report(Phase::Splitting(p))))
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
