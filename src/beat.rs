//! Tempo and beats, the classic way (no AI): decode a stretch of the song, find
//! where new sounds start (spectral flux), find the beat period from how that
//! repeats (autocorrelation), then the beat grid that fits it best. Also where
//! the music really starts and ends (skipping silence), for the mix.

use std::time::Duration;

use gst::prelude::*;
use realfft::RealFftPlanner;

pub(crate) const RATE: usize = 11025;
const WINDOW: usize = 1024;
const HOP: usize = 128;
const FPS: f64 = RATE as f64 / HOP as f64;

#[derive(Clone, Copy, Debug)]
pub struct Beats {
    /// Beats per minute, folded into 85-170.
    pub bpm: f64,
    /// One beat's time in the song (seconds); the others are `period` apart.
    pub beat: f64,
    pub period: f64,
    /// How sure: the beat's repeat strength over the average (above ~1.4 is clear).
    pub strength: f64,
    /// Where the sound starts and ends in the stretch analysed (song seconds).
    pub start: f64,
    pub end: f64,
}

/// Mono samples at RATE from `from` seconds, `length` seconds long (worker thread).
pub(crate) fn decode(uri: &str, from: f64, length: f64) -> Option<Vec<f32>> {
    gst::init().ok()?;
    let desc = format!(
        "uridecodebin uri=\"{uri}\" ! audioconvert ! audioresample ! audio/x-raw,format=F32LE,channels=1,rate={RATE} ! appsink name=sink sync=false"
    );
    let pipeline = gst::parse::launch(&desc).ok()?.downcast::<gst::Pipeline>().ok()?;
    let sink = pipeline.by_name("sink")?.downcast::<gst_app::AppSink>().ok()?;
    let bus = pipeline.bus()?;
    let wait = |what: gst::MessageType| {
        bus.timed_pop_filtered(gst::ClockTime::from_seconds(20), &[what, gst::MessageType::Error])
            .is_some_and(|m| m.type_() == what)
    };
    let _ = pipeline.set_state(gst::State::Paused);
    let mut ok = wait(gst::MessageType::AsyncDone);
    if ok && from > 0.0 {
        let at = gst::ClockTime::from_nseconds((from * 1e9) as u64);
        ok = pipeline.seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE, at).is_ok() && wait(gst::MessageType::AsyncDone);
    }
    let mut out: Vec<f32> = vec![];
    if ok {
        let _ = pipeline.set_state(gst::State::Playing);
        let want = (length * RATE as f64) as usize;
        while out.len() < want {
            let Some(sample) = sink.try_pull_sample(gst::ClockTime::from_seconds(10)) else { break };
            let Some(buf) = sample.buffer() else { continue };
            let Ok(map) = buf.map_readable() else { continue };
            out.extend(map.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])));
        }
    }
    let _ = pipeline.set_state(gst::State::Null);
    std::thread::sleep(Duration::from_millis(10));
    (out.len() > RATE * 8).then_some(out)
}

/// Each frame's spectral flux up to 5 kHz (the low end counting double), the
/// kick's own flux (below 150 Hz), loudness (RMS) and how much bass there is
/// (below 150 Hz).
fn fluxes(samples: &[f32]) -> Option<(Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>)> {
    let frames = samples.len().checked_sub(WINDOW)? / HOP;
    let mut planner = RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(WINDOW);
    let window: Vec<f32> = (0..WINDOW).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / WINDOW as f32).cos()).collect();
    let mut input = fft.make_input_vec();
    let mut spectrum = fft.make_output_vec();
    let bins = WINDOW / 2 * 5000 / (RATE / 2); // up to 5 kHz: kick, snare, hats' attack
    let kick = WINDOW * 150 / RATE; // below 150 Hz
    let mut last = vec![0f32; bins];
    let mut flux = vec![0f32; frames];
    let mut low = vec![0f32; frames];
    let mut loud = vec![0f32; frames];
    let mut bass = vec![0f32; frames];
    for f in 0..frames {
        let chunk = &samples[f * HOP..f * HOP + WINDOW];
        for (i, s) in chunk.iter().enumerate() {
            input[i] = s * window[i];
        }
        loud[f] = (chunk.iter().map(|s| s * s).sum::<f32>() / WINDOW as f32).sqrt();
        fft.process(&mut input, &mut spectrum).ok()?;
        let (mut sum, mut kick_sum) = (0.0, 0.0);
        bass[f] = (1.0 + spectrum.iter().take(kick + 1).skip(1).map(|c| c.norm()).sum::<f32>()).ln();
        for (b, c) in spectrum.iter().take(bins).enumerate() {
            let m = (1.0 + 100.0 * c.norm()).ln();
            let rise = m - last[b];
            if rise > 0.0 {
                sum += if b < bins / 20 { 2.0 * rise } else { rise }; // the low end (kick) counts double
                if (1..=kick).contains(&b) {
                    kick_sum += rise;
                }
            }
            last[b] = m;
        }
        flux[f] = sum;
        low[f] = kick_sum;
    }
    Some((flux, low, loud, bass))
}

/// Flux above its local average (a quarter second each side): where sounds start.
fn onsets(flux: &[f32]) -> Vec<f32> {
    let frames = flux.len();
    let half = (FPS * 0.25) as usize;
    (0..frames)
        .map(|f| {
            let (a, b) = (f.saturating_sub(half), (f + half).min(frames - 1));
            let mean = flux[a..=b].iter().sum::<f32>() / (b - a + 1) as f32;
            (flux[f] - mean).max(0.0)
        })
        .collect()
}

/// The song's beat as light: how hard each moment hits, 0-1, `fps` a second.
/// Mostly the kick, some of every other sound's attack; it rises at once and
/// falls away within a fifth of a second, like a light on a beat.
pub struct Pulse {
    pub fps: f64,
    /// The time of the first value (the analysis window's middle), seconds.
    pub offset: f64,
    pub level: Vec<f32>,
    /// How much bass there is, 0-1 (the song's own quiet bass 0, its strong bass 1).
    pub bass: Vec<f32>,
}

impl Pulse {
    /// How hard the music hits at `at` seconds into the song.
    pub fn at(&self, at: f64) -> f32 {
        self.value(&self.level, at)
    }

    /// How much bass there is at `at` seconds into the song.
    pub fn bass_at(&self, at: f64) -> f32 {
        self.value(&self.bass, at)
    }

    fn value(&self, of: &[f32], at: f64) -> f32 {
        let i = ((at - self.offset) * self.fps).round();
        if i < 0.0 {
            return 0.0;
        }
        of.get(i as usize).copied().unwrap_or(0.0)
    }
}

/// The whole song's pulse (worker thread).
pub fn pulse(uri: &str) -> Option<Pulse> {
    let samples = decode(uri, 0.0, 3600.0)?;
    let (flux, low, _, bass) = fluxes(&samples)?;
    drop(samples);
    // each part of it scaled by its own strong hits (the 95th percentile)
    let scaled = |o: Vec<f32>| -> Vec<f32> {
        let mut sorted: Vec<f32> = o.iter().copied().filter(|v| *v > 0.0).collect();
        sorted.sort_by(f32::total_cmp);
        let top = sorted.get(sorted.len() * 95 / 100).copied().unwrap_or(1.0).max(1e-6);
        o.into_iter().map(|v| (v / top).min(1.0)).collect()
    };
    let (all, kick) = (scaled(onsets(&flux)), scaled(onsets(&low)));
    let fall = (-1.0 / (FPS * 0.18)).exp() as f32;
    let mut level = Vec::with_capacity(all.len());
    let mut now = 0f32;
    for (a, k) in all.iter().zip(&kick) {
        let hit = (0.65 * k + 0.35 * a).min(1.0);
        now = hit.max(now * fall);
        level.push(now);
    }
    // the bass between the song's own quiet (20th percentile) and strong (95th)
    let mut sorted = bass.clone();
    sorted.sort_by(f32::total_cmp);
    let pick = |p: usize| sorted.get(sorted.len() * p / 100).copied().unwrap_or(0.0);
    let (low_bass, high_bass) = (pick(20), pick(95).max(pick(20) + 1e-3));
    let bass = bass.into_iter().map(|b| ((b - low_bass) / (high_bass - low_bass)).clamp(0.0, 1.0)).collect();
    Some(Pulse { fps: FPS, offset: WINDOW as f64 / 2.0 / RATE as f64, level, bass })
}

/// Analyses `length` seconds of a song from `from` (worker thread).
pub fn analyse(uri: &str, from: f64, length: f64) -> Option<Beats> {
    let samples = decode(uri, from, length)?;
    let (flux, _, loud, _) = fluxes(&samples)?;
    let frames = flux.len();
    let onset = onsets(&flux);

    // the period: autocorrelation over 70-180 BPM, leaning towards ~120
    let (lo, hi) = ((FPS * 60.0 / 180.0) as usize, (FPS * 60.0 / 70.0) as usize + 1);
    let ac = |lag: usize| -> f32 { onset.iter().zip(&onset[lag..]).map(|(a, b)| a * b).sum() };
    let scores: Vec<(usize, f32)> = (lo..=hi).map(|lag| (lag, ac(lag))).collect();
    let mean = scores.iter().map(|s| s.1).sum::<f32>() / scores.len() as f32;
    let weight = |lag: f64| {
        let bpm = 60.0 * FPS / lag;
        (-0.5 * ((bpm / 120.0).log2() / 0.9).powi(2)).exp()
    };
    let &(best, peak) = scores.iter().max_by(|a, b| (a.1 as f64 * weight(a.0 as f64)).total_cmp(&(b.1 as f64 * weight(b.0 as f64))))?;
    if mean <= 0.0 {
        return None;
    }
    // a finer period between the neighbouring lags
    let (l, r) = (ac(best - 1) as f64, ac(best + 1) as f64);
    let denom = l - 2.0 * peak as f64 + r;
    let lag = best as f64 + if denom.abs() > 1e-9 { 0.5 * (l - r) / denom } else { 0.0 };
    let mut bpm = 60.0 * FPS / lag;
    while bpm < 85.0 {
        bpm *= 2.0;
    }
    while bpm > 170.0 {
        bpm /= 2.0;
    }
    let period_frames = 60.0 * FPS / bpm;

    // the grid: the offset whose beats land on the most onsets
    let phase = (0..period_frames.ceil() as usize)
        .max_by(|&a, &b| {
            let score = |p: usize| -> f32 {
                let mut s = 0.0;
                let mut t = p as f64;
                while (t as usize) < frames {
                    s += onset[t as usize];
                    t += period_frames;
                }
                s
            };
            score(a).total_cmp(&score(b))
        })
        .unwrap_or(0);

    // where the sound is: within 30 dB of its loudest
    let top = loud.iter().cloned().fold(0f32, f32::max);
    let floor = top * 10f32.powf(-30.0 / 20.0);
    let first = loud.iter().position(|&v| v > floor).unwrap_or(0);
    let last_loud = loud.iter().rposition(|&v| v > floor).unwrap_or(frames - 1);
    let at = |frame: f64| from + (frame * HOP as f64 + WINDOW as f64 / 2.0) / RATE as f64;
    Some(Beats {
        bpm,
        beat: at(phase as f64),
        period: 60.0 / bpm,
        strength: peak as f64 / mean as f64,
        start: at(first as f64),
        end: at(last_loud as f64),
    })
}

/// How to go from one song to the next.
#[derive(Clone, Copy, Debug)]
pub struct Plan {
    /// When to start the next song, in the current song's time.
    pub at: f64,
    /// Where the next song starts (its first beat or first sound).
    pub from: f64,
    /// The next song's tempo, against its own (pitch kept).
    pub rate: f64,
    pub seconds: f64,
    /// Beats lined up (a DJ mix with a bass swap), or a smooth crossfade.
    pub beat_matched: bool,
}

/// A DJ mix when the tempos are close enough (within 8 %): 16 beats long, the
/// next song's first beat on a beat of this one, its tempo matched. Otherwise a
/// crossfade of `fade` seconds from this song's last sound to the next one's first.
pub fn plan(a: Option<Beats>, b: Option<Beats>, a_length: f64, fade: f64) -> Plan {
    let a_end = a.map(|x| x.end.min(a_length)).unwrap_or(a_length);
    let b_start = b.map(|x| x.start).unwrap_or(0.0);
    let crossfade = Plan { at: (a_end - fade).max(0.0), from: b_start, rate: 1.0, seconds: fade, beat_matched: false };
    let (Some(a), Some(b)) = (a, b) else { return crossfade };
    if a.strength < 1.35 || b.strength < 1.35 {
        return crossfade; // no clear beat in one of them
    }
    let ratio = a.bpm / b.bpm;
    if (ratio - 1.0).abs() > 0.08 {
        return crossfade;
    }
    let rate = if (ratio - 1.0).abs() < 0.015 { 1.0 } else { ratio };
    let seconds = (16.0 * a.period).clamp(5.0, 16.0);
    // this song: the last beat that leaves room for the whole mix
    let k = ((a_end - seconds - a.beat) / a.period).floor();
    let at = a.beat + k * a.period;
    // the next song: its first beat once the sound has started
    let j = ((b.start - b.beat) / b.period).ceil();
    let from = (b.beat + j * b.period).max(0.0);
    if at <= 0.0 {
        return crossfade;
    }
    Plan { at, from, rate, seconds, beat_matched: true }
}
