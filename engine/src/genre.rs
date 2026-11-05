//! A song's genres, from Deezer's free public catalogue (no account): the genres
//! of the album it's on, e.g. ["Rap/Hip Hop", "R&B"].

use crate::net;
use crate::track::{plain, Track};

/// The artists credited on a song, one by one ("A & B, C feat. D" -> A, B, C, D).
pub fn artists(credit: &str) -> Vec<String> {
    crate::track::split_credit(credit)
}

/// Looks the song up (worker thread). Empty when Deezer doesn't know it.
pub fn lookup(t: &Track) -> Vec<String> {
    let artist = artists(&t.artist).into_iter().next().unwrap_or_default();
    let q = format!("{artist} {}", t.title);
    let Some(found) = net::get_json("https://api.deezer.com/search", &[("q", q.as_str()), ("limit", "5")]) else { return vec![] };
    let hits = found["data"].as_array().cloned().unwrap_or_default();
    // the same artist first, else the first hit
    let hit = hits
        .iter()
        .find(|h| h["artist"]["name"].as_str().is_some_and(|n| plain(n) == plain(&artist)))
        .or_else(|| hits.first());
    let Some(album) = hit.and_then(|h| h["album"]["id"].as_u64()) else { return vec![] };
    let Some(info) = net::get_json(&format!("https://api.deezer.com/album/{album}"), &[]) else { return vec![] };
    info["genres"]["data"]
        .as_array()
        .map(|g| g.iter().filter_map(|x| x["name"].as_str().map(String::from)).collect())
        .unwrap_or_default()
}
