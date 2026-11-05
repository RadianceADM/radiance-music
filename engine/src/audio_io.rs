//! A song read as 44.1 kHz stereo, and a part written as Ogg Vorbis: what the splitters inside the
//! app (spleeter.rs; onnx_split.rs, the Demucs prototype) read and write, through GStreamer. Both
//! a piece at a time (the fast split never holds a song whole: a mix can be an hour long).

use std::path::{Path, PathBuf};

use gst::prelude::*;

pub const RATE: usize = 44_100;

/// A song being read: its sound as 44.1 kHz stereo (interleaved), a piece at a time.
pub struct Reader {
    pipeline: gst::Pipeline,
    sink: gst_app::AppSink,
}

impl Reader {
    pub fn open(song: &Path) -> Result<Reader, String> {
        gst::init().map_err(|e| e.to_string())?;
        let uri = gst::glib::filename_to_uri(song, None).map_err(|e| e.to_string())?;
        let desc = format!("uridecodebin uri=\"{uri}\" ! audioconvert ! audioresample ! audio/x-raw,format=F32LE,channels=2,rate={RATE},layout=interleaved ! appsink name=sink sync=false max-buffers=8");
        let pipeline = gst::parse::launch(&desc).map_err(|e| e.to_string())?.downcast::<gst::Pipeline>().map_err(|_| "not a pipeline")?;
        let sink = pipeline.by_name("sink").and_then(|s| s.downcast::<gst_app::AppSink>().ok()).ok_or("no sink")?;
        pipeline.set_state(gst::State::Playing).map_err(|e| e.to_string())?;
        Ok(Reader { pipeline, sink })
    }

    /// How long the song is (seconds), once the reading knows it.
    pub fn seconds(&self) -> Option<f64> {
        self.pipeline.query_duration::<gst::ClockTime>().map(|d| d.nseconds() as f64 / 1e9)
    }

    /// The next piece (interleaved stereo); empty at the song's end.
    pub fn next(&mut self) -> Result<Vec<f32>, String> {
        loop {
            let Ok(sample) = self.sink.pull_sample() else {
                let failed = self.pipeline.bus().and_then(|b| b.pop_filtered(&[gst::MessageType::Error])).map(|m| match m.view() {
                    gst::MessageView::Error(e) => e.error().to_string(),
                    _ => String::new(),
                });
                return match failed {
                    Some(e) => Err(format!("couldn't read the song: {e}")),
                    None => Ok(Vec::new()),
                };
            };
            let Some(buf) = sample.buffer() else { continue };
            let Ok(map) = buf.map_readable() else { continue };
            let piece: Vec<f32> = map.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
            if !piece.is_empty() {
                return Ok(piece);
            }
        }
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

/// The song as 44.1 kHz stereo, interleaved, whole (the Demucs prototype).
pub fn decode(song: &Path) -> Result<Vec<f32>, String> {
    let mut r = Reader::open(song)?;
    let mut x = Vec::new();
    loop {
        let piece = r.next()?;
        if piece.is_empty() {
            break;
        }
        x.extend_from_slice(&piece);
    }
    if x.len() < 2 * RATE {
        return Err("the song is shorter than a second".into());
    }
    Ok(x)
}

/// A part being written as Ogg Vorbis (quality 0.85, as split.py's), a piece at a time. Written
/// beside its place and moved there when finished; dropped unfinished, it leaves nothing.
pub struct OggWriter {
    pipeline: gst::Pipeline,
    src: gst_app::AppSrc,
    path: PathBuf,
    part: PathBuf,
    frames: u64,
    finished: bool,
}

impl OggWriter {
    pub fn create(path: &Path) -> Result<OggWriter, String> {
        gst::init().map_err(|e| e.to_string())?;
        // (block: a full queue waits for the encoder, so a long part is never all in memory)
        let desc = format!("appsrc name=src format=time block=true max-bytes=4194304 ! audio/x-raw,format=F32LE,channels=2,rate={RATE},layout=interleaved ! audioconvert ! vorbisenc quality=0.85 ! oggmux ! filesink name=file");
        let pipeline = gst::parse::launch(&desc).map_err(|e| e.to_string())?.downcast::<gst::Pipeline>().map_err(|_| "not a pipeline")?;
        let part = path.with_extension("ogg.part");
        pipeline.by_name("file").ok_or("no file")?.set_property("location", part.to_string_lossy().to_string());
        let src = pipeline.by_name("src").and_then(|s| s.downcast::<gst_app::AppSrc>().ok()).ok_or("no source")?;
        src.set_caps(Some(&gst::Caps::builder("audio/x-raw").field("format", "F32LE").field("channels", 2i32).field("rate", RATE as i32).field("layout", "interleaved").build()));
        pipeline.set_state(gst::State::Playing).map_err(|e| e.to_string())?;
        Ok(OggWriter { pipeline, src, path: path.to_path_buf(), part, frames: 0, finished: false })
    }

    /// The next piece (interleaved stereo), clipped to full scale (anything not a number: silence).
    pub fn push(&mut self, data: &[f32]) -> Result<(), String> {
        for chunk in data.chunks(2 * RATE) {
            let bytes: Vec<u8> = chunk.iter().flat_map(|v| if v.is_finite() { v.clamp(-1.0, 1.0) } else { 0.0 }.to_le_bytes()).collect();
            let frames = (chunk.len() / 2) as u64;
            let mut buf = gst::Buffer::from_mut_slice(bytes);
            {
                let b = buf.get_mut().ok_or("buffer")?;
                b.set_pts(gst::ClockTime::from_nseconds(self.frames * 1_000_000_000 / RATE as u64));
                b.set_duration(gst::ClockTime::from_nseconds(frames * 1_000_000_000 / RATE as u64));
            }
            self.frames += frames;
            self.src.push_buffer(buf).map_err(|e| format!("writing {}: {e:?}", self.path.display()))?;
        }
        Ok(())
    }

    /// Done: the rest encoded, the file put in place.
    pub fn finish(mut self) -> Result<(), String> {
        let _ = self.src.end_of_stream();
        // what's still queued is at most a few seconds of encoding (block=true above)
        let done = self
            .pipeline
            .bus()
            .and_then(|b| b.timed_pop_filtered(gst::ClockTime::from_seconds(300), &[gst::MessageType::Eos, gst::MessageType::Error]))
            .map(|m| m.type_() == gst::MessageType::Eos)
            .unwrap_or(false);
        let _ = self.pipeline.set_state(gst::State::Null);
        if !done {
            return Err(format!("couldn't write {}", self.path.display()));
        }
        std::fs::rename(&self.part, &self.path).map_err(|e| e.to_string())?;
        self.finished = true;
        Ok(())
    }
}

impl Drop for OggWriter {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.pipeline.set_state(gst::State::Null);
            let _ = std::fs::remove_file(&self.part);
        }
    }
}

/// A part as Ogg Vorbis, whole (the Demucs prototype).
pub fn write_ogg(path: &Path, data: &[f32]) -> Result<(), String> {
    let mut w = OggWriter::create(path)?;
    w.push(data)?;
    w.finish()
}
