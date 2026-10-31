//! Word timing after a split, as the apps do it (ui.rs time_words): the
//! song's lyrics go to align.py (MMS on its Vocals part) and the Lyricsfile it
//! makes is kept beside the song.
//!
//! - Already timed by MMS ("MMS forced alignment" in the file): left alone.
//! - A Lyricsfile with its own word times (not edited by hand): left alone.
//! - Edited by hand in the app: its lines are timed, each keeping its singer.
//! - Timed by line (a Lyricsfile without words, or an .lrc): timed within each line.
//! - Plain text: the whole song at once.

use serde_json::{json, Value};
use serde_yaml::Value as Yaml;

/// The notes the apps write at the top of a Lyricsfile (lyrics.rs).
pub const HAND_NOTE: &str = "edited by hand in Radiance Music";
pub const MMS_NOTE: &str = "word times: MMS forced alignment (Meta, torchaudio MMS_FA) on the song's Vocals part";

/// What to do with a song's lyrics after its split.
#[derive(Debug, PartialEq)]
pub enum Plan {
    /// Nothing (the reason, for the queue's note).
    Skip(&'static str),
    /// align.py's input; and, for hand-edited lyrics, each line's singer to put back.
    Align { given: Value, singers: Option<Vec<Option<String>>> },
}

#[derive(Debug, Clone)]
struct Line {
    start: f64,
    end: f64,
    text: String,
    singer: Option<String>,
    words: bool,
}

fn lines_of(doc: &Yaml) -> Vec<Line> {
    let ms = |v: &Yaml| v.as_f64().or_else(|| v.as_i64().map(|n| n as f64)).map(|n| n / 1000.0);
    doc.get("lines")
        .and_then(|l| l.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|l| {
                    let text = l.get("text")?.as_str()?.to_string();
                    let start = ms(l.get("start_ms")?)?;
                    let end = l.get("end_ms").and_then(ms).unwrap_or(start + 4.0);
                    let words = l.get("words").and_then(|w| w.as_sequence()).is_some_and(|w| !w.is_empty());
                    let singer = l.get("singer").or_else(|| l.get("agent")).and_then(|s| s.as_str()).map(String::from).filter(|s| !s.is_empty());
                    Some(Line { start, end, text, singer, words })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Whether a Lyricsfile has each word's time.
pub fn has_word_times(yaml: &str) -> bool {
    serde_yaml::from_str::<Yaml>(yaml).ok().is_some_and(|doc| {
        let lines = lines_of(&doc);
        !lines.is_empty() && lines.iter().all(|l| l.words)
    })
}

/// "[01:02.50]words" lines (an .lrc), each ending where the next starts (as the apps read them).
fn lrc_lines(text: &str) -> Vec<Line> {
    let mut stamped = vec![];
    for line in text.lines() {
        let mut rest = line.trim();
        let mut times = vec![];
        while let Some(stripped) = rest.strip_prefix('[') {
            let Some(close) = stripped.find(']') else { break };
            let Some((m, s)) = stripped[..close].split_once(':') else { break };
            let (Ok(m), Ok(s)) = (m.parse::<f64>(), s.parse::<f64>()) else { break };
            times.push(m * 60.0 + s);
            rest = &stripped[close + 1..];
        }
        for t in times {
            stamped.push((t, rest.trim().to_string()));
        }
    }
    stamped.sort_by(|a, b| a.0.total_cmp(&b.0));
    (0..stamped.len())
        .map(|i| {
            let (start, text) = &stamped[i];
            let next = stamped.get(i + 1).map(|n| n.0).unwrap_or(start + 5.0);
            Line { start: *start, end: next.min(start + 8.0).max(start + 0.3), text: text.clone(), singer: None, words: false }
        })
        .collect()
}

fn by_lines(lines: &[Line]) -> Value {
    json!({ "lines": lines.iter().filter(|l| !l.text.trim().is_empty()).map(|l| json!({ "start": l.start, "end": l.end, "text": l.text })).collect::<Vec<_>>() })
}

/// What align.py should be given for a song's lyrics: its Lyricsfile (`yaml`)
/// first, else its .lrc (`lrc`).
pub fn plan(yaml: Option<&str>, lrc: Option<&str>) -> Plan {
    if let Some(y) = yaml {
        if y.contains("MMS forced alignment") {
            return Plan::Skip("its words are timed already");
        }
        if let Ok(doc) = serde_yaml::from_str::<Yaml>(y) {
            let hand = y.contains(HAND_NOTE);
            let lines = lines_of(&doc);
            if !lines.is_empty() {
                if !hand && lines.iter().all(|l| l.words) {
                    return Plan::Skip("its lyrics have their own word times");
                }
                let kept: Vec<&Line> = lines.iter().filter(|l| !l.text.trim().is_empty()).collect();
                let singers = hand.then(|| kept.iter().map(|l| l.singer.clone()).collect());
                return Plan::Align { given: by_lines(&lines), singers };
            }
            if let Some(p) = doc.get("plain").and_then(|p| p.as_str()).filter(|p| !p.trim().is_empty()) {
                return Plan::Align { given: json!({ "plain": p }), singers: None };
            }
        }
    }
    if let Some(text) = lrc.filter(|t| !t.trim().is_empty()) {
        let lines = lrc_lines(text);
        return if lines.is_empty() { Plan::Align { given: json!({ "plain": text }), singers: None } } else { Plan::Align { given: by_lines(&lines), singers: None } };
    }
    Plan::Skip("it has no lyrics")
}

/// The Lyricsfile to keep from align.py's: as it is, or for hand-edited
/// lyrics, with each line's singer back and both notes on top (as the apps do).
pub fn finish(aligned: &str, singers: Option<&[Option<String>]>, original: Option<&str>) -> String {
    let Some(singers) = singers else { return aligned.to_string() };
    let Ok(doc) = serde_yaml::from_str::<Yaml>(aligned) else { return aligned.to_string() };
    let q = |s: &str| serde_json::to_string(s).unwrap_or_default();
    let mut y = format!("version: \"1.0\"\n# {HAND_NOTE}\n# {MMS_NOTE}\n");
    // the original's details (title, artist, album, length) stay
    if let Some(meta) = original.and_then(|o| serde_yaml::from_str::<Yaml>(o).ok()).and_then(|o| o.get("metadata").cloned()).and_then(|m| m.as_mapping().cloned()) {
        y += "metadata:\n";
        for (k, v) in meta {
            if let (Some(k), Some(v)) = (k.as_str(), v.as_str().map(|s| q(s)).or_else(|| v.as_i64().map(|n| n.to_string()))) {
                y += &format!("  {k}: {v}\n");
            }
        }
    }
    y += "lines:\n";
    let lines = doc.get("lines").and_then(|l| l.as_sequence()).cloned().unwrap_or_default();
    for (i, l) in lines.iter().enumerate() {
        let get = |v: &Yaml, k: &str| v.get(k).and_then(|x| x.as_i64()).unwrap_or(0);
        y += &format!("  - text: {}\n    start_ms: {}\n    end_ms: {}\n", q(l.get("text").and_then(|t| t.as_str()).unwrap_or("")), get(l, "start_ms"), get(l, "end_ms"));
        if let Some(Some(who)) = singers.get(i) {
            y += &format!("    singer: {}\n", q(who));
        }
        y += "    words:\n";
        for w in l.get("words").and_then(|w| w.as_sequence()).cloned().unwrap_or_default() {
            y += &format!("      - text: {}\n        start_ms: {}\n        end_ms: {}\n", q(w.get("text").and_then(|t| t.as_str()).unwrap_or("")), get(&w, "start_ms"), get(&w, "end_ms"));
        }
    }
    y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans() {
        assert_eq!(plan(None, None), Plan::Skip("it has no lyrics"));
        assert!(matches!(plan(Some("version: '1.0'\n# word times: MMS forced alignment\nlines: []\n"), None), Plan::Skip(_)));
        let words = "version: '1.0'\nlines:\n  - text: 'a b'\n    start_ms: 1000\n    end_ms: 2000\n    words:\n      - text: 'a '\n        start_ms: 1000\n        end_ms: 1500\n";
        assert!(matches!(plan(Some(words), None), Plan::Skip(_)));
        let lines = "version: '1.0'\nlines:\n  - text: 'hello there'\n    start_ms: 1000\n    end_ms: 3000\n";
        assert_eq!(plan(Some(lines), None), Plan::Align { given: json!({ "lines": [{ "start": 1.0, "end": 3.0, "text": "hello there" }] }), singers: None });
        let hand = "version: \"1.0\"\n# edited by hand in Radiance Music\nlines:\n  - text: \"hi\"\n    start_ms: 0\n    end_ms: 900\n    singer: \"Ann\"\n";
        assert!(matches!(plan(Some(hand), None), Plan::Align { singers: Some(s), .. } if s == vec![Some("Ann".to_string())]));
        assert_eq!(plan(None, Some("just words\nmore words")), Plan::Align { given: json!({ "plain": "just words\nmore words" }), singers: None });
        assert!(matches!(plan(None, Some("[00:01.00]one\n[00:03.00]two")), Plan::Align { given, .. } if given["lines"][0]["end"] == json!(3.0)));
    }

    #[test]
    fn hand_lyrics_keep_singers() {
        let aligned = "version: \"1.0\"\nlines:\n  - text: \"hi\"\n    start_ms: 100\n    end_ms: 800\n    words:\n      - text: \"hi\"\n        start_ms: 100\n        end_ms: 800\n";
        let out = finish(aligned, Some(&[Some("Ann".into())]), Some("metadata:\n  title: \"Song\"\n"));
        assert!(out.contains(HAND_NOTE) && out.contains(MMS_NOTE) && out.contains("singer: \"Ann\"") && out.contains("title: \"Song\""));
        assert!(has_word_times(&out));
    }
}
