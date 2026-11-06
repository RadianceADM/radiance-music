//! Word timing (MMS), Find Lyrics Again and the lyrics editor's save, moved out of the GTK app's ui.rs
//! so the Windows beta does the same (issue #45 stage 3). Worker-thread work: none of it touches a window.

use std::fs;

use crate::library;
use crate::lyrics::{self, lyrics_and_source, lyrics_file, lyrics_path, Lyrics};
use crate::settings;
use crate::split;
use crate::track::Track;
use gio::prelude::*;

/// Where splitting and word timing happen: the split server (yours, if you set one up), else this computer.
#[derive(Clone)]
pub enum Aligner {
    Server(settings::SplitServer),
    Here(settings::LocalSplitter),
}

impl Aligner {
    pub fn of(s: &settings::Settings) -> Option<Aligner> {
        match (&s.split, &s.local_split) {
            (Some(server), _) => Some(Aligner::Server(server.clone())),
            (None, Some(local)) => Some(Aligner::Here(local.clone())),
            _ => None,
        }
    }
}

/// A song's lyrics looked up again, as asked (worker thread; minutes when it's split): the kept ones go to
/// the Trash (they can be brought back), yours too; the ones found take their place; a split song gets every
/// word timed to its Vocals part. Ok(true): timed. Err: none found (the kept ones stay).
pub fn relook_lyrics(t: &Track, aligner: Option<&Aligner>) -> Result<bool, String> {
    let fresh = lyrics::find(t);
    if matches!(fresh, Lyrics::None) {
        return Err("No other lyrics found for this song: yours stay. Edit Lyrics… to type them in.".into());
    }
    let lrc = t.file.as_deref().map(|f| library::beside(f, "lrc"));
    for p in [Some(lyrics_path(t)), lrc.clone()].into_iter().flatten().filter(|p| p.exists()) {
        gio::File::for_path(&p).trash(gio::Cancellable::NONE).map_err(|e| format!("Couldn't move the old lyrics to the Trash: {e}"))?;
    }
    let written = match (&fresh, &lrc) {
        (Lyrics::Plain(text), Some(lrc)) => fs::write(lrc, text),
        (Lyrics::Plain(_), None) => Ok(()), // a song not saved: looked up each time
        _ => match lyrics::to_file(&fresh, Some(t), &[]) {
            Some((_, text)) => fs::write(lyrics_file(t), text),
            None => Ok(()),
        },
    };
    written.map_err(|e| format!("Couldn't keep the new lyrics: {e}"))?;
    crate::db::set_lyrics_checked(&t.key());
    crate::log::note(&format!("lyrics of “{}” looked up again: {}", t.title, match &fresh { Lyrics::Timed { lines, .. } => format!("{} timed lines", lines.len()), Lyrics::Plain(p) => format!("{} lines of text", p.lines().count()), Lyrics::None => "none".into() }));
    match (aligner, split::has_parts(t)) {
        (Some(a), true) => time_words(t, a).map(|_| true),
        _ => Ok(false),
    }
}

/// Keeps a split song in the server's library, its words timed first (worker thread).
pub fn keep_on_server(t: &Track, server: &settings::SplitServer, timing: &dyn Fn()) {
    if let Err(e) = split::host_copy(t, server, hosted_lyrics(t)) {
        crate::log::note(&format!("library copy of {}: {e}", t.title));
        return;
    }
    split::mark_hosted(&t.key());
    // the words' real times, from MMS on the server (the quick ones stand in till then)
    if !aligned_by_mms(t) {
        timing();
        if let Err(e) = time_words(t, &Aligner::Server(server.clone())) {
            crate::log::note(&format!("word times for {}: {e}", t.title));
        }
    }
}

/// The lyrics were edited by hand (in the lyrics editor): never replaced by a lookup.
pub fn hand_edited(t: &Track) -> bool {
    fs::read_to_string(lyrics_path(t)).is_ok_and(|y| y.contains(lyrics::HAND_NOTE))
}

/// Every word's time from MMS, on the split server or this computer (worker thread; minutes): for lyrics
/// edited by hand, their lines (and singers, kept); else LRCLIB's or YouTube Music's. The Lyricsfile is kept.
pub fn time_words(t: &Track, aligner: &Aligner) -> Result<(), String> {
    let mine = if hand_edited(t) {
        fs::read_to_string(lyrics_path(t)).ok().and_then(|y| lyrics::parse_lyricsfile(&y))
    } else {
        None
    };
    let source = mine.clone().unwrap_or_else(|| lyrics::find(t));
    let given = match &source {
        Lyrics::Timed { lines, exact } if mine.is_some() || !exact => serde_json::json!({
            "lines": lines.iter().filter(|l| !l.text.trim().is_empty()).map(|l| serde_json::json!({ "start": l.start, "end": l.end, "text": l.text })).collect::<Vec<_>>()
        }),
        Lyrics::Plain(text) => serde_json::json!({ "plain": text }),
        _ => return Ok(()), // none, or the song's own word times already
    };
    let yaml = match aligner {
        Aligner::Server(server) => split::align_on_server(t, server, &given)?,
        Aligner::Here(local) => split::align_here(t, local, &given)?,
    };
    let Some(Lyrics::Timed { mut lines, exact }) = lyrics::parse_lyricsfile(&yaml) else { return Err("nothing came back".into()) };
    let Some(Lyrics::Timed { lines: hand, .. }) = mine else {
        let _ = fs::write(lyrics_file(t), yaml);
        return Ok(());
    };
    // yours: the same lines in the same order, each with its singer again
    for (line, was) in lines.iter_mut().zip(hand.iter().filter(|l| !l.text.trim().is_empty())) {
        line.singer = was.singer.clone();
    }
    let mms = "word times: MMS forced alignment (Meta, torchaudio MMS_FA) on the song's Vocals part";
    if let Some((_, text)) = lyrics::to_file(&Lyrics::Timed { lines, exact }, Some(t), &[lyrics::HAND_NOTE, mms]) {
        let _ = fs::write(lyrics_file(t), text);
    }
    Ok(())
}

/// Its words were timed by MMS already.
pub fn aligned_by_mms(t: &Track) -> bool {
    fs::read_to_string(lyrics_file(t)).is_ok_and(|y| y.contains("MMS forced alignment"))
}

/// A split song's lyrics for the server's library (worker thread): its Lyricsfile (words timed from its
/// vocals when they can be), else plain text.
pub fn hosted_lyrics(t: &Track) -> Option<(&'static str, String)> {
    let (l, from_vocals) = lyrics_and_source(t);
    if let Some(text) = t.file.as_deref().and_then(|f| fs::read_to_string(library::beside(f, "lyrics.yaml")).ok()) {
        return Some(("lyrics.lyricsfile.yaml", text));
    }
    match &l {
        Lyrics::Timed { .. } => lyrics::to_file(&l, Some(t), if from_vocals { &[lyrics::VOCALS_NOTE] } else { &[] }).map(|(_, y)| ("lyrics.lyricsfile.yaml", y)),
        Lyrics::Plain(text) => Some(("lyrics.txt", text.clone())),
        Lyrics::None => None,
    }
}

/// The lyrics editor's lyrics written for the song (its Lyricsfile, marked as edited by hand so a lookup never
/// replaces them): the file it wrote.
pub fn write_edited(t: &Track, l: &Lyrics) -> Result<std::path::PathBuf, String> {
    let Some((_, text)) = lyrics::to_file(l, Some(t), &[lyrics::HAND_NOTE]) else { return Err("nothing to keep".into()) };
    let path = lyrics_file(t);
    fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(path)
}

/// The lyrics editor's Save (as the GTK app's): written for the song, its old .lrc (if any) to the Trash (the
/// Lyricsfile is the one now), and marked as looked up.
pub fn save_edited(t: &Track, l: &Lyrics) -> Result<(), String> {
    write_edited(t, l)?;
    if let Some(f) = t.file.as_deref() {
        let lrc = library::beside(f, "lrc");
        if lrc.exists() {
            let _ = gio::File::for_path(&lrc).trash(gio::Cancellable::NONE);
        }
    }
    crate::db::set_lyrics_checked(&t.key());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::Source;

    #[test]
    fn saving_edited_lyrics_marks_them_hand_made() {
        let dir = std::env::temp_dir().join(format!("radiance-word-times-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let song = dir.join("Someone - Song.opus");
        fs::write(&song, b"").unwrap();
        let t = Track { source: Source::YouTube, id: "edited".into(), title: "Song".into(), artist: "Someone".into(), album: String::new(), seconds: 200, art: String::new(), format: String::new(), file: Some(song.display().to_string()), video: false, genres: None };
        assert!(!hand_edited(&t));
        let l = lyrics::from_edited(&[(Some(1.0), "one".into(), None), (Some(4.0), "two".into(), Some("Nairud".into()))]);
        let path = write_edited(&t, &l).unwrap();
        assert_eq!(path, library::beside(t.file.as_deref().unwrap(), "lyrics.yaml"));
        assert!(hand_edited(&t), "a lookup must never replace them");
        let Some(Lyrics::Timed { lines, .. }) = fs::read_to_string(&path).ok().and_then(|y| lyrics::parse_lyricsfile(&y)) else { panic!("timed") };
        assert_eq!(lines[1].singer.as_deref(), Some("Nairud"));
        let _ = fs::remove_dir_all(&dir);
    }
}
