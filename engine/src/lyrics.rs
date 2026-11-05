//! Lyrics from LRCLIB (lrclib.net: free, open, no account), else the text YouTube
//! Music shows (its Lyrics tab) for a YouTube song. What lyrics are, and reading and
//! writing them, is the core's (core/src/lyrics.rs), re-exported here.

pub use radiance_core::lyrics::*;

use crate::net;
use crate::track::Track;

pub fn find(t: &Track) -> Lyrics {
    match lrclib(t) {
        Lyrics::None if t.source == crate::track::Source::YouTube => crate::youtube::lyrics(&t.id).map(Lyrics::Plain).unwrap_or(Lyrics::None),
        found => found,
    }
}

fn lrclib(t: &Track) -> Lyrics {
    let seconds = t.seconds.to_string();
    let mut query = vec![("track_name", t.title.as_str()), ("artist_name", t.artist.as_str())];
    if !t.album.is_empty() {
        query.push(("album_name", t.album.as_str()));
    }
    if t.seconds > 0 {
        query.push(("duration", seconds.as_str()));
    }
    let same_artist = |v: &serde_json::Value| by_them(t, v);
    if let Some(v) = net::get_json("https://lrclib.net/api/get", &query).filter(|v| same_artist(v)) {
        if let Some(l) = from(&v) {
            return l;
        }
    }
    // not that exact recording: the best match by title and artist
    let found = net::get_json(
        "https://lrclib.net/api/search",
        &[("track_name", t.title.as_str()), ("artist_name", t.artist.as_str())],
    );
    let list = found.as_ref().and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let close = |v: &&serde_json::Value| {
        same_artist(v) && (t.seconds == 0 || v["duration"].as_f64().is_some_and(|d| (d - t.seconds as f64).abs() < 8.0))
    };
    list.iter()
        .filter(close)
        .find(|v| v["hasWordSync"].as_bool() == Some(true))
        .or_else(|| list.iter().filter(close).find(|v| v["syncedLyrics"].is_string()))
        .or_else(|| list.iter().find(|v| close(v)))
        .and_then(from)
        .unwrap_or(Lyrics::None)
}

