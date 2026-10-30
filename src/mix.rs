//! "For you": songs like the ones you liked. The likes you play most (by your
//! plays and skips) each seed a YouTube Music radio; the radios are taken in
//! turns, without songs you already liked, doubles, or songs you keep skipping.

use std::thread;

use crate::track::{Source, Track};
use crate::youtube;

const SEEDS: usize = 5;
const MAX: usize = 40;

/// A song's radio, also for a lossless song (found on YouTube Music first).
pub fn radio(seed: &Track) -> Vec<Track> {
    let id = match seed.source {
        Source::YouTube => Some(seed.id.clone()),
        Source::Archive => youtube::search(&format!("{} {}", seed.artist, seed.title)).first().map(|t| t.id.clone()),
    };
    id.map(|id| youtube::radio(&id)).unwrap_or_default()
}

pub fn from_likes(liked: &[Track], seeds: &[Track], avoid: &std::collections::HashSet<String>) -> Vec<Track> {
    let radios: Vec<Vec<Track>> = seeds
        .iter()
        .take(SEEDS)
        .cloned()
        .map(|seed| thread::spawn(move || radio(&seed)))
        .collect::<Vec<_>>()
        .into_iter()
        .filter_map(|h| h.join().ok())
        .collect();
    let mut out: Vec<Track> = vec![];
    let longest = radios.iter().map(Vec::len).max().unwrap_or(0);
    'all: for i in 0..longest {
        for radio in &radios {
            let Some(t) = radio.get(i) else { continue };
            let known = |x: &Track| x.key() == t.key() || x.same_song(t);
            if !liked.iter().any(known) && !out.iter().any(known) && !avoid.contains(&t.key()) {
                out.push(t.clone());
                if out.len() >= MAX {
                    break 'all;
                }
            }
        }
    }
    out
}
