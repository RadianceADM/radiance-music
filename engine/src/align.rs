//! Word timing from the singing itself, no AI: lyrics timed line by line plus
//! the song's Vocals part (from a split) give each word its own time.
//!
//! The vocals are scanned for where the voice is (loudness) and where it starts
//! a new sound (a rise in its spectrum, 150 Hz-4 kHz: consonants and new
//! vowels). Within each line, every word has an expected start: the time the
//! voice is actually heard in the line (pauses left out) shared out by the
//! words' lengths. The word goes to the new sound that best fits, strong sounds
//! near the expected place winning, in order; with none near enough it keeps
//! its expected place.
//!
//! Measured on JamendoLyrics (hand-marked word starts, Demucs vocals): mean
//! error 0.69 s -> 0.50 s against words spread over the line (no more words lit
//! across gaps), 54 % -> 57 % of words within 0.3 s.

use crate::beat;
use crate::lyrics::{Line, Word};
use realfft::RealFftPlanner;

const RATE: f64 = beat::RATE as f64;
const WINDOW: usize = 512;
const HOP: usize = 110; // ~10 ms

/// What the vocals tell: per frame, whether the voice is there, and how
/// strongly a new sound starts (0-1); plus the sounds' starts (frame, strength).
struct Voice {
    voiced: Vec<bool>,
    starts: Vec<(usize, f32)>,
}

fn time(f: usize) -> f64 {
    (f * HOP + WINDOW / 2) as f64 / RATE
}

fn frame(t: f64) -> usize {
    ((t * RATE - (WINDOW / 2) as f64) / HOP as f64).max(0.0) as usize
}

fn listen(samples: &[f32]) -> Option<Voice> {
    let frames = samples.len().checked_sub(WINDOW)? / HOP;
    if frames < 100 {
        return None;
    }
    let mut planner = RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(WINDOW);
    let window: Vec<f32> = (0..WINDOW).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / WINDOW as f32).cos()).collect();
    let (mut input, mut spectrum) = (fft.make_input_vec(), fft.make_output_vec());
    let (lo, hi) = (150 * WINDOW / beat::RATE, 4000 * WINDOW / beat::RATE);
    let mut last = vec![0f32; hi + 1];
    let (mut loud, mut flux) = (vec![0f32; frames], vec![0f32; frames]);
    for f in 0..frames {
        let chunk = &samples[f * HOP..f * HOP + WINDOW];
        for (i, s) in chunk.iter().enumerate() {
            input[i] = s * window[i];
        }
        // (in f64: f32's log10f comes from glibc 2.43, and the Linux download runs on older systems too)
        loud[f] = (10.0 * ((chunk.iter().map(|s| s * s).sum::<f32>() / WINDOW as f32 + 1e-10) as f64).log10()) as f32;
        fft.process(&mut input, &mut spectrum).ok()?;
        let mut rise = 0.0;
        for b in lo..=hi {
            let m = (1.0 + 1000.0 * spectrum[b].norm()).ln();
            rise += (m - last[b]).max(0.0);
            last[b] = m;
        }
        flux[f] = rise;
    }
    // the voice: within 20 dB of its loud parts and clear of the quiet bleed
    // (other instruments left in the part); tiny gaps filled, specks dropped
    let mut sorted = loud.clone();
    sorted.sort_by(f32::total_cmp);
    let floor = (sorted[sorted.len() * 9 / 10] - 20.0).max(sorted[sorted.len() / 5] + 6.0);
    let mut voiced: Vec<bool> = loud.iter().map(|&l| l > floor).collect();
    let runs = |v: &Vec<bool>, want: bool| -> Vec<(usize, usize)> {
        let mut out = vec![];
        let mut i = 0;
        while i < v.len() {
            if v[i] == want {
                let s = i;
                while i < v.len() && v[i] == want {
                    i += 1;
                }
                out.push((s, i));
            } else {
                i += 1;
            }
        }
        out
    };
    for (s, e) in runs(&voiced, false) {
        if e - s < 6 && s > 0 {
            voiced[s..e].iter_mut().for_each(|x| *x = true);
        }
    }
    for (s, e) in runs(&voiced, true) {
        if e - s < 5 {
            voiced[s..e].iter_mut().for_each(|x| *x = false);
        }
    }
    // new sounds: flux above its local average, scaled by the strong ones
    let onset: Vec<f32> = (0..frames)
        .map(|f| {
            let (a, b) = (f.saturating_sub(10), (f + 10).min(frames - 1));
            (flux[f] - flux[a..=b].iter().sum::<f32>() / (b - a + 1) as f32).max(0.0)
        })
        .collect();
    let mut strong: Vec<f32> = onset.iter().zip(&voiced).filter(|(_, v)| **v).map(|(o, _)| *o).filter(|o| *o > 0.0).collect();
    strong.sort_by(f32::total_cmp);
    let top = strong.get(strong.len() * 95 / 100).copied().unwrap_or(1.0).max(1e-6);
    let mut starts = vec![];
    for f in 1..frames {
        let near_voice = voiced[f] || voiced.get(f + 3).copied().unwrap_or(false);
        let o = (onset[f] / top).min(1.0);
        if voiced[f] && !voiced[f - 1] {
            starts.push((f, 1.0)); // the voice comes in: a word starts here
        } else if near_voice && o > 0.12 && (f.saturating_sub(4)..=(f + 4).min(frames - 1)).all(|g| onset[g] <= onset[f]) {
            starts.push((f, o));
        }
    }
    Some(Voice { voiced, starts })
}

/// Times one line's words. False: no voice found in its time (left as it was).
fn line(l: &mut Line, from: f64, until: f64, voice: &Voice) -> bool {
    if l.words.is_empty() {
        return false;
    }
    let (fa, fb) = (frame(from), frame(until).min(voice.voiced.len().saturating_sub(1)));
    // the frames where the voice is heard in this line
    let heard: Vec<usize> = (fa..=fb).filter(|&f| voice.voiced[f]).collect();
    let (Some(&a), Some(&b)) = (heard.first(), heard.last()) else { return false };
    let (ta, tb) = (time(a), time(b) + HOP as f64 / RATE);
    if tb - ta < 0.2 {
        return false;
    }
    let n = l.words.len();
    let weights: Vec<usize> = l.words.iter().map(|w| w.text.trim().chars().count().max(1)).collect();
    let total: usize = weights.iter().sum();
    let mut cum = 0;
    let expected: Vec<f64> = weights
        .iter()
        .map(|w| {
            let e = time(heard[(heard.len() * cum / total).min(heard.len() - 1)]);
            cum += w;
            e
        })
        .collect();
    let each = (tb - ta) / n as f64;
    let reach = (0.75 * each).max(0.25);
    // the choices per word: a new sound near its expected start, or that start itself
    let options: Vec<Vec<(f64, f64)>> = expected
        .iter()
        .map(|&e| {
            let mut o: Vec<(f64, f64)> = voice
                .starts
                .iter()
                .map(|&(f, s)| (time(f), s as f64))
                .filter(|(t, _)| (t - e).abs() <= reach && *t >= ta - 0.03 && *t < tb)
                .map(|(t, s)| (t, ((t - e) / each).powi(2) - 0.6 * s))
                .collect();
            o.push((e, 0.35));
            o
        })
        .collect();
    // the best path through them, in order (each word after the last by 60 ms)
    let mut best: Vec<Vec<(f64, usize)>> = vec![];
    for (k, opts) in options.iter().enumerate() {
        let row: Vec<(f64, usize)> = opts
            .iter()
            .map(|&(t, c)| {
                if k == 0 {
                    return (c, 0);
                }
                options[k - 1]
                    .iter()
                    .enumerate()
                    .filter(|(_, (pt, _))| t >= pt + 0.06)
                    .map(|(j, _)| (best[k - 1][j].0 + c, j))
                    .min_by(|x, y| x.0.total_cmp(&y.0))
                    .unwrap_or((f64::INFINITY, 0))
            })
            .collect();
        best.push(row);
    }
    let Some((mut j, (cost, _))) = best[n - 1].iter().enumerate().min_by(|x, y| x.1 .0.total_cmp(&y.1 .0)).map(|(j, c)| (j, *c)) else { return false };
    if !cost.is_finite() {
        return false;
    }
    let mut starts = vec![0.0; n];
    for k in (0..n).rev() {
        starts[k] = options[k][j].0;
        j = best[k][j].1;
    }
    for k in 0..n {
        let end = if k + 1 < n { starts[k + 1] - 0.02 } else { tb.min(until) };
        l.words[k] = Word { start: starts[k], end: end.max(starts[k] + 0.05), text: l.words[k].text.clone() };
    }
    true
}

/// Gives every line's words their time from the vocals at `uri` (worker
/// thread). True when most lines found the voice (the words are then the
/// song's own timing, worth keeping).
pub fn words_from_vocals(lines: &mut [Line], uri: &str) -> bool {
    let Some(samples) = beat::decode(uri, 0.0, 3600.0) else { return false };
    let Some(voice) = listen(&samples) else { return false };
    drop(samples);
    let sung = lines.iter().filter(|l| !l.words.is_empty()).count();
    let mut timed = 0;
    for i in 0..lines.len() {
        let from = (lines[i].start - 0.35).max(if i > 0 { lines[i - 1].start + 0.05 } else { 0.0 });
        let until = lines[i].end + 0.15;
        if line(&mut lines[i], from, until, &voice) {
            timed += 1;
        }
    }
    sung > 0 && timed * 2 >= sung
}
