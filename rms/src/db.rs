//! server.db (SQLite): people, their invites and devices, the songs (who
//! added what), and the split queue. The files themselves stay in library/.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS people (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  role TEXT NOT NULL,              -- owner | friend
  created_ms INTEGER NOT NULL,
  revoked_ms INTEGER
);
CREATE TABLE IF NOT EXISTS invites (
  code TEXT PRIMARY KEY,           -- one-time, as typed without its dashes
  person_id INTEGER NOT NULL REFERENCES people(id),
  created_ms INTEGER NOT NULL,
  used_ms INTEGER,
  device_id INTEGER,
  revoked_ms INTEGER
);
CREATE TABLE IF NOT EXISTS devices (
  id INTEGER PRIMARY KEY,
  person_id INTEGER NOT NULL REFERENCES people(id),
  name TEXT NOT NULL,
  token_sha256 TEXT NOT NULL UNIQUE, -- the token itself is never kept
  created_ms INTEGER NOT NULL,
  seen_ms INTEGER,
  revoked_ms INTEGER
);
CREATE TABLE IF NOT EXISTS songs (
  id INTEGER PRIMARY KEY,
  file TEXT NOT NULL UNIQUE,       -- its name in library/
  artist TEXT NOT NULL,
  title TEXT NOT NULL,
  added_by INTEGER REFERENCES people(id),
  added_ms INTEGER NOT NULL,
  duration_ms INTEGER,
  gone INTEGER NOT NULL DEFAULT 0  -- its file left the folder (kept, in case it comes back)
);
CREATE TABLE IF NOT EXISTS jobs (
  id INTEGER PRIMARY KEY,
  song_id INTEGER NOT NULL REFERENCES songs(id),
  asked_by INTEGER REFERENCES people(id),
  state TEXT NOT NULL,             -- waiting | splitting | timing | done | failed | removed
  progress REAL NOT NULL DEFAULT 0,
  note TEXT,
  created_ms INTEGER NOT NULL,
  started_ms INTEGER,
  finished_ms INTEGER
);
CREATE INDEX IF NOT EXISTS jobs_state ON jobs(state);
";

pub fn open(path: &Path) -> Result<Connection, String> {
    let conn = Connection::open(path).map_err(|e| format!("can't open {}: {e}", path.display()))?;
    conn.pragma_update(None, "journal_mode", "WAL").map_err(|e| e.to_string())?;
    conn.pragma_update(None, "foreign_keys", "ON").map_err(|e| e.to_string())?;
    conn.busy_timeout(std::time::Duration::from_secs(5)).map_err(|e| e.to_string())?;
    conn.execute_batch(SCHEMA).map_err(|e| format!("can't set up the database: {e}"))?;
    Ok(conn)
}

/// The owner (id, name), once the setup page has been filled in.
pub fn owner(conn: &Connection) -> Option<(i64, String)> {
    conn.query_row("SELECT id, name FROM people WHERE role = 'owner' ORDER BY id LIMIT 1", [], |r| Ok((r.get(0)?, r.get(1)?))).optional().ok().flatten()
}

/// A person's name, by id (None: no such person).
pub fn person_name(conn: &Connection, id: i64) -> Option<String> {
    conn.query_row("SELECT name FROM people WHERE id = ?1", params![id], |r| r.get(0)).optional().ok().flatten()
}

/// A song as the database keeps it.
#[derive(Clone, Debug)]
pub struct Song {
    pub id: i64,
    pub file: String,
    pub artist: String,
    pub title: String,
    pub added_by: Option<(i64, String)>,
    pub added_ms: i64,
    pub duration_ms: Option<i64>,
}

const SONG_COLUMNS: &str = "s.id, s.file, s.artist, s.title, s.added_by, p.name, s.added_ms, s.duration_ms";

fn song_row(r: &rusqlite::Row) -> rusqlite::Result<Song> {
    let by: Option<i64> = r.get(4)?;
    let name: Option<String> = r.get(5)?;
    Ok(Song { id: r.get(0)?, file: r.get(1)?, artist: r.get(2)?, title: r.get(3)?, added_by: by.zip(name), added_ms: r.get(6)?, duration_ms: r.get(7)? })
}

pub fn songs(conn: &Connection) -> rusqlite::Result<Vec<Song>> {
    let mut st = conn.prepare(&format!("SELECT {SONG_COLUMNS} FROM songs s LEFT JOIN people p ON p.id = s.added_by WHERE s.gone = 0"))?;
    let rows = st.query_map([], song_row)?.collect();
    rows
}

pub fn song(conn: &Connection, id: i64) -> rusqlite::Result<Option<Song>> {
    conn.query_row(&format!("SELECT {SONG_COLUMNS} FROM songs s LEFT JOIN people p ON p.id = s.added_by WHERE s.id = ?1 AND s.gone = 0"), params![id], song_row).optional()
}

pub fn song_by_file(conn: &Connection, file: &str) -> rusqlite::Result<Option<Song>> {
    conn.query_row(&format!("SELECT {SONG_COLUMNS} FROM songs s LEFT JOIN people p ON p.id = s.added_by WHERE s.file = ?1 AND s.gone = 0"), params![file], song_row).optional()
}
