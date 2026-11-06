//! What's really playing, as the bar and Now Playing say it: "Lossless · FLAC · 24-bit · 96 kHz".
//! Moved out of the GTK app (ui.rs update_quality) so the Windows app says the same (issue #45).

/// The sound heard now, as the player knows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heard<'a> {
    /// A split song's parts in the Mixer: its four, or (`two`) Music and Vocals.
    Parts { two: bool },
    /// A music video: its picture's height, its sound's bit rate (0: a saved file).
    Video { height: u32, kbps: u32 },
    /// One half of a two-part split (`voice`: "Vocals", "Music", …).
    Half { voice: &'a str, bits: &'a str, rate: i32 },
    /// A lossless file (`bits` empty: not known yet).
    Flac { bits: &'a str, rate: i32 },
    /// A Radiance Music Server's stream, in its own format.
    Server { format: &'a str },
    /// A YouTube stream (`kbps` 0: not known).
    Opus { kbps: u32 },
    /// Picked, its sound not flowing yet (the stream being found, or the file opening).
    Loading,
}

/// "44.1 kHz", "96 kHz".
pub fn khz(rate: i32) -> String {
    if rate % 1000 == 0 {
        format!("{} kHz", rate / 1000)
    } else {
        format!("{:.1} kHz", rate as f64 / 1000.0)
    }
}

/// The words for what's heard.
pub fn text(h: Heard) -> String {
    match h {
        Heard::Parts { two: false } => "Mixer · its four parts".into(),
        Heard::Parts { two: true } => "Mixer · Music and Vocals".into(),
        Heard::Video { height, kbps: 0 } => format!("Video · {height}p"),
        Heard::Video { height, kbps } => format!("Video · {height}p · Opus {kbps} kbps"),
        Heard::Half { voice, bits, rate } => format!("{voice} · split · {bits} · {}", khz(rate)),
        Heard::Flac { bits: "", .. } => "Lossless · FLAC".into(),
        Heard::Flac { bits, rate } => format!("Lossless · FLAC · {bits} · {}", khz(rate)),
        Heard::Server { format } => format!("Compressed · {format}"),
        Heard::Opus { kbps: 0 } => "Compressed · Opus".into(),
        Heard::Opus { kbps } => format!("Compressed · Opus · {kbps} kbps"),
        Heard::Loading => "Loading…".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_words_the_linux_app_says() {
        assert_eq!(text(Heard::Parts { two: false }), "Mixer · its four parts");
        // (a song split in two plays both halves in the beta's Mixer; the Linux app plays one half file)
        assert_eq!(text(Heard::Parts { two: true }), "Mixer · Music and Vocals");
        assert_eq!(text(Heard::Video { height: 720, kbps: 160 }), "Video · 720p · Opus 160 kbps");
        assert_eq!(text(Heard::Video { height: 720, kbps: 0 }), "Video · 720p");
        assert_eq!(text(Heard::Half { voice: "Vocals", bits: "16-bit", rate: 44_100 }), "Vocals · split · 16-bit · 44.1 kHz");
        assert_eq!(text(Heard::Flac { bits: "24-bit", rate: 96_000 }), "Lossless · FLAC · 24-bit · 96 kHz");
        assert_eq!(text(Heard::Flac { bits: "", rate: 0 }), "Lossless · FLAC");
        assert_eq!(text(Heard::Server { format: "AAC" }), "Compressed · AAC");
        assert_eq!(text(Heard::Opus { kbps: 128 }), "Compressed · Opus · 128 kbps");
        assert_eq!(text(Heard::Opus { kbps: 0 }), "Compressed · Opus");
        assert_eq!(text(Heard::Loading), "Loading…");
    }

    #[test]
    fn rates_in_khz() {
        assert_eq!(khz(44_100), "44.1 kHz");
        assert_eq!(khz(48_000), "48 kHz");
        assert_eq!(khz(88_200), "88.2 kHz");
    }
}
