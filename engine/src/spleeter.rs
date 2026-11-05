//! The fast two-part split (#43, owner 2026-11-05: Split asks "Music + Vocals (fast)" or "Four
//! parts"): Spleeter's two-part model (Deezer's 2 stems, as ONNX), the one the phone runs since
//! 0.5.17, inside the app: no Python. As Spleeter does it (ribs02 split-bench/spleeter_ort.py):
//! the song's spectrum (4096-sample frames, 1024 apart, a Hann window); the two models see its
//! loudness in the lower 1024 bins, 512 frames at a time, and each says how much of every bin is
//! the voice and how much the rest; the shares (squared, so they add up to 1) shape the song's own
//! spectrum into Vocals and Music, the higher bins taking each frame's average share. Vocals and
//! Music add up to the song.

use std::path::Path;
use std::sync::OnceLock;

use ort::environment::Environment;
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;
use realfft::num_complex::Complex32;
use realfft::RealFftPlanner;

use crate::audio_io::{decode, write_ogg};

/// The two models' files (in two_part.rs' folder).
pub const VOCALS: &str = "vocals.fp16.onnx";
pub const MUSIC: &str = "accompaniment.fp16.onnx";

/// A frame's length and the step between frames (samples).
pub const N: usize = 4096;
pub const H: usize = 1024;
/// The bins the models see, and the frames they see at once.
pub const F: usize = 1024;
pub const T: usize = 512;
const BINS: usize = N / 2 + 1;
const EPS: f32 = 1e-10;

/// The periodic Hann window.
fn window() -> Vec<f32> {
    (0..N).map(|i| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / N as f64).cos() as f32).collect()
}

/// How many frames a channel of `n` samples has (padded with a frame before and a frame and a
/// step after, as Spleeter pads it).
pub fn frames(n: usize) -> usize {
    1 + (n + 2 * N + H - N) / H
}

/// What the models answer for one block of frames: given the loudness `mag`
/// (2 channels × [T] frames × [F] bins, in that order), the voice's and the rest's (same shape).
pub type Run<'a> = dyn FnMut(&[f32]) -> Result<(Vec<f32>, Vec<f32>), String> + 'a;

/// The song's two channels (each `n` samples) split into (vocals, music), each two channels of
/// `n` samples. `run` is the models; `report` hears how far along it is (0..1).
pub fn separate(x: [&[f32]; 2], run: &mut Run, report: &dyn Fn(f64)) -> Result<([Vec<f32>; 2], [Vec<f32>; 2]), String> {
    let n = x[0].len();
    if x[1].len() != n {
        return Err("the channels differ in length".into());
    }
    let win = window();
    let mut planner = RealFftPlanner::<f32>::new();
    let fwd = planner.plan_fft_forward(N);
    let inv = planner.plan_fft_inverse(N);
    let fr = frames(n);
    let len = N + H * (fr - 1);
    // each channel padded as Spleeter pads it
    let padded: Vec<Vec<f32>> = x.iter().map(|c| {
        let mut p = vec![0f32; n + 2 * N + H];
        p[N..N + n].copy_from_slice(c);
        p
    }).collect();
    let mut vocals = [vec![0f32; len], vec![0f32; len]];
    let mut music = [vec![0f32; len], vec![0f32; len]];
    let mut wsum = vec![0f32; len];
    for t in 0..fr {
        for (i, w) in win.iter().enumerate() {
            wsum[t * H + i] += w * w;
        }
    }
    let blocks = fr.div_ceil(T);
    let mut frame_in = fwd.make_input_vec();
    let mut scratch_out = inv.make_output_vec();
    let mut spec_buf = fwd.make_output_vec();
    for b in 0..blocks {
        // the block's spectrum (frames past the song's end stay silent, as Spleeter pads them)
        let mut spec = vec![Complex32::new(0.0, 0.0); 2 * T * BINS];
        let mut mag = vec![0f32; 2 * T * F];
        for ch in 0..2 {
            for k in 0..T {
                let t = b * T + k;
                if t >= fr {
                    break;
                }
                let start = t * H;
                for i in 0..N {
                    frame_in[i] = padded[ch][start + i] * win[i];
                }
                fwd.process(&mut frame_in, &mut spec_buf).map_err(|e| e.to_string())?;
                let at = (ch * T + k) * BINS;
                spec[at..at + BINS].copy_from_slice(&spec_buf);
                for f in 0..F {
                    mag[(ch * T + k) * F + f] = spec_buf[f].norm();
                }
            }
        }
        let (yv, ya) = run(&mag)?;
        if yv.len() != mag.len() || ya.len() != mag.len() {
            return Err(format!("the model answered {} and {} values, not {}", yv.len(), ya.len(), mag.len()));
        }
        for ch in 0..2 {
            for k in 0..T {
                let t = b * T + k;
                if t >= fr {
                    break;
                }
                let (mv, ma) = shares(&yv[(ch * T + k) * F..][..F], &ya[(ch * T + k) * F..][..F]);
                let at = (ch * T + k) * BINS;
                for (mask, out) in [(&mv, &mut vocals[ch]), (&ma, &mut music[ch])] {
                    let rest = mask.iter().sum::<f32>() / F as f32;
                    for (f, s) in spec_buf.iter_mut().enumerate() {
                        *s = spec[at + f] * if f < F { mask[f] } else { rest };
                    }
                    // (the first and last bins of a real signal's spectrum have no imaginary part)
                    spec_buf[0].im = 0.0;
                    spec_buf[BINS - 1].im = 0.0;
                    inv.process(&mut spec_buf, &mut scratch_out).map_err(|e| e.to_string())?;
                    for i in 0..N {
                        out[t * H + i] += scratch_out[i] / N as f32 * win[i];
                    }
                }
            }
        }
        report((b + 1) as f64 / blocks as f64);
    }
    let cut = |v: &[f32]| -> Vec<f32> { (N..N + n).map(|i| v[i] / wsum[i].max(1e-8)).collect() };
    Ok(([cut(&vocals[0]), cut(&vocals[1])], [cut(&music[0]), cut(&music[1])]))
}

/// ONNX Runtime allows one environment per program: made at the first split, kept.
static ENV: OnceLock<Result<Environment, String>> = OnceLock::new();

fn environment(runtime: &Path) -> Result<&'static Environment, String> {
    ENV.get_or_init(|| ort::init_from(runtime).map_err(|e| format!("ONNX Runtime: {e}"))?.build().map_err(|e| format!("ONNX Runtime: {e}")))
        .as_ref()
        .map_err(Clone::clone)
}

fn session(env: &Environment, model: &Path, threads: usize) -> Result<Session, String> {
    let name = model.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let fail = |e: String| format!("the model {name}: {e}");
    let mut b = Session::builder(env)
        .map_err(|e| fail(e.to_string()))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| fail(e.to_string()))?
        .with_intra_threads(threads)
        .map_err(|e| fail(e.to_string()))?
        .with_inter_threads(1)
        .map_err(|e| fail(e.to_string()))?;
    b.commit_from_file(model).map_err(|e| fail(e.to_string()))
}

/// One model's answer for a block: its output, as the loudness it was given is shaped.
fn answer(session: &mut Session, mag: &[f32]) -> Result<Vec<f32>, String> {
    let input = Tensor::from_array(([2usize, 1, T, F], mag.to_vec())).map_err(|e| e.to_string())?;
    let outputs = session.run(ort::inputs![input]).map_err(|e| e.to_string())?;
    let (_, y) = outputs[0].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
    Ok(y.to_vec())
}

/// Splits `song` into `out` (made if missing) as vocals.ogg and music.ogg; their names.
/// `runtime`: ONNX Runtime's library; `models`: the folder with [VOCALS] and [MUSIC].
pub fn split(runtime: &Path, models: &Path, song: &Path, out: &Path, threads: usize, report: &dyn Fn(f64)) -> Result<Vec<String>, String> {
    let env = environment(runtime)?;
    let mut voice = session(env, &models.join(VOCALS), threads)?;
    let mut rest = session(env, &models.join(MUSIC), threads)?;
    report(0.02);
    let x = decode(song)?;
    let (l, r): (Vec<f32>, Vec<f32>) = x.chunks_exact(2).map(|f| (f[0], f[1])).unzip();
    let mut run = |mag: &[f32]| Ok((answer(&mut voice, mag)?, answer(&mut rest, mag)?));
    let (vocals, music) = separate([&l, &r], &mut run, &|p| report(0.02 + 0.9 * p))?;
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let both = |p: &[Vec<f32>; 2]| -> Vec<f32> { p[0].iter().zip(&p[1]).flat_map(|(a, b)| [*a, *b]).collect() };
    let (vocals, music) = (both(&vocals), both(&music));
    // both at once, each its own encoder
    let names = std::thread::scope(|s| {
        let jobs: Vec<_> = [("vocals", &vocals), ("music", &music)]
            .into_iter()
            .map(|(name, data)| {
                let path = out.join(format!("{name}.ogg"));
                s.spawn(move || write_ogg(&path, data).map(|_| format!("{name}.ogg")))
            })
            .collect();
        jobs.into_iter().map(|j| j.join().map_err(|_| "a writer stopped".to_string()).and_then(|r| r)).collect::<Result<Vec<_>, _>>()
    })?;
    report(1.0);
    Ok(names)
}

/// Each bin's share of the voice and of the rest, from the models' two answers: squared, so
/// the two shares add up to 1.
pub fn shares(yv: &[f32], ya: &[f32]) -> (Vec<f32>, Vec<f32>) {
    yv.iter()
        .zip(ya)
        .map(|(v, a)| {
            let den = v * v + a * a + EPS;
            ((v * v + EPS / 2.0) / den, (a * a + EPS / 2.0) / den)
        })
        .unzip()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_io::RATE;

    /// How close `b` is to `a`, in dB (higher is closer).
    fn closeness(a: &[f32], b: &[f32]) -> f64 {
        let s: f64 = a.iter().map(|v| (*v as f64).powi(2)).sum();
        let e: f64 = a.iter().zip(b).map(|(x, y)| (*x as f64 - *y as f64).powi(2)).sum();
        10.0 * (s / e.max(1e-30)).log10()
    }

    /// Some music-like sound: a few tones and a little noise, 3.5 s (a block and a bit).
    fn song(n: usize, seed: u32) -> Vec<f32> {
        let mut r = seed;
        (0..n)
            .map(|i| {
                r = r.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let t = i as f32 / RATE as f32;
                0.3 * (2.0 * std::f32::consts::PI * 220.0 * t).sin() + 0.2 * (2.0 * std::f32::consts::PI * 1310.0 * t).sin() + 0.05 * ((r >> 8) as f32 / (1u32 << 24) as f32 - 0.5)
            })
            .collect()
    }

    #[test]
    fn the_shares_add_up_to_one() {
        let (v, a) = shares(&[0.0, 1.0, 3.0, 0.2], &[0.0, 1.0, 1.0, 5.0]);
        for (x, y) in v.iter().zip(&a) {
            assert!((x + y - 1.0).abs() < 1e-6, "{x} + {y}");
        }
        assert!((v[1] - 0.5).abs() < 1e-6);
        assert!((v[2] - 0.9).abs() < 1e-6, "3² / (3² + 1²)");
    }

    #[test]
    fn vocals_and_music_add_up_to_the_song() {
        let n = RATE * 7 / 2; // more than one block of 512 frames
        let (l, r) = (song(n, 1), song(n, 2));
        // a "model" that hears the voice in the low bins and the rest above
        let mut run = |mag: &[f32]| -> Result<(Vec<f32>, Vec<f32>), String> {
            let v = mag.iter().enumerate().map(|(i, m)| if i % F < 100 { *m } else { 0.1 * m }).collect();
            let a = mag.iter().enumerate().map(|(i, m)| if i % F < 100 { 0.2 * m } else { *m }).collect();
            Ok((v, a))
        };
        let (vocals, music) = separate([&l, &r], &mut run, &|_| {}).unwrap();
        for ch in 0..2 {
            assert_eq!(vocals[ch].len(), n);
            let sum: Vec<f32> = vocals[ch].iter().zip(&music[ch]).map(|(a, b)| a + b).collect();
            let x = if ch == 0 { &l } else { &r };
            let db = closeness(x, &sum);
            assert!(db > 60.0, "channel {ch}: the parts are {db:.1} dB from the song");
        }
        // and they're really split: the 220 Hz tone (low bins) is mostly in Vocals
        let low = |v: &[f32]| v.iter().map(|s| (*s as f64).powi(2)).sum::<f64>();
        assert!(low(&vocals[0]) > low(&music[0]), "the low tone went to the voice");
    }

    #[test]
    fn all_voice_gives_the_song_back_as_vocals() {
        let n = RATE;
        let x = song(n, 3);
        let mut run = |mag: &[f32]| -> Result<(Vec<f32>, Vec<f32>), String> { Ok((mag.to_vec(), vec![0.0; mag.len()])) };
        let last = std::cell::Cell::new(0.0);
        let (vocals, music) = separate([&x, &x], &mut run, &|p| last.set(p)).unwrap_or_else(|e| panic!("{e}"));
        assert!(closeness(&x, &vocals[1]) > 60.0);
        assert!(music[1].iter().all(|s| s.abs() < 1e-3));
        assert_eq!(last.get(), 1.0, "it says when it's done");
    }

    /// The real models: run with RADIANCE_SPLEETER_CHECK=DIR holding libonnxruntime.so (or
    /// onnxruntime.dll) and the two models (two_part.rs' folder will do). The parts must add up to
    /// the song; with clip.wav (44.1 kHz stereo) and the Python reference's vocals.f32 / music.f32
    /// (ribs02 split-bench/spleeter_ort.py, interleaved) there too, they must match it.
    #[test]
    #[ignore]
    fn the_real_models_split_a_song() {
        let Some(dir) = std::env::var_os("RADIANCE_SPLEETER_CHECK").map(std::path::PathBuf::from) else { return };
        let runtime = ["libonnxruntime.so", "onnxruntime.dll"].iter().map(|n| dir.join(n)).find(|p| p.exists()).expect("the runtime");
        let env = environment(&runtime).unwrap();
        let (mut voice, mut rest) = (session(env, &dir.join(VOCALS), 4).unwrap(), session(env, &dir.join(MUSIC), 4).unwrap());
        let (l, r): (Vec<f32>, Vec<f32>) = if dir.join("clip.wav").exists() {
            decode(&dir.join("clip.wav")).unwrap().chunks_exact(2).map(|f| (f[0], f[1])).unzip()
        } else {
            (song(RATE * 20, 4), song(RATE * 20, 5))
        };
        let started = std::time::Instant::now();
        let mut run = |mag: &[f32]| Ok((answer(&mut voice, mag)?, answer(&mut rest, mag)?));
        let (vocals, music) = separate([&l, &r], &mut run, &|_| {}).unwrap();
        let took = started.elapsed().as_secs_f64();
        let both = |p: &[Vec<f32>; 2]| -> Vec<f32> { p[0].iter().zip(&p[1]).flat_map(|(a, b)| [*a, *b]).collect() };
        let (v, m) = (both(&vocals), both(&music));
        let x: Vec<f32> = l.iter().zip(&r).flat_map(|(a, b)| [*a, *b]).collect();
        let whole = closeness(&x, &v.iter().zip(&m).map(|(a, b)| a + b).collect::<Vec<f32>>());
        eprintln!("{took:.1} s for {:.0} s; parts vs song {whole:.1} dB", l.len() as f64 / RATE as f64);
        assert!(whole > 40.0);
        if dir.join("vocals.f32").exists() {
            let read = |name: &str| -> Vec<f32> { std::fs::read(dir.join(name)).unwrap().chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect() };
            let (rv, rm) = (read("vocals.f32"), read("music.f32"));
            assert_eq!(v.len(), rv.len(), "as long as the reference");
            let (dv, dm) = (closeness(&rv, &v), closeness(&rm, &m));
            eprintln!("vocals {dv:.1} dB, music {dm:.1} dB from the reference");
            assert!(dv > 30.0 && dm > 30.0, "vocals {dv:.1} dB, music {dm:.1} dB");
        }
    }

    #[test]
    fn a_wrong_answer_from_the_model_is_an_error() {
        let x = vec![0.0; RATE];
        let mut run = |_: &[f32]| -> Result<(Vec<f32>, Vec<f32>), String> { Ok((vec![0.0; 3], vec![0.0; 3])) };
        assert!(separate([&x, &x], &mut run, &|_| {}).is_err());
    }
}
