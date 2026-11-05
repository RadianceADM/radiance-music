//! Lyrics from LRCLIB (lrclib.net: free, open, no account), else the text YouTube
//! Music shows (its Lyrics tab) for a YouTube song. What lyrics are, and reading and
//! writing them, is the core's (core/src/lyrics.rs), re-exported here.

pub use radiance_core::lyrics::*;

use gio::prelude::*;

use crate::net;
use crate::track::{Source, Track};

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

/// A saved song's own Lyricsfile first, else LRCLIB, else its old .lrc (worker
/// thread). A song saved before word timing asks LRCLIB once more and keeps the
/// answer as a Lyricsfile, so its words light up too.
///
/// Lines without their words' times get them from the song's Vocals part once
/// it's been split (see align.rs); a saved song keeps that in its Lyricsfile.
pub fn lyrics_for(t: &Track) -> Lyrics {
    lyrics_and_source(t).0
}

/// lyrics_for, and whether its words were timed from the vocals just now.
pub fn lyrics_and_source(t: &Track) -> (Lyrics, bool) {
    lyrics_and_source_with(t, find)
}

/// lyrics_and_source, with `find` looking lyrics up on the internet (LRCLIB, YouTube Music:
/// [find]; a test's own).
pub fn lyrics_and_source_with(t: &Track, find: impl Fn(&Track) -> Lyrics) -> (Lyrics, bool) {
    let file = t.file.as_deref();
    let kept = std::fs::read_to_string(lyrics_file(t)).ok().and_then(|y| parse_lyricsfile(&y)).or_else(|| server_lyrics(t));
    let fresh = kept.is_none();
    let mut l = kept.unwrap_or_else(|| {
        let found = find(t);
        match (file, &found) {
            (None, _) | (_, Lyrics::Timed { .. }) => found,
            (Some(f), _) => match std::fs::read_to_string(crate::library::beside(f, "lrc")) {
                Ok(text) => {
                    let lines = parse_lrc(&text);
                    if lines.is_empty() { Lyrics::Plain(text) } else { Lyrics::Timed { lines, exact: false } }
                }
                Err(_) => found,
            },
        }
    });
    let mut from_vocals = false;
    if let Lyrics::Timed { lines, exact } = &mut l {
        if let (false, Some(vocals)) = (*exact, crate::split::ready_part(t, "vocals")) {
            if crate::align::words_from_vocals(lines, &gio::File::for_path(&vocals).uri()) {
                *exact = true;
                from_vocals = true;
            }
        }
    }
    if let (Some(f), true, Lyrics::Timed { .. }) = (file, fresh || from_vocals, &l) {
        if let Some((ext, text)) = to_file(&l, Some(t), if from_vocals { &[VOCALS_NOTE] } else { &[] }) {
            let _ = std::fs::write(crate::library::beside(f, ext), text);
        }
    }
    (l, from_vocals)
}

/// A server's song (not saved): the lyrics its server keeps for it, a Lyricsfile
/// kept here too (in the cache, as any song's), else its .lrc text (worker thread).
pub fn server_lyrics(t: &Track) -> Option<Lyrics> {
    if t.source != Source::Server || t.file.is_some() {
        return None;
    }
    let (yaml, text) = crate::servers::lyrics_text(t)?;
    if yaml {
        let l = parse_lyricsfile(&text)?;
        let _ = std::fs::write(lyrics_file(t), text);
        return Some(l);
    }
    let lines = parse_lrc(&text);
    Some(if lines.is_empty() { Lyrics::Plain(text) } else { Lyrics::Timed { lines, exact: false } })
}


/// Where a song's Lyricsfile is kept: next to a saved song, else in the cache.
pub fn lyrics_file(t: &Track) -> std::path::PathBuf {
    let p = lyrics_path(t);
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    p
}

pub fn lyrics_path(t: &Track) -> std::path::PathBuf {
    match t.file.as_deref() {
        Some(f) => crate::library::beside(f, "lyrics.yaml"),
        None => glib::user_cache_dir().join("radiance-music/lyrics").join(format!("{}.lyrics.yaml", crate::track::file_name(&t.key()))),
    }
}
