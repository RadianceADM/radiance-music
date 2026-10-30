//! The app's data in one SQLite database, ~/.local/share/radiance-music/radiance.db:
//! liked songs, every play (the history; all of it is kept), where you left off
//! (the queue, the song in it and how far in) and which splits the server's
//! library has. Settings stay in settings.json, a small file to read or edit.
//!
//! The JSON files earlier versions wrote are read in when they're newer than
//! the last time (so nothing is lost switching over) and left as they are, as a
//! backup. A screenshot copy (RADIANCE_SHOT) reads but never writes.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gtk::glib;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

use crate::track::Track;

static DB: Mutex<Option<Connection>> = Mutex::new(None);

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS meta (name TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS liked (
    key TEXT PRIMARY KEY, place INTEGER NOT NULL,
    title TEXT NOT NULL, artist TEXT NOT NULL, album TEXT NOT NULL, track TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS plays (
    id INTEGER PRIMARY KEY, key TEXT NOT NULL, played_at INTEGER NOT NULL,
    title TEXT NOT NULL, artist TEXT NOT NULL, track TEXT NOT NULL, UNIQUE (key, played_at));
CREATE INDEX IF NOT EXISTS plays_by_time ON plays (played_at);
CREATE TABLE IF NOT EXISTS session (
    id INTEGER PRIMARY KEY CHECK (id = 1), queue TEXT NOT NULL, place INTEGER NOT NULL, position REAL NOT NULL);
CREATE TABLE IF NOT EXISTS hosted (key TEXT PRIMARY KEY, at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS skips (id INTEGER PRIMARY KEY, key TEXT NOT NULL, at INTEGER NOT NULL);
CREATE INDEX IF NOT EXISTS skips_by_key ON skips (key);
CREATE TABLE IF NOT EXISTS playlists (id INTEGER PRIMARY KEY, name TEXT NOT NULL, created INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS playlist_songs (
    playlist INTEGER NOT NULL, place INTEGER NOT NULL, key TEXT NOT NULL, track TEXT NOT NULL, PRIMARY KEY (playlist, key));
CREATE TABLE IF NOT EXISTS lyrics_checked (key TEXT PRIMARY KEY, at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS videos (video_id TEXT PRIMARY KEY, song_key TEXT NOT NULL, file TEXT NOT NULL, saved_at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS six_parts (key TEXT PRIMARY KEY, at INTEGER NOT NULL);
";

fn dir() -> PathBuf {
    glib::user_data_dir().join("radiance-music")
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn shot() -> bool {
    std::env::var_os("RADIANCE_SHOT").is_some()
}

fn open() -> rusqlite::Result<Connection> {
    let _ = fs::create_dir_all(dir());
    let c = Connection::open(dir().join("radiance.db"))?;
    c.busy_timeout(Duration::from_secs(3))?;
    c.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")?;
    c.execute_batch(SCHEMA)?;
    read_json(&c)?;
    Ok(c)
}

/// Runs `f` on the database (opened the first time). None if it can't be used.
fn with<T>(f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Option<T> {
    let mut guard = DB.lock().ok()?;
    if guard.is_none() {
        *guard = open().map_err(|e| eprintln!("database: {e}")).ok();
    }
    f(guard.as_ref()?).map_err(|e| eprintln!("database: {e}")).ok()
}

/// A change (never from a screenshot copy).
fn write(f: impl FnOnce(&Connection) -> rusqlite::Result<()>) {
    if !shot() {
        with(f);
    }
}

fn json(t: &Track) -> String {
    t.to_json().to_string()
}

fn track(text: String) -> Option<Track> {
    serde_json::from_str::<Value>(&text).ok().as_ref().and_then(Track::from_json)
}

/// Earlier versions' JSON files, when newer than the last time they were read.
fn read_json(c: &Connection) -> rusqlite::Result<()> {
    let since: i64 = c.query_row("SELECT value FROM meta WHERE name = 'json_read_at'", [], |r| r.get::<_, String>(0)).optional()?.and_then(|v| v.parse().ok()).unwrap_or(0);
    let newer = |name: &str| -> Option<Value> {
        let p = dir().join(name);
        let changed = fs::metadata(&p).and_then(|m| m.modified()).ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
        (changed > since).then(|| fs::read_to_string(&p).ok().and_then(|t| serde_json::from_str(&t).ok())).flatten()
    };
    let tx = c.unchecked_transaction()?;
    if let Some(v) = newer("liked.json") {
        let liked: Vec<Track> = v.as_array().map(|a| a.iter().filter_map(Track::from_json).collect()).unwrap_or_default();
        put_liked(&tx, &liked)?;
    }
    if let Some(v) = newer("session.json") {
        let queue: Vec<Value> = v["queue"].as_array().cloned().unwrap_or_default();
        tx.execute(
            "INSERT OR REPLACE INTO session (id, queue, place, position) VALUES (1, ?1, ?2, ?3)",
            params![Value::Array(queue).to_string(), v["index"].as_i64().unwrap_or(0), v["position"].as_f64().unwrap_or(0.0)],
        )?;
        for h in v["history"].as_array().into_iter().flatten() {
            if let Some(t) = Track::from_json(h) {
                tx.execute(
                    "INSERT OR IGNORE INTO plays (key, played_at, title, artist, track) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![t.key(), h["played_at"].as_i64().unwrap_or(0), t.title, t.artist, json(&t)],
                )?;
            }
        }
    }
    if let Some(v) = newer("hosted.json") {
        for k in v.as_array().into_iter().flatten().filter_map(|k| k.as_str()) {
            tx.execute("INSERT OR IGNORE INTO hosted (key, at) VALUES (?1, ?2)", params![k, now()])?;
        }
    }
    tx.execute("INSERT OR REPLACE INTO meta (name, value) VALUES ('json_read_at', ?1)", params![now().to_string()])?;
    tx.commit()
}

fn put_liked(c: &Connection, liked: &[Track]) -> rusqlite::Result<()> {
    c.execute("DELETE FROM liked", [])?;
    let mut add = c.prepare("INSERT OR REPLACE INTO liked (key, place, title, artist, album, track) VALUES (?1, ?2, ?3, ?4, ?5, ?6)")?;
    for (i, t) in liked.iter().enumerate() {
        add.execute(params![t.key(), i as i64, t.title, t.artist, t.album, json(t)])?;
    }
    Ok(())
}

// ------------------------------------------------------------------ liked songs

/// The liked songs, newest first.
pub fn liked() -> Vec<Track> {
    with(|c| {
        let mut q = c.prepare("SELECT track FROM liked ORDER BY place")?;
        let rows = q.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(|r| r.ok().and_then(track)).collect())
    })
    .unwrap_or_default()
}

pub fn set_liked(liked: &[Track]) {
    write(|c| {
        let tx = c.unchecked_transaction()?;
        put_liked(&tx, liked)?;
        tx.commit()
    });
}

// ------------------------------------------------------------------ where you left off

/// The queue, the song in it and how far in.
pub fn session() -> (Vec<Track>, usize, f64) {
    with(|c| {
        c.query_row("SELECT queue, place, position FROM session WHERE id = 1", [], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, f64>(2)?)))
            .optional()
    })
    .flatten()
    .map(|(q, place, position)| {
        let v: Value = serde_json::from_str(&q).unwrap_or(Value::Null);
        let queue: Vec<Track> = v.as_array().map(|a| a.iter().filter_map(Track::from_json).collect()).unwrap_or_default();
        let place = (place.max(0) as usize).min(queue.len().saturating_sub(1));
        (queue, place, position.max(0.0))
    })
    .unwrap_or_default()
}

pub fn set_session(queue: &[Track], place: usize, position: f64) {
    let q = Value::Array(queue.iter().map(Track::to_json).collect()).to_string();
    write(|c| c.execute("INSERT OR REPLACE INTO session (id, queue, place, position) VALUES (1, ?1, ?2, ?3)", params![q, place as i64, position]).map(|_| ()));
}

// ------------------------------------------------------------------ history

/// The latest `n` plays, newest first: (song, when, unix seconds).
pub fn plays(n: usize) -> Vec<(Track, i64)> {
    with(|c| {
        let mut q = c.prepare("SELECT track, played_at FROM plays ORDER BY played_at DESC, id DESC LIMIT ?1")?;
        let rows = q.query_map(params![n as i64], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        Ok(rows.filter_map(|r| r.ok()).filter_map(|(t, at)| Some((track(t)?, at))).collect())
    })
    .unwrap_or_default()
}

pub fn add_play(t: &Track, at: i64) {
    write(|c| {
        c.execute("INSERT OR IGNORE INTO plays (key, played_at, title, artist, track) VALUES (?1, ?2, ?3, ?4, ?5)", params![t.key(), at, t.title, t.artist, json(t)]).map(|_| ())
    });
}

// ------------------------------------------------------------------ the server's library

pub fn hosted() -> std::collections::HashSet<String> {
    with(|c| {
        let mut q = c.prepare("SELECT key FROM hosted")?;
        let rows = q.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    })
    .unwrap_or_default()
}

pub fn add_hosted(key: &str) {
    write(|c| c.execute("INSERT OR REPLACE INTO hosted (key, at) VALUES (?1, ?2)", params![key, now()]).map(|_| ()));
}

// ------------------------------------------------------------------ six parts

/// The song was ticked to be in six parts (Piano and Guitar too).
pub fn wants_six(key: &str) -> bool {
    with(|c| c.query_row("SELECT 1 FROM six_parts WHERE key = ?1", params![key], |_| Ok(())).optional()).flatten().is_some()
}

pub fn set_six(key: &str, on: bool) {
    if on {
        write(|c| c.execute("INSERT OR REPLACE INTO six_parts (key, at) VALUES (?1, ?2)", params![key, now()]).map(|_| ()));
    } else {
        write(|c| c.execute("DELETE FROM six_parts WHERE key = ?1", params![key]).map(|_| ()));
    }
}

// ------------------------------------------------------------------ saved music videos

/// A saved music video's file, by its YouTube id (if it's still there).
pub fn video_file(video_id: &str) -> Option<String> {
    with(|c| c.query_row("SELECT file FROM videos WHERE video_id = ?1", params![video_id], |r| r.get::<_, String>(0)).optional())
        .flatten()
        .filter(|f| !f.is_empty() && std::path::Path::new(f).exists())
}

/// The songs (keys) whose music video is saved, or that have none.
pub fn songs_with_video() -> std::collections::HashSet<String> {
    with(|c| {
        let mut q = c.prepare("SELECT song_key, file FROM videos")?;
        let rows = q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        // a saved file, or "" for a song that has no official video
        Ok(rows.filter_map(|r| r.ok()).filter(|(_, f)| f.is_empty() || std::path::Path::new(f).exists()).map(|(k, _)| k).collect())
    })
    .unwrap_or_default()
}

pub fn add_video(video_id: &str, song_key: &str, file: &str) {
    write(|c| c.execute("INSERT OR REPLACE INTO videos (video_id, song_key, file, saved_at) VALUES (?1, ?2, ?3, ?4)", params![video_id, song_key, file, now()]).map(|_| ()));
}

// ------------------------------------------------------------------ lyrics checked

/// The song's kept lyrics were checked against its artist (see ui::recheck_lyrics).
pub fn lyrics_checked(key: &str) -> bool {
    with(|c| c.query_row("SELECT 1 FROM lyrics_checked WHERE key = ?1", params![key], |_| Ok(())).optional()).flatten().is_some()
}

pub fn set_lyrics_checked(key: &str) {
    write(|c| c.execute("INSERT OR REPLACE INTO lyrics_checked (key, at) VALUES (?1, ?2)", params![key, now()]).map(|_| ()));
}

// ------------------------------------------------------------------ playlists

/// Your playlists, oldest first: (id, name, how many songs).
pub fn playlists() -> Vec<(i64, String, usize)> {
    with(|c| {
        let mut q = c.prepare("SELECT p.id, p.name, (SELECT COUNT(*) FROM playlist_songs s WHERE s.playlist = p.id) FROM playlists p ORDER BY p.created, p.id")?;
        let rows = q.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)? as usize)))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    })
    .unwrap_or_default()
}

/// A playlist's songs, in order.
pub fn playlist_songs(id: i64) -> Vec<Track> {
    with(|c| {
        let mut q = c.prepare("SELECT track FROM playlist_songs WHERE playlist = ?1 ORDER BY place")?;
        let rows = q.query_map(params![id], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(|r| r.ok().and_then(track)).collect())
    })
    .unwrap_or_default()
}

pub fn create_playlist(name: &str) -> Option<i64> {
    if shot() {
        return None;
    }
    with(|c| {
        c.execute("INSERT INTO playlists (name, created) VALUES (?1, ?2)", params![name, now()])?;
        Ok(c.last_insert_rowid())
    })
}

pub fn rename_playlist(id: i64, name: &str) {
    write(|c| c.execute("UPDATE playlists SET name = ?2 WHERE id = ?1", params![id, name]).map(|_| ()));
}

pub fn delete_playlist(id: i64) {
    write(|c| {
        let tx = c.unchecked_transaction()?;
        tx.execute("DELETE FROM playlist_songs WHERE playlist = ?1", params![id])?;
        tx.execute("DELETE FROM playlists WHERE id = ?1", params![id])?;
        tx.commit()
    });
}

/// Adds a song at the end (once: a song already there stays where it is). True if added.
pub fn add_to_playlist(id: i64, t: &Track) -> bool {
    if shot() {
        return false;
    }
    with(|c| {
        let next: i64 = c.query_row("SELECT COALESCE(MAX(place), -1) + 1 FROM playlist_songs WHERE playlist = ?1", params![id], |r| r.get(0))?;
        let n = c.execute("INSERT OR IGNORE INTO playlist_songs (playlist, place, key, track) VALUES (?1, ?2, ?3, ?4)", params![id, next, t.key(), json(t)])?;
        Ok(n > 0)
    })
    .unwrap_or(false)
}

pub fn remove_from_playlist(id: i64, key: &str) {
    write(|c| c.execute("DELETE FROM playlist_songs WHERE playlist = ?1 AND key = ?2", params![id, key]).map(|_| ()));
}

// ------------------------------------------------------------------ the library moved

/// Every song's kept path from folder `from` to folder `to` (liked songs,
/// history, the queue, playlists, saved videos).
pub fn relocate(from: &std::path::Path, to: &std::path::Path) {
    let raw = |p: &std::path::Path| format!("{}/", p.display());
    let esc = |p: &std::path::Path| serde_json::to_string(&raw(p)).unwrap_or_default().trim_matches('"').to_string();
    let (jf, jt, rf, rt) = (esc(from), esc(to), raw(from), raw(to));
    write(|c| {
        let tx = c.unchecked_transaction()?;
        for (table, column) in [("liked", "track"), ("plays", "track"), ("session", "queue"), ("playlist_songs", "track")] {
            tx.execute(&format!("UPDATE {table} SET {column} = replace({column}, ?1, ?2)"), params![jf, jt])?;
        }
        tx.execute("UPDATE videos SET file = replace(file, ?1, ?2) WHERE file != ''", params![rf, rt])?;
        tx.commit()
    });
}

// ------------------------------------------------------------------ plays and skips

/// How many times each song was played, and skipped (moved on from early).
pub fn counts() -> std::collections::HashMap<String, (usize, usize)> {
    let mut out: std::collections::HashMap<String, (usize, usize)> = std::collections::HashMap::new();
    with(|c| {
        let mut q = c.prepare("SELECT key, COUNT(*) FROM plays GROUP BY key")?;
        for r in q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?.flatten() {
            out.entry(r.0).or_default().0 = r.1 as usize;
        }
        let mut q = c.prepare("SELECT key, COUNT(*) FROM skips GROUP BY key")?;
        for r in q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?.flatten() {
            out.entry(r.0).or_default().1 = r.1 as usize;
        }
        Ok(())
    });
    out
}

pub fn add_skip(key: &str) {
    write(|c| c.execute("INSERT INTO skips (key, at) VALUES (?1, ?2)", params![key, now()]).map(|_| ()));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(id: &str) -> Track {
        Track { source: crate::track::Source::YouTube, id: id.into(), title: format!("Song {id}"), artist: "A & B".into(), album: String::new(), seconds: 200, art: String::new(), format: "Opus".into(), file: None, video: false, genres: None }
    }

    #[test]
    fn schema_and_round_trips() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(SCHEMA).unwrap();
        put_liked(&c, &[song("x"), song("y")]).unwrap();
        let mut q = c.prepare("SELECT track FROM liked ORDER BY place").unwrap();
        let back: Vec<Track> = q.query_map([], |r| r.get::<_, String>(0)).unwrap().filter_map(|r| r.ok().and_then(track)).collect();
        assert_eq!(back.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["x", "y"]);
        // a play recorded twice at the same moment is kept once
        for _ in 0..2 {
            c.execute("INSERT OR IGNORE INTO plays (key, played_at, title, artist, track) VALUES (?1, ?2, ?3, ?4, ?5)", params!["yt:x", 100, "t", "a", json(&song("x"))]).unwrap();
        }
        let n: i64 = c.query_row("SELECT COUNT(*) FROM plays", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
    }
}
