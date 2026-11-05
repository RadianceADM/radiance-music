//! Liking and saving, for both apps: liking puts the song first in Liked Songs and saves a copy
//! (lossless songs, a server's songs; YouTube songs only in builds with "save-youtube"); unliking
//! takes it out and moves its saved files to the Trash.

use std::path::Path;

use crate::library;
use crate::sources::Sources;
use crate::track::{Source, Track};

/// What a heart's click did.
#[derive(Clone, Debug)]
pub enum Like {
    /// Liked; `save`: a copy is to be saved.
    Liked { save: bool },
    /// Taken out of Liked (`gone`, with its file if it had one, for the Trash).
    Unliked { gone: Track },
}

/// The heart's click for `t`: kept (first, its file not known yet) or taken out; the list stored.
pub fn toggle(liked: &mut Vec<Track>, t: &Track) -> Like {
    let key = t.key();
    if let Some(i) = liked.iter().position(|x| x.key() == key) {
        let gone = liked.remove(i);
        library::store(liked);
        return Like::Unliked { gone };
    }
    let mut t = t.clone();
    t.file = None;
    let save = library::saves(&t);
    liked.insert(0, t);
    library::store(liked);
    Like::Liked { save }
}

/// Saves a liked song (worker thread): as `library::save`, the download and the lyrics through
/// `sources`. The saved file's path.
pub fn save(t: &Track, sources: &dyn Sources) -> Result<String, String> {
    library::save_with(t, &|url, to| sources.download(url, to), &|t| sources.lyrics(t))
}

/// The liked songs whose file isn't here (to be saved), and how many music videos are missing
/// (only in builds that save YouTube songs: a video, or known to have none, is in the database).
pub fn missing(liked: &[Track]) -> (Vec<Track>, usize) {
    let songs = liked.iter().filter(|t| library::saves(t) && t.file.as_ref().is_none_or(|f| !Path::new(f).exists())).cloned().collect();
    let videos = if cfg!(feature = "save-youtube") {
        let have = crate::db::songs_with_video();
        liked.iter().filter(|t| t.source == Source::YouTube && !t.video && !have.contains(&t.key())).count()
    } else {
        0
    };
    (songs, videos)
}
