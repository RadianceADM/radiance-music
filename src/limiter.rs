//! The Mixer keeps a split song as loud as it was whole: when parts are turned
//! down, what's left is raised to make up for them (never louder than the
//! whole song, and at most MOST), and a look-ahead limiter keeps every peak
//! under full scale, so nothing clips or crackles. The sound waits LOOKAHEAD,
//! so the limiter can lower the gain smoothly before a peak arrives. It comes
//! back up over RELEASE.

use std::collections::VecDeque;

/// The most the rest is raised: +6 dB.
pub const MOST: f32 = 2.0;
/// The peaks' ceiling while parts are mixed, just under full scale (the
/// song's own file alone goes up to full scale, untouched).
pub const CEILING: f32 = 0.97;
const LOOKAHEAD_S: f32 = 0.008;
const RELEASE_S: f32 = 0.25;
/// A change of the make-up gain glides in over this long (no clicks).
const GLIDE_S: f32 = 0.25;

/// What the make-up gain should be: the whole song's loudness over what's
/// left of it. `parts`: each part's loudness (mean power) and its volume
/// (0-1, as set in the Mixer).
pub fn makeup(parts: &[(f64, f64)]) -> f32 {
    let whole: f64 = parts.iter().map(|(e, _)| e).sum();
    let left: f64 = parts.iter().map(|(e, g)| e * g * g).sum();
    if whole < 1e-9 {
        return 1.0; // silence so far: nothing to make up
    }
    ((whole / left.max(1e-12)).sqrt() as f32).clamp(1.0, MOST)
}

pub struct Limiter {
    channels: usize,
    rate: u32,
    /// The frames waiting (LOOKAHEAD of them), and where the next one goes.
    delay: Vec<f32>,
    at: usize,
    len: usize,
    /// The lowest gain wanted over the look-ahead (frame number, gain), and
    /// that smoothed over the look-ahead (the frames' gains and their sum).
    held: VecDeque<(usize, f32)>,
    smooth: Vec<f32>,
    sum: f64,
    n: usize,
    /// The gain wanted after its release, and the make-up gain now.
    released: f32,
    makeup: f32,
    k_release: f32,
    k_glide: f32,
    /// The lowest gain applied since last asked (checks).
    lowest: f32,
}

impl Limiter {
    pub fn new(channels: usize, rate: u32) -> Limiter {
        let len = ((LOOKAHEAD_S * rate as f32) as usize).max(1);
        let k = |s: f32| 1.0 - (-1.0 / (s * rate as f32)).exp();
        Limiter {
            channels: channels.max(1),
            rate,
            delay: vec![0.0; len * channels.max(1)],
            at: 0,
            len,
            held: VecDeque::new(),
            smooth: vec![1.0; len],
            sum: len as f64,
            n: 0,
            released: 1.0,
            makeup: 1.0,
            k_release: k(RELEASE_S),
            k_glide: k(GLIDE_S),
            lowest: 1.0,
        }
    }

    /// Made for this sound (else a new one is needed).
    pub fn fits(&self, channels: usize, rate: u32) -> bool {
        self.channels == channels && self.rate == rate
    }

    /// The lowest gain since last asked.
    pub fn take_lowest(&mut self) -> f32 {
        std::mem::replace(&mut self.lowest, 1.0)
    }

    /// `samples` (interleaved), raised by `target` (gliding there) and kept
    /// under `ceiling`, in place.
    pub fn process(&mut self, samples: &mut [f32], target: f32, ceiling: f32) {
        let target = target.clamp(1.0, MOST);
        let ceiling = ceiling.clamp(0.5, 1.0);
        let (ch, len) = (self.channels, self.len);
        for frame in samples.chunks_exact_mut(ch) {
            self.makeup += (target - self.makeup) * self.k_glide;
            let peak = frame.iter().fold(0.0f32, |p, s| p.max((s * self.makeup).abs()));
            let wanted = if peak > ceiling { ceiling / peak } else { 1.0 };
            // down at once, back up over the release
            self.released = wanted.min(self.released + (1.0 - self.released) * self.k_release);
            // the lowest over this frame and the look-ahead before it
            let n = self.n;
            while self.held.back().is_some_and(|&(_, g)| g >= self.released) {
                self.held.pop_back();
            }
            self.held.push_back((n, self.released));
            while self.held.front().is_some_and(|&(i, _)| i + len < n) {
                self.held.pop_front();
            }
            let low = self.held.front().map(|&(_, g)| g).unwrap_or(1.0);
            // smoothed over the look-ahead: it's all the way down when the peak comes out
            let slot = n % len;
            self.sum += (low - self.smooth[slot]) as f64;
            self.smooth[slot] = low;
            let gain = (self.sum / len as f64) as f32;
            self.lowest = self.lowest.min(gain);
            let base = self.at * ch;
            for (c, s) in frame.iter_mut().enumerate() {
                let out = self.delay[base + c];
                self.delay[base + c] = *s * self.makeup;
                *s = (out * gain).clamp(-1.0, 1.0);
            }
            self.at = (self.at + 1) % len;
            self.n += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(amp: f32, frames: usize) -> Vec<f32> {
        (0..frames).flat_map(|i| {
            let s = amp * (i as f32 * 2.0 * std::f32::consts::PI * 220.0 / 48_000.0).sin();
            [s, s]
        }).collect()
    }

    #[test]
    fn loud_peaks_never_pass_the_ceiling() {
        let mut l = Limiter::new(2, 48_000);
        let mut s = tone(1.6, 48_000);
        l.process(&mut s, 1.0, CEILING);
        assert!(s.iter().all(|x| x.abs() <= CEILING + 1e-4), "max {}", s.iter().fold(0.0f32, |m, x| m.max(x.abs())));
        // a sudden peak too, after quiet
        let mut s = tone(0.1, 4_800);
        s.extend(tone(1.9, 4_800));
        l.process(&mut s, 1.0, CEILING);
        assert!(s.iter().all(|x| x.abs() <= CEILING + 1e-4));
    }

    #[test]
    fn quiet_sound_passes_untouched_only_later() {
        let mut l = Limiter::new(2, 48_000);
        let input = tone(0.99, 4_800);
        let mut s = input.clone();
        l.process(&mut s, 1.0, 1.0); // the song's own file: up to full scale, untouched
        let lag = 384 * 2; // 8 ms of stereo frames
        for (out, was) in s[lag..].iter().zip(&input) {
            assert!((out - was).abs() < 1e-6);
        }
    }

    #[test]
    fn what_is_left_is_raised_to_the_whole_songs_loudness() {
        // vocals off: the beat is raised so the song is as loud as before
        let parts = [(0.2, 1.0), (0.1, 1.0), (0.1, 1.0), (0.2, 0.0)];
        let m = makeup(&parts);
        assert!((m - (0.6f32 / 0.4).sqrt()).abs() < 1e-4);
        assert_eq!(makeup(&[(0.2, 1.0), (0.1, 1.0)]), 1.0); // all up: as it is
        assert_eq!(makeup(&[(0.2, 0.0), (0.1, 0.0)]), MOST); // all down: at most +6 dB
        assert_eq!(makeup(&[(0.0, 0.3)]), 1.0); // silence
        let mut l = Limiter::new(2, 48_000);
        let mut s = tone(0.2, 48_000);
        l.process(&mut s, 1.5, CEILING);
        let tail = s[24_000..].iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!((tail - 0.3).abs() < 0.01, "raised to {tail}");
    }
}
