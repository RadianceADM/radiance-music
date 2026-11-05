//! The Mixer's effects done in the player itself (owner 2026-11-02: "no
//! external app needed"): low cut, high cut and reverb, one setting for every
//! speaker (effects.rs), on whatever plays.
//!
//! Every step works sample by sample, in place, as the sound passes (a pad
//! probe, as the limiter): nothing is held back, so the Mixer's sliders stay
//! instant. (v0.5.17-18's reverb was a block filter in the parts' pipeline
//! and made the sliders answer seconds late.) With all three off, the sound
//! isn't touched at all.

use std::sync::Mutex;

use gst::prelude::*;

use crate::effects::{self, Sound};

/// A biquad (RBJ's cookbook), in transposed direct form II, state in f64.
#[derive(Clone, Copy, Default)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    fn new(high_pass: bool, freq: f64, q: f64, rate: f64) -> Biquad {
        let w = 2.0 * std::f64::consts::PI * (freq / rate).min(0.49);
        let (cos, alpha) = (w.cos(), w.sin() / (2.0 * q));
        let a0 = 1.0 + alpha;
        let (b0, b1) = if high_pass { ((1.0 + cos) / 2.0, -(1.0 + cos)) } else { ((1.0 - cos) / 2.0, 1.0 - cos) };
        Biquad { b0: b0 / a0, b1: b1 / a0, b2: b0 / a0, a1: -2.0 * cos / a0, a2: (1.0 - alpha) / a0, z1: 0.0, z2: 0.0 }
    }

    #[inline]
    fn run(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// A 4th-order Butterworth cut: two biquads (Q 0.541, 1.307), 24 dB an octave.
#[derive(Clone, Copy)]
struct Cut([Biquad; 2]);

impl Cut {
    fn new(high_pass: bool, freq: f64, rate: f64) -> Cut {
        Cut([Biquad::new(high_pass, freq, 0.541_196_1, rate), Biquad::new(high_pass, freq, 1.306_563, rate)])
    }

    #[inline]
    fn run(&mut self, x: f64) -> f64 {
        let y = self.0[0].run(x);
        self.0[1].run(y)
    }
}

/// A delay line with feedback, low-passed (Freeverb's comb).
struct Comb {
    buf: Vec<f32>,
    at: usize,
    store: f32,
}

impl Comb {
    #[inline]
    fn run(&mut self, x: f32, feedback: f32, damp: f32) -> f32 {
        let out = self.buf[self.at];
        self.store = out * (1.0 - damp) + self.store * damp;
        if self.store.abs() < 1e-20 {
            self.store = 0.0;
        }
        self.buf[self.at] = x + self.store * feedback;
        self.at = (self.at + 1) % self.buf.len();
        out
    }
}

/// Freeverb's allpass.
struct Allpass {
    buf: Vec<f32>,
    at: usize,
}

impl Allpass {
    #[inline]
    fn run(&mut self, x: f32) -> f32 {
        let held = self.buf[self.at];
        let out = held - x;
        let mut keep = x + held * 0.5;
        if keep.abs() < 1e-20 {
            keep = 0.0;
        }
        self.buf[self.at] = keep;
        self.at = (self.at + 1) % self.buf.len();
        out
    }
}

/// Jezar's Freeverb (public domain): eight combs and four allpasses a side,
/// the right side's lines a little longer (stereo spread). A room, one size.
struct Room {
    combs: [Vec<Comb>; 2],
    allpasses: [Vec<Allpass>; 2],
}

const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASSES: [usize; 4] = [556, 441, 341, 225];
const SPREAD: usize = 23;
/// Freeverb's room size 0.75 and damping 0.5, in its own terms.
const FEEDBACK: f32 = 0.75 * 0.28 + 0.7;
const DAMP: f32 = 0.5 * 0.4;
const INPUT_GAIN: f32 = 0.015;

impl Room {
    fn new(rate: f64) -> Room {
        let len = |n: usize| ((n as f64 * rate / 44_100.0).round() as usize).max(1);
        let side = |extra: usize| -> (Vec<Comb>, Vec<Allpass>) {
            (
                COMBS.iter().map(|&n| Comb { buf: vec![0.0; len(n + extra)], at: 0, store: 0.0 }).collect(),
                ALLPASSES.iter().map(|&n| Allpass { buf: vec![0.0; len(n + extra)], at: 0 }).collect(),
            )
        };
        let (cl, al) = side(0);
        let (cr, ar) = side(SPREAD);
        Room { combs: [cl, cr], allpasses: [al, ar] }
    }

    /// The room's sound for this frame's input (both sides hear the mono mix).
    #[inline]
    fn run(&mut self, mono: f32) -> [f32; 2] {
        let x = mono * INPUT_GAIN;
        let mut out = [0.0f32; 2];
        for side in 0..2 {
            let mut s: f32 = self.combs[side].iter_mut().map(|c| c.run(x, FEEDBACK, DAMP)).sum();
            for a in self.allpasses[side].iter_mut() {
                s = a.run(s);
            }
            out[side] = s;
        }
        out
    }
}

/// One player's effects: its filters' and room's state, made for its rate
/// and channels, set again whenever the Mixer moves.
pub struct Fx {
    rate: u32,
    channels: usize,
    set: Sound,
    low: Option<Vec<Cut>>,
    high: Option<Vec<Cut>>,
    room: Option<Room>,
}

impl Fx {
    pub fn new(rate: u32, channels: usize) -> Fx {
        Fx { rate, channels, set: Sound::OFF, low: None, high: None, room: None }
    }

    pub fn fits(&self, rate: u32, channels: usize) -> bool {
        self.rate == rate && self.channels == channels
    }

    /// Interleaved f32 samples, in place, as `now` says.
    pub fn process(&mut self, samples: &mut [f32], now: Sound) {
        if now != self.set {
            self.retune(now);
        }
        if now.is_off() || self.channels == 0 {
            return; // untouched
        }
        let ch = self.channels;
        let wet = (now.reverb / 100.0).clamp(0.0, 1.0) as f32;
        // the room around the music: the dry sound eases down a little as it grows
        let dry = 1.0 - 0.4 * wet;
        // a room "one control": wet up to Freeverb's own full (scale 3)
        let wet_gain = 3.0 * wet / 2.0;
        for frame in samples.chunks_exact_mut(ch) {
            for (c, s) in frame.iter_mut().enumerate() {
                let mut v = *s as f64;
                if let Some(low) = self.low.as_mut() {
                    v = low[c].run(v);
                }
                if let Some(high) = self.high.as_mut() {
                    v = high[c].run(v);
                }
                *s = v as f32;
            }
            if let Some(room) = self.room.as_mut() {
                let mono = frame.iter().sum::<f32>() / ch as f32;
                let [l, r] = room.run(mono);
                if ch == 1 {
                    frame[0] = frame[0] * dry + (l + r) * 0.5 * wet_gain;
                } else {
                    frame[0] = frame[0] * dry + l * wet_gain;
                    frame[1] = frame[1] * dry + r * wet_gain;
                    for extra in frame.iter_mut().skip(2) {
                        *extra *= dry;
                    }
                }
            }
        }
    }

    /// Filters and room as `now` says (a filter switched on starts from rest;
    /// one only moved keeps its state, so a slider glides without clicks).
    fn retune(&mut self, now: Sound) {
        let rate = self.rate as f64;
        let ch = self.channels;
        let low_on = now.low_cut > effects::LOW_CUT.0 + 0.5;
        let high_on = now.high_cut < effects::HIGH_CUT.1 - 0.5;
        let fresh = |hp: bool, f: f64| vec![Cut::new(hp, f, rate); ch];
        let moved = |old: &mut Vec<Cut>, hp: bool, f: f64| {
            for cut in old.iter_mut() {
                let new = Cut::new(hp, f, rate);
                for (b, n) in cut.0.iter_mut().zip(new.0) {
                    (b.b0, b.b1, b.b2, b.a1, b.a2) = (n.b0, n.b1, n.b2, n.a1, n.a2);
                }
            }
        };
        match (&mut self.low, low_on) {
            (Some(old), true) => moved(old, true, now.low_cut),
            (None, true) => self.low = Some(fresh(true, now.low_cut)),
            (_, false) => self.low = None,
        }
        match (&mut self.high, high_on) {
            (Some(old), true) => moved(old, false, now.high_cut),
            (None, true) => self.high = Some(fresh(false, now.high_cut)),
            (_, false) => self.high = None,
        }
        if now.reverb >= 0.5 {
            if self.room.is_none() {
                self.room = Some(Room::new(rate));
            }
        } else {
            self.room = None;
        }
        self.set = now;
    }
}

/// The effects on the sound leaving `element` (its src pad: 32-bit float
/// samples), as the Mixer says now; each player has its own state.
pub fn listen(element: &gst::Element) {
    let Some(pad) = element.static_pad("src") else { return };
    let state: Mutex<Option<Fx>> = Mutex::new(None);
    pad.add_probe(gst::PadProbeType::BUFFER, move |pad, info| {
        let now = effects::now();
        let mut fx = state.lock().unwrap();
        if now.is_off() && fx.as_ref().is_none_or(|f| f.set.is_off()) {
            return gst::PadProbeReturn::Ok; // nothing to do, not even a copy
        }
        let Some(caps) = pad.current_caps() else { return gst::PadProbeReturn::Ok };
        let Some(s) = caps.structure(0) else { return gst::PadProbeReturn::Ok };
        if s.get::<&str>("format").ok() != Some("F32LE") {
            return gst::PadProbeReturn::Ok;
        }
        let (Ok(rate), Ok(channels)) = (s.get::<i32>("rate"), s.get::<i32>("channels")) else { return gst::PadProbeReturn::Ok };
        let (rate, channels) = (rate.max(1) as u32, channels.max(1) as usize);
        if fx.as_ref().is_none_or(|f| !f.fits(rate, channels)) {
            *fx = Some(Fx::new(rate, channels));
        }
        let Some(gst::PadProbeData::Buffer(buffer)) = info.data.as_mut() else { return gst::PadProbeReturn::Ok };
        let buffer = buffer.make_mut();
        let Ok(mut map) = buffer.map_writable() else { return gst::PadProbeReturn::Ok };
        let bytes = map.as_mut_slice();
        let mut samples: Vec<f32> = bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
        if let Some(f) = fx.as_mut() {
            f.process(&mut samples, now);
        }
        for (b, v) in bytes.chunks_exact_mut(4).zip(samples) {
            b.copy_from_slice(&v.to_le_bytes());
        }
        gst::PadProbeReturn::Ok
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(freq: f64, rate: f64, n: usize) -> Vec<f32> {
        (0..n).flat_map(|i| {
            let v = (2.0 * std::f64::consts::PI * freq * i as f64 / rate).sin() as f32 * 0.5;
            [v, v]
        }).collect()
    }

    fn rms(x: &[f32]) -> f64 {
        (x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len() as f64).sqrt()
    }

    #[test]
    fn off_leaves_the_sound_untouched() {
        let mut fx = Fx::new(48_000, 2);
        let mut x = tone(440.0, 48_000.0, 4800);
        let was = x.clone();
        fx.process(&mut x, Sound::OFF);
        assert_eq!(x, was);
    }

    #[test]
    fn low_cut_takes_out_the_bass_and_keeps_the_rest() {
        let s = Sound { low_cut: 400.0, ..Sound::OFF };
        let (mut bass, mut voice) = (tone(60.0, 48_000.0, 48_000), tone(3000.0, 48_000.0, 48_000));
        let (b0, v0) = (rms(&bass), rms(&voice));
        Fx::new(48_000, 2).process(&mut bass, s);
        Fx::new(48_000, 2).process(&mut voice, s);
        let tail = |x: &[f32]| rms(&x[x.len() / 2..]);
        assert!(20.0 * (tail(&bass) / b0).log10() < -60.0, "60 Hz down by 24 dB/octave over 2.7 octaves");
        assert!(20.0 * (tail(&voice) / v0).log10() > -0.5, "3 kHz kept");
    }

    #[test]
    fn high_cut_takes_out_the_top() {
        let s = Sound { high_cut: 1500.0, ..Sound::OFF };
        let mut hiss = tone(12_000.0, 48_000.0, 48_000);
        let h0 = rms(&hiss);
        Fx::new(48_000, 2).process(&mut hiss, s);
        assert!(20.0 * (rms(&hiss[hiss.len() / 2..]) / h0).log10() < -60.0);
    }

    #[test]
    fn reverb_rings_on_after_the_sound_stops() {
        let s = Sound { reverb: 50.0, ..Sound::OFF };
        let mut x = tone(500.0, 48_000.0, 4800);
        x.extend(vec![0.0; 2 * 24_000]); // half a second of silence after
        Fx::new(48_000, 2).process(&mut x, s);
        let after = &x[2 * 4800 + 2 * 2400..2 * 4800 + 2 * 12_000];
        assert!(rms(after) > 1e-3, "a tail in the silence");
        assert!(x.iter().all(|v| v.is_finite() && v.abs() < 2.0));
    }
}
