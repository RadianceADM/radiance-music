//! Liked songs: the list (kept in the app's database, see db.rs) and
//! the saved copies in ~/Music/Radiance Music, each with its lyrics (a
//! .lyrics.yaml Lyricsfile, or .lrc) and album art (.jpg) beside it.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use gio::prelude::*;

use crate::track::{file_name, Source, Track};
use crate::{lyrics, net, youtube};

static MUSIC_DIR: std::sync::RwLock<Option<PathBuf>> = std::sync::RwLock::new(None);

/// Where liked songs are kept: the folder picked in Storage, else ~/Music/Radiance Music.
pub fn music_dir() -> PathBuf {
    if let Some(d) = MUSIC_DIR.read().ok().and_then(|d| d.clone()) {
        return d;
    }
    glib::user_special_dir(glib::UserDirectory::Music)
        .unwrap_or_else(|| glib::home_dir().join("Music"))
        .join("Radiance Music")
}

pub fn set_music_dir(dir: Option<PathBuf>) {
    if let Ok(mut d) = MUSIC_DIR.write() {
        *d = dir;
    }
}

pub fn load() -> Vec<Track> {
    crate::db::liked()
}

pub fn store(liked: &[Track]) {
    crate::db::set_liked(liked);
}

/// Whether liking a song saves a copy: lossless songs and a server's songs
/// always; YouTube songs only in builds with the "save-youtube" feature (on by
/// default; a build without it only streams them).
pub fn saves(t: &Track) -> bool {
    t.source != Source::YouTube || cfg!(feature = "save-youtube")
}

/// Saves a liked song (worker thread): the audio as the source sends it (FLAC
/// stays FLAC), its lyrics and its album art. Returns the audio file's path.
/// A server's song comes with everything the server has for it (servers::save).
pub fn save(t: &Track) -> Result<String, String> {
    save_with(t, &download, &lyrics::find)
}

/// As [save], with the download and the lyrics lookup given (the engine's sources: tests give theirs).
pub fn save_with(t: &Track, download: &dyn Fn(&str, &Path) -> Result<(), String>, lyrics_of: &dyn Fn(&Track) -> lyrics::Lyrics) -> Result<String, String> {
    let dir = music_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    if t.source == Source::Server {
        return crate::servers::save(t, &dir);
    }
    let base = file_name(&if t.artist.is_empty() { t.title.clone() } else { format!("{} - {}", t.artist, t.title) });
    // rule 1 (owner, 2026-11-09): a joined server's copy first: its song, parts, lyrics and cover
    // (even in a build that only streams YouTube songs: it's the server's copy that's saved)
    let mut path = None;
    if let Some((server, song)) = crate::servers::copy_of(t) {
        match crate::servers::save_copy(&server, &song, &dir, &base) {
            Ok(p) => path = Some(p),
            Err(e) => crate::log::note(&format!("saving “{}” from {}: {e}; from YouTube instead", t.title, server.name)),
        }
    }
    let path = match path {
        Some(p) => p,
        None if !saves(t) => return Err(STREAMS_ONLY.into()),
        None => match t.source {
            Source::YouTube => youtube::save(&t.id, &dir, &base)?,
            Source::Server => unreachable!("a server's song is saved above"),
            Source::Archive => {
                let path = dir.join(format!("{base}.flac"));
                download(&t.id, &path)?;
                path.display().to_string()
            }
        },
    };
    // its lyrics and cover, unless they came with it
    if !beside(&path, "lyrics.yaml").exists() && !beside(&path, "lrc").exists() {
        if let Some((ext, text)) = lyrics::to_file(&lyrics_of(t), Some(t), &[]) {
            let _ = fs::write(beside(&path, ext), text);
        }
    }
    if !beside(&path, "jpg").exists() {
        if let Some(bytes) = net::get_bytes(&t.art) {
            let _ = fs::write(beside(&path, "jpg"), bytes);
        }
    }
    Ok(path)
}

/// What saving a YouTube song says in a build that only streams them (and no joined server has it):
/// liked, not saved, and nothing went wrong.
pub const STREAMS_ONLY: &str = "YouTube songs stream, they aren't saved";

/// A file kept beside a saved song: "<the song's name>.<ext>". Older builds cut
/// names that have a dot in them ("Mood (feat. Muric)" became "Mood (feat.lrc");
/// such a file gets its full name the first time it's looked for.
pub fn beside(song: &str, ext: &str) -> PathBuf {
    let stem = Path::new(song).with_extension("");
    let right = PathBuf::from(format!("{}.{ext}", stem.display()));
    let cut = stem.with_extension(ext);
    if cut != right && !right.exists() && cut.exists() {
        let _ = fs::rename(&cut, &right);
    }
    right
}

pub fn download(url: &str, to: &Path) -> Result<(), String> {
    let part = to.with_extension("part");
    let mut resp = net::agent().get(url).call().map_err(|e| e.to_string())?;
    let mut body = resp.body_mut().as_reader();
    let mut file = fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = body.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
    }
    drop(file);
    fs::rename(&part, to).map_err(|e| e.to_string())
}

/// Unliked: its saved files go to the Trash (so they can still be brought back).
pub fn unsave(t: &Track) {
    let Some(path) = &t.file else { return };
    let stem = Path::new(path).with_extension("");
    let halves = ["vocals", "music", "drums", "bass", "other", "guitar", "piano"]
        .iter()
        .flat_map(|p| ["flac", "ogg", "m4a"].map(|e| PathBuf::from(format!("{}.{p}.{e}", stem.display()))))
        .collect::<Vec<_>>();
    let kept = ["lyrics.yaml", "lrc", "jpg", "video.mkv", "video.webm", "video.mp4"].map(|ext| beside(path, ext));
    for p in [PathBuf::from(path)].into_iter().chain(kept).chain(halves) {
        if p.exists() {
            let _ = gio::File::for_path(&p).trash(gio::Cancellable::NONE);
        }
    }
}
