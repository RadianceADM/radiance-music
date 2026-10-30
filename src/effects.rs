//! The Mixer's effects (low cut, high cut, reverb) belong to Speaker EQ, a
//! separate app that filters whatever plays on a speaker. Radiance Music asks
//! it through its command line (`speaker-eq --effects`, `--set-effects`);
//! without Speaker EQ installed, the Mixer simply has no effects.

use std::process::Command;

use gtk::glib;

/// Each effect's range in Hz (its "off" end first); Speaker EQ says its own.
pub const LOW_CUT: (f64, f64) = (20.0, 500.0);
pub const HIGH_CUT: (f64, f64) = (1500.0, 20000.0);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sound {
    pub low_cut: f64,
    pub high_cut: f64,
    /// 0-100 %.
    pub reverb: f64,
}

/// The speaker playing (Speaker EQ's number for it, its name) and its effects.
#[derive(Clone, Debug)]
pub struct Playing {
    pub device: usize,
    pub name: String,
    pub sound: Sound,
    /// Its filter chain is running (the effects are heard).
    pub running: bool,
}

fn speaker_eq() -> Option<std::path::PathBuf> {
    glib::find_program_in_path("speaker-eq")
}

/// Speaker EQ is installed.
pub fn available() -> bool {
    speaker_eq().is_some()
}

/// The speaker playing and its effects (None: no Speaker EQ, or it doesn't
/// filter that speaker). Worker thread: it asks the sound server.
pub fn effects() -> Option<Playing> {
    let out = Command::new(speaker_eq()?).arg("--effects").output().ok()?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    Some(Playing {
        device: v["device"].as_u64()? as usize,
        name: v["name"].as_str().unwrap_or("speaker").to_string(),
        sound: Sound { low_cut: v["low_cut"].as_f64()?, high_cut: v["high_cut"].as_f64()?, reverb: v["reverb"].as_f64()? },
        running: v["running"].as_bool().unwrap_or(false),
    })
}

/// Sets them (saved by Speaker EQ, heard at once). Worker thread.
pub fn set_effects(device: usize, low_cut: f64, high_cut: f64, reverb: f64) {
    if let Some(p) = speaker_eq() {
        let _ = Command::new(p).arg("--set-effects").args([device.to_string(), low_cut.to_string(), high_cut.to_string(), reverb.to_string()]).output();
    }
}
