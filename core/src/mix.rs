//! The Mixer's rules, as the Linux app has them (moved from ui.rs, issue #45 stage 3): each part's
//! volume, the voice buttons (Song / No vocals / Vocals only) as presets, and the parts' titles.

use std::collections::HashMap;

/// Every part a song can be split into, in the order the Mixer lists them (six parts: piano and guitar too).
pub const SIX: [&str; 6] = ["drums", "bass", "other", "vocals", "guitar", "piano"];

/// The voice buttons on Now Playing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Voice {
    /// The song as it is.
    Song,
    /// Karaoke: the music without the voice.
    NoVocals,
    VocalsOnly,
}

impl Voice {
    /// The half file it plays from a song split in two ("music", "vocals"), none for the song itself.
    pub fn stem(self) -> Option<&'static str> {
        match self {
            Voice::Song => None,
            Voice::NoVocals => Some("music"),
            Voice::VocalsOnly => Some("vocals"),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Voice::Song => "Song",
            Voice::NoVocals => "No vocals",
            Voice::VocalsOnly => "Vocals only",
        }
    }
}

/// Every part's volume for the player, at full unless its slider says otherwise. Every part,
/// split in six or not (a song split in four plays no piano or guitar file; one split in six
/// before six went off keeps them): not split in six, piano and guitar follow Other.
pub fn part_volumes(sliders: &HashMap<String, f64>, six: bool) -> HashMap<String, f64> {
    let mut g: HashMap<String, f64> = SIX.iter().map(|p| (p.to_string(), sliders.get(*p).copied().unwrap_or(1.0))).collect();
    if !six {
        let other = g["other"];
        g.insert("piano".into(), other);
        g.insert("guitar".into(), other);
    }
    // a song split in two: Music has its own slider (it was left out, so it did nothing)
    g.insert("music".into(), sliders.get("music").copied().unwrap_or(1.0));
    g
}

/// A voice button on a song with its four parts: a Mixer preset (only the vocals turn).
pub fn preset(voice: Voice, part: &str) -> f64 {
    match voice {
        Voice::Song => 1.0,
        Voice::NoVocals => if part == "vocals" { 0.0 } else { 1.0 },
        Voice::VocalsOnly => if part == "vocals" { 1.0 } else { 0.0 },
    }
}

/// A part's title on its Mixer row.
pub fn part_title(part: &str) -> &'static str {
    match part {
        "drums" => "Drums",
        "bass" => "Bass",
        "other" => "Other",
        "piano" => "Piano",
        "guitar" => "Guitar",
        "music" => "Music",
        _ => "Vocals",
    }
}

/// A part's meter (0..1): its power in dB, -60 dB (nothing) to -6 dB (as loud as a part gets), as the
/// Linux app's update_meters.
pub fn meter(power: f64) -> f64 {
    let db = 10.0 * power.max(1e-12).log10();
    ((db + 60.0) / 54.0).clamp(0.0, 1.0)
}

/// What a part's meter says when you point at it (`power` None: its parts aren't playing).
pub fn meter_tip(name: &str, power: Option<f64>) -> String {
    let label = name.get(..1).map(|f| f.to_uppercase() + &name[1..]).unwrap_or_default();
    match power.map(|p| 10.0 * p.max(1e-12).log10()) {
        Some(d) if d < -42.0 => format!("{label}: almost nothing in the song here"),
        Some(d) => format!("{label}: {d:.0} dB in the song here"),
        None => format!("{label}: how loud it is in the song (while its parts play)"),
    }
}

#[cfg(test)]
mod tests {

    /// A part's meter, as the Linux app's update_meters: its power in dB, -60 dB (nothing) to -6 dB
    /// (as loud as a part gets) across the bar (final review I1: the beta showed the raw power, 0-5 %).
    #[test]
    fn a_meter_reads_decibels() {
        assert_eq!(meter(1e-6), 0.0);
        assert!((meter(10f64.powf(-0.6)) - 1.0).abs() < 1e-9);
        assert!((meter(1e-3) - 30.0 / 54.0).abs() < 1e-9);
        assert_eq!(meter(0.0), 0.0);
        assert_eq!(meter_tip("drums", Some(1e-5)), "Drums: almost nothing in the song here");
        assert_eq!(meter_tip("vocals", Some(0.01)), "Vocals: -20 dB in the song here");
        assert_eq!(meter_tip("bass", None), "Bass: how loud it is in the song (while its parts play)");
    }
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn every_part_gets_a_volume() {
        // after Reset (nothing set) and Vocals down: drums, bass and other go at full, said
        let after_reset: HashMap<String, f64> = [("vocals".to_string(), 0.3)].into();
        let g = part_volumes(&after_reset, false);
        for p in ["drums", "bass", "other", "piano", "guitar"] {
            assert_eq!(g[p], 1.0, "{p}");
        }
        assert_eq!(g["vocals"], 0.3);
        // five parts not split in six: piano and guitar follow Other
        let other_down: HashMap<String, f64> = [("other".to_string(), 0.5), ("piano".to_string(), 0.9)].into();
        let g = part_volumes(&other_down, false);
        assert_eq!((g["piano"], g["guitar"]), (0.5, 0.5));
        assert_eq!(part_volumes(&other_down, true)["piano"], 0.9);
        // a song in two: Music's slider counts too
        let music_down: HashMap<String, f64> = [("music".to_string(), 0.25)].into();
        assert_eq!(part_volumes(&music_down, false)["music"], 0.25);
        assert_eq!(part_volumes(&after_reset, false)["music"], 1.0);
    }

    #[test]
    fn presets_turn_only_the_vocals() {
        let parts = ["drums", "bass", "other", "piano", "guitar", "music", "vocals"];
        for p in parts {
            assert_eq!(preset(Voice::Song, p), 1.0, "{p}");
            assert_eq!(preset(Voice::NoVocals, p), if p == "vocals" { 0.0 } else { 1.0 }, "{p}");
            assert_eq!(preset(Voice::VocalsOnly, p), if p == "vocals" { 1.0 } else { 0.0 }, "{p}");
        }
        assert_eq!([Voice::Song.label(), Voice::NoVocals.label(), Voice::VocalsOnly.label()], ["Song", "No vocals", "Vocals only"]);
        assert_eq!(part_title("other"), "Other");
        assert_eq!(part_title("music"), "Music");
    }
}
