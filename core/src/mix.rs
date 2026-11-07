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

/// What the voice buttons show, both computer apps (owner, 2026-11-07, the beta's Mixer: "diba dapat automatic sya
/// babalek sa song kase di pa downloaded", "walang loading indicator sa mixer kung splitting pa"): the voice picked
/// once the song is `ready` for it (split); till then Song, what's heard, and the voice it's splitting for while
/// something `splitting` it (the Mixer's loading line). The voice picked stays picked and comes on once it's split.
pub fn shown_voice(wanted: Voice, ready: bool, splitting: bool) -> (Voice, Option<Voice>) {
    if wanted == Voice::Song || ready {
        return (wanted, None);
    }
    (Voice::Song, splitting.then_some(wanted))
}

/// Where the song playing is in the splitting (the Mixer's line): not at all, split in two now, split in four now
/// (how far, 0..1, when known), or waiting in line (how many songs go first).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SplitState {
    None,
    Two,
    Four(Option<f64>),
    Waiting(usize),
}

/// The Mixer's split line and choices for the song playing: what its split is doing (none: nothing), how far (a
/// four-part one), whether it's for the voice picked (Turn Off then), and the song menu's split choices.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MixerSplit {
    pub line: Option<String>,
    pub progress: Option<f64>,
    pub for_voice: bool,
    pub items: Vec<crate::menu::Item>,
}

/// Owner, 2026-11-08 (the beta's Mixer): "bakit di ko paren nakikita sa mixer kung splitting pa lang" (any split of
/// the song playing is said there, not only one for a voice picked) and "walang option sa mixer to split 4 ways"
/// (the menu's split choices are in the Mixer too). Both computer apps.
pub fn mixer_split(t: &crate::track::Track, ability: &crate::menu::SplitAbility, splitting_for: Option<Voice>, state: SplitState) -> MixerSplit {
    use crate::words;
    let line = match (splitting_for, state) {
        (Some(v), _) => Some(words::splitting_for(v.label())),
        (None, SplitState::None) => None,
        (None, SplitState::Two) => Some(words::SPLITTING_TWO.to_string()),
        (None, SplitState::Four(p)) => Some(words::splitting_four(p)),
        (None, SplitState::Waiting(n)) => Some(words::split_waiting(n)),
    };
    let progress = match state {
        SplitState::Four(p) => p,
        _ => None,
    };
    MixerSplit { line, progress, for_voice: splitting_for.is_some(), items: crate::menu::split_items(t, ability) }
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

    /// Owner, 2026-11-07: "diba dapat automatic sya babalek sa song kase di pa downloaded". The voice buttons show
    /// what's heard: Song while the voice picked waits for the song's split, and what it's splitting for (a loading
    /// line) only while something splits it; the voice picked as it is once the song is ready.
    /// Owner, 2026-11-08 (the beta's Mixer): "bakit di ko paren nakikita sa mixer kung splitting pa lang", "walang option
    /// sa mixer to split 4 ways". The Mixer says what the song playing's split is doing (any split, not only for a voice
    /// picked), how far a four-part one is, and offers the menu's split choices (Four Parts too).
    #[test]
    fn the_mixer_says_how_the_songs_split_is_going_and_offers_the_splits() {
        use crate::menu::{Action, SplitAbility};
        use crate::track::{Source, Track};
        let t = Track { source: Source::YouTube, id: "s".into(), title: "S".into(), artist: "A".into(), album: String::new(), seconds: 200, art: String::new(), format: String::new(), file: None, video: false, genres: None };
        let all = SplitAbility { online: true, parts: 0, can_two: true, can_four: true, busy_two: false, busy_four: false };
        let actions = |m: &MixerSplit| m.items.iter().map(|i| i.action.clone()).collect::<Vec<_>>();
        let idle = mixer_split(&t, &all, None, SplitState::None);
        assert_eq!((idle.line.as_deref(), idle.for_voice), (None, false));
        assert_eq!(actions(&idle), [Action::SplitTwo, Action::SplitFour], "both splits offered in the Mixer");
        let halves = SplitAbility { parts: 2, ..all };
        assert_eq!(mixer_split(&t, &halves, None, SplitState::None).items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(), ["Split in Four Parts"]);
        let two = mixer_split(&t, &SplitAbility { busy_two: true, ..all }, None, SplitState::Two);
        assert_eq!((two.line.as_deref(), two.progress), (Some("Splitting into Music and Vocals…"), None));
        assert_eq!(actions(&two), [Action::SplitFour], "the one splitting isn't offered again");
        let four = mixer_split(&t, &SplitAbility { busy_four: true, ..all }, None, SplitState::Four(Some(0.4)));
        assert_eq!((four.line.as_deref(), four.progress), (Some("Splitting into four parts · 40%"), Some(0.4)));
        assert_eq!(mixer_split(&t, &all, None, SplitState::Four(None)).line.as_deref(), Some("Splitting into four parts…"));
        assert_eq!(mixer_split(&t, &all, None, SplitState::Waiting(0)).line.as_deref(), Some("Next in line to be split"));
        assert_eq!(mixer_split(&t, &all, None, SplitState::Waiting(2)).line.as_deref(), Some("Waiting to be split (2 songs first)"));
        let voice = mixer_split(&t, &all, Some(Voice::NoVocals), SplitState::Two);
        assert_eq!((voice.line.as_deref(), voice.for_voice), (Some("Splitting for No vocals…"), true), "for the voice picked: Turn Off too");
        let video = Track { video: true, ..t.clone() };
        assert!(mixer_split(&video, &all, None, SplitState::None).items.is_empty(), "a video isn't split");
    }

    #[test]
    fn the_buttons_show_song_while_the_voice_waits_for_its_split() {
        assert_eq!(shown_voice(Voice::NoVocals, false, true), (Voice::Song, Some(Voice::NoVocals)), "splitting for it");
        assert_eq!(shown_voice(Voice::VocalsOnly, false, false), (Voice::Song, None), "waiting, nothing splitting it");
        assert_eq!(shown_voice(Voice::NoVocals, true, false), (Voice::NoVocals, None), "split: on");
        assert_eq!(shown_voice(Voice::NoVocals, true, true), (Voice::NoVocals, None), "split already (four parts asked too)");
        assert_eq!(shown_voice(Voice::Song, false, true), (Voice::Song, None), "Song waits for nothing");
    }

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
