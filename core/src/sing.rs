//! Lyric timing: which lines are sung at a moment of the song, and how much of a line is sung.
//! Moved out of the GTK app (ui.rs) so the Windows app lights the same lines (issue #45).

use crate::lyrics::{Line, Word};

/// A word lights this much before it's sung (seconds): it looks on time.
pub const WORD_LEAD: f64 = 0.08;
/// A line lights this much before it starts (seconds).
pub const LINE_LEAD: f64 = 0.35;

/// Where each word sits in the line's text, in characters (words that can't be
/// found follow on from the last one).
pub fn word_spans(text: &str, words: &[Word]) -> Vec<(usize, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let mut pos = 0;
    words
        .iter()
        .map(|w| {
            let word: Vec<char> = w.text.trim().chars().collect();
            let found = (pos..=chars.len().saturating_sub(word.len())).find(|&i| !word.is_empty() && chars[i..i + word.len()] == word[..]);
            let a = found.unwrap_or(pos).min(chars.len());
            let b = (a + word.len()).min(chars.len());
            pos = b;
            (a, b)
        })
        .collect()
}

/// How much of `line` has been sung at `at` (characters; `spans` from [word_spans]): each
/// word lights whole, the moment it's sung (no sweep across its letters); all of it once
/// the line is over, or when its words have no times.
pub fn sung_chars(line: &Line, spans: &[(usize, usize)], at: f64) -> usize {
    if at >= line.end || spans.is_empty() {
        return line.text.chars().count();
    }
    let at = at + WORD_LEAD;
    let mut k = 0;
    for (w, &(_, b)) in line.words.iter().zip(spans) {
        if at >= w.start {
            k = b;
        } else {
            break;
        }
    }
    k
}

/// The lines lit at `at`: the ones that have started and not ended (two voices can overlap);
/// the latest one stays lit until the next starts.
pub fn lit_lines<'a>(lines: impl IntoIterator<Item = &'a Line>, at: f64) -> Vec<usize> {
    let times: Vec<(f64, f64)> = lines.into_iter().map(|l| (l.start, l.end)).collect();
    match times.iter().rposition(|&(start, _)| start <= at + LINE_LEAD) {
        Some(l) => (0..=l).filter(|&i| i == l || at + LINE_LEAD < times[i].1).collect(),
        None => vec![],
    }
}

/// A timed line as both apps show it: an empty line is a note ("♪", a break in the singing); its
/// words' places in its text; who sings it, only where the voice changes from the line before
/// (a line without a singer keeps the voice).
#[derive(Clone, Debug)]
pub struct SungLine {
    pub line: Line,
    pub head: Option<String>,
    pub spans: Vec<(usize, usize)>,
}

pub fn prepare(lines: Vec<Line>) -> Vec<SungLine> {
    let mut voice: Option<String> = None;
    lines
        .into_iter()
        .map(|mut line| {
            if line.text.trim().is_empty() {
                line.text = "♪".into();
                line.words.clear();
            }
            let spans = word_spans(&line.text, &line.words);
            let head = line.singer.clone().filter(|s| voice.as_deref() != Some(s.as_str()));
            if line.singer.is_some() {
                voice = line.singer.clone();
            }
            SungLine { line, head, spans }
        })
        .collect()
}

/// The lines lit at `at`, each with how many of its characters are sung (whole words).
pub fn sung(lines: &[SungLine], at: f64) -> Vec<(usize, usize)> {
    lit_lines(lines.iter().map(|l| &l.line), at).into_iter().map(|i| (i, sung_chars(&lines[i].line, &lines[i].spans, at))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(start: f64, end: f64) -> Line {
        Line { start, end, text: format!("line {start}"), words: vec![], singer: None }
    }
    fn word(start: f64, text: &str) -> Word {
        Word { start, end: start + 0.3, text: text.into() }
    }

    #[test]
    fn heads_only_where_the_voice_changes() {
        let mut lines = vec![line(1.0, 2.0), line(2.0, 3.0), line(3.0, 4.0), line(4.0, 5.0)];
        lines[0].singer = Some("Ann".into());
        lines[1].singer = Some("Ann".into());
        lines[3].singer = Some("Bo".into());
        let heads: Vec<Option<String>> = prepare(lines).into_iter().map(|l| l.head).collect();
        assert_eq!(heads, [Some("Ann".to_string()), None, None, Some("Bo".to_string())], "a line without a singer keeps the voice");
    }

    #[test]
    fn empty_lines_are_notes() {
        let mut l = line(1.0, 2.0);
        l.text = "  ".into();
        l.words = vec![word(1.0, "x")];
        let p = prepare(vec![l]);
        assert_eq!(p[0].line.text, "♪");
        assert!(p[0].line.words.is_empty() && p[0].spans.is_empty());
    }

    #[test]
    fn sung_lights_lines_then_words() {
        let mut a = line(10.0, 12.0);
        a.text = "Hello world".into();
        a.words = vec![word(10.0, "Hello"), word(11.0, "world")];
        let mut b = line(13.0, 14.0);
        b.text = "Bye".into();
        let lines = prepare(vec![a, b]);
        assert_eq!(sung(&lines, 9.0), vec![], "not yet: a line leads by 0.35 s");
        assert_eq!(sung(&lines, 9.8), vec![(0, 0)]);
        assert_eq!(sung(&lines, 9.95), vec![(0, 5)], "a word leads by 0.08 s");
        assert_eq!(sung(&lines, 11.0), vec![(0, 11)]);
        assert_eq!(sung(&lines, 13.0), vec![(1, 3)], "no word times: the whole line");
    }

    #[test]
    fn words_found_in_their_line() {
        let w = |t: &str| Word { start: 0.0, end: 0.0, text: t.into() };
        assert_eq!(word_spans("Stay until the morning", &[w("Stay "), w("until "), w("the "), w("morning")]), vec![(0, 4), (5, 10), (11, 14), (15, 22)]);
        // a word the text doesn't have follows on from the last one
        assert_eq!(word_spans("Oh, là là", &[w("Oh,"), w("xyz"), w("là")]), vec![(0, 3), (3, 6), (7, 9)]);
    }

    #[test]
    fn the_latest_started_line_stays_lit_until_the_next() {
        let l = vec![line(0.0, 3.0), line(2.5, 4.0), line(6.0, 8.0)];
        assert_eq!(lit_lines(&l, 1.0), vec![0]);
        assert_eq!(lit_lines(&l, 2.3), vec![0, 1]); // line 1 starts within LINE_LEAD, line 0 still on
        assert_eq!(lit_lines(&l, 3.5), vec![1]); // line 0 is over
        assert_eq!(lit_lines(&l, 5.0), vec![1]); // the latest stays lit until the next starts
        assert_eq!(lit_lines(&l, 5.7), vec![2]);
        assert!(lit_lines(&l, -1.0).is_empty());
        assert!(lit_lines(&Vec::<Line>::new(), 5.0).is_empty());
    }

    #[test]
    fn words_light_whole_a_moment_early() {
        let mut l = line(10.0, 14.0);
        l.text = "Stay until the morning".into();
        l.words = vec![word(10.0, "Stay "), word(11.0, "until "), word(12.0, "the "), word(13.0, "morning")];
        let spans = word_spans(&l.text, &l.words);
        assert_eq!(sung_chars(&l, &spans, 9.0), 0);
        assert_eq!(sung_chars(&l, &spans, 10.90), 4); // "Stay"; "until" lights at 11 - WORD_LEAD
        assert_eq!(sung_chars(&l, &spans, 10.93), 10);
        assert_eq!(sung_chars(&l, &spans, 20.0), 22); // over: all of it
        assert_eq!(sung_chars(&l, &[], 11.0), 22); // no word times: the line lights whole
    }
}
