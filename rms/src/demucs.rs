//! Demucs (htdemucs, as ONNX: the model the phone and the app's prototype run)
//! splitting a song into drums, bass, other and vocals, in this process.
//!
//! As the phone's Separate.kt and engine/src/onnx_split.rs: the song in pieces
//! of N frames (7.8 s, fixed when the model was exported), each overlapping the
//! last by a quarter, faded across and blended. As split.py: the song
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
const OVERLAP: usize = N / 4;
const STRIDE: usize = N - OVERLAP;

/// The model on one piece: planar stereo in ([left N][right N]), the four
/// parts out ([part][channel][N]).
pub type Model<'a> = &'a mut dyn FnMut(&[f32]) -> Result<Vec<f32>, String>;

/// The fade: in over the overlap, out over the overlap, never quite 0 (a
/// sample only one piece covers, the song's first or last, keeps its weight).
fn window() -> Vec<f32> {
    (0..N)
        .map(|i| {
            if i < OVERLAP {
                (i + 1) as f32 / (OVERLAP + 1) as f32
            } else if i >= N - OVERLAP {
                (N - i) as f32 / (OVERLAP + 1) as f32
            } else {
                1.0
            }
        })
        .collect()
}

/// The four parts of interleaved stereo `x` (each interleaved, as long as `x`).
pub fn separate(x: &[f32], run: Model, progress: &dyn Fn(f64), stop: &AtomicBool) -> Result<[Vec<f32>; 4], String> {
    let total = x.len() / 2;
    if total == 0 {
        return Err("the song has no sound".into());
    }
    let mono = |i: usize| 0.5 * (x[2 * i] + x[2 * i + 1]) as f64;
    let mean = (0..total).map(mono).sum::<f64>() / total as f64;
    let std = ((0..total).map(|i| (mono(i) - mean).powi(2)).sum::<f64>() / total as f64).sqrt() + 1e-8;
    let (mean, std) = (mean as f32, std as f32);
    let window = window();
    let mut acc: [Vec<f32>; 4] = std::array::from_fn(|_| vec![0.0; 2 * total]);
    let mut weight = vec![0.0f32; total];
    let pieces = if total <= N { 1 } else { 1 + (total - N).div_ceil(STRIDE) };
    let mut piece = vec![0.0f32; 2 * N];
    for c in 0..pieces {
        if stop.load(Ordering::Relaxed) {
            return Err(REMOVED.into());
        }
        let s = c * STRIDE;
        let len = N.min(total - s);
        piece.fill(0.0);
        for i in 0..len {
            piece[i] = (x[2 * (s + i)] - mean) / std;
            piece[N + i] = (x[2 * (s + i) + 1] - mean) / std;
        }
        let y = run(&piece)?;
        if y.len() < 8 * N {
            return Err(format!("the splitting model gave {} values, not {}", y.len(), 8 * N));
        }
        for (p, part) in acc.iter_mut().enumerate() {
            for ch in 0..2 {
                let base = (p * 2 + ch) * N;
                for i in 0..len {
                    part[2 * (s + i) + ch] += y[base + i] * window[i];
                }
            }
        }
        for i in 0..len {
            weight[s + i] += window[i];
        }
        progress((c + 1) as f64 / pieces as f64);
    }
    for part in acc.iter_mut() {
        for i in 0..total {
            let w = weight[i];
            part[2 * i] = part[2 * i] / w * std + mean;
            part[2 * i + 1] = part[2 * i + 1] / w * std + mean;
        }
    }
    Ok(acc)
}

/// Splits `song` into `out` (made if missing): the files' names, vocals and
/// music first. `run`: the model on one piece (see `Model`).
pub fn split(t: &Tools, run: Model, song: &Path, out: &Path, progress: &dyn Fn(f64), stop: &AtomicBool) -> Result<Vec<String>, String> {
    let x = audio::decode(t, song, RATE, 2)?;
    let kind = audio::kind(t, song);
    progress(0.02);
    let parts = separate(&x, run, &|p| progress(0.02 + 0.93 * p), stop)?;
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
            let total = x.len() / 2;
            let want = if total <= N { 1 } else { 1 + (total - N).div_ceil(STRIDE) };
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
