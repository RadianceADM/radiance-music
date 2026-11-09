//! Sound in and out of the splitter, through ffmpeg (in the image and
//! install.sh; fetched beside the models on Windows): any song to plain samples
//! at the rate a model wants, and the parts back to files as split.py wrote them
//! (FLAC from a lossless song, 16- or 24-bit as it was; Ogg Vorbis otherwise).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// ffmpeg and ffprobe.
#[derive(Clone, Debug)]
pub struct Tools {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}

/// How a part is written.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Out {
    Flac16,
    Flac24,
    /// Ogg Vorbis, quality 0.85 (split.py's compression level 0.15; the app's vorbisenc 0.85)
    Vorbis,
}

impl Out {
    pub fn ext(self) -> &'static str {
        if self == Out::Vorbis {
            "ogg"
        } else {
            "flac"
        }
    }
}

fn command(program: &Path) -> Command {
    let mut c = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: never a console window over the owner's desktop
    }
    c.stdin(Stdio::null());
    c
}

fn works(p: &Path) -> bool {
    command(p).arg("-version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

/// ffmpeg and ffprobe: beside the models (fetched there on Windows), else on the PATH.
pub fn tools(models: &Path) -> Option<Tools> {
    let exe = if cfg!(windows) { ".exe" } else { "" };
    let here = Tools { ffmpeg: models.join(format!("ffmpeg{exe}")), ffprobe: models.join(format!("ffprobe{exe}")) };
    if here.ffmpeg.is_file() && here.ffprobe.is_file() && works(&here.ffmpeg) {
        return Some(here);
    }
    let path = Tools { ffmpeg: PathBuf::from("ffmpeg"), ffprobe: PathBuf::from("ffprobe") };
    (works(&path.ffmpeg) && works(&path.ffprobe)).then_some(path)
}

fn said(stderr: &[u8]) -> String {
    String::from_utf8_lossy(stderr).lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string()
}

fn name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string())
}

/// `path`'s sound as samples (interleaved when `channels` is 2) at `rate`.
pub fn decode(t: &Tools, path: &Path, rate: u32, channels: u16) -> Result<Vec<f32>, String> {
    let out = command(&t.ffmpeg)
        .args(["-nostdin", "-v", "error", "-i"])
        .arg(path)
        .args(["-vn", "-f", "f32le", "-ac", &channels.to_string(), "-ar", &rate.to_string(), "-"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("couldn't start ffmpeg: {e}"))?;
    if !out.status.success() {
        return Err(format!("couldn't read {}: {}", name(path), said(&out.stderr)));
    }
    if out.stdout.is_empty() {
        return Err(format!("{} has no sound", name(path)));
    }
    Ok(out.stdout.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect())
}

/// How `song`'s parts are written, as split.py chose: FLAC (its bits) from
/// FLAC or PCM, Ogg Vorbis from anything compressed.
pub fn kind(t: &Tools, song: &Path) -> Out {
    let Ok(out) = command(&t.ffprobe)
        .args(["-v", "error", "-select_streams", "a:0", "-show_entries", "stream=codec_name,sample_fmt,bits_per_raw_sample", "-of", "default=noprint_wrappers=1"])
        .arg(song)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
    else {
        return Out::Vorbis;
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let field = |k: &str| text.lines().find_map(|l| l.strip_prefix(&format!("{k}="))).unwrap_or("").trim().to_string();
    let codec = field("codec_name");
    let bits16 = field("bits_per_raw_sample") == "16" || field("sample_fmt").starts_with("s16");
    match codec.as_str() {
        "flac" if bits16 => Out::Flac16,
        "flac" => Out::Flac24,
        "pcm_s16le" | "pcm_s16be" | "pcm_u8" | "pcm_s8" => Out::Flac16,
        c if c.starts_with("pcm_") => Out::Flac24,
        _ => Out::Vorbis,
    }
}

/// Writes interleaved stereo `data` at `rate` to `stem`.EXT; its file's name.
pub fn write_part(t: &Tools, stem: &Path, data: &[f32], rate: u32, out: Out) -> Result<String, String> {
    let file = stem.with_extension(out.ext());
    let codec: &[&str] = match out {
        Out::Flac16 => &["-c:a", "flac", "-sample_fmt", "s16"],
        Out::Flac24 => &["-c:a", "flac", "-sample_fmt", "s32", "-bits_per_raw_sample", "24"],
        Out::Vorbis => &["-c:a", "libvorbis", "-q:a", "8.5"],
    };
    let mut child = command(&t.ffmpeg)
        .args(["-v", "error", "-f", "f32le", "-ar", &rate.to_string(), "-ac", "2", "-i", "-"])
        .args(codec)
        .arg("-y")
        .arg(&file)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("couldn't start ffmpeg: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("ffmpeg's input")?;
    let result = std::thread::scope(|s| {
        let feeder = s.spawn(move || {
            let mut buf = Vec::with_capacity(1 << 20);
            for chunk in data.chunks(1 << 18) {
                buf.clear();
                buf.extend(chunk.iter().flat_map(|x| x.to_le_bytes()));
                stdin.write_all(&buf)?;
            }
            Ok::<_, std::io::Error>(())
        });
        let out = child.wait_with_output();
        (feeder.join(), out)
    });
    let out = match result {
        (_, Err(e)) => return Err(format!("ffmpeg: {e}")),
        (_, Ok(o)) => o,
    };
    if !out.status.success() {
        return Err(format!("couldn't write {}: {}", name(&file), said(&out.stderr)));
    }
    Ok(name(&file))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(n: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rms-audio-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn t() -> Tools {
        tools(Path::new("/nowhere")).expect("ffmpeg on the PATH (ribs02)")
    }

    /// One second of a stereo tone: left a sine, right a quieter cosine.
    fn tone(rate: u32) -> Vec<f32> {
        (0..rate as usize).flat_map(|i| {
            let x = 2.0 * std::f32::consts::PI * 440.0 * i as f32 / rate as f32;
            [0.6 * x.sin(), 0.3 * x.cos()]
        }).collect()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    /// A file made by ffmpeg itself (`args` after the tone's input).
    fn made(d: &Path, file: &str, args: &[&str]) -> PathBuf {
        let p = d.join(file);
        let ok = Command::new("ffmpeg").args(["-v", "error", "-f", "lavfi", "-i", "sine=frequency=440:duration=1:sample_rate=44100"]).args(args).arg("-y").arg(&p).status().unwrap();
        assert!(ok.success());
        p
    }

    #[test]
    fn flac_gives_the_samples_back() {
        let d = dir("flac");
        let x = tone(44_100);
        assert_eq!(write_part(&t(), &d.join("vocals"), &x, 44_100, Out::Flac16).unwrap(), "vocals.flac");
        let y = decode(&t(), &d.join("vocals.flac"), 44_100, 2).unwrap();
        assert_eq!(y.len(), x.len());
        let worst = x.iter().zip(&y).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
        assert!(worst < 1e-4, "{worst}");
        assert_eq!(kind(&t(), &d.join("vocals.flac")), Out::Flac16);
    }

    #[test]
    fn vorbis_keeps_its_loudness_and_length() {
        let d = dir("ogg");
        let x = tone(44_100);
        assert_eq!(write_part(&t(), &d.join("drums"), &x, 44_100, Out::Vorbis).unwrap(), "drums.ogg");
        let y = decode(&t(), &d.join("drums.ogg"), 44_100, 2).unwrap();
        assert!((y.len() as i64 - x.len() as i64).abs() <= 2 * 1024, "{} {}", y.len(), x.len());
        let (a, b) = (rms(&x), rms(&y));
        assert!((a - b).abs() / a < 0.05, "{a} {b}");
        assert_eq!(kind(&t(), &d.join("drums.ogg")), Out::Vorbis);
    }

    #[test]
    fn a_mono_song_comes_back_as_two_equal_sides() {
        let d = dir("mono");
        let p = made(&d, "mono.wav", &["-ac", "1"]);
        let y = decode(&t(), &p, 44_100, 2).unwrap();
        assert_eq!(y.len(), 2 * 44_100);
        assert!(y.chunks(2).all(|c| c[0] == c[1]));
        assert_eq!(kind(&t(), &p), Out::Flac16, "16-bit WAV");
    }

    #[test]
    fn word_timing_gets_its_own_rate() {
        let d = dir("rate");
        let p = made(&d, "song.flac", &["-ac", "2", "-sample_fmt", "s32", "-bits_per_raw_sample", "24"]);
        let y = decode(&t(), &p, 16_000, 1).unwrap();
        assert!((y.len() as i64 - 16_000).abs() <= 2, "{}", y.len());
        assert_eq!(kind(&t(), &p), Out::Flac24);
    }

    #[test]
    fn a_missing_song_says_so() {
        let e = decode(&t(), Path::new("/nowhere/Dee - Four.flac"), 44_100, 2).unwrap_err();
        assert!(e.contains("couldn't read Dee - Four.flac"), "{e}");
        assert!(tools(Path::new("/nowhere")).is_some());
    }
}
