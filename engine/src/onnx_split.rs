//! Splitting a song inside the app, with no Python (owner 2026-11-02: "wala
//! bang compiled programming language"): Demucs (htdemucs) as ONNX, the same
//! model the phone runs, through ONNX Runtime (its library loaded at run
//! time). Measured on ribs02 with Python's onnxruntime first: about 10 % less
//! work than PyTorch for the same 60 s, the vocals 37.5 dB from PyTorch's
//! (two PyTorch runs differ by ~30 dB). Prototype (feature "onnx-split"):
//! tried with `--probe-onnx-split`, not wired into splitting yet.
//!
//! As the phone's Separate.kt: the song in pieces of N frames (7.8 s, fixed
//! when the model was exported), each overlapping the last by a quarter,
//! faded across and blended. As split.py: the song normalised first (its mean
//! and spread), the parts written as Ogg Vorbis (vocals, music = all but the
//! vocals, drums, bass, other), all at once.

use std::path::Path;
use std::time::Instant;

use gst::prelude::*;
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;

/// The model's parts, in its output order.
const PARTS: [&str; 4] = ["drums", "bass", "other", "vocals"];
const N: usize = 343_980;
const OVERLAP: usize = N / 4;
const STRIDE: usize = N - OVERLAP;
const RATE: usize = 44_100;

/// How long each step took (the probe prints it).
#[derive(Default, Debug)]
pub struct Took {
    pub load: f64,
    pub decode: f64,
    pub separate: f64,
    pub write: f64,
}

/// Splits `song` into `out` (made if missing): the files' names, and how long
/// each step took. `runtime`: ONNX Runtime's library; `model`: the ONNX file.
pub fn split(runtime: &Path, model: &Path, song: &Path, out: &Path, threads: usize, report: &dyn Fn(f64)) -> Result<(Vec<String>, Took), String> {
    let mut took = Took::default();
    let t = Instant::now();
    let env = ort::init_from(runtime).map_err(|e| format!("ONNX Runtime: {e}"))?.build().map_err(|e| e.to_string())?;
    let mut builder = Session::builder(&env)
        .map_err(|e| e.to_string())?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| e.to_string())?
        .with_intra_threads(threads)
        .map_err(|e| e.to_string())?
        .with_inter_threads(1)
        .map_err(|e| e.to_string())?;
    let mut session = builder.commit_from_file(model).map_err(|e| format!("the model: {e}"))?;
    took.load = t.elapsed().as_secs_f64();

    let t = Instant::now();
    let x = decode(song)?;
    took.decode = t.elapsed().as_secs_f64();

    let t = Instant::now();
    let parts = separate(&mut session, &x, report)?;
    took.separate = t.elapsed().as_secs_f64();

    let t = Instant::now();
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let vocals = &parts[3];
    let music: Vec<f32> = (0..vocals.len()).map(|i| parts[0][i] + parts[1][i] + parts[2][i]).collect();
    let mut files: Vec<(&str, &[f32])> = vec![("vocals", vocals), ("music", &music)];
    files.extend(PARTS.iter().zip(parts.iter()).filter(|(n, _)| **n != "vocals").map(|(n, p)| (*n, p.as_slice())));
    // all at once, each its own encoder
    let names = std::thread::scope(|s| {
        let jobs: Vec<_> = files
            .iter()
            .map(|(name, data)| {
                let path = out.join(format!("{name}.ogg"));
                s.spawn(move || write_ogg(&path, data).map(|_| format!("{name}.ogg")))
            })
            .collect();
        jobs.into_iter().map(|j| j.join().map_err(|_| "a writer stopped".to_string()).and_then(|r| r)).collect::<Result<Vec<_>, _>>()
    })?;
    took.write = t.elapsed().as_secs_f64();
    Ok((names, took))
}

/// The song as 44.1 kHz stereo, interleaved.
fn decode(song: &Path) -> Result<Vec<f32>, String> {
    gst::init().map_err(|e| e.to_string())?;
    let uri = gst::glib::filename_to_uri(song, None).map_err(|e| e.to_string())?;
    let desc = format!("uridecodebin uri=\"{uri}\" ! audioconvert ! audioresample ! audio/x-raw,format=F32LE,channels=2,rate={RATE},layout=interleaved ! appsink name=sink sync=false");
    let pipeline = gst::parse::launch(&desc).map_err(|e| e.to_string())?.downcast::<gst::Pipeline>().map_err(|_| "not a pipeline")?;
    let sink = pipeline.by_name("sink").and_then(|s| s.downcast::<gst_app::AppSink>().ok()).ok_or("no sink")?;
    pipeline.set_state(gst::State::Playing).map_err(|e| e.to_string())?;
    let mut x: Vec<f32> = Vec::new();
    while let Ok(sample) = sink.pull_sample() {
        let Some(buf) = sample.buffer() else { continue };
        let Ok(map) = buf.map_readable() else { continue };
        x.extend(map.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])));
    }
    let failed = pipeline.bus().and_then(|b| b.pop_filtered(&[gst::MessageType::Error])).map(|m| match m.view() {
        gst::MessageView::Error(e) => e.error().to_string(),
        _ => String::new(),
    });
    let _ = pipeline.set_state(gst::State::Null);
    match failed {
        Some(e) => Err(format!("couldn't read the song: {e}")),
        None if x.len() < 2 * RATE => Err("the song is shorter than a second".into()),
        None => Ok(x),
    }
}

/// The four parts (each interleaved, as long as `x`), piece by piece.
fn separate(session: &mut Session, x: &[f32], report: &dyn Fn(f64)) -> Result<[Vec<f32>; 4], String> {
    let total = x.len() / 2;
    // normalised as split.py does: by the mono mix's mean and spread
    let mono = |i: usize| 0.5 * (x[2 * i] + x[2 * i + 1]) as f64;
    let mean = (0..total).map(mono).sum::<f64>() / total as f64;
    let std = ((0..total).map(|i| (mono(i) - mean).powi(2)).sum::<f64>() / total as f64).sqrt() + 1e-8;
    let (mean, std) = (mean as f32, std as f32);
    // in over the overlap, out over the overlap (as the phone)
    let window: Vec<f32> = (0..N)
        .map(|i| {
            if i < OVERLAP {
                i as f32 / (OVERLAP - 1) as f32
            } else if i >= N - OVERLAP {
                (N - 1 - i) as f32 / (OVERLAP - 1) as f32
            } else {
                1.0
            }
        })
        .collect();
    let mut acc: [Vec<f32>; 4] = std::array::from_fn(|_| vec![0.0; 2 * total]);
    let mut weight = vec![0.0f32; total];
    let pieces = total.div_ceil(STRIDE).max(1);
    for c in 0..pieces {
        let s = c * STRIDE;
        let len = N.min(total - s);
        // planar: [left N][right N], zeros past the end
        let mut piece = vec![0.0f32; 2 * N];
        for i in 0..len {
            piece[i] = (x[2 * (s + i)] - mean) / std;
            piece[N + i] = (x[2 * (s + i) + 1] - mean) / std;
        }
        let input = Tensor::from_array(([1usize, 2, N], piece)).map_err(|e| e.to_string())?;
        let outputs = session.run(ort::inputs![input]).map_err(|e| e.to_string())?;
        let (_, y) = outputs[0].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
        if y.len() < 4 * 2 * N {
            return Err(format!("the model gave {} values, not {}", y.len(), 8 * N));
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
        report((c + 1) as f64 / pieces as f64);
    }
    for part in acc.iter_mut() {
        for i in 0..total {
            let w = weight[i].max(1e-6);
            part[2 * i] = part[2 * i] / w * std + mean;
            part[2 * i + 1] = part[2 * i + 1] / w * std + mean;
        }
    }
    Ok(acc)
}

/// A part as Ogg Vorbis (quality 0.85, as split.py's), clipped to full scale.
fn write_ogg(path: &Path, data: &[f32]) -> Result<(), String> {
    let desc = format!("appsrc name=src format=time ! audio/x-raw,format=F32LE,channels=2,rate={RATE},layout=interleaved ! audioconvert ! vorbisenc quality=0.85 ! oggmux ! filesink name=file");
    let pipeline = gst::parse::launch(&desc).map_err(|e| e.to_string())?.downcast::<gst::Pipeline>().map_err(|_| "not a pipeline")?;
    let part = path.with_extension("ogg.part");
    pipeline.by_name("file").ok_or("no file")?.set_property("location", part.to_string_lossy().to_string());
    let src = pipeline.by_name("src").and_then(|s| s.downcast::<gst_app::AppSrc>().ok()).ok_or("no source")?;
    src.set_caps(Some(&gst::Caps::builder("audio/x-raw").field("format", "F32LE").field("channels", 2i32).field("rate", RATE as i32).field("layout", "interleaved").build()));
    pipeline.set_state(gst::State::Playing).map_err(|e| e.to_string())?;
    let block = RATE; // a second at a time
    let mut frame = 0usize;
    for chunk in data.chunks(2 * block) {
        let bytes: Vec<u8> = chunk.iter().flat_map(|v| v.clamp(-1.0, 1.0).to_le_bytes()).collect();
        let mut buf = gst::Buffer::from_mut_slice(bytes);
        {
            let b = buf.get_mut().ok_or("buffer")?;
            b.set_pts(gst::ClockTime::from_nseconds((frame as u64) * 1_000_000_000 / RATE as u64));
            b.set_duration(gst::ClockTime::from_nseconds((chunk.len() as u64 / 2) * 1_000_000_000 / RATE as u64));
        }
        frame += chunk.len() / 2;
        src.push_buffer(buf).map_err(|e| format!("writing {}: {e:?}", path.display()))?;
    }
    let _ = src.end_of_stream();
    let done = pipeline
        .bus()
        .and_then(|b| b.timed_pop_filtered(gst::ClockTime::from_seconds(120), &[gst::MessageType::Eos, gst::MessageType::Error]))
        .map(|m| m.type_() == gst::MessageType::Eos)
        .unwrap_or(false);
    let _ = pipeline.set_state(gst::State::Null);
    if !done {
        return Err(format!("couldn't write {}", path.display()));
    }
    std::fs::rename(&part, path).map_err(|e| e.to_string())
}
