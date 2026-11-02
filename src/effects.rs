//! The Mixer's effects: low cut, high cut and reverb, done by Radiance Music
//! itself (fx.rs; owner 2026-11-02: "no external app needed", "one setting
//! for all"). Speaker EQ, the separate app that used to do them per speaker,
//! is gone. One setting for every speaker, kept in settings.json and read by
//! every player as the sound passes, so a slider is heard at once.

use std::sync::atomic::{AtomicU64, Ordering};

/// Each effect's range in Hz (its "off" end first for the low cut, last for the high cut).
pub const LOW_CUT: (f64, f64) = (20.0, 500.0);
pub const HIGH_CUT: (f64, f64) = (1500.0, 20000.0);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sound {
    pub low_cut: f64,
    pub high_cut: f64,
    /// 0-100 %.
    pub reverb: f64,
}

impl Sound {
    pub const OFF: Sound = Sound { low_cut: LOW_CUT.0, high_cut: HIGH_CUT.1, reverb: 0.0 };

    /// All three at their "off" end: the sound passes untouched.
    pub fn is_off(&self) -> bool {
        self.low_cut <= LOW_CUT.0 + 0.5 && self.high_cut >= HIGH_CUT.1 - 0.5 && self.reverb < 0.5
    }

    /// From settings.json's "effects" (missing: all off).
    pub fn from_json(v: &serde_json::Value) -> Sound {
        let get = |k: &str, d: f64| v[k].as_f64().unwrap_or(d);
        Sound {
            low_cut: get("low_cut", LOW_CUT.0).clamp(LOW_CUT.0, LOW_CUT.1),
            high_cut: get("high_cut", HIGH_CUT.1).clamp(HIGH_CUT.0, HIGH_CUT.1),
            reverb: get("reverb", 0.0).clamp(0.0, 100.0),
        }
    }
}

// what every player reads, each buffer (f64 bits)
static LOW: AtomicU64 = AtomicU64::new(0x4034_0000_0000_0000); // 20.0
static HIGH: AtomicU64 = AtomicU64::new(0x40D3_8800_0000_0000); // 20000.0
static REVERB: AtomicU64 = AtomicU64::new(0);

/// The effects as the Mixer says now.
pub fn now() -> Sound {
    let get = |a: &AtomicU64| f64::from_bits(a.load(Ordering::Relaxed));
    Sound { low_cut: get(&LOW), high_cut: get(&HIGH), reverb: get(&REVERB) }
}

/// New effects, heard at once in whatever plays (saving them is the caller's).
pub fn set(s: Sound) {
    LOW.store(s.low_cut.to_bits(), Ordering::Relaxed);
    HIGH.store(s.high_cut.to_bits(), Ordering::Relaxed);
    REVERB.store(s.reverb.to_bits(), Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_off_and_reads_back() {
        assert_eq!(f64::from_bits(0x4034_0000_0000_0000), 20.0);
        assert_eq!(f64::from_bits(0x40D3_8800_0000_0000), 20000.0);
        assert!(Sound::from_json(&serde_json::Value::Null).is_off());
        let s = Sound::from_json(&serde_json::json!({ "low_cut": 120.0, "high_cut": 8000.0, "reverb": 30.0 }));
        assert_eq!(s, Sound { low_cut: 120.0, high_cut: 8000.0, reverb: 30.0 });
        assert!(!s.is_off());
    }
}
