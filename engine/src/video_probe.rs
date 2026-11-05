//! A developer's check of music videos on Windows (issue #45, stage 2's spike; ribs03 only): a
//! video file played the way the Windows app plays videos (Player::load_video_out's pipeline,
//! the sound silent), its frames counted, its swapchain handed to a test window. Run over SSH
//! (`--probe-video FILE OUT`) or in a test window (`--video-probe FILE`).

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gst::glib;
use gst::prelude::*;

use crate::player::{swapchain_of, video_desc, PICTURE_OUT};

/// A video file playing, silent, into the app's picture sink.
pub struct VideoProbe {
    pipeline: gst::Pipeline,
    picture: gst::Element,
    frames: Arc<AtomicU64>,
}

impl VideoProbe {
    /// Starts `file`; waits (up to 10 s) for its first picture.
    pub fn start(file: &Path) -> Result<VideoProbe, String> {
        gst::init().map_err(|e| e.to_string())?;
        let uri = glib::filename_to_uri(file, None).map_err(|e| e.to_string())?;
        let desc = video_desc(&uri, &uri, PICTURE_OUT, "fakesink sync=true");
        let pipeline = gst::parse::launch(&desc).map_err(|e| e.to_string())?.downcast::<gst::Pipeline>().map_err(|_| "not a pipeline")?;
        let picture = pipeline.by_name("picture").ok_or("no picture")?;
        let frames = Arc::new(AtomicU64::new(0));
        let counted = frames.clone();
        picture.static_pad("sink").ok_or("no picture pad")?.add_probe(gst::PadProbeType::BUFFER, move |_, _| {
            counted.fetch_add(1, Ordering::Relaxed);
            gst::PadProbeReturn::Ok
        });
        pipeline.set_state(gst::State::Playing).map_err(|e| e.to_string())?;
        let probe = VideoProbe { pipeline, picture, frames };
        let bus = probe.pipeline.bus().ok_or("no bus")?;
        match bus.timed_pop_filtered(gst::ClockTime::from_seconds(10), &[gst::MessageType::AsyncDone, gst::MessageType::Error]) {
            Some(m) if m.type_() == gst::MessageType::AsyncDone => Ok(probe),
            Some(m) => Err(match m.view() {
                gst::MessageView::Error(e) => format!("{} ({})", e.error(), e.debug().map(|d| d.to_string()).unwrap_or_default()),
                _ => "it didn't start".into(),
            }),
            None => Err("no picture in 10 s".into()),
        }
    }

    /// d3d12swapchainsink's swapchain (0: not that sink).
    pub fn swapchain(&self) -> u64 {
        swapchain_of(&self.picture)
    }

    /// Pictures that reached the sink so far.
    pub fn frames(&self) -> u64 {
        self.frames.load(Ordering::Relaxed)
    }

    /// The panel is `w`×`h` pixels now (the swapchain's buffers follow; Windows).
    pub fn resize(&self, w: u32, h: u32) {
        if self.picture.factory().is_some_and(|f| f.name() == "d3d12swapchainsink") && w > 0 && h > 0 {
            self.picture.emit_by_name::<()>("resize", &[&w, &h]);
        }
    }

    /// What it found: the picture's sink and its buffers, the decoder, the caps that reached
    /// the sink, the frames, and any error since.
    pub fn report(&self) -> Vec<String> {
        let mut out = Vec::new();
        let sink = self.picture.factory().map(|f| f.name().to_string()).unwrap_or_default();
        out.push(format!("picture sink: {sink}"));
        if self.picture.find_property("swapchain-width").is_some() {
            out.push(format!(
                "swapchain: {:#x}, {}x{}",
                self.swapchain(),
                self.picture.property::<u32>("swapchain-width"),
                self.picture.property::<u32>("swapchain-height")
            ));
        }
        let decoders: Vec<String> = self
            .pipeline
            .iterate_recurse()
            .into_iter()
            .flatten()
            .filter_map(|e| e.factory())
            .filter(|f| f.metadata("klass").is_some_and(|k| k.contains("Decoder") && k.contains("Video")))
            .map(|f| f.name().to_string())
            .collect();
        out.push(format!("video decoder: {}", if decoders.is_empty() { "none".into() } else { decoders.join(", ") }));
        if let Some(caps) = self.picture.static_pad("sink").and_then(|p| p.current_caps()) {
            out.push(format!("picture caps: {caps}"));
        }
        out.push(format!("frames: {}", self.frames()));
        if let Some(bus) = self.pipeline.bus() {
            while let Some(m) = bus.pop_filtered(&[gst::MessageType::Error, gst::MessageType::Warning]) {
                match m.view() {
                    gst::MessageView::Error(e) => out.push(format!("error: {}", e.error())),
                    gst::MessageView::Warning(w) => out.push(format!("warning: {}", w.error())),
                    _ => {}
                }
            }
        }
        out
    }

    pub fn stop(&self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

impl Drop for VideoProbe {
    fn drop(&mut self) {
        self.stop();
    }
}

/// `--probe-video`: `file` played for `secs` seconds, with no window. Its report, "ok" last
/// (pictures reached the sink, and on Windows a swapchain was made) or "failed: …".
pub fn probe(file: &Path, secs: f64) -> Vec<String> {
    let started = Instant::now();
    let p = match VideoProbe::start(file) {
        Ok(p) => p,
        Err(e) => return vec![format!("failed: {e}")],
    };
    let first = started.elapsed();
    std::thread::sleep(Duration::from_secs_f64(secs));
    let mut out = vec![format!("first picture in {} ms", first.as_millis())];
    out.extend(p.report());
    let swapchain_ok = !cfg!(windows) || p.swapchain() != 0;
    out.push(if p.frames() > 0 && swapchain_ok { "ok".into() } else { format!("failed: {} frames, swapchain {:#x}", p.frames(), p.swapchain()) });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small video (Matroska: a test picture, a quiet tone), as the controller's tests make.
    fn test_video(secs: u32) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("radiance-video-probe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("probe.mkv");
        gst::init().unwrap();
        let desc = format!(
            "videotestsrc num-buffers={} ! video/x-raw,format=I420,width=64,height=36,framerate=10/1 ! queue ! mux. \
             audiotestsrc num-buffers={} samplesperbuffer=4410 volume=0.05 ! audio/x-raw,format=S16LE,rate=44100,channels=1 ! queue ! mux. \
             matroskamux name=mux ! filesink location=\"{}\"",
            secs * 10,
            secs * 10,
            path.display()
        );
        let p = gst::parse::launch(&desc).unwrap();
        p.set_state(gst::State::Playing).unwrap();
        let done = p.bus().unwrap().timed_pop_filtered(gst::ClockTime::from_seconds(60), &[gst::MessageType::Eos, gst::MessageType::Error]);
        p.set_state(gst::State::Null).unwrap();
        assert_eq!(done.map(|m| m.type_()), Some(gst::MessageType::Eos));
        path
    }

    #[test]
    fn a_probe_counts_the_pictures() {
        let video = test_video(3);
        let report = probe(&video, 1.0);
        assert_eq!(report.last().map(String::as_str), Some("ok"), "{report:?}");
        assert!(report.iter().any(|l| l.starts_with("picture sink: fakesink")), "{report:?}");
        let frames: u64 = report.iter().find_map(|l| l.strip_prefix("frames: ")).and_then(|n| n.parse().ok()).unwrap();
        assert!((5..=25).contains(&frames), "{frames} pictures in about a second at 10 a second");
        let _ = std::fs::remove_dir_all(video.parent().unwrap());
    }

    #[test]
    fn a_file_that_isnt_a_video_fails_plainly() {
        let dir = std::env::temp_dir().join(format!("radiance-video-probe-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bad = dir.join("not-a-video.mkv");
        std::fs::write(&bad, b"not a video at all").unwrap();
        let report = probe(&bad, 0.1);
        assert_eq!(report.len(), 1, "{report:?}");
        assert!(report[0].starts_with("failed: "), "{report:?}");
        let _ = std::fs::remove_dir_all(dir);
    }
}
