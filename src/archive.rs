//! Internet Archive: free, lossless (FLAC) recordings. Searched first.
//!
//! The search finds items (an album, a concert, a 78 rpm record) that have FLAC
//! files; each item's file list then gives the songs, kept only when every word
//! searched for is in the song's title, artist or album.

use std::thread;

use serde_json::Value;

use crate::net;
use crate::track::{parse_clock, Source, Track};

const ITEMS: usize = 8;
const MAX: usize = 10;

pub fn search(query: &str) -> Vec<Track> {
    let words: Vec<String> = query.split_whitespace().map(|w| w.to_lowercase()).collect();
    if words.is_empty() {
        return vec![];
    }
    let escaped: String = query.chars().filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '\'').collect();
    let q = format!("({escaped}) AND mediatype:(audio) AND format:(\"Flac\" OR \"24bit Flac\")");
    let rows = ITEMS.to_string();
    let Some(found) = net::get_json(
        "https://archive.org/advancedsearch.php",
        &[("q", &q), ("fl[]", "identifier"), ("rows", &rows), ("sort[]", "downloads desc"), ("output", "json")],
    ) else {
        return vec![];
    };
    let ids: Vec<String> = found["response"]["docs"]
        .as_array()
        .map(|docs| docs.iter().filter_map(|d| d["identifier"].as_str().map(String::from)).collect())
        .unwrap_or_default();
    // the items' file lists, all at once
    let lists: Vec<_> = ids
        .into_iter()
        .map(|id| thread::spawn(move || net::get_json(&format!("https://archive.org/metadata/{id}"), &[]).map(|m| (id, m))))
        .collect();
    let mut out: Vec<Track> = vec![];
    for handle in lists {
        let Some((id, meta)) = handle.join().ok().flatten() else { continue };
        for t in tracks(&id, &meta) {
            let hay = format!("{} {} {}", t.title, t.artist, t.album).to_lowercase();
            if words.iter().all(|w| hay.contains(w.as_str())) && !out.iter().any(|o| o.id == t.id) {
                out.push(t);
            }
        }
    }
    // the song's own title matching first, then hi-res
    let phrase = query.to_lowercase();
    out.sort_by_key(|t| (!t.title.to_lowercase().contains(&phrase), t.format != "24-bit FLAC"));
    out.truncate(MAX);
    out
}

fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(", "),
        _ => String::new(),
    }
}

/// An item's FLAC files as songs.
fn tracks(id: &str, meta: &Value) -> Vec<Track> {
    let m = &meta["metadata"];
    let (item_title, item_artist) = (text(&m["title"]), text(&m["creator"]));
    let Some(files) = meta["files"].as_array() else { return vec![] };
    files
        .iter()
        .filter_map(|f| {
            let format = f["format"].as_str()?;
            let format = match format {
                "Flac" => "FLAC",
                "24bit Flac" => "24-bit FLAC",
                _ => return None,
            };
            let name = f["name"].as_str()?;
            let title = Some(text(&f["title"])).filter(|t| !t.is_empty()).unwrap_or_else(|| {
                name.rsplit('/').next().unwrap_or(name).trim_end_matches(".flac").replace('_', " ")
            });
            let artist = [text(&f["artist"]), text(&f["creator"]), item_artist.clone()]
                .into_iter()
                .find(|a| !a.is_empty())
                .unwrap_or_default();
            let album = Some(text(&f["album"])).filter(|a| !a.is_empty()).unwrap_or_else(|| item_title.clone());
            Some(Track {
                source: Source::Archive,
                id: format!("https://archive.org/download/{id}/{}", encode_path(name)),
                title,
                artist,
                album,
                seconds: f["length"].as_str().and_then(parse_clock).unwrap_or(0),
                art: format!("https://archive.org/services/img/{id}"),
                format: format.into(),
                file: None,
                video: false,
                genres: None,
            })
        })
        .collect()
}

/// Percent-encodes a file path for a URL, keeping the slashes.
fn encode_path(path: &str) -> String {
    let mut out = String::new();
    for b in path.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
