//! The fast two-part split (#43, owner 2026-11-05: Split asks "Music + Vocals (fast)" or "Four
//! parts"): Spleeter's two-part model (Deezer's 2 stems, as ONNX), the one the phone runs since
//! 0.5.17, inside the app: no Python. As Spleeter does it (ribs02 split-bench/spleeter_ort.py):
//! the song's spectrum (4096-sample frames, 1024 apart, a Hann window); the two models see its
//! loudness in the lower 1024 bins, 512 frames at a time, and each says how much of every bin is
//! the voice and how much the rest; the shares (squared, so they add up to 1) shape the song's own
//! spectrum into Vocals and Music, the higher bins taking each frame's average share. Vocals and
//! Music add up to the song. Block by block from reading to writing: a song is never held whole
//! (a mix can be an hour; the app plays music meanwhile).

use std::path::Path;
use std::sync::Mutex;

use ort::environment::Environment;
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;
use realfft::num_complex::Complex32;
use realfft::RealFftPlanner;

use crate::audio_io::{OggWriter, Reader, RATE};

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
/// The song's next piece (interleaved stereo); empty at its end.
pub type Feed<'a> = dyn FnMut() -> Result<Vec<f32>, String> + 'a;
/// The parts as they're ready, in order: (vocals, music), interleaved stereo, the same length.
pub type Emit<'a> = dyn FnMut(&[f32], &[f32]) -> Result<(), String> + 'a;

/// Splits the song `feed` gives into Vocals and Music, given to `emit` as they're ready; `run`
/// is the models; `report` hears how many samples (each channel) are done. The song's length.
pub fn separate(feed: &mut Feed, run: &mut Run, emit: &mut Emit, report: &dyn Fn(usize)) -> Result<usize, String> {
    let win = window();
    let mut planner = RealFftPlanner::<f32>::new();
    let fwd = planner.plan_fft_forward(N);
    let inv = planner.plan_fft_inverse(N);
    let mut frame_in = fwd.make_input_vec();
    let mut frame_out = inv.make_output_vec();
    let mut bins = fwd.make_output_vec();
    // the song as it comes: each channel from song sample `base` on; its length once it ended
    let mut song: [Vec<f32>; 2] = [Vec::new(), Vec::new()];
    let mut base = 0usize;
    let mut ended: Option<usize> = None;
    // the parts' sums ([vocals, music][channel]) from padded position `out_base` on (padded = song + N)
    let mut acc: [[Vec<f32>; 2]; 2] = Default::default();
    let mut out_base = 0usize;
    let mut emitted = 0usize;
    let mut spec = vec![Complex32::new(0.0, 0.0); 2 * T * BINS];
    let mut mag = vec![0f32; 2 * T * F];
    for b in 0.. {
        let t0 = b * T;
        // the block's last frame reads the song up to (its start) samples in
        let need = (t0 + T - 1) * H;
        while ended.is_none() && base + song[0].len() < need {
            let piece = feed()?;
            if piece.is_empty() {
                ended = Some(base + song[0].len());
                break;
            }
            for f in piece.chunks_exact(2) {
                song[0].push(f[0]);
                song[1].push(f[1]);
            }
        }
        let total = ended.map(frames);
        if total.is_some_and(|fr| t0 >= fr) {
            break;
        }
        let count = total.map_or(T, |fr| (fr - t0).min(T));
        // the block's spectrum and loudness (frames past the song's end stay silent, as Spleeter pads them)
        spec.iter_mut().for_each(|c| *c = Complex32::new(0.0, 0.0));
        mag.iter_mut().for_each(|m| *m = 0.0);
        for ch in 0..2 {
            for k in 0..count {
                let start = (t0 + k) * H; // padded; the song's sample i is padded i + N
                for (i, x) in frame_in.iter_mut().enumerate() {
                    let at = (start + i).checked_sub(N).filter(|s| *s >= base && *s < base + song[ch].len());
                    *x = at.map_or(0.0, |s| song[ch][s - base]) * win[i];
                }
                fwd.process(&mut frame_in, &mut bins).map_err(|e| e.to_string())?;
                let at = (ch * T + k) * BINS;
                spec[at..at + BINS].copy_from_slice(&bins);
                for f in 0..F {
                    mag[(ch * T + k) * F + f] = bins[f].norm();
                }
            }
        }
        let (yv, ya) = run(&mag)?;
        if yv.len() != mag.len() || ya.len() != mag.len() {
            return Err(format!("the model answered {} and {} values, not {}", yv.len(), ya.len(), mag.len()));
        }
        // each frame's two parts, back into sound, added up where the frames overlap
        let end = (t0 + count - 1) * H + N - out_base;
        for part in acc.iter_mut() {
            for c in part.iter_mut() {
                if c.len() < end {
                    c.resize(end, 0.0);
                }
            }
        }
        for ch in 0..2 {
            for k in 0..count {
                let (mv, ma) = shares(&yv[(ch * T + k) * F..][..F], &ya[(ch * T + k) * F..][..F]);
                let at = (ch * T + k) * BINS;
                let from = (t0 + k) * H - out_base;
                for (part, mask) in [&mv, &ma].into_iter().enumerate() {
                    let rest = mask.iter().sum::<f32>() / F as f32;
                    for (f, s) in bins.iter_mut().enumerate() {
                        *s = spec[at + f] * if f < F { mask[f] } else { rest };
                    }
                    // (the first and last bins of a real signal's spectrum have no imaginary part)
                    bins[0].im = 0.0;
                    bins[BINS - 1].im = 0.0;
                    inv.process(&mut bins, &mut frame_out).map_err(|e| e.to_string())?;
                    let out = &mut acc[part][ch][from..from + N];
                    for i in 0..N {
                        out[i] += frame_out[i] / N as f32 * win[i];
                    }
                }
            }
        }
        // what no later frame touches is done: given out, then let go of
        let last = total.is_some_and(|fr| t0 + count >= fr);
        let done = if last { (t0 + count - 1) * H + N } else { (t0 + count) * H };
        let upto = ended.map_or(done - N, |n| (done - N).min(n));
        if upto > emitted {
            let (mut v, mut m) = (Vec::with_capacity(2 * (upto - emitted)), Vec::with_capacity(2 * (upto - emitted)));
            for i in emitted..upto {
                let p = i + N;
                // the window's square, summed over the frames that cover this sample
                let lo = (p + 1).saturating_sub(N).div_ceil(H);
                let hi = (p / H).min(t0 + count - 1);
                let w: f32 = (lo..=hi).map(|t| win[p - t * H] * win[p - t * H]).sum::<f32>().max(1e-8);
                for ch in 0..2 {
                    v.push(acc[0][ch][p - out_base] / w);
                    m.push(acc[1][ch][p - out_base] / w);
                }
            }
            emit(&v, &m)?;
            let gone = upto + N - out_base;
            for part in acc.iter_mut() {
                for c in part.iter_mut() {
                    c.drain(..gone.min(c.len()));
                }
            }
            out_base += gone;
            emitted = upto;
        }
        // the song before the next block's first frame is let go of
        let keep = ((t0 + T) * H).saturating_sub(N);
        if keep > base {
            let gone = (keep - base).min(song[0].len());
            for c in song.iter_mut() {
                c.drain(..gone);
            }
            base += gone;
        }
        report(emitted);
        if last {
            break;
        }
    }
    Ok(ended.unwrap_or(emitted))
}

/// ONNX Runtime allows one environment per program: made at the first split that works, kept.
static ENV: Mutex<Option<&'static Environment>> = Mutex::new(None);

fn environment(runtime: &Path) -> Result<&'static Environment, String> {
    let mut env = ENV.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(e) = *env {
        return Ok(e);
    }
    #[cfg(windows)]
    load_c_runtime(runtime.parent().unwrap_or(Path::new(".")))?;
    let made = ort::init_from(runtime).map_err(|e| format!("ONNX Runtime: {e}"))?.build().map_err(|e| format!("ONNX Runtime: {e}"))?;
    let made: &'static Environment = Box::leak(Box::new(made));
    *env = Some(made);
    Ok(made)
}

/// Windows: the Visual C++ runtime ONNX Runtime needs, from its folder (two_part.rs fetched it),
/// before ONNX Runtime: a DLL's needs are found by name among what's loaded already, so a PC
/// without the Visual C++ runtime installed works too.
#[cfg(windows)]
fn load_c_runtime(dir: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::LibraryLoader::{LoadLibraryExW, LOAD_WITH_ALTERED_SEARCH_PATH};
    for name in crate::two_part::C_RUNTIME {
        let path: Vec<u16> = dir.join(name).as_os_str().encode_wide().chain([0]).collect();
        // kept loaded for the program's life (ONNX Runtime uses it from now on)
        let module = unsafe { LoadLibraryExW(path.as_ptr(), std::ptr::null_mut(), LOAD_WITH_ALTERED_SEARCH_PATH) };
        if module.is_null() {
            return Err(format!("the Visual C++ runtime ({name}) couldn't be loaded: {}", std::io::Error::last_os_error()));
        }
    }
    Ok(())
}

fn session(env: &Environment, model: &Path, threads: usize) -> Result<Session, String> {
    let name = model.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let fail = |e: String| format!("the model {name}: {e}");
    // (no spinning: while one model runs, the other's threads sleep)
    let mut b = Session::builder(env)
        .map_err(|e| fail(e.to_string()))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| fail(e.to_string()))?
        .with_intra_threads(threads)
        .map_err(|e| fail(e.to_string()))?
        .with_inter_threads(1)
        .map_err(|e| fail(e.to_string()))?
        .with_intra_op_spinning(false)
        .map_err(|e| fail(e.to_string()))?
        .with_memory_pattern(false)
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
/// `runtime`: ONNX Runtime's library; `models`: the folder with [VOCALS] and [MUSIC]. When ONNX
/// Runtime or a model won't load, its files are checked again (two_part::check_again): a damaged
/// one goes, and comes again before the next split.
pub fn split(runtime: &Path, models: &Path, song: &Path, out: &Path, threads: usize, report: &dyn Fn(f64)) -> Result<Vec<String>, String> {
    let loaded = environment(runtime).and_then(|env| Ok((session(env, &models.join(VOCALS), threads)?, session(env, &models.join(MUSIC), threads)?)));
    let (mut voice, mut rest) = loaded.inspect_err(|_| crate::two_part::check_again())?;
    report(0.02);
    let mut reader = Reader::open(song)?;
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let mut vocals = OggWriter::create(&out.join("vocals.ogg"))?;
    let mut music = OggWriter::create(&out.join("music.ogg"))?;
    let samples = {
        let length = std::cell::Cell::new(None::<f64>);
        let mut feed = || {
            let piece = reader.next();
            if length.get().is_none() {
                length.set(reader.seconds().map(|s| s * RATE as f64));
            }
            piece
        };
        let mut run = |mag: &[f32]| Ok((answer(&mut voice, mag)?, answer(&mut rest, mag)?));
        let mut emit = |v: &[f32], m: &[f32]| {
            vocals.push(v)?;
            music.push(m)
        };
        separate(&mut feed, &mut run, &mut emit, &|done| {
            if let Some(all) = length.get().filter(|a| *a > 0.0) {
                report(0.02 + 0.95 * (done as f64 / all).min(1.0));
            }
        })?
    };
    if samples < RATE {
        return Err("the song is shorter than a second".into());
    }
    vocals.finish()?;
    music.finish()?;
    report(1.0);
    Ok(vec!["vocals.ogg".into(), "music.ogg".into()])
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

    /// Some music-like sound: a few tones and a little noise.
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

    /// The song fed `chunk` frames at a time; the parts as they came (each channel), how many
    /// times they came, and how much of the song had been read when they first came.
    struct Got {
        vocals: [Vec<f32>; 2],
        music: [Vec<f32>; 2],
        emits: usize,
        read_at_first: usize,
    }

    fn split_fed(l: &[f32], r: &[f32], chunk: usize, run: &mut Run) -> Result<Got, String> {
        let x: Vec<f32> = l.iter().zip(r).flat_map(|(a, b)| [*a, *b]).collect();
        let read = std::cell::Cell::new(0usize);
        let mut feed = || -> Result<Vec<f32>, String> {
            let at = read.get();
            let end = (at + 2 * chunk).min(x.len());
            read.set(end);
            Ok(x[at..end].to_vec())
        };
        let mut got = Got { vocals: [vec![], vec![]], music: [vec![], vec![]], emits: 0, read_at_first: 0 };
        let mut emit = |v: &[f32], m: &[f32]| -> Result<(), String> {
            assert_eq!(v.len(), m.len());
            if got.emits == 0 {
                got.read_at_first = read.get() / 2;
            }
            got.emits += 1;
            for (i, s) in v.iter().enumerate() {
                got.vocals[i % 2].push(*s);
            }
            for (i, s) in m.iter().enumerate() {
                got.music[i % 2].push(*s);
            }
            Ok(())
        };
        let n = separate(&mut feed, run, &mut emit, &|_| {})?;
        assert_eq!(n, l.len());
        Ok(got)
    }

    /// A "model" that hears the voice in the low bins and the rest above.
    fn low_is_voice(mag: &[f32]) -> Result<(Vec<f32>, Vec<f32>), String> {
        let v = mag.iter().enumerate().map(|(i, m)| if i % F < 100 { *m } else { 0.1 * m }).collect();
        let a = mag.iter().enumerate().map(|(i, m)| if i % F < 100 { 0.2 * m } else { *m }).collect();
        Ok((v, a))
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
        // whatever pieces the song comes in
        for chunk in [4096, 777, 100_000] {
            let got = split_fed(&l, &r, chunk, &mut low_is_voice).unwrap();
            for ch in 0..2 {
                assert_eq!(got.vocals[ch].len(), n);
                let sum: Vec<f32> = got.vocals[ch].iter().zip(&got.music[ch]).map(|(a, b)| a + b).collect();
                let x = if ch == 0 { &l } else { &r };
                let db = closeness(x, &sum);
                assert!(db > 60.0, "chunk {chunk}, channel {ch}: the parts are {db:.1} dB from the song");
            }
            // and they're really split: the 220 Hz tone (low bins) is mostly in Vocals
            let power = |v: &[f32]| v.iter().map(|s| (*s as f64).powi(2)).sum::<f64>();
            assert!(power(&got.vocals[0]) > power(&got.music[0]), "the low tone went to the voice");
        }
    }

    /// A long song is never held whole (owner's 14 GB laptop; a mix can be an hour): the parts
    /// come out block by block while the song is still being read.
    #[test]
    fn the_parts_come_out_while_the_song_is_still_being_read() {
        let n = RATE * 40;
        let (l, r) = (song(n, 6), song(n, 7));
        let got = split_fed(&l, &r, 8192, &mut low_is_voice).unwrap();
        assert!(got.emits >= 3, "came {} times", got.emits);
        assert!(got.read_at_first < n / 2, "the first parts came after {} of {n} samples were read", got.read_at_first);
    }

    #[test]
    fn all_voice_gives_the_song_back_as_vocals() {
        let x = song(RATE, 3);
        let mut run = |mag: &[f32]| -> Result<(Vec<f32>, Vec<f32>), String> { Ok((mag.to_vec(), vec![0.0; mag.len()])) };
        let got = split_fed(&x, &x, 5000, &mut run).unwrap();
        assert!(closeness(&x, &got.vocals[1]) > 60.0);
        assert!(got.music[1].iter().all(|s| s.abs() < 1e-3));
    }

    #[test]
    fn a_wrong_answer_from_the_model_is_an_error() {
        let x = vec![0.0; RATE];
        let mut run = |_: &[f32]| -> Result<(Vec<f32>, Vec<f32>), String> { Ok((vec![0.0; 3], vec![0.0; 3])) };
        assert!(split_fed(&x, &x, 4096, &mut run).is_err());
    }

    /// A whole song through the real split (reading, both models, both Ogg files), for measuring
    /// it: RADIANCE_SPLEETER_CHECK=DIR as below and RADIANCE_SPLEETER_SONG=FILE.
    #[test]
    #[ignore]
    fn the_real_split_of_a_whole_song() {
        let (Some(dir), Some(song)) = (std::env::var_os("RADIANCE_SPLEETER_CHECK"), std::env::var_os("RADIANCE_SPLEETER_SONG")) else { return };
        let dir = std::path::PathBuf::from(dir);
        let runtime = ["libonnxruntime.so", "onnxruntime.dll"].iter().map(|n| dir.join(n)).find(|p| p.exists()).expect("the runtime");
        let out = std::env::temp_dir().join(format!("radiance-spleeter-song-{}", std::process::id()));
        let started = std::time::Instant::now();
        let names = split(&runtime, &dir, std::path::Path::new(&song), &out, 4, &|_| {}).unwrap();
        assert_eq!(names, ["vocals.ogg", "music.ogg"]);
        for n in &names {
            assert!(std::fs::metadata(out.join(n)).unwrap().len() > 10_000);
        }
        eprintln!("split in {:.1} s", started.elapsed().as_secs_f64());
        let _ = std::fs::remove_dir_all(&out);
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
            crate::audio_io::decode(&dir.join("clip.wav")).unwrap().chunks_exact(2).map(|f| (f[0], f[1])).unzip()
        } else {
            (song(RATE * 20, 4), song(RATE * 20, 5))
        };
        let started = std::time::Instant::now();
        let mut run = |mag: &[f32]| Ok((answer(&mut voice, mag)?, answer(&mut rest, mag)?));
        let got = split_fed(&l, &r, 4096, &mut run).unwrap();
        let took = started.elapsed().as_secs_f64();
        let both = |p: &[Vec<f32>; 2]| -> Vec<f32> { p[0].iter().zip(&p[1]).flat_map(|(a, b)| [*a, *b]).collect() };
        let (v, m) = (both(&got.vocals), both(&got.music));
        let x: Vec<f32> = l.iter().zip(&r).flat_map(|(a, b)| [*a, *b]).collect();
        let whole = closeness(&x, &v.iter().zip(&m).map(|(a, b)| a + b).collect::<Vec<f32>>());
        eprintln!("{took:.1} s for {:.0} s; parts vs song {whole:.1} dB", l.len() as f64 / RATE as f64);
        assert!(whole > 40.0);
        // Windows: ONNX Runtime runs on the Visual C++ runtime from its own folder, not the PC's
        #[cfg(windows)]
        {
            use windows_sys::Win32::System::LibraryLoader::{GetModuleFileNameW, GetModuleHandleW};
            for name in crate::two_part::C_RUNTIME {
                let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
                let module = unsafe { GetModuleHandleW(wide.as_ptr()) };
                assert!(!module.is_null(), "{name} is loaded");
                let mut buf = [0u16; 1024];
                let n = unsafe { GetModuleFileNameW(module, buf.as_mut_ptr(), buf.len() as u32) } as usize;
                let file = String::from_utf16_lossy(&buf[..n]).to_lowercase();
                eprintln!("{name}: {file}");
                assert!(file.starts_with(&dir.to_string_lossy().to_lowercase()), "{name} from {file}");
            }
        }
        if dir.join("vocals.f32").exists() {
            let read = |name: &str| -> Vec<f32> { std::fs::read(dir.join(name)).unwrap().chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect() };
            let (rv, rm) = (read("vocals.f32"), read("music.f32"));
            assert_eq!(v.len(), rv.len(), "as long as the reference");
            let (dv, dm) = (closeness(&rv, &v), closeness(&rm, &m));
            eprintln!("vocals {dv:.1} dB, music {dm:.1} dB from the reference");
            assert!(dv > 30.0 && dm > 30.0, "vocals {dv:.1} dB, music {dm:.1} dB");
        }
    }
}
