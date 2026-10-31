//! A song, wherever it comes from.

use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// Internet Archive: lossless FLAC, free to download.
    Archive,
    /// YouTube Music: compressed (Opus), the fallback.
    YouTube,
    /// A Radiance Music Server you joined (servers.rs): its library's song.
    Server,
}

#[derive(Clone, Debug)]
pub struct Track {
    pub source: Source,
    /// YouTube: the video id. Archive: the file's download URL. Server: the
    /// song's file there, https://HOST/files/NAME (no key in it; the server's
    /// origin as servers::origin writes it, as the phone app does).
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub seconds: u32,
    pub art: String,
    /// What the source says about its quality, e.g. "FLAC" or "24-bit FLAC".
    pub format: String,
    /// The saved copy in ~/Music, once liked and saved.
    pub file: Option<String>,
    /// A YouTube video (shown in Now playing), not just a song.
    pub video: bool,
    /// Its genres (looked up for liked songs); None until looked up.
    pub genres: Option<Vec<String>>,
}

impl Track {
    /// Unique across sources.
    pub fn key(&self) -> String {
        match self.source {
            Source::Archive => format!("ia:{}", self.id),
            Source::YouTube => format!("yt:{}", self.id),
            // a server's file address can be long (its name, escaped): its key is short, and the
            // same as the phone app's (model/Track.kt): the first 16 hex digits of the address's SHA-256
            Source::Server => format!("rms:{}", digest(&self.id)),
        }
    }

    pub fn lossless(&self) -> bool {
        match self.source {
            Source::Archive => true,
            Source::YouTube => false,
            Source::Server => self.format == "FLAC",
        }
    }

    pub fn length(&self) -> String {
        clock(self.seconds as f64)
    }

    /// Everyone on the song: the credited artists, then anyone featured in the
    /// title ("Tamaraw (feat. KKRAM)"), each once.
    pub fn artists(&self) -> Vec<String> {
        let mut all = split_credit(&self.artist);
        let low = self.title.to_lowercase();
        for tag in ["feat.", "ft.", "featuring "] {
            let Some(at) = low.find(tag) else { continue };
            let rest = &self.title[at + tag.len()..];
            let end = rest.find([')', ']']).unwrap_or(rest.len());
            all.extend(split_credit(&rest[..end]));
            break;
        }
        let mut seen = std::collections::HashSet::new();
        all.retain(|a| seen.insert(plain(a)));
        all
    }

    /// Everyone on the song in one line: "A", "A & B", "A, B & C".
    pub fn credit(&self) -> String {
        let all = self.artists();
        match all.len() {
            0 => String::new(),
            1 => all[0].clone(),
            n => format!("{} & {}", all[..n - 1].join(", "), all[n - 1]),
        }
    }

    /// "Artists · Album" (whichever are known).
    pub fn byline(&self) -> String {
        let credit = self.credit();
        [credit.as_str(), self.album.as_str()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" · ")
    }

    /// Same song, ignoring case, punctuation and "(Remastered)"-style extras.
    pub fn same_song(&self, other: &Track) -> bool {
        plain(&self.title) == plain(&other.title) && plain(&self.artist) == plain(&other.artist)
    }

    pub fn to_json(&self) -> Value {
        json!({
            "source": match self.source { Source::Archive => "archive", Source::YouTube => "youtube", Source::Server => "server" },
            "id": self.id, "title": self.title, "artist": self.artist, "album": self.album,
            "seconds": self.seconds, "art": self.art, "format": self.format, "file": self.file,
            "video": self.video, "genres": self.genres,
        })
    }

    pub fn from_json(v: &Value) -> Option<Track> {
        let s = |k: &str| v[k].as_str().unwrap_or_default().to_string();
        Some(Track {
            source: match v["source"].as_str()? {
                "archive" => Source::Archive,
                "youtube" => Source::YouTube,
                "server" => Source::Server,
                _ => return None,
            },
            id: v["id"].as_str()?.to_string(),
            title: s("title"),
            artist: s("artist"),
            album: s("album"),
            seconds: v["seconds"].as_u64().unwrap_or(0) as u32,
            art: s("art"),
            format: s("format"),
            file: v["file"].as_str().map(String::from),
            video: v["video"].as_bool().unwrap_or(false),
            genres: v["genres"].as_array().map(|g| g.iter().filter_map(|x| x.as_str().map(String::from)).collect()),
        })
    }
}

/// The first 16 hex digits of `text`'s SHA-256 (a short name that stays the same).
pub fn digest(text: &str) -> String {
    gtk::glib::compute_checksum_for_data(gtk::glib::ChecksumType::Sha256, text.as_bytes()).map(|h| h.chars().take(16).collect()).unwrap_or_default()
}

/// 245.3 -> "4:05".
pub fn clock(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// "4:05" or "1:02:03" or "245.3" -> seconds.
pub fn parse_clock(text: &str) -> Option<u32> {
    let text = text.trim();
    if text.contains(':') {
        text.split(':').try_fold(0u32, |acc, part| part.trim().parse::<f64>().ok().map(|n| acc * 60 + n as u32))
    } else {
        text.parse::<f64>().ok().map(|n| n as u32)
    }
}

/// Lowercase letters and digits only, without bracketed extras.
pub fn plain(text: &str) -> String {
    let mut depth = 0;
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            _ if depth == 0 && c.is_alphanumeric() => out.extend(c.to_lowercase()),
            _ => {}
        }
    }
    out
}

/// A name that's safe as a file name.
pub fn file_name(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() { '_' } else { c })
        .collect();
    cleaned.trim().trim_matches('.').chars().take(150).collect()
}

/// A title as filed A to Z: accents, case and leading quotes or symbols
/// left out ("'Sapababy" under S, "Ñ" under N); letters first, then "#".
pub fn file_key(title: &str) -> (u8, String) {
    let folded: String = title.chars().map(fold).collect::<String>().to_lowercase();
    let key = folded.trim_start_matches(|c: char| !c.is_alphanumeric()).to_string();
    let rank = if key.starts_with(|c: char| c.is_ascii_lowercase()) { 0 } else { 1 };
    (rank, key)
}

/// The letter a title files under: A to Z, or '#'.
pub fn letter_of(title: &str) -> char {
    match file_key(title) {
        (0, key) => key.chars().next().map(|c| c.to_ascii_uppercase()).unwrap_or('#'),
        _ => '#',
    }
}

/// Whether a song answers a search: every word typed is in its title, its
/// artists or its album (accents, case and spaces aside: "loki" finds "Lo ki").
pub fn matches(t: &Track, q: &str) -> bool {
    let plain = |s: &str| s.chars().map(fold).collect::<String>().to_lowercase();
    let hay = plain(&format!("{} {} {}", t.title, t.credit(), t.album));
    let joined: String = hay.chars().filter(|c| c.is_alphanumeric()).collect();
    plain(q).split_whitespace().all(|w| {
        let w: String = w.chars().filter(|c| c.is_alphanumeric()).collect();
        w.is_empty() || hay.contains(&w) || joined.contains(&w)
    })
}

fn fold(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'Á' | 'À' | 'Â' | 'Ä' | 'Ã' | 'Å' => 'a',
        'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => 'i',
        'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'Ó' | 'Ò' | 'Ô' | 'Ö' | 'Õ' => 'o',
        'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => 'u',
        'ñ' | 'Ñ' => 'n',
        'ç' | 'Ç' => 'c',
        'ý' | 'ÿ' | 'ȳ' | 'Ý' | 'Ȳ' => 'y',
        c => c,
    }
}

/// "A feat. B & C, D" -> [A, B, C, D].
pub fn split_credit(credit: &str) -> Vec<String> {
    let mut parts = vec![credit.to_string()];
    for sep in [" feat. ", " ft. ", " featuring ", " with ", " x ", " X ", " & ", ", ", " and "] {
        parts = parts.iter().flat_map(|p| p.split(sep).map(str::to_string).collect::<Vec<_>>()).collect();
    }
    parts.into_iter().map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(artist: &str, title: &str) -> Track {
        Track { source: Source::YouTube, id: "x".into(), title: title.into(), artist: artist.into(), album: String::new(), seconds: 0, art: String::new(), format: String::new(), file: None, video: false, genres: None }
    }

    #[test]
    fn featured_artists_count() {
        assert_eq!(song("Uncle Dags", "Tamaraw (feat. KKRAM)").artists(), ["Uncle Dags", "KKRAM"]);
        assert_eq!(song("Gat Putch, Icetizy & Uncle Dags", "MATIKAS (feat. Smugglaz)").credit(), "Gat Putch, Icetizy, Uncle Dags & Smugglaz");
        assert_eq!(song("SUPAFLY", "PUFF ME UP (feat. Gat Putch, Gaspari, OG CRY & Ghoti Scale)").artists().len(), 5);
        assert_eq!(song("Hev Abi", "Para Sa Streets").credit(), "Hev Abi");
        // named twice: once
        assert_eq!(song("Al James & Muric", "Mood (feat. Muric)").artists(), ["Al James", "Muric"]);
    }

    #[test]
    fn titles_file_a_to_z() {
        assert_eq!(letter_of("'Sapababy"), 'S');
        assert_eq!(letter_of("Ñino"), 'N');
        assert_eq!(letter_of("213 (feat. SUPAFLY)"), '#');
        assert_eq!(letter_of("$hottas"), 'H');
        let mut t = vec!["luhod", "Alam Mo Ba Girl", "213", "'Sapababy", "BMW"];
        t.sort_by_cached_key(|x| file_key(x));
        assert_eq!(t, ["Alam Mo Ba Girl", "BMW", "luhod", "'Sapababy", "213"]);
    }

    #[test]
    fn search_finds_liked_songs() {
        let mut t = song("Lo ki", "Oh Yeah");
        t.album = "Oh Yeah".into();
        assert!(matches(&t, "lo ki yeah"));
        assert!(matches(&t, "loki")); // spaces aside
        assert!(matches(&t, "  OH  "));
        assert!(!matches(&t, "owshi"));
        assert!(matches(&song("Shanti Dope", "Ñino (feat. Lo ki)"), "nino loki"));
        assert!(matches(&song("Hev Abi", "'Sapababy"), "sapababy"));
    }
}
