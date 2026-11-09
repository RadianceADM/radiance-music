//! Word timing in this process (server/align.py in Rust, no Python, #72):
//! Meta's MMS forced aligner (as ONNX, the `models-mms-1` model: one model for
//! 1100+ languages, Tagalog included) on the song's Vocals part, then each
//! word placed as align.py places it, written as align.py writes its Lyricsfile.
//!
//! Its pieces follow torchaudio's to the letter (checked against fixtures
//! torchaudio and align.py made: tests/fixtures/align): the resampling to
//! 16 kHz, the CTC forced alignment (its pruning and its tie-breaking, in
//! float32), the merging of a word's frames into letters.

use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::Value;
use unicode_normalization::char::canonical_combining_class;
use unicode_normalization::UnicodeNormalization;

use crate::engine::REMOVED;

/// The model's rate, and its letters (index 0: the blank).
pub const RATE: u32 = 16_000;
const LETTERS: &str = "-aienoutsrmkldghybpwcvjzf'qx";
pub const CLASSES: usize = 28;
/// Seconds a pass, and the sound kept either side of it (its attention grows with the square).
const WINDOW: f64 = 30.0;
const CONTEXT: f64 = 2.0;

/// The model on some 16 kHz sound: its scores (frames × CLASSES, row by row) and its frames.
pub type Model<'a> = &'a mut dyn FnMut(&[f32]) -> Result<(Vec<f32>, usize), String>;

/// The letters the model knows: lowercase a-z and ', accents off (align.py's norm).
pub fn norm(word: &str) -> String {
    word.to_lowercase().nfkd().filter(|c| canonical_combining_class(*c) == 0).filter(|c| c.is_ascii_lowercase() || *c == '\'').collect()
}

fn token(c: char) -> usize {
    LETTERS.find(c).unwrap_or(0)
}

/// Python's str.split(): its whitespace (Unicode's, and the four separators \x1c-\x1f).
fn words(text: &str) -> Vec<&str> {
    text.split(|c: char| c.is_whitespace() || ('\x1c'..='\x1f').contains(&c)).filter(|w| !w.is_empty()).collect()
}

/// Python's str.splitlines().
fn lines(text: &str) -> Vec<String> {
    let text = text.replace("\r\n", "\n");
    text.split(|c: char| matches!(c, '\n' | '\r' | '\x0b' | '\x0c' | '\x1c' | '\x1d' | '\x1e' | '\u{85}' | '\u{2028}' | '\u{2029}')).map(String::from).collect()
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// torchaudio.functional.resample (sinc_interp_hann, width 6, rolloff 0.99), in float32 as it does.
pub fn resample(x: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to {
        return x.to_vec();
    }
    let g = gcd(from, to);
    let (o, n) = ((from / g) as usize, (to / g) as usize);
    let base64 = o.min(n) as f64 * 0.99;
    let width = (6.0 * o as f64 / base64).ceil() as usize;
    let (base, w6) = (base64 as f32, 6.0f32);
    let taps = 2 * width + o;
    // the kernel: n filters of `taps`
    let mut kernel = vec![0.0f32; n * taps];
    let scale = (base64 / o as f64) as f32;
    for j in 0..n {
        for k in 0..taps {
            let idx = (k as f32 - width as f32) / o as f32;
            let mut t = (-(j as f32)) / n as f32 + idx;
            t *= base;
            t = t.clamp(-w6, w6);
            let window = (t * std::f32::consts::PI / w6 / 2.0).cos().powi(2);
            t *= std::f32::consts::PI;
            let sinc = if t == 0.0 { 1.0 } else { t.sin() / t };
            kernel[j * taps + k] = sinc * (window * scale);
        }
    }
    let len = x.len();
    let at = |i: isize| if i >= 0 && (i as usize) < len { x[i as usize] } else { 0.0 };
    let frames = len / o + 1;
    let target = ((n * len) as f64 / o as f64).ceil() as usize;
    let mut y = Vec::with_capacity(frames * n);
    for c in 0..frames {
        for j in 0..n {
            let mut acc = 0.0f32;
            let k0 = (c * o) as isize - width as isize;
            for k in 0..taps {
                acc += kernel[j * taps + k] * at(k0 + k as isize);
            }
            y.push(acc);
        }
    }
    y.truncate(target);
    y
}

/// torchaudio's CTC forced alignment (blank 0, float32): the letter (or blank)
/// of every frame, or None when there are too few frames for the letters.
pub fn forced_align(em: &[f32], frames: usize, classes: usize, targets: &[usize]) -> Option<Vec<usize>> {
    let (t_len, l) = (frames, targets.len());
    let s_len = 2 * l + 1;
    let r = (1..l).filter(|&i| targets[i] == targets[i - 1]).count();
    if l == 0 || t_len < l + r || frames == 0 {
        return None;
    }
    let lp = |t: usize, c: usize| em[t * classes + c];
    let label = |i: usize| if i % 2 == 0 { 0 } else { targets[i / 2] };
    let ninf = f32::NEG_INFINITY;
    let mut alphas = [vec![ninf; s_len], vec![ninf; s_len]];
    let mut back = vec![-1i8; t_len * s_len];
    let mut start: usize = if t_len > l + r { 0 } else { 1 };
    let mut end: usize = if s_len == 1 { 1 } else { 2 };
    for i in start..end {
        alphas[0][i] = lp(0, label(i));
    }
    for t in 1..t_len {
        if t_len - t <= l + r {
            if start % 2 == 1 && targets[start / 2] != targets[start / 2 + 1] {
                start += 1;
            }
            start += 1;
        }
        if t <= l + r {
            if end % 2 == 0 && end < 2 * l && targets[end / 2 - 1] != targets[end / 2] {
                end += 1;
            }
            end += 1;
        }
        let (cur, prev) = (t % 2, (t - 1) % 2);
        let mut startloop = start;
        alphas[cur].fill(ninf);
        if start == 0 {
            alphas[cur][0] = alphas[prev][0] + lp(t, 0);
            back[t * s_len] = 0;
            startloop += 1;
        }
        for i in startloop..end {
            let x0 = alphas[prev][i];
            let x1 = alphas[prev][i - 1];
            let mut x2 = ninf;
            if i % 2 != 0 && i != 1 && targets[i / 2] != targets[i / 2 - 1] {
                x2 = alphas[prev][i - 2];
            }
            let (result, b) = if x2 > x1 && x2 > x0 {
                (x2, 2)
            } else if x1 > x0 && x1 > x2 {
                (x1, 1)
            } else {
                (x0, 0)
            };
            back[t * s_len + i] = b;
            alphas[cur][i] = result + lp(t, label(i));
        }
    }
    let last = (t_len - 1) % 2;
    // scores that aren't numbers (the model's output gone wrong): no path, not a crash
    if !alphas[last][s_len - 1].is_finite() && !alphas[last][s_len - 2].is_finite() {
        return None;
    }
    let mut idx = if alphas[last][s_len - 1] > alphas[last][s_len - 2] { s_len - 1 } else { s_len - 2 };
    let mut path = vec![0usize; t_len];
    for t in (0..t_len).rev() {
        path[t] = label(idx);
        if t == 0 {
            break;
        }
        let b = back[t * s_len + idx];
        if b < 0 || (b as usize) > idx {
            return None;
        }
        idx -= b as usize;
    }
    Some(path)
}

/// A letter's frames: start..end.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    pub token: usize,
    pub start: usize,
    pub end: usize,
}

/// torchaudio's merge_tokens: runs of the same letter (blanks left out).
pub fn merge_tokens(path: &[usize]) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut i = 0;
    while i < path.len() {
        let mut j = i + 1;
        while j < path.len() && path[j] == path[i] {
            j += 1;
        }
        if path[i] != 0 {
            spans.push(Span { token: path[i], start: i, end: j });
        }
        i = j;
    }
    spans
}

/// The aligner (torchaudio's MMS_FA aligner): each word's letters' spans.
pub fn align_words(em: &[f32], frames: usize, classes: usize, words: &[Vec<usize>]) -> Option<Vec<Vec<Span>>> {
    let flat: Vec<usize> = words.iter().flatten().copied().collect();
    let spans = merge_tokens(&forced_align(em, frames, classes, &flat)?);
    let mut out = Vec::with_capacity(words.len());
    let mut at = 0;
    for w in words {
        out.push(spans.get(at..at + w.len())?.to_vec());
        at += w.len();
    }
    Some(out)
}

/// The scores for the whole song, 30 s a pass with 2 s either side (as align.py): (scores, frames).
pub fn emissions(run: Model, wav: &[f32], progress: &dyn Fn(f64), stop: &AtomicBool) -> Result<(Vec<f32>, usize), String> {
    let rate = RATE as f64;
    let total = wav.len() as f64 / rate;
    let (mut all, mut frames, mut at) = (Vec::new(), 0usize, 0.0f64);
    while at < total {
        if stop.load(Ordering::Relaxed) {
            return Err(REMOVED.into());
        }
        let (a, b) = ((at - CONTEXT).max(0.0), (at + WINDOW + CONTEXT).min(total));
        let (lo, hi) = (((a * rate) as usize).min(wav.len()), ((b * rate) as usize).min(wav.len()));
        let (em, n) = run(&wav[lo..hi])?;
        if n == 0 || em.len() < n * CLASSES {
            return Err("the word-timing model gave nothing back".into());
        }
        let per = (b - a) / n as f64;
        let keep_from = (((at - a) / per).round_ties_even() as usize).min(n);
        let keep_to = ((((total.min(at + WINDOW)) - a) / per).round_ties_even() as usize).min(n);
        if keep_to > keep_from {
            all.extend_from_slice(&em[keep_from * CLASSES..keep_to * CLASSES]);
            frames += keep_to - keep_from;
        }
        at += WINDOW;
        progress(0.05 + 0.75 * (at / total).min(1.0));
    }
    Ok((all, frames))
}

type Times = Vec<Option<(f64, f64)>>;

/// Each word's (start, end) in seconds, aligned within lo..hi; None if it can't be (align.py's place).
fn place(em: &[f32], frames: usize, spf: f64, words: &[&str], lo: f64, hi: f64) -> Times {
    let a = ((lo / spf) as i64).max(0) as usize;
    let b = (((hi / spf) as i64) + 1).min(frames as i64).max(0) as usize;
    let keep: Vec<(usize, String)> = words.iter().enumerate().map(|(i, w)| (i, norm(w))).filter(|(_, w)| !w.is_empty()).collect();
    let mut times = vec![None; words.len()];
    if keep.is_empty() || b < a + 2 {
        return times;
    }
    let tokens: Vec<Vec<usize>> = keep.iter().map(|(_, w)| w.chars().map(token).collect()).collect();
    let Some(spans) = align_words(&em[a * CLASSES..b * CLASSES], b - a, CLASSES, &tokens) else { return times };
    for ((i, _), sp) in keep.iter().zip(spans) {
        if let (Some(first), Some(last)) = (sp.first(), sp.last()) {
            times[*i] = Some(((a + first.start) as f64 * spf, (a + last.end) as f64 * spf));
        }
    }
    times
}

/// align.py's Lyricsfile: words with nothing to sing (numbers, signs) take the gap before the next.
pub fn lyricsfile(groups: &[(String, Times)]) -> String {
    let q = |s: &str| serde_json::to_string(s).unwrap_or_default();
    let ms = |s: f64| (s * 1000.0).round_ties_even() as i64;
    let mut y = vec!["version: \"1.0\"".to_string(), "# word times: MMS forced alignment (Meta, torchaudio MMS_FA) on the song's Vocals part".to_string(), "lines:".to_string()];
    for (text, times) in groups {
        let ws = words(text);
        let known: Vec<(f64, f64)> = times.iter().flatten().copied().collect();
        if known.is_empty() {
            continue;
        }
        let mut filled: Vec<(f64, f64)> = Vec::with_capacity(times.len());
        for (i, t) in times.iter().enumerate() {
            let t = match t {
                Some(t) => *t,
                None => {
                    let next = times[i + 1..].iter().flatten().map(|x| x.0).next();
                    let s = filled.last().map(|f| f.1).or(next).unwrap_or(known[0].0);
                    (s, s)
                }
            };
            filled.push(t);
        }
        let start = filled[0].0;
        let end = filled.iter().map(|f| f.1).fold(f64::NEG_INFINITY, f64::max);
        y.push(format!("  - text: {}", q(text)));
        y.push(format!("    start_ms: {}", ms(start)));
        y.push(format!("    end_ms: {}", ms(end.max(start + 0.2))));
        y.push("    words:".into());
        for (i, (w, (s, e))) in ws.iter().zip(&filled).enumerate() {
            let tail = if i + 1 < ws.len() { " " } else { "" };
            y.push(format!("      - text: {}", q(&format!("{w}{tail}"))));
            y.push(format!("        start_ms: {}", ms(*s)));
            y.push(format!("        end_ms: {}", ms(e.max(*s))));
        }
    }
    y.join("\n") + "\n"
}

/// The Lyricsfile for `given` ({"lines": [{start, end?, text}]} timed by
/// line, or {"plain": "..."}) on 16 kHz mono `wav` (align.py's main).
pub fn align(run: Model, wav: &[f32], given: &Value, progress: &dyn Fn(f64), stop: &AtomicBool) -> Result<String, String> {
    progress(0.05);
    let (em, frames) = emissions(run, wav, progress, stop)?;
    let total = wav.len() as f64 / RATE as f64;
    let spf = total / frames as f64;
    let mut groups: Vec<(String, Times)> = Vec::new();
    if let Some(given_lines) = given.get("lines").and_then(Value::as_array) {
        let lines: Vec<&Value> = given_lines.iter().filter(|l| l.get("text").and_then(Value::as_str).is_some_and(|t| !t.trim().is_empty())).collect();
        let start = |l: &Value| l.get("start").and_then(Value::as_f64).unwrap_or(0.0);
        for (i, l) in lines.iter().enumerate() {
            let text = l["text"].as_str().unwrap_or("").to_string();
            let lo = (start(l) - 0.5).max(0.0);
            let hi = match lines.get(i + 1) {
                Some(next) => start(next),
                None => l.get("end").and_then(Value::as_f64).unwrap_or(start(l) + 6.0),
            } + 0.5;
            let times = place(&em, frames, spf, &words(&text), lo, hi);
            groups.push((text, times));
        }
    } else {
        let plain = given.get("plain").and_then(Value::as_str).unwrap_or("");
        let texts: Vec<String> = lines(plain).into_iter().filter(|t| {
            let s = t.trim();
            !s.is_empty() && !(s.starts_with('[') && s.ends_with(']'))
        }).collect();
        let all: Vec<&str> = texts.iter().flat_map(|t| words(t)).collect();
        let times = place(&em, frames, spf, &all, 0.0, total);
        let mut k = 0;
        for text in texts {
            let n = words(&text).len();
            groups.push((text, times[k..k + n].to_vec()));
            k += n;
        }
    }
    progress(0.95);
    let text = lyricsfile(&groups);
    progress(1.0);
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Value {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/align").join(name);
        serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
    }

    fn floats(v: &Value) -> Vec<f32> {
        v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap() as f32).collect()
    }

    #[test]
    fn the_letters_are_torchaudios() {
        assert_eq!(LETTERS.chars().count(), CLASSES);
        assert_eq!((token('-'), token('a'), token('h'), token('\''), token('x')), (0, 1, 15, 25, 27));
    }

    #[test]
    fn words_keep_only_what_the_model_knows() {
        assert_eq!(norm("Ikáw"), "ikaw");
        assert_eq!(norm("can't"), "can't");
        assert_eq!(norm("Ooh,"), "ooh");
        assert_eq!(norm("été"), "ete");
        assert_eq!(norm("123"), "");
        assert_eq!(norm("ﬁne"), "fine", "ligatures come apart (NFKD)");
        assert_eq!(norm("Ñoño"), "nono");
    }

    #[test]
    fn forced_alignment_is_torchaudios() {
        for case in fixture("forced_align.json")["cases"].as_array().unwrap() {
            let frames = case["frames"].as_u64().unwrap() as usize;
            let em = floats(&case["emission"]);
            let words: Vec<Vec<usize>> = case["words"].as_array().unwrap().iter().map(|w| w.as_array().unwrap().iter().map(|t| t.as_u64().unwrap() as usize).collect()).collect();
            let flat: Vec<usize> = words.iter().flatten().copied().collect();
            let path = forced_align(&em, frames, CLASSES, &flat);
            match case["path"].as_array() {
                None => assert!(path.is_none(), "{frames} {words:?}"),
                Some(want) => {
                    let want: Vec<usize> = want.iter().map(|t| t.as_u64().unwrap() as usize).collect();
                    assert_eq!(path.as_ref().unwrap(), &want, "{frames} frames, {words:?}");
                    let spans = align_words(&em, frames, CLASSES, &words).unwrap();
                    let want: Vec<Vec<[usize; 3]>> = serde_json::from_value(case["spans"].clone()).unwrap();
                    let got: Vec<Vec<[usize; 3]>> = spans.iter().map(|w| w.iter().map(|s| [s.token, s.start, s.end]).collect()).collect();
                    assert_eq!(got, want, "{frames} frames, {words:?}");
                }
            }
        }
    }

    #[test]
    fn scores_that_arent_numbers_time_nothing_and_never_crash() {
        let em = vec![f32::NAN; 40 * CLASSES];
        assert!(forced_align(&em, 40, CLASSES, &[15, 3, 12, 12, 5]).is_none());
        let mut inf = vec![0.0f32; 40 * CLASSES];
        inf.iter_mut().skip(3).step_by(7).for_each(|x| *x = f32::NEG_INFINITY);
        let _ = forced_align(&inf, 40, CLASSES, &[15, 3, 12, 12, 5]);
        let wav = vec![0.0f32; 2 * RATE as usize];
        let given = serde_json::json!({ "lines": [{ "start": 0.0, "end": 2.0, "text": "hello" }] });
        let yaml = align(&mut |_| Ok((vec![f32::NAN; 100 * CLASSES], 100)), &wav, &given, &|_| {}, &AtomicBool::new(false)).unwrap();
        assert!(!yaml.contains("hello"), "{yaml}");
    }

    #[test]
    fn too_few_frames_for_the_letters_is_none() {
        // "hello": 5 letters and a repeat need 6 frames
        let em = vec![0.0f32; 5 * CLASSES];
        assert!(forced_align(&em, 5, CLASSES, &[15, 3, 12, 12, 5]).is_none());
        assert!(forced_align(&vec![0.0f32; 6 * CLASSES], 6, CLASSES, &[15, 3, 12, 12, 5]).is_some());
    }

    #[test]
    fn resampling_is_torchaudios() {
        for case in fixture("resample.json")["cases"].as_array().unwrap() {
            let (x, want) = (floats(&case["x"]), floats(&case["y"]));
            let got = resample(&x, case["rate"].as_u64().unwrap() as u32, RATE);
            assert_eq!(got.len(), want.len());
            let worst = got.iter().zip(&want).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
            assert!(worst < 2e-5, "{}: {worst}", case["rate"]);
        }
    }

    #[test]
    fn the_lyricsfile_is_align_pys_byte_for_byte() {
        for run in fixture("align_py.json")["runs"].as_array().unwrap() {
            let (frames, samples) = (run["frames"].as_u64().unwrap() as usize, run["samples"].as_u64().unwrap() as usize);
            let em = floats(&run["emission"]);
            let wav = vec![0.0f32; samples];
            let got = align(&mut |_| Ok((em.clone(), frames)), &wav, &run["given"], &|_| {}, &AtomicBool::new(false)).unwrap();
            assert_eq!(got, run["yaml"].as_str().unwrap(), "{}", run["name"]);
        }
    }

    #[test]
    fn a_long_song_goes_in_passes_and_can_be_stopped() {
        // 75 s: three passes (0-32, 28-62, 58-75), 50 frames a second
        let wav = vec![0.0f32; 75 * RATE as usize];
        let asked = std::sync::Mutex::new(Vec::new());
        let mut run = |w: &[f32]| {
            asked.lock().unwrap().push(w.len());
            let n = w.len() / 320;
            Ok((vec![0.0f32; n * CLASSES], n))
        };
        let (_, frames) = emissions(&mut run, &wav, &|_| {}, &AtomicBool::new(false)).unwrap();
        assert_eq!(*asked.lock().unwrap(), [32 * 16_000, 34 * 16_000, 17 * 16_000]);
        assert!((frames as i64 - 75 * 50).abs() <= 2, "{frames}");
        let stop = AtomicBool::new(true);
        assert_eq!(emissions(&mut run, &wav, &|_| {}, &stop).unwrap_err(), REMOVED);
    }
}
