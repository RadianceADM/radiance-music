//! Playback: GStreamer's playbin3 into PipeWire (through its PulseAudio side),
//! at the song's own sample rate and bit depth.
//!
//! Two decks, like a DJ's: one plays, the other can cue the next song (silent,
//! ready at the right spot, at the right tempo) so the two can be mixed. During
//! a mix the volumes cross in equal power (it never gets louder) and, in a DJ
//! mix, the bass swaps from the old song to the new one halfway through.

use std::cell::{Cell, RefCell};
use std::f64::consts::FRAC_PI_2;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::limiter::{makeup, Limiter, CEILING};

use gst::glib;
use gst::prelude::*;

pub enum Event {
    /// Started, or settled after a seek: the output format is known now.
    Ready,
    Ended,
    Error(String),
}

/// Bass cut on the side being swapped out (dB, the EQ's lowest band).
const BASS_CUT: f64 = -24.0;

struct Deck {
    /// Its player and sound output; both new after a stop (see `reset`).
    playbin: RefCell<gst::Element>,
    sink: RefCell<gst::Element>,
    /// Its own volume, in the app, before the sound output: fades and the app's
    /// volume move this, never the sound server's volume for the app (playbin's
    /// "volume" is the server's: a fade to 0 there was remembered, and the next
    /// song started silent).
    level: RefCell<gst::Element>,
    /// The mixing filters' EQ (tempo + 3-band EQ), only on a deck cued for a DJ mix.
    eq: RefCell<Option<gst::Element>>,
    /// A cue waiting for the song to load: (seconds, tempo rate).
    pending: Cell<Option<(f64, f64)>>,
    /// A song loaded to start part-way (the other half of a split song).
    /// Where to start once loaded; with when it was asked for, the song moves
    /// on meanwhile (a switch plays onward, never back).
    start_at: Cell<Option<(f64, Option<Instant>)>>,
    cued: Cell<bool>,
    /// Playing where it was asked to (after its seek, if any).
    ready: Cell<bool>,
    bus: RefCell<Option<gst::bus::BusWatchGuard>>,
    /// What it's made from again: its place, the deck playing, both decks, the player's events.
    i: usize,
    active: Rc<Cell<usize>>,
    decks: Rc<RefCell<Vec<Rc<Deck>>>>,
    on_event: Rc<dyn Fn(Event)>,
}

/// Stops a player on a worker thread and lets it go there: stopping waits on the
/// sound server, and one that doesn't answer (an EQ chain that won't start)
/// would hold up the window for its whole 30 s timeout.
fn retire(e: gst::Element) {
    let keep = e.clone();
    if std::thread::Builder::new().name("stop a sound".into()).spawn(move || {
            let _ = e.set_state(gst::State::Null);
        })
        .is_err() {
        let _ = keep.set_state(gst::State::Null);
    }
}

/// A deck's player, its sound output, its own volume and its messages.
fn build(i: usize, active: Rc<Cell<usize>>, decks: Rc<RefCell<Vec<Rc<Deck>>>>, on_event: Rc<dyn Fn(Event)>) -> Result<(gst::Element, gst::Element, gst::Element, gst::bus::BusWatchGuard), String> {
        let playbin = gst::ElementFactory::make("playbin3").build().map_err(|e| e.to_string())?;
        // RADIANCE_SILENT: no sound at all (tests)
        let silent = std::env::var_os("RADIANCE_SILENT").is_some();
        let sink = gst::ElementFactory::make(if silent { "fakesink" } else { "pulsesink" }).build().map_err(|e| e.to_string())?;
        if silent {
            sink.set_property("sync", true);
        }
        let props = gst::Structure::builder("props")
            .field("media.role", "music")
            .field("application.name", "Radiance Music")
            .field("application.icon_name", "io.github.radianceadm.Music")
            .build();
        if !silent {
            sink.set_property("stream-properties", &props);
            sink.set_property("client-name", "Radiance Music");
        }
        // the deck's own volume, then the sound output, as the player's audio sink
        let level = gst::ElementFactory::make("volume").name("level").build().map_err(|e| e.to_string())?;
        let out = gst::Bin::builder().name("out").build();
        out.add_many([&level, &sink]).map_err(|e| e.to_string())?;
        level.link(&sink).map_err(|e| e.to_string())?;
        let pad = level.static_pad("sink").ok_or("no pad")?;
        out.add_pad(&gst::GhostPad::with_target(&pad).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        playbin.set_property("audio-sink", &out);
        // songs only: a cover picture inside a file never opens a video window
        let no_video = gst::ElementFactory::make("fakesink").build().map_err(|e| e.to_string())?;
        playbin.set_property("video-sink", &no_video);
        let bus = playbin.bus().ok_or("no bus")?;
        let guard = bus
            .add_watch_local(move |_, msg| {
                use gst::MessageView;
                if std::env::var_os("RADIANCE_DEBUG").is_some() {
                    eprintln!("[deck {i}] {:?} from {:?}", msg.type_(), msg.src().map(|s| s.name()));
                }
                let deck = decks.borrow().get(i).cloned();
                let is_active = active.get() == i;
                match msg.view() {
                    MessageView::AsyncDone(_) if is_active => {
                        if let Some((at, since)) = deck.as_ref().and_then(|d| d.start_at.take()) {
                            deck.as_ref().map(|d| d.seek_to(onward(at, since), 1.0)); // then "ready" once there
                        } else {
                            if let Some(d) = deck.as_ref() {
                                d.ready.set(true);
                            }
                            on_event(Event::Ready);
                        }
                    }
                    MessageView::AsyncDone(_) => {
                        // a cue: once loaded, go to the spot; then it's ready
                        if let Some(d) = deck {
                            match d.pending.take() {
                                Some((at, rate)) => d.seek_to(at, rate),
                                None => d.cued.set(true),
                            }
                        }
                    }
                    MessageView::Eos(_) if is_active => on_event(Event::Ended),
                    MessageView::Error(e) if is_active => on_event(Event::Error(e.error().to_string())),
                    MessageView::Error(_) => {
                        if let Some(d) = deck {
                            d.reset(); // a cue that failed: no mix, the next song starts normally
                        }
                    }
                    _ => {}
                }
                glib::ControlFlow::Continue
            })
            .map_err(|e| e.to_string())?;
        Ok((playbin, sink, level, guard))
}

impl Deck {
    fn new(i: usize, active: Rc<Cell<usize>>, decks: Rc<RefCell<Vec<Rc<Deck>>>>, on_event: Rc<dyn Fn(Event)>) -> Result<Rc<Deck>, String> {
        let (playbin, sink, level, guard) = build(i, active.clone(), decks.clone(), on_event.clone())?;
        Ok(Rc::new(Deck {
            playbin: RefCell::new(playbin),
            sink: RefCell::new(sink),
            level: RefCell::new(level),
            eq: RefCell::new(None),
            pending: Cell::new(None),
            start_at: Cell::new(None),
            ready: Cell::new(false),
            cued: Cell::new(false),
            bus: RefCell::new(Some(guard)),
            i,
            active,
            decks,
            on_event,
        }))
    }

    fn pb(&self) -> gst::Element {
        self.playbin.borrow().clone()
    }

    fn sink(&self) -> gst::Element {
        self.sink.borrow().clone()
    }

    /// The deck's own volume (0-1, the app's: see `level`).
    fn set_level(&self, v: f64) {
        self.level.borrow().set_property("volume", v);
    }

    fn level(&self) -> f64 {
        self.level.borrow().property::<f64>("volume")
    }

    /// Stopped, with nothing loaded. One that's busy goes to a worker thread to
    /// stop (see `retire`) and a new player takes its place at once.
    fn reset(&self) {
        let pb = self.pb();
        let idle = pb.current_state() == gst::State::Null && pb.pending_state() == gst::State::VoidPending;
        if !idle {
            match build(self.i, self.active.clone(), self.decks.clone(), self.on_event.clone()) {
                Ok((playbin, sink, level, guard)) => {
                    *self.playbin.borrow_mut() = playbin;
                    *self.sink.borrow_mut() = sink;
                    *self.level.borrow_mut() = level;
                    *self.bus.borrow_mut() = Some(guard); // the old one's messages no longer count
                    *self.eq.borrow_mut() = None;
                    retire(pb);
                }
                Err(_) => {
                    let _ = pb.set_state(gst::State::Null);
                }
            }
        }
        self.set_level(1.0f64);
        self.pending.set(None);
        self.cued.set(false);
        self.ready.set(false);
    }

    fn seek_to(&self, at: f64, rate: f64) {
        let at = gst::ClockTime::from_nseconds((at.max(0.0) * 1e9) as u64);
        let flags = gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE;
        let _ = self.pb().seek(rate, flags, gst::SeekType::Set, at, gst::SeekType::None, gst::ClockTime::NONE);
    }

    /// Tempo + EQ filters for a DJ mix, or none (the untouched signal).
    fn set_filters(&self, dj: bool) {
        if dj {
            if let Ok(bin) = gst::parse::bin_from_description("scaletempo ! audioconvert ! equalizer-3bands name=eq", true) {
                *self.eq.borrow_mut() = bin.by_name("eq");
                self.pb().set_property("audio-filter", bin.upcast_ref::<gst::Element>());
                return;
            }
        }
        *self.eq.borrow_mut() = None;
        self.pb().set_property("audio-filter", None::<&gst::Element>);
    }

    fn set_bass(&self, db: f64) {
        if let Some(eq) = self.eq.borrow().as_ref() {
            eq.set_property("band0", db);
        }
    }
}

/// A split song's four parts played together, each at its own volume.
/// A split song with its parts at hand: its own file and its four parts play
/// together, in step (the mixer lines them up sample for sample: they're the
/// same recording), so moving a Mixer slider never loads anything. With every
/// part up, the song's own file is heard, untouched; turning one down blends
/// over to the parts (the two sound the same, so the blend can't be heard;
/// only the slider's change is). Stem players do the same: everything plays,
/// only volumes change.
struct StemPipe {
    pipeline: gst::Pipeline,
    master: gst::Element,
    parts: std::collections::HashMap<String, gst::Element>,
    /// The song's own file (None: only its parts were at hand).
    whole: Option<gst::Element>,
    /// The Mixer's volumes, how much of the song's own file is heard now (1:
    /// all of it, 0: the parts), and the blend under way.
    gains: RefCell<std::collections::HashMap<String, f64>>,
    blend: Rc<Cell<f64>>,
    whole_on: Cell<bool>,
    blending: RefCell<Option<glib::SourceId>>,
    keep: Arc<Keep>,
    ready: Rc<Cell<bool>>,
    /// Its events count: it's the song playing (not while cued or fading out).
    live: Rc<Cell<bool>>,
    _bus: gst::bus::BusWatchGuard,
}

/// How long the blend between the song's own file and its parts takes.
const BLEND_S: f64 = 0.3;

impl StemPipe {
    /// Every volume as the blend and the sliders say.
    fn apply(&self) {
        let w = self.blend.get();
        if let Some(e) = &self.whole {
            e.set_property("volume", w);
        }
        for (name, e) in &self.parts {
            let g = self.gains.borrow().get(name).copied().unwrap_or(1.0);
            e.set_property("volume", g * (1.0 - w));
        }
        self.keep.whole.store(w.to_bits(), Ordering::Relaxed);
    }

    /// The Mixer's volumes now, all at once (a cued song isn't heard yet: no blend).
    fn retune(&self, gains: &std::collections::HashMap<String, f64>) {
        for (name, g) in gains {
            if self.parts.contains_key(name) {
                self.gains.borrow_mut().insert(name.clone(), g.clamp(0.0, 1.0));
                if let Some(level) = self.keep.parts.lock().unwrap().get(name) {
                    level.set_gain(g.clamp(0.0, 1.0));
                }
            }
        }
        let all_up = self.parts.keys().all(|n| self.gains.borrow().get(n).copied().unwrap_or(1.0) >= 0.999);
        let on = all_up && self.whole.is_some();
        if let Some(id) = self.blending.borrow_mut().take() {
            id.remove();
        }
        self.whole_on.set(on);
        self.blend.set(if on { 1.0 } else { 0.0 });
        self.apply();
    }

    /// Blends to the song's own file (true) or to its parts, over BLEND_S.
    fn set_whole(self_: &Rc<StemPipe>, on: bool) {
        let on = on && self_.whole.is_some();
        if self_.whole_on.replace(on) == on && self_.blending.borrow().is_none() {
            return;
        }
        if let Some(id) = self_.blending.borrow_mut().take() {
            id.remove();
        }
        let (from, to) = (self_.blend.get(), if on { 1.0 } else { 0.0 });
        let start = Instant::now();
        let me = Rc::downgrade(self_);
        let id = glib::timeout_add_local(Duration::from_millis(10), move || {
            let Some(me) = me.upgrade() else { return glib::ControlFlow::Break };
            let x = (start.elapsed().as_secs_f64() / BLEND_S).min(1.0);
            me.blend.set(from + (to - from) * x);
            me.apply();
            if x >= 1.0 {
                me.blending.borrow_mut().take();
                return glib::ControlFlow::Break;
            }
            glib::ControlFlow::Continue
        });
        *self_.blending.borrow_mut() = Some(id);
    }
}

/// A switch between the two forms of the song playing (its file, its parts,
/// or one of its halves): the one playing fades out over SWITCH_OUT_S, then,
/// once the other is ready (playing on from where the song has got to), it
/// fades in over SWITCH_IN_S. Never both at once (no beat heard twice); a
/// moment's quiet between them instead. Both follow the volume slider's own
/// curve, so they sound even.
const SWITCH_OUT_S: f64 = 0.6;
const SWITCH_IN_S: f64 = 1.2;
/// The longest the quiet may last before the new one comes in regardless.
const SWITCH_WAIT_S: f64 = 15.0;

/// The sound fading out in a switch.
enum Leaving {
    Deck(Rc<Deck>),
    Pipe { pipeline: gst::Element, volume: gst::Element },
}

impl Leaving {
    fn set_volume(&self, v: f64) {
        match self {
            Leaving::Deck(d) => d.set_level(v),
            Leaving::Pipe { volume, .. } => volume.set_property("volume", v),
        }
    }

    fn finish(self) {
        match self {
            Leaving::Deck(d) => d.reset(),
            Leaving::Pipe { pipeline, .. } => retire(pipeline),
        }
    }
}

/// The sound fading in: its volume, and whether it's ready.
enum Coming {
    Deck(Rc<Deck>),
    Stems { volume: gst::Element, ready: Rc<Cell<bool>> },
}

impl Coming {
    fn set_volume(&self, v: f64) {
        match self {
            Coming::Deck(d) => d.set_level(v),
            Coming::Stems { volume, .. } => volume.set_property("volume", v),
        }
    }

    fn ready(&self) -> bool {
        match self {
            Coming::Deck(d) => d.ready.get(),
            Coming::Stems { ready, .. } => ready.get(),
        }
    }
}

/// A crossfade between two songs: its clock, the song going and the one coming.
struct Mixing {
    timer: glib::SourceId,
    old: Rc<RefCell<Option<Leaving>>>,
    new: Coming,
}

impl Leaving {
    fn set_state(&self, state: gst::State) {
        let _ = match self {
            Leaving::Deck(d) => d.pb().set_state(state),
            Leaving::Pipe { pipeline, .. } => pipeline.set_state(state),
        };
    }
}

impl Coming {
    /// The same sound, for a second owner (the crossfade's clock).
    fn clone_volume(&self) -> Coming {
        match self {
            Coming::Deck(d) => Coming::Deck(d.clone()),
            Coming::Stems { volume, ready } => Coming::Stems { volume: volume.clone(), ready: ready.clone() },
        }
    }
}

struct Switch {
    timer: Option<glib::SourceId>,
    leaving: Option<Leaving>,
    coming: Coming,
}

/// Where a switch starts: where the song was, plus however long the new one
/// took to load (and a tenth of a second for the seek itself), if it moves on.
fn onward(at: f64, since: Option<Instant>) -> f64 {
    let moved = since.map(|s| s.elapsed().as_secs_f64() + 0.1).unwrap_or(0.0);
    if moved > 0.0 {
        crate::log::note(&format!("switched onward: from {at:.1} s to {:.1} s (it took {:.1} s to get ready)", at + moved, moved - 0.1));
    }
    at + moved
}

/// One part's loudness (its mean power over the last few seconds, before its
/// Mixer volume) and that volume; written by the sound's own threads.
struct PartLevel {
    power: AtomicU64,
    gain: AtomicU64,
    /// How much of it has been heard since it started or was sought (seconds):
    /// until LOUDNESS_S, the average is of all of it.
    heard: AtomicU64,
}

/// How long a part's loudness is averaged over.
const LOUDNESS_S: f64 = 8.0;

impl PartLevel {
    fn new(gain: f64) -> PartLevel {
        PartLevel { power: AtomicU64::new(0f64.to_bits()), gain: AtomicU64::new(gain.to_bits()), heard: AtomicU64::new(0f64.to_bits()) }
    }

    fn set_gain(&self, gain: f64) {
        self.gain.store(gain.to_bits(), Ordering::Relaxed);
    }

    fn now(&self) -> (f64, f64) {
        (f64::from_bits(self.power.load(Ordering::Relaxed)), f64::from_bits(self.gain.load(Ordering::Relaxed)))
    }

    /// Measures the sound going into `volume` (the part's Mixer volume).
    fn listen(self: Arc<Self>, volume: &gst::Element) {
        let Some(pad) = volume.static_pad("sink") else { return };
        pad.add_probe(gst::PadProbeType::BUFFER | gst::PadProbeType::EVENT_FLUSH, move |pad, info| {
            if let Some(gst::PadProbeData::Event(e)) = info.data.as_ref() {
                if e.type_() == gst::EventType::FlushStop {
                    self.heard.store(0f64.to_bits(), Ordering::Relaxed); // sought: heard afresh
                }
                return gst::PadProbeReturn::Ok;
            }
            let (Some(gst::PadProbeData::Buffer(buffer)), Some((rate, _))) = (info.data.as_ref(), format_of(pad)) else { return gst::PadProbeReturn::Ok };
            let Ok(map) = buffer.map_readable() else { return gst::PadProbeReturn::Ok };
            let samples = map.as_slice().chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64);
            let (sum, n) = samples.fold((0.0, 0usize), |(s, n), x| (s + x * x, n + 1));
            if n > 0 {
                let (mean, dt) = (sum / n as f64, n as f64 / rate as f64);
                let heard = f64::from_bits(self.heard.load(Ordering::Relaxed)) + dt;
                self.heard.store(heard.to_bits(), Ordering::Relaxed);
                // all of it so far at first, then the last few seconds
                let k = (dt / heard).max(1.0 - (-dt / LOUDNESS_S).exp());
                let was = self.now().0;
                self.power.store((was + (mean - was) * k).to_bits(), Ordering::Relaxed);
            }
            gst::PadProbeReturn::Ok
        });
    }
}

/// The parts' loudness, and the make-up gain + limiter after the mix.
struct Keep {
    parts: Mutex<std::collections::HashMap<String, Arc<PartLevel>>>,
    limiter: Mutex<(Option<Limiter>, Vec<f32>)>,
    /// The make-up gain lately, and the loudest sample out since last asked (for checks).
    target: AtomicU64,
    peak: AtomicU64,
    /// The lowest limiter gain since last asked (1: it didn't touch the sound).
    squeeze: AtomicU64,
    /// How much of the song's own file is heard (it needs no make-up, and goes up to full scale).
    whole: AtomicU64,
}

impl Default for Keep {
    fn default() -> Keep {
        Keep { parts: Mutex::default(), limiter: Mutex::default(), target: AtomicU64::new(1f64.to_bits()), peak: AtomicU64::new(0), squeeze: AtomicU64::new(1f64.to_bits()), whole: AtomicU64::new(0) }
    }
}

impl Keep {
    /// Raises and limits the mix as it leaves `keeper` (see limiter.rs).
    fn listen(self: Arc<Self>, keeper: &gst::Element) {
        let Some(pad) = keeper.static_pad("src") else { return };
        pad.add_probe(gst::PadProbeType::BUFFER, move |pad, info| {
            let Some((rate, channels)) = format_of(pad) else { return gst::PadProbeReturn::Ok };
            let w = f64::from_bits(self.whole.load(Ordering::Relaxed)).clamp(0.0, 1.0) as f32;
            let target = 1.0 + (makeup(&self.parts.lock().unwrap().values().map(|p| p.now()).collect::<Vec<_>>()) - 1.0) * (1.0 - w);
            let ceiling = CEILING + (1.0 - CEILING) * w;
            let Some(gst::PadProbeData::Buffer(buffer)) = info.data.as_mut() else { return gst::PadProbeReturn::Ok };
            let buffer = buffer.make_mut();
            let Ok(mut map) = buffer.map_writable() else { return gst::PadProbeReturn::Ok };
            let mut state = self.limiter.lock().unwrap();
            let (limiter, samples) = &mut *state;
            if limiter.as_ref().is_none_or(|l| !l.fits(channels, rate)) {
                *limiter = Some(Limiter::new(channels, rate));
            }
            let bytes = map.as_mut_slice();
            samples.clear();
            samples.extend(bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])));
            if let Some(l) = limiter.as_mut() {
                l.process(samples, target, ceiling);
                let low = f64::from_bits(self.squeeze.load(Ordering::Relaxed));
                self.squeeze.store(low.min(l.take_lowest() as f64).to_bits(), Ordering::Relaxed);
            }
            let mut loudest = f64::from_bits(self.peak.load(Ordering::Relaxed));
            for (b, x) in bytes.chunks_exact_mut(4).zip(samples.iter()) {
                b.copy_from_slice(&x.to_le_bytes());
                loudest = loudest.max(x.abs() as f64);
            }
            self.peak.store(loudest.to_bits(), Ordering::Relaxed);
            self.target.store((target as f64).to_bits(), Ordering::Relaxed);
            gst::PadProbeReturn::Ok
        });
    }
}

/// A pad's sample rate and channels (F32LE, as the stems pipeline asks).
fn format_of(pad: &gst::Pad) -> Option<(u32, usize)> {
    let caps = pad.current_caps()?;
    let s = caps.structure(0)?;
    Some((s.get::<i32>("rate").ok()? as u32, s.get::<i32>("channels").ok()? as usize))
}

/// A video: its picture and its sound (two streams) played together, in sync.
struct VideoPipe {
    pipeline: gst::Pipeline,
    volume: gst::Element,
    _bus: gst::bus::BusWatchGuard,
}

pub struct Player {
    decks: Rc<RefCell<Vec<Rc<Deck>>>>,
    active: Rc<Cell<usize>>,
    mixing: Rc<RefCell<Option<Mixing>>>,
    /// The app's own volume (linear gain, at most 1: never louder than the system).
    master: Rc<Cell<f64>>,
    video: RefCell<Option<VideoPipe>>,
    stems: RefCell<Option<Rc<StemPipe>>>,
    /// The next song with its parts at hand, cued for a crossfade.
    cued_parts: RefCell<Option<Rc<StemPipe>>>,
    /// A switch fading one form of the song out and the other in.
    switch: Rc<RefCell<Option<Switch>>>,
    on_event: Rc<dyn Fn(Event)>,
}

impl Player {
    pub fn new(on_event: impl Fn(Event) + 'static) -> Result<Player, String> {
        gst::init().map_err(|e| e.to_string())?;
        // Windows: the GTK 4 video sink is inside the app (music videos)
        #[cfg(windows)]
        let _ = gstgtk4::plugin_register_static();
        let on_event: Rc<dyn Fn(Event)> = Rc::new(on_event);
        let active = Rc::new(Cell::new(0));
        let decks: Rc<RefCell<Vec<Rc<Deck>>>> = Rc::default();
        for i in 0..2 {
            let d = Deck::new(i, active.clone(), decks.clone(), on_event.clone())?;
            decks.borrow_mut().push(d);
        }
        Ok(Player { decks, active, mixing: Rc::default(), master: Rc::new(Cell::new(1.0)), video: RefCell::new(None), stems: RefCell::new(None), cued_parts: RefCell::new(None), switch: Rc::default(), on_event })
    }

    fn deck(&self) -> Rc<Deck> {
        self.decks.borrow()[self.active.get()].clone()
    }

    fn other(&self) -> Rc<Deck> {
        self.decks.borrow()[1 - self.active.get()].clone()
    }

    /// What's playing, taken out to fade away on its own (a switch): the parts'
    /// pipeline, or the deck playing (the other deck takes over). None: nothing
    /// playing (or a video), nothing to fade.
    fn leave(&self) -> Option<Leaving> {
        if let Some(m) = self.stems.borrow_mut().take() {
            m.live.set(false);
            return Some(Leaving::Pipe { pipeline: m.pipeline.clone().upcast(), volume: m.master.clone() });
        }
        if self.video.borrow().is_some() {
            return None;
        }
        let d = self.deck();
        let pb = d.pb();
        if pb.current_state() != gst::State::Playing && pb.pending_state() != gst::State::Playing {
            return None;
        }
        self.other().reset();
        self.active.set(1 - self.active.get());
        Some(Leaving::Deck(d))
    }

    /// Fades `leaving` out, then `coming` in once it's ready (see Switch).
    fn fade_over(&self, leaving: Leaving, coming: Coming) {
        coming.set_volume(0.0);
        let start = Instant::now();
        let (master, switch) = (self.master.clone(), self.switch.clone());
        let mut came: Option<Instant> = None;
        crate::log::note("switch: fading out, then in once it's ready");
        let id = glib::timeout_add_local(Duration::from_millis(20), move || {
            let mut slot = switch.borrow_mut();
            let Some(sw) = slot.as_mut() else { return glib::ControlFlow::Break };
            let m = master.get();
            // as the volume slider moves: even to the ear
            let x = start.elapsed().as_secs_f64() / SWITCH_OUT_S;
            if sw.leaving.is_some() {
                if x >= 1.0 {
                    if let Some(l) = sw.leaving.take() {
                        l.set_volume(0.0);
                        l.finish();
                    }
                } else if let Some(l) = sw.leaving.as_ref() {
                    l.set_volume(m * (1.0 - x).powi(3));
                }
            }
            let waited = start.elapsed().as_secs_f64() > SWITCH_WAIT_S;
            if came.is_none() && sw.leaving.is_none() && (sw.coming.ready() || waited) {
                came = Some(Instant::now());
                crate::log::note(&format!("switch: fading in after {:.1} s", start.elapsed().as_secs_f64()));
            }
            let Some(c) = came else { return glib::ControlFlow::Continue };
            let y = c.elapsed().as_secs_f64() / SWITCH_IN_S;
            if y >= 1.0 {
                sw.coming.set_volume(m);
                sw.timer = None;
                *slot = None;
                return glib::ControlFlow::Break;
            }
            sw.coming.set_volume(m * y.powi(3));
            glib::ControlFlow::Continue
        });
        *self.switch.borrow_mut() = Some(Switch { timer: Some(id), leaving: Some(leaving), coming });
    }

    /// Ends a switch at once: the one fading out stops, the one coming in is at full volume.
    fn cancel_switch(&self) {
        let Some(mut sw) = self.switch.borrow_mut().take() else { return };
        if let Some(id) = sw.timer.take() {
            id.remove();
        }
        if let Some(l) = sw.leaving.take() {
            l.finish();
        }
        sw.coming.set_volume(self.master.get());
    }

    /// Ends a mix at once: the song coming in plays on alone.
    fn cancel_mix(&self) {
        let mixing = self.mixing.borrow_mut().take();
        if let Some(m) = mixing {
            m.timer.remove();
            if let Some(old) = m.old.borrow_mut().take() {
                old.finish(); // the song that was fading out
            }
            m.new.set_volume(self.master.get());
        }
        let d = self.deck();
        d.set_level(self.master.get());
        d.set_bass(0.0);
    }

    /// The app's volume, 0-1 on a loudness-like scale (cubic); 1 = as loud as
    /// the system plays it, never more.
    pub fn set_volume(&self, level: f64) {
        let gain = level.clamp(0.0, 1.0).powi(3);
        self.master.set(gain);
        if self.switch.borrow().is_some() {
            return; // the switch's fades follow it
        }
        if let Some(v) = self.video.borrow().as_ref() {
            v.volume.set_property("volume", gain);
        } else if let Some(m) = self.stems.borrow().as_ref() {
            m.master.set_property("volume", gain);
        } else if !self.mixing() {
            self.deck().set_level(gain);
        }
    }

    fn stop_video(&self) {
        if let Some(v) = self.video.borrow_mut().take() {
            retire(v.pipeline.clone().upcast());
        }
        if let Some(m) = self.stems.borrow_mut().take() {
            m.live.set(false);
            retire(m.pipeline.clone().upcast());
        }
        if let Some(m) = self.cued_parts.borrow_mut().take() {
            retire(m.pipeline.clone().upcast());
        }
    }

    /// Plays a split song with its parts at hand (see StemPipe): `whole` its
    /// own file (None: only the parts), `parts` (name, file uri), from `at`
    /// seconds, each part at `gains[name]` (0-1); all up and `whole` there:
    /// the song's own file is heard. `rate`: the song's own sample rate (kept).
    /// `switch`: from the song playing as a plain file (it fades out, this
    /// fades in, on from where it's got to). (Reverb and the cuts are the
    /// Speaker EQ's, after the player: see the Mixer.)
    pub fn load_parts(&self, whole: Option<&str>, parts: &[(String, String)], at: f64, gains: &std::collections::HashMap<String, f64>, rate: Option<u32>, switch: bool) -> Result<(), String> {
        self.cancel_switch();
        self.cancel_mix();
        let leaving = if switch { self.leave() } else { None };
        if leaving.is_none() {
            self.stop();
        }
        let start = (at > 0.5).then(|| (at, switch.then(Instant::now)));
        let m = self.build_parts(whole, parts, gains, rate, start, true)?;
        m.master.set_property("volume", if leaving.is_some() { 0.0 } else { self.master.get() });
        m.pipeline.set_state(gst::State::Playing).map_err(|e| e.to_string())?;
        let (master, ready) = (m.master.clone(), m.ready.clone());
        *self.stems.borrow_mut() = Some(m);
        if let Some(l) = leaving {
            self.fade_over(l, Coming::Stems { volume: master, ready });
        }
        Ok(())
    }

    /// The pipeline for a song with its parts at hand. `start`: where to go
    /// once loaded (and since when the song has moved on). `live`: its events
    /// count from the start (not a cue).
    fn build_parts(&self, whole: Option<&str>, parts: &[(String, String)], gains: &std::collections::HashMap<String, f64>, rate: Option<u32>, start: Option<(f64, Option<Instant>)>, live: bool) -> Result<Rc<StemPipe>, String> {
        let silent = std::env::var_os("RADIANCE_SILENT").is_some();
        // RADIANCE_RECORD=file.wav: what would be heard goes into the file instead (tests; in real time)
        // ("{n}" in the name: a file per pipeline, numbered, for a crossfade's two songs)
        static RECORDED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let record = std::env::var("RADIANCE_RECORD").ok().map(|f| f.replace("{n}", &RECORDED.fetch_add(1, Ordering::Relaxed).to_string()));
        let all_up = parts.iter().all(|(n, _)| gains.get(n).copied().unwrap_or(1.0) >= 0.999);
        let w = if whole.is_some() && all_up { 1.0 } else { 0.0 };
        let rate = rate.map(|r| format!(",rate={r}")).unwrap_or_default();
        let mut desc = format!(
            "audiomixer name=mix ! audioconvert ! audio/x-raw,format=F32LE{rate} ! identity name=keeper ! volume name=master ! audioconvert ! {}{} name=sound ",
            if record.is_some() { "audio/x-raw,format=F32LE,channels=2 ! " } else { "" },
            // recording: in real time, its samples go into the file (below)
            if record.is_some() || silent { "fakesink sync=true" } else { "pulsesink" }
        );
        if let Some(uri) = whole {
            desc += &format!(" uridecodebin name=wholefile caps=audio/x-raw uri=\"{uri}\" ! audioconvert ! audioresample ! audio/x-raw,format=F32LE ! volume name=whole volume={w:.3} ! mix.");
        }
        for (name, uri) in parts {
            let g = gains.get(name).copied().unwrap_or(1.0) * (1.0 - w);
            desc += &format!(" uridecodebin caps=audio/x-raw uri=\"{uri}\" ! audioconvert ! audioresample ! audio/x-raw,format=F32LE ! volume name=part_{name} volume={g:.3} ! mix.");
        }
        let pipeline = gst::parse::launch(&desc).map_err(|e| e.to_string())?.downcast::<gst::Pipeline>().map_err(|_| "not a pipeline")?;
        let master = pipeline.by_name("master").ok_or("no master")?;
        let mut by_name = std::collections::HashMap::new();
        let keep = Arc::new(Keep::default());
        keep.whole.store(w.to_bits(), Ordering::Relaxed);
        for (name, _) in parts {
            if let Some(e) = pipeline.by_name(&format!("part_{name}")) {
                let level = Arc::new(PartLevel::new(gains.get(name).copied().unwrap_or(1.0)));
                level.clone().listen(&e);
                keep.parts.lock().unwrap().insert(name.clone(), level);
                by_name.insert(name.clone(), e);
            }
        }
        if let Some(keeper) = pipeline.by_name("keeper") {
            keep.clone().listen(&keeper);
        }
        // recording (tests): every sample as it reaches the sink, raw (32-bit float, interleaved)
        if let (Some(f), Some(out)) = (&record, pipeline.by_name("sound")) {
            if let (Ok(file), Some(pad)) = (std::fs::File::create(f), out.static_pad("sink")) {
                let file = Mutex::new(file);
                pad.add_probe(gst::PadProbeType::BUFFER, move |_, info| {
                    if let Some(gst::PadProbeData::Buffer(b)) = info.data.as_ref() {
                        if let Ok(map) = b.map_readable() {
                            use std::io::Write;
                            let _ = file.lock().unwrap().write_all(map.as_slice());
                        }
                    }
                    gst::PadProbeReturn::Ok
                });
            }
        }
        if let (false, None, Some(out)) = (silent, &record, pipeline.by_name("sound")) {
            let props = gst::Structure::builder("props")
                .field("media.role", "music")
                .field("application.name", "Radiance Music")
                .field("application.icon_name", "io.github.radianceadm.Music")
                .build();
            out.set_property("stream-properties", &props);
            out.set_property("client-name", "Radiance Music");
        }
        let on_event = self.on_event.clone();
        let start = Cell::new(start);
        let (ready, live) = (Rc::new(Cell::new(false)), Rc::new(Cell::new(live)));
        let (r, l) = (ready.clone(), live.clone());
        let p = pipeline.downgrade();
        let guard = pipeline
            .bus()
            .ok_or("no bus")?
            .add_watch_local(move |_, msg| {
                use gst::MessageView;
                match msg.view() {
                    MessageView::AsyncDone(_) => match (start.take(), p.upgrade()) {
                        (Some((at, since)), Some(pipe)) => {
                            let at = gst::ClockTime::from_nseconds((onward(at, since) * 1e9) as u64);
                            let _ = pipe.seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE, at);
                        }
                        _ => {
                            r.set(true);
                            if l.get() {
                                on_event(Event::Ready)
                            }
                        }
                    },
                    MessageView::Eos(_) if l.get() => on_event(Event::Ended),
                    MessageView::Error(e) if l.get() => on_event(Event::Error(e.error().to_string())),
                    _ => {}
                }
                glib::ControlFlow::Continue
            })
            .map_err(|e| e.to_string())?;
        let whole_el = pipeline.by_name("whole");
        Ok(Rc::new(StemPipe {
            pipeline,
            master,
            parts: by_name,
            whole: whole_el,
            gains: RefCell::new(gains.clone()),
            blend: Rc::new(Cell::new(w)),
            whole_on: Cell::new(w > 0.5),
            blending: RefCell::default(),
            keep,
            ready,
            live,
            _bus: guard,
        }))
    }

    /// Blends to the song's own file (true: every part is up) or to its parts.
    pub fn set_whole(&self, on: bool) {
        if let Some(m) = self.stems.borrow().as_ref() {
            StemPipe::set_whole(m, on);
        }
    }

    /// The parts are what's heard (not the song's own file).
    pub fn hears_parts(&self) -> bool {
        self.stems.borrow().as_ref().is_some_and(|m| !m.whole_on.get())
    }

    /// Cues the next song with its parts at hand (a crossfade into it).
    pub fn cue_parts(&self, whole: &str, parts: &[(String, String)], at: f64, gains: &std::collections::HashMap<String, f64>, rate: Option<u32>) {
        if self.mixing() || self.is_video() || self.switch.borrow().is_some() {
            return;
        }
        self.drop_cue();
        match self.build_parts(Some(whole), parts, gains, rate, (at > 0.05).then_some((at, None)), false) {
            Ok(m) => {
                m.master.set_property("volume", 0.0f64);
                let _ = m.pipeline.set_state(gst::State::Paused);
                *self.cued_parts.borrow_mut() = Some(m);
            }
            Err(e) => crate::log::note(&format!("couldn't cue the next song's parts: {e}")),
        }
    }

    /// The next song's Mixer volumes, while it's cued for a crossfade with its
    /// parts at hand: the sliders moved meanwhile reach it before it's heard.
    pub fn retune_cued(&self, gains: &std::collections::HashMap<String, f64>) {
        if let Some(m) = self.cued_parts.borrow().as_ref() {
            m.retune(gains);
        }
    }

    /// Each part's loudness right now (its mean power over the last seconds,
    /// before its Mixer volume): how much of it this song has here.
    pub fn part_levels(&self) -> Vec<(String, f64)> {
        let Some(keep) = self.stems.borrow().as_ref().map(|m| m.keep.clone()) else { return vec![] };
        let parts = keep.parts.lock().unwrap();
        parts.iter().map(|(n, l)| (n.clone(), l.now().0)).collect()
    }

    /// One part's volume (0-1) while the parts play.
    pub fn set_part(&self, name: &str, gain: f64) {
        if let Some(m) = self.stems.borrow().as_ref() {
            m.gains.borrow_mut().insert(name.to_string(), gain.clamp(0.0, 1.0));
            m.apply();
            if let Some(level) = m.keep.parts.lock().unwrap().get(name) {
                level.set_gain(gain.clamp(0.0, 1.0));
            }
        }
    }

    /// While the parts play: the make-up gain, the loudest sample out and the
    /// limiter's lowest gain since last asked.
    pub fn stems_level(&self) -> Option<(f64, f64, f64)> {
        let m = self.stems.borrow();
        let keep = &m.as_ref()?.keep;
        Some((
            f64::from_bits(keep.target.load(Ordering::Relaxed)),
            f64::from_bits(keep.peak.swap(0f64.to_bits(), Ordering::Relaxed)),
            f64::from_bits(keep.squeeze.swap(1f64.to_bits(), Ordering::Relaxed)),
        ))
    }

    pub fn is_stems(&self) -> bool {
        self.stems.borrow().is_some()
    }

    /// Plays a video: `picture` and `sound` are its two streams. Returns what
    /// to show it in (a GdkPaintable for a gtk::Picture).
    pub fn load_video(&self, picture: &str, sound: &str) -> Result<gtk::gdk::Paintable, String> {
        self.stop();
        let silent = std::env::var_os("RADIANCE_SILENT").is_some();
        let out = if silent { "fakesink sync=true" } else { "pulsesink" };
        // a saved video: one file, picture and sound together; else two streams
        let desc = if picture == sound {
            format!(
                "uridecodebin name=file uri=\"{picture}\" file. ! queue ! videoconvert ! gtk4paintablesink name=picture \
                 file. ! queue ! audioconvert ! audioresample ! volume name=volume ! {out} name=sound"
            )
        } else {
            format!(
                "uridecodebin uri=\"{picture}\" ! videoconvert ! gtk4paintablesink name=picture \
                 uridecodebin uri=\"{sound}\" ! audioconvert ! audioresample ! volume name=volume ! {out} name=sound"
            )
        };
        let pipeline = gst::parse::launch(&desc).map_err(|e| e.to_string())?.downcast::<gst::Pipeline>().map_err(|_| "not a pipeline")?;
        let shown = pipeline.by_name("picture").ok_or("no picture")?.property::<gtk::gdk::Paintable>("paintable");
        let volume = pipeline.by_name("volume").ok_or("no volume")?;
        volume.set_property("volume", self.master.get());
        if let (false, Some(out)) = (silent, pipeline.by_name("sound")) {
            let props = gst::Structure::builder("props")
                .field("media.role", "music")
                .field("application.name", "Radiance Music")
                .field("application.icon_name", "io.github.radianceadm.Music")
                .build();
            out.set_property("stream-properties", &props);
            out.set_property("client-name", "Radiance Music");
        }
        let on_event = self.on_event.clone();
        let guard = pipeline
            .bus()
            .ok_or("no bus")?
            .add_watch_local(move |_, msg| {
                use gst::MessageView;
                match msg.view() {
                    MessageView::AsyncDone(_) => on_event(Event::Ready),
                    MessageView::Eos(_) => on_event(Event::Ended),
                    MessageView::Error(e) => on_event(Event::Error(e.error().to_string())),
                    _ => {}
                }
                glib::ControlFlow::Continue
            })
            .map_err(|e| e.to_string())?;
        pipeline.set_state(gst::State::Playing).map_err(|e| e.to_string())?;
        *self.video.borrow_mut() = Some(VideoPipe { pipeline, volume, _bus: guard });
        Ok(shown)
    }

    pub fn is_video(&self) -> bool {
        self.video.borrow().is_some()
    }

    /// The element playing now: the video, or the active deck's playbin.
    fn now(&self) -> gst::Element {
        if let Some(v) = self.video.borrow().as_ref() {
            return v.pipeline.clone().upcast();
        }
        if let Some(m) = self.stems.borrow().as_ref() {
            return m.pipeline.clone().upcast();
        }
        self.deck().pb()
    }

    pub fn load(&self, uri: &str) {
        self.load_at(uri, 0.0);
    }

    /// Plays `uri` from `at` seconds (where the song was left, say).
    pub fn load_at(&self, uri: &str, at: f64) {
        self.load_from(uri, at, false);
    }

    /// Plays `uri` from `at` seconds plus however long it takes to load (a
    /// switch to another version of the song playing: it plays on from where
    /// the song has got to by then, never back; a moment's quiet instead).
    pub fn load_onward(&self, uri: &str, at: f64) {
        self.load_from(uri, at, true);
    }

    fn load_from(&self, uri: &str, at: f64, onward: bool) {
        crate::log::note(&format!("load {} from {at:.0} s at volume {:.2}", uri.chars().take(90).collect::<String>(), self.master.get()));
        self.cancel_switch();
        self.cancel_mix();
        // a switch: the form of the song playing fades out on its own, this fades in
        let leaving = if onward { self.leave() } else { None };
        if leaving.is_none() {
            self.stop_video();
            self.other().reset();
        }
        let d = self.deck();
        d.reset(); // the song before stops on a worker thread (see `retire`)
        d.set_filters(false);
        d.set_level(if leaving.is_some() { 0.0 } else { self.master.get() });
        d.pb().set_property("uri", uri);
        d.start_at.set((at > 0.5).then_some((at, onward.then(Instant::now))));
        let _ = d.pb().set_state(gst::State::Playing);
        if let Some(l) = leaving {
            self.fade_over(l, Coming::Deck(d));
        }
    }

    pub fn stop(&self) {
        self.cancel_switch();
        self.stop_video();
        self.cancel_mix();
        for d in self.decks.borrow().iter() {
            d.reset();
        }
    }

    pub fn playing(&self) -> bool {
        let e = self.now();
        e.current_state() == gst::State::Playing || e.pending_state() == gst::State::Playing
    }

    pub fn set_playing(&self, on: bool) {
        if !on {
            self.cancel_switch();
        }
        let state = if on { gst::State::Playing } else { gst::State::Paused };
        let _ = self.now().set_state(state);
        if let Some(m) = self.mixing.borrow().as_ref() {
            if let Some(old) = m.old.borrow().as_ref() {
                old.set_state(state); // the song fading out pauses too
            }
        }
    }

    pub fn position(&self) -> Option<f64> {
        self.now().query_position::<gst::ClockTime>().map(|t| t.nseconds() as f64 / 1e9)
    }

    pub fn duration(&self) -> Option<f64> {
        self.now().query_duration::<gst::ClockTime>().map(|t| t.nseconds() as f64 / 1e9)
    }

    /// Seeking drops a mix in progress and any cue (the plan no longer fits).
    pub fn seek(&self, seconds: f64) {
        self.cancel_switch();
        if self.video.borrow().is_some() || self.stems.borrow().is_some() {
            let at = gst::ClockTime::from_nseconds((seconds.max(0.0) * 1e9) as u64);
            let _ = self.now().seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE, at);
            return;
        }
        self.cancel_mix();
        self.other().reset();
        self.deck().seek_to(seconds, 1.0);
    }

    /// The song's own format as decoded: (sample rate, bits), e.g. (96000, "24-bit").
    pub fn output(&self) -> Option<(i32, String)> {
        if self.is_video() || self.hears_parts() {
            return None;
        }
        // the song's own file among its parts, or the deck's player
        let parts_file = self.stems.borrow().as_ref().and_then(|m| m.pipeline.by_name("wholefile"));
        let d = self.deck();
        let pb = parts_file.unwrap_or_else(|| d.pb());
        let bin = pb.downcast_ref::<gst::Bin>()?;
        // the decoder's output (not the sink's: mixing filters may sit between)
        let decoder = bin.iterate_recurse().into_iter().flatten().find(|e| {
            e.factory().is_some_and(|f| {
                let klass = f.metadata("klass").unwrap_or_default();
                klass.contains("Decoder") && klass.contains("Audio")
            })
        });
        let caps = decoder
            .and_then(|e| e.static_pad("src"))
            .and_then(|p| p.current_caps())
            .or_else(|| d.sink().static_pad("sink").and_then(|p| p.current_caps()))?;
        let s = caps.structure(0)?;
        let rate = s.get::<i32>("rate").ok()?;
        let format = s.get::<&str>("format").unwrap_or("");
        let bits = match format {
            f if f.starts_with("S16") || f.starts_with("U16") => "16-bit",
            f if f.starts_with("S24") || f.starts_with("U24") => "24-bit",
            f if f.starts_with("S32") || f.starts_with("U32") => "32-bit",
            f if f.starts_with("F32") => "32-bit float",
            f if f.starts_with("F64") => "64-bit float",
            _ => "",
        };
        Some((rate, bits.to_string()))
    }

    /// Readies the next song on the other deck, silent: from `at` seconds, at
    /// `rate` times its tempo (pitch kept), with the DJ filters if `dj`.
    pub fn cue(&self, uri: &str, at: f64, rate: f64, dj: bool) {
        if self.mixing() || self.is_video() || self.switch.borrow().is_some() {
            return; // (in a switch the other deck is the one fading out)
        }
        if let Some(m) = self.cued_parts.borrow_mut().take() {
            retire(m.pipeline.clone().upcast());
        }
        let d = self.other();
        d.reset();
        d.set_filters(dj);
        d.set_bass(if dj { BASS_CUT } else { 0.0 });
        d.set_level(0.0f64);
        d.pb().set_property("uri", uri);
        if at > 0.05 || (rate - 1.0).abs() > 1e-3 {
            d.pending.set(Some((at, rate)));
        }
        let _ = d.pb().set_state(gst::State::Paused);
    }

    /// Forgets a cued song (not one already mixing in).
    pub fn drop_cue(&self) {
        if !self.mixing() {
            self.other().reset();
            if let Some(m) = self.cued_parts.borrow_mut().take() {
                retire(m.pipeline.clone().upcast());
            }
        }
    }

    pub fn cued(&self) -> bool {
        self.other().cued.get() || self.cued_parts.borrow().as_ref().is_some_and(|m| m.ready.get())
    }

    /// Outside a crossfade, the song playing on a deck is at the app's volume;
    /// if not (a fade that didn't finish: it plays, silently), it's put back.
    /// Some((was, now)) when it was fixed.
    pub fn heal_volume(&self) -> Option<(f64, f64)> {
        if self.mixing() || self.switch.borrow().is_some() || self.video.borrow().is_some() || self.stems.borrow().is_some() {
            return None; // (a switch's fades are its own)
        }
        let d = self.deck();
        if d.pb().current_state() != gst::State::Playing {
            return None;
        }
        let (was, want) = (d.level(), self.master.get());
        if (was - want).abs() > 0.02 {
            d.set_level(want);
            return Some((was, want));
        }
        None
    }

    /// For the log: how the sound goes out now (which player, its volume, the speaker).
    pub fn sound_report(&self) -> String {
        let sink_of = |e: &gst::Element| -> String {
            ["current-device", "device"].iter().find_map(|p| e.find_property(p).and_then(|_| e.property::<Option<String>>(p))).unwrap_or_else(|| "the default speaker".into())
        };
        if let Some(m) = self.stems.borrow().as_ref() {
            let out = m.pipeline.by_name("sound").map(|s| sink_of(&s)).unwrap_or_default();
            let heard = if m.whole_on.get() { "its own file heard" } else { "its parts heard" };
            return format!("the song with its parts at hand ({heard}), volume {:.2}, to {out}", m.master.property::<f64>("volume"));
        }
        if let Some(v) = self.video.borrow().as_ref() {
            let out = v.pipeline.by_name("sound").map(|s| sink_of(&s)).unwrap_or_default();
            return format!("the video, volume {:.2}, to {out}", v.volume.property::<f64>("volume"));
        }
        let d = self.deck();
        format!("deck {}, volume {:.2} (the app's {:.2}), to {}", self.active.get(), d.level(), self.master.get(), sink_of(&d.sink()))
    }

    pub fn mixing(&self) -> bool {
        self.mixing.borrow().is_some()
    }

    /// Starts the cued song and crosses over `seconds`. The new song is the one
    /// "playing" from now on (position, seek, events); the old one fades out.
    /// Either can be a song with its parts at hand (a beat-matched DJ mix is
    /// always deck to deck: tempo and bass are the decks').
    pub fn mix(&self, seconds: f64, bass_swap: bool) {
        if !self.cued() || self.mixing() {
            return;
        }
        self.cancel_switch();
        let coming_parts = self.cued_parts.borrow_mut().take().filter(|m| m.ready.get());
        // the song going: its parts' pipeline, or the deck playing (no longer the active one)
        let old = match self.stems.borrow_mut().take() {
            Some(m) => {
                m.live.set(false);
                Leaving::Pipe { pipeline: m.pipeline.clone().upcast(), volume: m.master.clone() }
            }
            None => {
                let d = self.deck();
                self.active.set(1 - self.active.get());
                Leaving::Deck(d)
            }
        };
        // the song coming: with its parts, or on the other deck (the active one now)
        let new = match coming_parts {
            Some(m) => {
                m.live.set(true);
                let _ = m.pipeline.set_state(gst::State::Playing);
                let c = Coming::Stems { volume: m.master.clone(), ready: m.ready.clone() };
                *self.stems.borrow_mut() = Some(m);
                c
            }
            None => {
                if matches!(old, Leaving::Pipe { .. }) {
                    // from a song with its parts: the cue is on the deck that was "other"
                    self.active.set(1 - self.active.get());
                }
                let d = self.deck();
                d.cued.set(false);
                let _ = d.pb().set_state(gst::State::Playing);
                Coming::Deck(d)
            }
        };
        let decks = match (&old, &new) {
            (Leaving::Deck(a), Coming::Deck(b)) if bass_swap => Some((a.clone(), b.clone())),
            _ => None,
        };
        let start = Instant::now();
        let mixing = self.mixing.clone();
        let master = self.master.clone();
        let smooth = |a: f64, b: f64, x: f64| {
            let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        };
        crate::log::note(&format!(
            "crossfade over {seconds:.1} s{}{}",
            if decks.is_some() { ", bass swapped" } else { "" },
            match (&old, &new) {
                (Leaving::Pipe { .. }, Coming::Stems { .. }) => ", parts to parts",
                (Leaving::Pipe { .. }, _) => ", from a song's parts",
                (_, Coming::Stems { .. }) => ", into a song's parts",
                _ => "",
            }
        ));
        let old = Rc::new(RefCell::new(Some(old)));
        let (going, coming) = (old.clone(), new.clone_volume());
        let timer = glib::timeout_add_local(Duration::from_millis(25), move || {
            let p = (start.elapsed().as_secs_f64() / seconds.max(0.1)).min(1.0);
            let m = master.get();
            if let Some(o) = going.borrow().as_ref() {
                o.set_volume(m * (p * FRAC_PI_2).cos());
            }
            coming.set_volume(m * (p * FRAC_PI_2).sin());
            if let Some((a, b)) = &decks {
                let swap = smooth(0.42, 0.58, p);
                a.set_bass(BASS_CUT * swap);
                b.set_bass(BASS_CUT * (1.0 - swap));
            }
            if p >= 1.0 {
                if let Some(o) = going.borrow_mut().take() {
                    o.finish();
                }
                coming.set_volume(master.get());
                if let Some((_, b)) = &decks {
                    b.set_bass(0.0);
                }
                crate::log::note(&format!("crossfade done: the new song at volume {:.2}", master.get()));
                mixing.borrow_mut().take();
                return glib::ControlFlow::Break;
            }
            glib::ControlFlow::Continue
        });
        *self.mixing.borrow_mut() = Some(Mixing { timer, old, new });
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop();
    }
}
