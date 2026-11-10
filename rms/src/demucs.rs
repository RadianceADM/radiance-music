//! Demucs (htdemucs, as ONNX: the model the phone and the app's prototype run)
//! splitting a song into drums, bass, other and vocals, in this process.
//!
//! As Demucs's own apply_model, the way split.py calls it (checked against it:
//! tests/fixtures/demucs): the song in pieces of N frames (7.8 s, fixed when the
//! model was exported), a new one every three quarters of a piece, blended by a
//! triangle (owner, 2026-11-09: "Blend like Python"). As split.py: the song
//! normalised first (its mono mix's mean and spread), then the parts written
//! (vocals, music = all but the vocals, drums, bass, other), all at once.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::audio::{self, Tools};
use crate::engine::REMOVED;

/// The model's rate and its parts, in its output order.
pub const RATE: u32 = 44_100;
pub const PARTS: [&str; 4] = ["drums", "bass", "other", "vocals"];
/// A piece's frames (the exported model's fixed length), and its overlap.
pub const N: usize = 343_980;
/// How far apart pieces start (Demucs: int(0.75 * N)).
#[cfg(test)]
const STRIDE: usize = 257_985;
/// The longest song the server splits: the splitting runs in the server's own memory (a 20-minute
/// song's samples take about 2.7 GB while it works), and a song that takes it down takes everyone's down.
pub const MOST_MINUTES: usize = 20;

/// Why a song of `frames` (at RATE) is too long to split, if it is.
pub fn too_long(frames: usize) -> Option<String> {
    let minutes = frames / RATE as usize / 60;
    (frames > MOST_MINUTES * 60 * RATE as usize).then(|| format!("it's {minutes} minutes long, and the server splits songs up to {MOST_MINUTES} minutes"))
}

/// The model on one piece: planar stereo in ([left N][right N]), the four
/// parts out ([part][channel][N]).
pub type Model<'a> = &'a mut dyn FnMut(&[f32]) -> Result<Vec<f32>, String>;

/// Demucs's weights for a piece of `n` frames (apply_model): a triangle, highest in the middle, never 0
/// (1, 2, … n/2, … 2, 1, over its highest), so each frame's parts are mostly from the piece it's
/// nearest the middle of.
fn triangle(n: usize) -> Vec<f32> {
    let half = n / 2;
    let w: Vec<f32> = (1..=half).chain((1..=n - half).rev()).map(|v| v as f32).collect();
    let most = w.iter().copied().fold(0.0f32, f32::max);
    w.into_iter().map(|v| v / most).collect()
}

/// The four parts of interleaved stereo `x` (each interleaved, as long as `x`).
pub fn separate(x: &[f32], run: Model, progress: &dyn Fn(f64), stop: &AtomicBool) -> Result<[Vec<f32>; 4], String> {
    separate_n(x, N, run, progress, stop)
}

/// `separate` with pieces of `n` frames (the model's own: N; the tests' small ones), as Demucs's
/// apply_model does it for split.py (split, overlap 0.25; its random shift left out): the song
/// normalised by its mono mix (mean, and spread over n - 1 as torch's std); a piece every 3/4 of a
/// piece to the song's end; a piece shorter than `n` (the song's last, or a short song) widened
/// around its middle with the song on either side (silence past its ends), its parts trimmed back;
/// the pieces blended by the triangle.
pub fn separate_n(x: &[f32], n: usize, run: Model, progress: &dyn Fn(f64), stop: &AtomicBool) -> Result<[Vec<f32>; 4], String> {
    let total = x.len() / 2;
    if total == 0 {
        return Err("the song has no sound".into());
    }
    let mono = |i: usize| 0.5 * (x[2 * i] + x[2 * i + 1]) as f64;
    let mean = (0..total).map(mono).sum::<f64>() / total as f64;
    let spread = (0..total).map(|i| (mono(i) - mean).powi(2)).sum::<f64>() / (total.max(2) - 1) as f64;
    let (mean, std) = (mean as f32, spread.sqrt() as f32 + 1e-8);
    let norm = |i: isize, ch: usize| if i >= 0 && (i as usize) < total { (x[2 * i as usize + ch] - mean) / std } else { 0.0 };
    let weight = triangle(n);
    let stride = (0.75 * n as f64) as usize;
    let starts: Vec<usize> = (0..total).step_by(stride.max(1)).collect();
    let mut acc: [Vec<f32>; 4] = std::array::from_fn(|_| vec![0.0; 2 * total]);
    let mut sum = vec![0.0f32; total];
    let mut piece = vec![0.0f32; 2 * n];
    for (c, &s) in starts.iter().enumerate() {
        if stop.load(Ordering::Relaxed) {
            return Err(REMOVED.into());
        }
        let len = n.min(total - s);
        // widened around its middle to the model's length (apply_model's TensorChunk.padded)
        let pad = (n - len) / 2;
        let from = s as isize - pad as isize;
        for k in 0..n {
            piece[k] = norm(from + k as isize, 0);
            piece[n + k] = norm(from + k as isize, 1);
        }
        let y = run(&piece)?;
        if y.len() < 8 * n {
            return Err(format!("the splitting model gave {} values, not {}", y.len(), 8 * n));
        }
        // its parts trimmed back around the middle (center_trim), weighted from the triangle's start
        for (p, part) in acc.iter_mut().enumerate() {
            for ch in 0..2 {
                let base = (p * 2 + ch) * n + pad;
                for i in 0..len {
                    part[2 * (s + i) + ch] += weight[i] * y[base + i];
                }
            }
        }
        for i in 0..len {
            sum[s + i] += weight[i];
        }
        progress((c + 1) as f64 / starts.len() as f64);
    }
    for part in acc.iter_mut() {
        for i in 0..total {
            part[2 * i] = part[2 * i] / sum[i] * std + mean;
            part[2 * i + 1] = part[2 * i + 1] / sum[i] * std + mean;
        }
    }
    Ok(acc)
}

/// Splits `song` into `out` (made if missing): the files' names, vocals and
/// music first. `run`: the model on one piece (see `Model`).
pub fn split(t: &Tools, run: Model, song: &Path, out: &Path, progress: &dyn Fn(f64), stop: &AtomicBool) -> Result<Vec<String>, String> {
    let x = audio::decode(t, song, RATE, 2)?;
    if let Some(why) = too_long(x.len() / 2) {
        return Err(why);
    }
    let kind = audio::kind(t, song);
    progress(0.02);
    let parts = separate(&x, run, &|p| progress(0.02 + 0.93 * p), stop)?;
    drop(x);
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let vocals = &parts[3];
    let music: Vec<f32> = (0..vocals.len()).map(|i| parts[0][i] + parts[1][i] + parts[2][i]).collect();
    let mut files: Vec<(&str, &[f32])> = vec![("vocals", vocals), ("music", &music)];
    files.extend(PARTS.iter().zip(parts.iter()).filter(|(n, _)| **n != "vocals").map(|(n, p)| (*n, p.as_slice())));
    // all at once, each its own encoder (split.py: five files in 0.8 s, not 3.6)
    let names = std::thread::scope(|s| {
        let jobs: Vec<_> = files.iter().map(|(name, data)| s.spawn(move || audio::write_part(t, &out.join(name), data, RATE, kind))).collect();
        jobs.into_iter().map(|j| j.join().map_err(|_| "a writer stopped".to_string()).and_then(|r| r)).collect::<Result<Vec<_>, _>>()
    })?;
    progress(1.0);
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A model that gives each part the piece itself (the blend must give the song back).
    fn identity(piece: &[f32]) -> Result<Vec<f32>, String> {
        Ok((0..4).flat_map(|_| piece.iter().copied()).collect())
    }

    fn song(secs: f64) -> Vec<f32> {
        let n = (secs * RATE as f64) as usize;
        (0..n).flat_map(|i| {
            let t = i as f32 / RATE as f32;
            [0.5 * (2.0 * std::f32::consts::PI * 220.0 * t).sin() + 0.05, 0.25 * (2.0 * std::f32::consts::PI * 331.0 * t).cos() - 0.02]
        }).collect()
    }

    /// The reference's stand-in model (gen_demucs.py's Ramp): each part is its piece times a ramp along the
    /// piece, times the part's number, so where pieces start, how a short one is padded, and how they're
    /// weighted all show.
    fn ramp(n: usize) -> impl FnMut(&[f32]) -> Result<Vec<f32>, String> {
        move |piece: &[f32]| {
            let mut y = vec![0.0f32; 8 * n];
            for p in 0..4 {
                for ch in 0..2 {
                    for k in 0..n {
                        let r = 1.0f32 + k as f32 / n as f32;
                        y[(p * 2 + ch) * n + k] = piece[ch * n + k] * r * (p + 1) as f32;
                    }
                }
            }
            Ok(y)
        }
    }

    #[test]
    fn the_blend_is_demucs_apply_models() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/demucs/apply_model.json");
        let fixture: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let n = fixture["n"].as_u64().unwrap() as usize;
        let floats = |v: &serde_json::Value| v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap() as f32).collect::<Vec<f32>>();
        for case in fixture["cases"].as_array().unwrap() {
            let x = floats(&case["x"]);
            let parts = separate_n(&x, n, &mut ramp(n), &|_| {}, &AtomicBool::new(false)).unwrap();
            for (p, want) in case["parts"].as_array().unwrap().iter().enumerate() {
                let want = floats(want);
                assert_eq!(parts[p].len(), want.len());
                let worst = parts[p].iter().zip(&want).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
                assert!(worst < 2e-5, "{} frames, part {p}: {worst}", case["total"]);
            }
        }
    }

    #[test]
    fn a_model_that_changes_nothing_gives_the_song_back() {
        // shorter than one piece, two pieces, four
        for secs in [3.0, 10.0, 20.0] {
            let x = song(secs);
            let calls = Mutex::new(0);
            let parts = separate(&x, &mut |p| {
                *calls.lock().unwrap() += 1;
                identity(p)
            }, &|_| {}, &AtomicBool::new(false))
            .unwrap();
            // as Demucs: a piece every STRIDE frames, to the song's end
            let want = (x.len() / 2).div_ceil(STRIDE);
            assert_eq!(*calls.lock().unwrap(), want, "{secs} s");
            for part in &parts {
                assert_eq!(part.len(), x.len());
                let worst = x.iter().zip(part).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
                assert!(worst < 1e-5, "{secs} s: {worst}");
            }
        }
    }

    #[test]
    fn taken_out_of_the_queue_it_stops_before_the_next_piece() {
        let x = song(20.0);
        let stop = AtomicBool::new(false);
        let calls = Mutex::new(0);
        let r = separate(&x, &mut |p| {
            *calls.lock().unwrap() += 1;
            stop.store(true, Ordering::Relaxed);
            identity(p)
        }, &|_| {}, &stop);
        assert_eq!(r.unwrap_err(), REMOVED);
        assert_eq!(*calls.lock().unwrap(), 1);
    }

    #[test]
    fn songs_past_twenty_minutes_are_too_long() {
        assert!(too_long(19 * 60 * RATE as usize).is_none());
        assert!(too_long(20 * 60 * RATE as usize).is_none());
        let why = too_long(61 * 60 * RATE as usize).unwrap();
        assert_eq!(why, "it's 61 minutes long, and the server splits songs up to 20 minutes");
    }

    #[test]
    fn progress_rises_to_the_end() {
        let seen = Mutex::new(Vec::new());
        separate(&song(20.0), &mut identity, &|p| seen.lock().unwrap().push(p), &AtomicBool::new(false)).unwrap();
        let seen = seen.into_inner().unwrap();
        assert!(seen.windows(2).all(|w| w[1] > w[0]), "{seen:?}");
        assert_eq!(*seen.last().unwrap(), 1.0);
    }

    #[test]
    fn a_model_that_gives_too_little_says_so() {
        let e = separate(&song(1.0), &mut |_| Ok(vec![0.0; 10]), &|_| {}, &AtomicBool::new(false)).unwrap_err();
        assert!(e.contains("the splitting model gave 10 values"), "{e}");
        assert_eq!(separate(&[], &mut identity, &|_| {}, &AtomicBool::new(false)).unwrap_err(), "the song has no sound");
    }

    #[test]
    fn a_song_becomes_five_files() {
        let t = audio::tools(Path::new("/nowhere")).expect("ffmpeg (ribs02)");
        let d = std::env::temp_dir().join(format!("rms-demucs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let src = d.join("Dee - Four.flac");
        let ok = std::process::Command::new("ffmpeg").args(["-v", "error", "-f", "lavfi", "-i", "sine=frequency=440:duration=2:sample_rate=44100", "-ac", "2"]).arg(&src).status().unwrap();
        assert!(ok.success());
        let names = split(&t, &mut identity, &src, &d.join("out"), &|_| {}, &AtomicBool::new(false)).unwrap();
        assert_eq!(names, ["vocals.flac", "music.flac", "drums.flac", "bass.flac", "other.flac"]);
        let x = audio::decode(&t, &src, RATE, 2).unwrap();
        let v = audio::decode(&t, &d.join("out/vocals.flac"), RATE, 2).unwrap();
        assert_eq!(v.len(), x.len());
        assert!(x.iter().zip(&v).all(|(a, b)| (a - b).abs() < 1e-3));
    }
}
