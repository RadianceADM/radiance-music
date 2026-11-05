//! Lyrics: their kinds, reading and writing them (Lyricsfile, LRC), and whether an LRCLIB
//! entry is the song's own. Finding them on the internet is the engine's (engine/src/lyrics.rs).
//!
//! Best first: its Lyricsfile (YAML; each line with its start and end, and often
//! each word's own timing), else the classic synced lyrics (a time per line;
//! the words are then spread over the line by their length), else plain text.
//! Lines may overlap (two voices at once): every line being sung is lit.

use serde_yaml::Value as Yaml;

use crate::track::Track;

#[derive(Clone, Debug)]
pub struct Word {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct Line {
    pub start: f64,
    pub end: f64,
    pub text: String,
    /// Each word with its time (exact from a word-synced Lyricsfile, else spread).
    pub words: Vec<Word>,
    /// Who sings it (set in the lyrics editor; a "singer" key in our Lyricsfiles,
    /// which the format doesn't define yet: other apps just skip it).
    pub singer: Option<String>,
}

/// Notes kept at the top of a Lyricsfile: where its times came from.
pub const VOCALS_NOTE: &str = "word times: from the song's own Vocals part (Radiance Music), not hand-made";
pub const HAND_NOTE: &str = "edited by hand in Radiance Music";

#[derive(Clone)]
pub enum Lyrics {
    /// Timed lines; `exact`: the words' timing is the song's own.
    Timed { lines: Vec<Line>, exact: bool },
    Plain(String),
    None,
}

/// An LRCLIB entry is the song's own: filed under its artist (a title alone,
/// "Moolah", matches others' songs), and its lyrics don't name another one in
/// their own "[ar: …]" tag (someone filed Young Greatness's "Moolah" under
/// SCOOP DOGG & daarth's).
pub fn by_them(t: &Track, v: &serde_json::Value) -> bool {
    let names: Vec<String> = t.artists().iter().map(|a| crate::track::plain(a)).filter(|a| a.chars().count() >= 2).collect();
    let ours = |name: &str| {
        let theirs = crate::track::plain(name);
        !theirs.is_empty() && names.iter().any(|n| theirs.contains(n.as_str()) || n.contains(theirs.as_str()))
    };
    ours(v["artistName"].as_str().unwrap_or_default()) && tagged_artist(v).is_none_or(|a| ours(&a))
}

/// The artist the lyrics' own text names ("[ar: Young Greatness]"), if any:
/// someone filed another song's lyrics under this one (LRCLIB is open to all).
fn tagged_artist(v: &serde_json::Value) -> Option<String> {
    ["syncedLyrics", "plainLyrics"].iter().filter_map(|k| v[*k].as_str()).find_map(|text| {
        let at = text.find("[ar:")?;
        let rest = &text[at + 4..];
        let name = rest[..rest.find(']')?].trim();
        (!name.is_empty()).then(|| name.to_string())
    })
}

/// The best lyrics in an LRCLIB entry: its Lyricsfile, else its synced lines, else its text.
pub fn from(v: &serde_json::Value) -> Option<Lyrics> {
    if let Some(l) = v["lyricsfile"].as_str().and_then(parse_lyricsfile) {
        return Some(l);
    }
    if let Some(lrc) = v["syncedLyrics"].as_str().filter(|s| !s.trim().is_empty()) {
        let lines = parse_lrc(lrc);
        if !lines.is_empty() {
            return Some(Lyrics::Timed { lines, exact: false });
        }
    }
    v["plainLyrics"].as_str().filter(|s| !s.trim().is_empty()).map(|s| Lyrics::Plain(s.to_string()))
}

/// A Lyricsfile (YAML): lines with start_ms / end_ms, maybe words with theirs.
pub fn parse_lyricsfile(text: &str) -> Option<Lyrics> {
    let doc: Yaml = serde_yaml::from_str(text).ok()?;
    if doc.get("lines").and_then(|l| l.as_sequence()).is_none_or(|l| l.is_empty()) {
        return doc.get("plain").and_then(|p| p.as_str()).filter(|p| !p.trim().is_empty()).map(|p| Lyrics::Plain(p.to_string()));
    }
    let ms = |v: &Yaml| v.as_f64().or_else(|| v.as_i64().map(|n| n as f64)).map(|n| n / 1000.0);
    let mut exact = true;
    let mut lines: Vec<Line> = doc
        .get("lines")?
        .as_sequence()?
        .iter()
        .filter_map(|l| {
            let text = l.get("text")?.as_str()?.to_string();
            let start = ms(l.get("start_ms")?)?;
            let end = l.get("end_ms").and_then(ms).unwrap_or(start + 4.0);
            let words: Vec<Word> = l
                .get("words")
                .and_then(|w| w.as_sequence())
                .map(|ws| {
                    ws.iter()
                        .filter_map(|w| Some(Word { start: ms(w.get("start_ms")?)?, end: ms(w.get("end_ms")?)?, text: w.get("text")?.as_str()?.to_string() }))
                        .collect()
                })
                .unwrap_or_default();
            let singer = l.get("singer").or_else(|| l.get("agent")).and_then(|s| s.as_str()).map(String::from).filter(|s| !s.is_empty());
            Some(Line { start, end, text, words, singer })
        })
        .collect();
    if lines.is_empty() {
        return None;
    }
    // times typed or tapped by hand are kept just as they are, hums too
    let by_hand = text.contains(HAND_NOTE);
    for line in &mut lines {
        if line.words.is_empty() {
            exact = false;
            line.words = spread(&line.text, line.start, line.end);
        } else if !by_hand {
            even_hums(line);
        }
    }
    lines.sort_by(|a, b| a.start.total_cmp(&b.start));
    Some(Lyrics::Timed { lines, exact })
}

/// A hum: "mm", "hmm", "mmmh", "mhm", "mm-hmm", or a drawn-out "ooh", "ahh",
/// "uhh". It has no sounds of speech for the word timer (MMS) to hold on to.
pub(crate) fn is_hum(word: &str) -> bool {
    let w: String = word.chars().filter(|c| c.is_alphabetic()).flat_map(char::to_lowercase).collect();
    let n = w.chars().count();
    let mh = n >= 2 && w.contains('m') && w.chars().all(|c| c == 'm' || c == 'h');
    // three letters or more: a short "oh", "ah", "uh" (and "aha", "huh") stay words
    let drawn = n >= 3 && w.starts_with(|c| "oau".contains(c)) && w.chars().all(|c| "oauh".contains(c)) && (w.ends_with('h') || w.chars().all(|c| c == 'o'));
    mh || drawn
}

/// Hums timed by the word timer come out bunched up or with holes (IF U,
/// 2026-11-02: four "mmmh" in 9.75 s, two at the start and a 7.5 s hole), so
/// each run of hums lights one by one at a steady pace instead: from the
/// first hum's start to the next real word, or the end of the line. (Owner's
/// choice until a hum detector learns where hums really are.)
pub(crate) fn even_hums(line: &mut Line) {
    let w = &mut line.words;
    let mut i = 0;
    while i < w.len() {
        if !is_hum(&w[i].text) {
            i += 1;
            continue;
        }
        let first = i;
        while i < w.len() && is_hum(&w[i].text) {
            i += 1;
        }
        let start = w[first].start;
        let end = if i < w.len() { w[i].start } else { line.end.max(w[i - 1].end) };
        let n = (i - first) as f64;
        let step = (end - start).max(0.0) / n;
        for (k, word) in w[first..i].iter_mut().enumerate() {
            word.start = start + step * k as f64;
            word.end = start + step * (k as f64 + 1.0);
        }
    }
}

/// A line's words over its time, each by its length (karaoke without word times).
pub fn spread(text: &str, start: f64, end: f64) -> Vec<Word> {
    let pieces: Vec<String> = text.split_inclusive(' ').map(String::from).collect();
    let total: usize = pieces.iter().map(|p| p.trim().chars().count().max(1)).sum();
    let span = (end - start).max(0.3);
    let mut t = start;
    pieces
        .into_iter()
        .map(|p| {
            let d = span * p.trim().chars().count().max(1) as f64 / total.max(1) as f64;
            let w = Word { start: t, end: t + d, text: p };
            t += d;
            w
        })
        .collect()
}

/// "[01:02.50]words" lines, in time order; each ends where the next starts.
pub fn parse_lrc(text: &str) -> Vec<Line> {
    let mut stamped = vec![];
    for line in text.lines() {
        let mut rest = line.trim();
        let mut times = vec![];
        while let Some(stripped) = rest.strip_prefix('[') {
            let Some(close) = stripped.find(']') else { break };
            let stamp = &stripped[..close];
            let Some((m, s)) = stamp.split_once(':') else { break };
            let (Ok(m), Ok(s)) = (m.parse::<f64>(), s.parse::<f64>()) else { break };
            times.push(m * 60.0 + s);
            rest = &stripped[close + 1..];
        }
        for t in times {
            stamped.push((t, rest.trim().to_string()));
        }
    }
    stamped.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out = vec![];
    for (i, (start, text)) in stamped.iter().enumerate() {
        let next = stamped.get(i + 1).map(|n| n.0).unwrap_or(start + 5.0);
        let end = next.min(start + 8.0).max(start + 0.3);
        out.push(Line { start: *start, end, words: spread(text, *start, end), text: text.clone(), singer: None });
    }
    out
}

/// The text kept next to a saved song: a Lyricsfile when timed (word times
/// included, with the song's details so the file stands on its own), else the
/// plain lyrics. `notes` go at the top as comments (where the times came from,
/// e.g. VOCALS_NOTE, HAND_NOTE), so they can be redone, or left alone.
pub fn to_file(l: &Lyrics, t: Option<&Track>, notes: &[&str]) -> Option<(&'static str, String)> {
    let q = |s: &str| serde_json::to_string(s).unwrap_or_default();
    match l {
        Lyrics::Timed { lines, exact } => {
            let mut y = String::from("version: \"1.0\"\n");
            for n in notes {
                y += &format!("# {n}\n");
            }
            if let Some(t) = t {
                y += &format!("metadata:\n  title: {}\n  artist: {}\n", q(&t.title), q(&t.credit()));
                if !t.album.is_empty() {
                    y += &format!("  album: {}\n", q(&t.album));
                }
                if t.seconds > 0 {
                    y += &format!("  duration_ms: {}\n", t.seconds as u64 * 1000);
                }
            }
            y += "lines:\n";
            for line in lines {
                y += &format!("  - text: {}\n    start_ms: {}\n    end_ms: {}\n", serde_json::to_string(&line.text).ok()?, (line.start * 1000.0) as i64, (line.end * 1000.0) as i64);
                if let Some(who) = &line.singer {
                    y += &format!("    singer: {}\n", q(who));
                }
                if *exact {
                    y += "    words:\n";
                    for w in &line.words {
                        y += &format!("      - text: {}\n        start_ms: {}\n        end_ms: {}\n", serde_json::to_string(&w.text).ok()?, (w.start * 1000.0) as i64, (w.end * 1000.0) as i64);
                    }
                }
            }
            Some(("lyrics.yaml", y))
        }
        // typed by hand, not timed yet: a Lyricsfile all the same (so it's known as yours)
        Lyrics::Plain(text) if notes.contains(&HAND_NOTE) => {
            let mut y = String::from("version: \"1.0\"\n");
            for n in notes {
                y += &format!("# {n}\n");
            }
            y += "plain: |\n";
            for line in text.lines() {
                y += &format!("  {line}\n");
            }
            Some(("lyrics.yaml", y))
        }
        Lyrics::Plain(text) => Some(("lrc", text.clone())),
        Lyrics::None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // the Lyricsfile project's own examples (examples/*.lyricsfile.yaml)
    const WORDS: &str = "version: '1.0'\n\nmetadata:\n  title: 'Small Hours'\n  artist: 'Example Artist'\n\nlines:\n  - text: 'Stay until the morning'\n    start_ms: 4200\n    end_ms: 6800\n    words:\n      - text: 'Stay '\n        start_ms: 4200\n        end_ms: 4800\n      - text: 'until '\n        start_ms: 4800\n        end_ms: 5400\n      - text: 'the '\n        start_ms: 5400\n        end_ms: 5750\n      - text: 'morning'\n        start_ms: 5750\n        end_ms: 6800\n\nplain: |\n  Stay until the morning\n";
    const OVERLAP: &str = "version: '1.0'\n\nlines:\n  - text: 'I will follow the river'\n    start_ms: 10000\n    end_ms: 15000\n  - text: 'Follow the river home'\n    start_ms: 12500\n    end_ms: 16500\n";

    #[test]
    fn word_synced_lyricsfile() {
        let Some(Lyrics::Timed { lines, exact }) = parse_lyricsfile(WORDS) else { panic!("not timed") };
        assert!(exact);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "Stay until the morning");
        assert_eq!((lines[0].start, lines[0].end), (4.2, 6.8));
        assert_eq!(lines[0].words.len(), 4);
        assert_eq!(lines[0].words[3].text, "morning");
        assert_eq!((lines[0].words[3].start, lines[0].words[3].end), (5.75, 6.8));
    }

    // made-up words; the hums timed as the word timer did IF U's (bunched, then a hole)
    const HUMS: &str = "version: '1.0'\nlines:\n  - text: 'Mmmh mmmh mmmmh mmmh'\n    start_ms: 10000\n    end_ms: 18000\n    words:\n      - {text: 'Mmmh ', start_ms: 10000, end_ms: 10200}\n      - {text: 'mmmh ', start_ms: 10200, end_ms: 10900}\n      - {text: 'mmmmh ', start_ms: 16500, end_ms: 16800}\n      - {text: 'mmmh', start_ms: 16900, end_ms: 17600}\n  - text: 'blue ooh ooh river'\n    start_ms: 18000\n    end_ms: 21000\n    words:\n      - {text: 'blue ', start_ms: 18000, end_ms: 18400}\n      - {text: 'ooh ', start_ms: 18400, end_ms: 18500}\n      - {text: 'ooh ', start_ms: 18500, end_ms: 18600}\n      - {text: 'river', start_ms: 20000, end_ms: 21000}\n";

    #[test]
    fn hums_light_at_a_steady_pace() {
        let Some(Lyrics::Timed { lines, .. }) = parse_lyricsfile(HUMS) else { panic!("not timed") };
        let at: Vec<f64> = lines[0].words.iter().map(|w| w.start).collect();
        assert_eq!(at, vec![10.0, 12.0, 14.0, 16.0], "four hums over the whole line, no hole");
        assert_eq!(lines[0].words[3].end, 18.0);
        let w = &lines[1].words;
        assert_eq!((w[0].start, w[0].end), (18.0, 18.4), "a real word keeps its time");
        assert_eq!((w[1].start, w[2].start), (18.4, 19.2), "hums share the time up to the next word");
        assert_eq!(w[3].start, 20.0);
    }

    #[test]
    fn hums_typed_by_hand_are_kept() {
        let text = HUMS.replace("version: '1.0'\n", &format!("version: '1.0'\n# {HAND_NOTE}\n"));
        let Some(Lyrics::Timed { lines, .. }) = parse_lyricsfile(&text) else { panic!("not timed") };
        assert_eq!(lines[0].words[2].start, 16.5);
    }

    #[test]
    fn what_counts_as_a_hum() {
        for h in ["mm", "Mmmh,", "hmm", "mhm", "mm-hmm", "ooh", "Oooh!", "ahh", "uhh", "ooo"] {
            assert!(is_hum(h), "{h}");
        }
        for w in ["oh", "ah", "uh", "aha", "huh", "ma", "home", "na", "yeah", "ha"] {
            assert!(!is_hum(w), "{w}");
        }
    }

    #[test]
    fn overlapping_lines_get_spread_words() {
        let Some(Lyrics::Timed { lines, exact }) = parse_lyricsfile(OVERLAP) else { panic!("not timed") };
        assert!(!exact);
        assert_eq!(lines.len(), 2);
        assert!(lines[1].start < lines[0].end, "the two voices overlap");
        let w = &lines[0].words;
        assert_eq!(w.len(), 5);
        assert_eq!(w[0].start, 10.0);
        assert!((w[4].end - 15.0).abs() < 1e-9, "words fill the line's time");
    }

    #[test]
    fn lrc_lines_end_where_the_next_starts() {
        let lines = parse_lrc("[00:01.00]one two\n[00:03.50]three\n[00:20.00][00:30.00]again");
        assert_eq!(lines.len(), 4);
        assert_eq!((lines[0].start, lines[0].end), (1.0, 3.5));
        assert_eq!(lines[1].end, 3.5 + 8.0, "a long gap holds a line at most 8 s");
        assert_eq!(lines[3].start, 30.0);
    }

    #[test]
    fn saved_lyricsfile_reads_back() {
        let Some(l) = parse_lyricsfile(WORDS) else { panic!() };
        let Some((ext, text)) = to_file(&l, None, &[VOCALS_NOTE]) else { panic!() };
        assert_eq!(ext, "lyrics.yaml");
        let Some(Lyrics::Timed { lines, exact }) = parse_lyricsfile(&text) else { panic!("{text}") };
        assert!(exact);
        assert_eq!(lines[0].words[1].text, "until ");
        assert_eq!(lines[0].words[1].start, 4.8);
    }

    #[test]
    fn singers_are_kept() {
        let Some(Lyrics::Timed { mut lines, exact }) = parse_lyricsfile(WORDS) else { panic!() };
        lines[0].singer = Some("Hev Abi".into());
        let (_, text) = to_file(&Lyrics::Timed { lines, exact }, None, &[HAND_NOTE]).unwrap();
        assert!(text.contains("# edited by hand"));
        let Some(Lyrics::Timed { lines, .. }) = parse_lyricsfile(&text) else { panic!() };
        assert_eq!(lines[0].singer.as_deref(), Some("Hev Abi"));
    }

    #[test]
    fn another_songs_lyrics_are_turned_away() {
        let t = Track { source: crate::track::Source::YouTube, id: "Vcc5qonI9l4".into(), title: "Moolah".into(), artist: "SCOOP DOGG & daarth".into(), album: "Moolah".into(), seconds: 158, art: String::new(), format: String::new(), file: None, video: false, genres: None };
        let theirs = serde_json::json!({ "artistName": "SCOOP DOGG, daarth", "syncedLyrics": "[id: pshzethp]\n[ar: Young Greatness]\n[ti: Moolah]\n[00:02.38]Yeah" });
        assert!(!by_them(&t, &theirs));
        let own = serde_json::json!({ "artistName": "Scoop Dogg & Daarth", "plainLyrics": "[ar: SCOOP DOGG]\nDami nilang tumatawag" });
        assert!(by_them(&t, &own));
        assert!(by_them(&t, &serde_json::json!({ "artistName": "daarth", "plainLyrics": "no tags at all" })));
        assert!(!by_them(&t, &serde_json::json!({ "artistName": "Young Greatness", "plainLyrics": "Yeah" })));
    }
}
