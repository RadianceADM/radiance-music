//! The library: one folder of files named exactly as the apps save them, so a
//! copy of your Music folder works as it is, and the apps' "skip what you
//! have" plan (Share Songs) fits:
//!
//!   Artist - Title.opus | .flac | .m4a      the song
//!   Artist - Title.drums.ogg (.flac)         its parts, from the splitter:
//!     .bass .other .vocals .music            (FLAC when the song was lossless)
//!   Artist - Title.lyrics.yaml (or .lrc)     its lyrics (a Lyricsfile)
//!   Artist - Title.jpg                       its cover
//!   Artist - Title.video.mkv (.webm, .mp4)   its music video
//!
//! The database only adds who added each song (and its length).

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::extract::{Multipart, Path as UrlPath, Request, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use rusqlite::params;
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::auth::{Listener, Who};
use crate::db::{self, Song};
use crate::{bad, broken, not_found, now_ms, ApiError, ApiResult, Shared};

/// A song's own file.
pub const SONG_EXTS: [&str; 3] = ["opus", "flac", "m4a"];
/// Its parts (Demucs' four, the voice's other half, and the six-part model's two).
pub const PARTS: [&str; 7] = ["drums", "bass", "other", "vocals", "music", "guitar", "piano"];
/// The four the apps' mixer plays: with all of them, a song is split.
pub const FOUR: [&str; 4] = ["drums", "bass", "other", "vocals"];
const PART_EXTS: [&str; 3] = ["flac", "ogg", "m4a"];
const VIDEO_EXTS: [&str; 3] = ["mkv", "webm", "mp4"];

/// Uploads: a song with its video can be big; nothing bigger than this.
pub const MOST_UPLOAD: u64 = 6 << 30;
const MOST_SONG: u64 = 1 << 30;
const MOST_VIDEO: u64 = 4 << 30;
const MOST_COVER: u64 = 20 << 20;
const MOST_LYRICS: u64 = 2 << 20;

/// A name as the apps file it (track.rs file_name): no characters a file
/// system refuses, at most 150 characters.
pub fn file_name(text: &str) -> String {
    let cleaned: String = text.chars().map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() { '_' } else { c }).collect();
    cleaned.trim().trim_matches('.').chars().take(150).collect()
}

/// "Artist - Title" (a file's name without its extension) as its two halves.
pub fn artist_title(stem: &str) -> (String, String) {
    match stem.split_once(" - ") {
        Some((a, t)) => (a.trim().to_string(), t.trim().to_string()),
        None => (String::new(), stem.trim().to_string()),
    }
}

fn split_ext(name: &str) -> Option<(&str, String)> {
    let (stem, ext) = name.rsplit_once('.')?;
    Some((stem, ext.to_ascii_lowercase()))
}

/// A song's own file (not one of its parts or extras).
pub fn is_song(name: &str) -> bool {
    if name.starts_with('.') {
        return false;
    }
    let Some((stem, ext)) = split_ext(name) else { return false };
    SONG_EXTS.contains(&ext.as_str()) && !PARTS.iter().any(|p| stem.ends_with(&format!(".{p}")))
}

/// A song found in the folder, with everything kept beside it.
#[derive(Clone, Debug)]
pub struct Found {
    pub file: String,
    /// kind (song, drums, bass, other, vocals, music, lyrics, cover, video) → (file name, size)
    pub files: BTreeMap<String, (String, u64)>,
}

impl Found {
    pub fn stem(&self) -> &str {
        self.file.rsplit_once('.').map(|(s, _)| s).unwrap_or(&self.file)
    }

    pub fn split(&self) -> bool {
        FOUR.iter().all(|p| self.files.contains_key(*p))
    }
}

/// What's beside the song `file` among `names` (every file in the folder, with its size).
fn beside(file: &str, names: &HashMap<String, u64>) -> Found {
    let stem = file.rsplit_once('.').map(|(s, _)| s).unwrap_or(file);
    let mut files = BTreeMap::new();
    let mut take = |kind: &str, candidates: Vec<String>| {
        if let Some((name, size)) = candidates.into_iter().find_map(|n| names.get(&n).map(|s| (n, *s))) {
            files.insert(kind.to_string(), (name, size));
        }
    };
    take("song", vec![file.to_string()]);
    for p in PARTS {
        take(p, PART_EXTS.iter().map(|e| format!("{stem}.{p}.{e}")).collect());
    }
    take("lyrics", vec![format!("{stem}.lyrics.yaml"), format!("{stem}.lrc")]);
    take("cover", vec![format!("{stem}.jpg")]);
    take("video", VIDEO_EXTS.iter().map(|e| format!("{stem}.video.{e}")).collect());
    Found { file: file.to_string(), files }
}

/// Every song in the folder (blocking: reads the folder).
pub fn scan(dir: &Path) -> Vec<Found> {
    let mut names = HashMap::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for e in entries.flatten() {
            let Ok(meta) = e.metadata() else { continue };
            if meta.is_file() {
                names.insert(e.file_name().to_string_lossy().into_owned(), meta.len());
            }
        }
    }
    let mut songs: Vec<Found> = names.keys().filter(|n| is_song(n)).map(|n| beside(n, &names)).collect();
    songs.sort_by(|a, b| a.file.to_lowercase().cmp(&b.file.to_lowercase()));
    songs
}

/// One song's files, now (blocking).
pub fn found(dir: &Path, file: &str) -> Option<Found> {
    let stem = file.rsplit_once('.').map(|(s, _)| s)?;
    let mut names = HashMap::new();
    for e in fs::read_dir(dir).ok()?.flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        if n.starts_with(stem) {
            if let Ok(m) = e.metadata() {
                names.insert(n, m.len());
            }
        }
    }
    names.contains_key(file).then(|| beside(file, &names))
}

/// The folder and the database agree (blocking): songs put in the folder by
/// hand join the list (added by nobody), songs taken out leave it.
pub fn sync(app: &Shared) -> Result<Vec<(Song, Found)>, ApiError> {
    let found = scan(&app.cfg.library());
    let known: HashMap<String, (i64, Option<i64>, bool)> = {
        let conn = app.db();
        let mut st = conn.prepare("SELECT file, id, duration_ms, gone FROM songs")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, (r.get(1)?, r.get(2)?, r.get(3)?))))?.collect::<Result<_, _>>()?;
        rows
    };
    // lengths are read outside the database's lock (a few bytes of each new file)
    let new: Vec<(String, Option<i64>)> = found.iter().filter(|f| !known.contains_key(&f.file)).map(|f| (f.file.clone(), crate::media::duration_ms(&app.cfg.library().join(&f.file)))).collect();
    let missing: Vec<(i64, Option<i64>)> = found
        .iter()
        .filter_map(|f| known.get(&f.file).filter(|(_, d, _)| d.is_none()).map(|(id, ..)| (*id, crate::media::duration_ms(&app.cfg.library().join(&f.file)))))
        .filter(|(_, d)| d.is_some())
        .collect();
    {
        let mut conn = app.db();
        let tx = conn.transaction()?;
        let now = now_ms();
        for (file, duration) in &new {
            let (artist, title) = artist_title(file.rsplit_once('.').map(|(s, _)| s).unwrap_or(file));
            tx.execute("INSERT OR IGNORE INTO songs (file, artist, title, added_ms, duration_ms) VALUES (?1, ?2, ?3, ?4, ?5)", params![file, artist, title, now, duration])?;
        }
        for (id, duration) in &missing {
            tx.execute("UPDATE songs SET duration_ms = ?1 WHERE id = ?2", params![duration, id])?;
        }
        let here: std::collections::HashSet<&str> = found.iter().map(|f| f.file.as_str()).collect();
        for (file, (id, _, gone)) in &known {
            // only the songs that came or went
            let now_gone = !here.contains(file.as_str());
            if now_gone != *gone {
                tx.execute("UPDATE songs SET gone = ?1 WHERE id = ?2", params![now_gone, id])?;
            }
        }
        tx.commit()?;
    }
    let songs = db::songs(&app.db())?;
    let mut by_file: HashMap<String, Found> = found.into_iter().map(|f| (f.file.clone(), f)).collect();
    let mut out: Vec<(Song, Found)> = songs.into_iter().filter_map(|s| by_file.remove(&s.file).map(|f| (s, f))).collect();
    out.sort_by(|a, b| (a.0.artist.to_lowercase(), a.0.title.to_lowercase()).cmp(&(b.0.artist.to_lowercase(), b.0.title.to_lowercase())));
    Ok(out)
}

/// What's escaped in a file's address: all but letters, digits and - . _ ~
const IN_PATH: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');

/// A file's address on the server (players add `?t=TOKEN`).
pub fn url(name: &str) -> String {
    format!("/files/{}", percent_encoding::utf8_percent_encode(name, IN_PATH))
}

/// Whether a song's lyrics have each word's time (MMS, or the Lyricsfile's own).
fn word_times(dir: &Path, f: &Found) -> bool {
    f.files.get("lyrics").filter(|(n, _)| n.ends_with(".lyrics.yaml")).and_then(|(n, _)| fs::read_to_string(dir.join(n)).ok()).is_some_and(|y| crate::lyrics::has_word_times(&y))
}

/// A song as the apps see it: what it is, who added it, and every file it has
/// (the same kinds as Share Songs: song, drums, bass, other, vocals, music,
/// lyrics, cover, video).
pub fn song_json(dir: &Path, s: &Song, f: &Found) -> Value {
    let files: serde_json::Map<String, Value> = f.files.iter().map(|(kind, (name, size))| (kind.clone(), json!({ "name": name, "size": size, "url": url(name) }))).collect();
    let parts: Vec<&str> = PARTS.iter().copied().filter(|p| f.files.contains_key(*p)).collect();
    json!({
        "id": s.id,
        "artist": s.artist,
        "title": s.title,
        "file": s.file,
        "url": url(&s.file),
        "duration_ms": s.duration_ms,
        "split": f.split(),
        "parts": parts,
        "lyrics": f.files.contains_key("lyrics"),
        "word_times": word_times(dir, f),
        "cover": f.files.get("cover").map(|(n, _)| url(n)),
        "video": f.files.get("video").map(|(n, _)| url(n)),
        "files": files,
        "added_by": s.added_by.as_ref().map(|(id, name)| json!({ "id": id, "name": name })),
        "added_ms": s.added_ms,
    })
}

/// GET /api/library: every song, with its parts, lyrics, cover and video, and who added it.
pub async fn list(State(app): State<Shared>, _who: Who) -> ApiResult<Json<Value>> {
    let a = app.clone();
    let songs = tokio::task::spawn_blocking(move || sync(&a)).await.map_err(|e| broken(e.to_string()))??;
    let dir = app.cfg.library();
    let list: Vec<Value> = songs.iter().map(|(s, f)| song_json(&dir, s, f)).collect();
    Ok(Json(json!({ "server": app.server_name(), "count": list.len(), "songs": list })))
}

fn mime_for(name: &str) -> mime::Mime {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    let kind = match ext.as_str() {
        // Ogg Opus and Ogg Vorbis: what browsers' <audio> know them as
        "opus" | "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "m4a" => "audio/mp4",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "mp4" => "video/mp4",
        "jpg" | "jpeg" => "image/jpeg",
        "yaml" => "text/yaml; charset=utf-8",
        "lrc" | "txt" => "text/plain; charset=utf-8",
        "json" => "application/json",
        _ => "application/octet-stream",
    };
    kind.parse().unwrap_or(mime::APPLICATION_OCTET_STREAM)
}

/// GET /files/NAME: any file in the library, with Range (seeking) and
/// If-Modified-Since. Only files right in the folder: no paths.
pub async fn file(State(app): State<Shared>, Listener(_who): Listener, UrlPath(name): UrlPath<String>, req: Request) -> Response {
    if name.is_empty() || name.contains(['/', '\\', '\0']) || name.starts_with('.') {
        return not_found("There's no such file in the library.").into_response();
    }
    let path: PathBuf = app.cfg.library().join(&name);
    if !path.is_file() {
        return not_found("There's no such file in the library.").into_response();
    }
    match tower_http::services::ServeFile::new_with_mime(&path, &mime_for(&name)).oneshot(req).await {
        Ok(mut res) => {
            res.headers_mut().insert(header::CACHE_CONTROL, header::HeaderValue::from_static("private, no-cache"));
            res.map(Body::new)
        }
        Err(e) => match e {},
    }
}

/// An upload's pieces wait here, then move into the library together (or not at all).
struct Upload(PathBuf);

impl Drop for Upload {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A file field to disk, a piece at a time (a video is never held in memory).
async fn save_field(field: &mut axum::extract::multipart::Field<'_>, to: &Path, most: u64, what: &str) -> Result<u64, ApiError> {
    use tokio::io::AsyncWriteExt;
    let mut out = tokio::fs::File::create(to).await.map_err(|e| broken(e.to_string()))?;
    let mut size = 0u64;
    while let Some(chunk) = field.chunk().await.map_err(|e| bad(format!("The upload stopped: {e}")))? {
        size += chunk.len() as u64;
        if size > most {
            return Err(ApiError(StatusCode::PAYLOAD_TOO_LARGE, format!("The {what} is too big (at most {} MB).", most >> 20)));
        }
        out.write_all(&chunk).await.map_err(|e| broken(format!("Couldn't save the upload: {e}")))?;
    }
    out.flush().await.map_err(|e| broken(e.to_string()))?;
    Ok(size)
}

/// Moves a finished file into the library (into place in one step: never half a file).
pub fn put(from: &Path, to: &Path) -> std::io::Result<()> {
    let part = to.with_file_name(format!(".{}.part", to.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
    if fs::rename(from, &part).is_err() {
        fs::copy(from, &part)?;
    }
    fs::rename(&part, to)
}

/// POST /api/songs (multipart): `song` (the file), and maybe `cover` (.jpg),
/// `lyrics` (.lyrics.yaml or .lrc), `video` (.mkv/.webm/.mp4), `artist` and
/// `title` (else from the file's name, "Artist - Title.opus"). A song the
/// library has already (same artist and title) is skipped; only the extras it
/// lacks are kept.
pub async fn upload(State(app): State<Shared>, who: Who, mut form: Multipart) -> ApiResult<(StatusCode, Json<Value>)> {
    // friends add songs unless the owner turned it off for them (owner, 2026-11-01)
    if !who.can_add {
        return Err(crate::auth::switched_off(&app, "adding songs"));
    }
    let tmp = Upload(app.cfg.work().join(format!("upload-{}", &crate::auth::new_token()[..16])));
    fs::create_dir_all(&tmp.0).map_err(|e| broken(e.to_string()))?;
    let mut texts: HashMap<String, String> = HashMap::new();
    // kind → (the name it came with, where it waits)
    let mut files: HashMap<String, (String, PathBuf)> = HashMap::new();
    while let Some(mut field) = form.next_field().await.map_err(|e| bad(format!("The upload didn't come through: {e}")))? {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "song" | "cover" | "lyrics" | "video" => {
                let original = field.file_name().unwrap_or("").to_string();
                let (most, what) = match name.as_str() {
                    "song" => (MOST_SONG, "song"),
                    "video" => (MOST_VIDEO, "video"),
                    "cover" => (MOST_COVER, "cover"),
                    _ => (MOST_LYRICS, "lyrics file"),
                };
                let to = tmp.0.join(&name);
                let size = save_field(&mut field, &to, most, what).await?;
                if size > 0 {
                    files.insert(name, (original, to));
                }
            }
            "artist" | "title" | "seconds" => {
                let text = field.text().await.map_err(|e| bad(e.to_string()))?;
                texts.insert(name, text.chars().take(300).collect());
            }
            _ => while field.chunk().await.ok().flatten().is_some() {},
        }
    }
    let Some((song_name, song_tmp)) = files.remove("song") else { return Err(bad("Choose the song's file (.opus, .flac or .m4a).")) };
    let ext = split_ext(&song_name).map(|(_, e)| e).unwrap_or_default();
    if !SONG_EXTS.contains(&ext.as_str()) {
        return Err(bad("Songs come as .opus, .flac or .m4a files (as the apps save them)."));
    }
    let from_name = artist_title(split_ext(&song_name).map(|(s, _)| s).unwrap_or(&song_name));
    let text = |k: &str| texts.get(k).map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    let artist = text("artist").unwrap_or(from_name.0);
    let title = text("title").unwrap_or(from_name.1);
    if title.is_empty() {
        return Err(bad("The song needs a title."));
    }
    let base = file_name(&if artist.is_empty() { title.clone() } else { format!("{artist} - {title}") });
    if base.is_empty() {
        return Err(bad("The song needs a title."));
    }
    let a = app.clone();
    let songs = tokio::task::spawn_blocking(move || sync(&a)).await.map_err(|e| broken(e.to_string()))??;
    let dir = app.cfg.library();
    let same = songs.iter().find(|(s, f)| f.stem().to_lowercase() == base.to_lowercase() || (s.artist.to_lowercase() == artist.to_lowercase() && s.title.to_lowercase() == title.to_lowercase()));
    // the song itself, unless it's here already
    let (song_file, skipped) = match same {
        Some((_, f)) => (f.file.clone(), true),
        None => {
            let file = format!("{base}.{ext}");
            put(&song_tmp, &dir.join(&file)).map_err(|e| broken(format!("Couldn't keep the song: {e}")))?;
            let seconds: Option<i64> = text("seconds").and_then(|s| s.parse::<f64>().ok()).filter(|s| *s > 0.0).map(|s| (s * 1000.0) as i64);
            let duration = seconds.or_else(|| crate::media::duration_ms(&dir.join(&file)));
            app.db().execute(
                "INSERT INTO songs (file, artist, title, added_by, added_ms, duration_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(file) DO UPDATE SET artist = ?2, title = ?3, added_by = ?4, added_ms = ?5, duration_ms = ?6, gone = 0",
                params![file, artist, title, who.person, now_ms(), duration],
            )?;
            (file, false)
        }
    };
    // the extras it doesn't have yet
    let stem = song_file.rsplit_once('.').map(|(s, _)| s.to_string()).unwrap_or_else(|| song_file.clone());
    let have = found(&dir, &song_file);
    let mut added = vec![];
    for (kind, (original, path)) in &files {
        if have.as_ref().is_some_and(|h| h.files.contains_key(kind)) {
            continue;
        }
        let lower = original.to_lowercase();
        let name = match kind.as_str() {
            "cover" => format!("{stem}.jpg"),
            "lyrics" if lower.ends_with(".lrc") || lower.ends_with(".txt") => format!("{stem}.lrc"),
            "lyrics" => format!("{stem}.lyrics.yaml"),
            "video" => {
                let e = split_ext(&lower).map(|(_, e)| e).filter(|e| VIDEO_EXTS.contains(&e.as_str())).unwrap_or_else(|| "mkv".into());
                format!("{stem}.video.{e}")
            }
            _ => continue,
        };
        put(path, &dir.join(&name)).map_err(|e| broken(format!("Couldn't keep the {kind}: {e}")))?;
        added.push(kind.clone());
    }
    added.sort();
    let a = app.clone();
    let songs = tokio::task::spawn_blocking(move || sync(&a)).await.map_err(|e| broken(e.to_string()))??;
    let song = songs.iter().find(|(_, f)| f.stem() == stem).map(|(s, f)| song_json(&dir, s, f));
    if !skipped {
        crate::log(&format!("{} added {stem}", who.name));
    }
    let status = if skipped { StatusCode::OK } else { StatusCode::CREATED };
    Ok((status, Json(json!({ "added": !skipped, "skipped": skipped, "extras": added, "song": song }))))
}

/// A song named in a request: its id (number or text), or its file's name.
/// Not known yet: the folder is read again first (a song copied in by hand).
pub fn resolve(app: &Shared, v: &Value) -> Result<Song, ApiError> {
    match find(app, v)? {
        Some(song) => Ok(song),
        None => {
            sync(app)?;
            find(app, v)?.ok_or_else(|| not_found("That song isn't in the library."))
        }
    }
}

fn find(app: &Shared, v: &Value) -> Result<Option<Song>, ApiError> {
    let conn = app.db();
    let found = match v {
        Value::Number(n) => n.as_i64().map(|id| db::song(&conn, id)).transpose()?.flatten(),
        Value::String(s) => match s.parse::<i64>() {
            Ok(id) => db::song(&conn, id)?,
            Err(_) => db::song_by_file(&conn, s)?,
        },
        Value::Object(o) => match o.get("id").and_then(|i| i.as_i64()) {
            Some(id) => db::song(&conn, id)?,
            None => None,
        },
        _ => None,
    };
    Ok(found)
}
