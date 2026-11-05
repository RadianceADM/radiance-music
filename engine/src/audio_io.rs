//! A song read as 44.1 kHz stereo, and a part written as Ogg Vorbis: what the splitters inside the
//! app (spleeter.rs; onnx_split.rs, the Demucs prototype) read and write, through GStreamer.

use std::path::Path;

use gst::prelude::*;

pub const RATE: usize = 44_100;

/// The song as 44.1 kHz stereo, interleaved.
pub fn decode(song: &Path) -> Result<Vec<f32>, String> {
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

/// A part as Ogg Vorbis (quality 0.85, as split.py's), clipped to full scale.
pub fn write_ogg(path: &Path, data: &[f32]) -> Result<(), String> {
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
