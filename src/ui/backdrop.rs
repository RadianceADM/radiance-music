//! Now playing's backdrop, the way Apple Music moves its own: the album art,
//! blurred, filling the page, with soft round patches of it on top, each its
//! own size and place, slowly turning (some one way, some the other) and
//! wandering, so the art's colours keep flowing into each other, like a lava
//! lamp. The music moves it the way the keyboard's lights follow a song: each
//! kick pushes the flow ahead for a moment and swells the picture at once,
//! fading evenly over a third of a second; strong bass makes it all flow
//! faster. It never zooms in past MAX_ZOOM, so the art's own shapes always
//! show. A widget of its own that only redraws: the GPU moves the pictures each
//! frame (in step with the screen), nothing is laid out again, and it never
//! asks for any size. A new song's art fades in over the last one's.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::beat::Pulse;

use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, graphene, gsk};

const FADE_US: f64 = 900_000.0;

/// The most the art is ever blown up: the whole picture covers the page's
/// diagonal (so no corner shows as it turns) times this, at the most.
const MAX_ZOOM: f32 = 1.15;

/// How much faster the flow runs on strong bass (1 + this, at the most).
const BASS_SPEEDS: f64 = 3.0;

/// A kick's push: the flow runs this much faster at once, settling within a
/// tenth of a second or so (about 3 s of flow in one go).
const PUSH: f64 = 25.0;
const PUSH_SETTLES_S: f64 = 0.12;

/// A kick's swell (part of the size) and how fast it fades (per second: gone
/// in a third of a second, like the keyboard's beat light).
const SWELL: f32 = 0.06;
const SWELL_FADES: f64 = 3.0;

/// The patches on top: where each one wanders around (part of the page),
/// its size (part of the page's longer side), its turn (degrees a second,
/// minus: the other way round) and how much it shows.
const PATCHES: [(f32, f32, f32, f64, f32); 3] = [(0.28, 0.3, 0.95, 7.0, 0.85), (0.76, 0.42, 0.8, -9.0, 0.8), (0.45, 0.82, 1.05, 6.0, 0.75)];

/// The art, whole and as a soft round patch.
#[derive(Clone)]
pub struct Art {
    pub whole: gdk::Texture,
    pub patch: gdk::Texture,
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Backdrop {
        pub now: RefCell<Option<Art>>,
        pub before: RefCell<Option<Art>>,
        /// When the newest art started fading in (frame time, µs).
        pub fade_from: Cell<Option<i64>>,
        /// The flow's own clock (seconds); it stands still while paused, runs faster on bass.
        pub t: Cell<f64>,
        pub last: Cell<Option<i64>>,
        pub drifting: Cell<bool>,
        pub tick: RefCell<Option<gtk::TickCallbackId>>,
        /// The song's beat and bass, and where the song was at a frame time: (seconds, µs).
        pub pulse: RefCell<Option<Rc<Pulse>>>,
        pub song_at: Cell<Option<(f64, i64)>>,
        /// The bass now, eased (0-1).
        pub bass: Cell<f64>,
        /// The beat a frame ago (a kick is it rising), the push left and the swell now (0-1).
        pub hit: Cell<f32>,
        pub push: Cell<f64>,
        pub swell: Cell<f64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Backdrop {
        const NAME: &'static str = "RadianceBackdrop";
        type Type = super::Backdrop;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for Backdrop {}

    impl WidgetImpl for Backdrop {
        fn measure(&self, _: gtk::Orientation, _: i32) -> (i32, i32, i32, i32) {
            (0, 0, -1, -1) // takes whatever it's given
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let (w, h) = (obj.width() as f32, obj.height() as f32);
            if w <= 0.0 || h <= 0.0 {
                return;
            }
            let fade = match (self.fade_from.get(), obj.frame_clock()) {
                (Some(from), Some(clock)) => ((clock.frame_time() - from) as f64 / FADE_US).clamp(0.0, 1.0),
                _ => 1.0,
            };
            let (t, swell) = (self.t.get(), self.swell.get() as f32);
            snapshot.push_clip(&graphene::Rect::new(0.0, 0.0, w, h));
            if fade < 1.0 {
                if let Some(old) = self.before.borrow().as_ref() {
                    draw(snapshot, old, w, h, t, swell);
                }
            }
            if let Some(art) = self.now.borrow().as_ref() {
                snapshot.push_opacity(fade);
                draw(snapshot, art, w, h, t, swell);
                snapshot.pop();
            }
            snapshot.pop();
        }
    }

    /// One picture of the art at `size` (a square's side), centred at (x, y), turned `angle` degrees.
    fn layer(snapshot: &gtk::Snapshot, tex: &gdk::Texture, x: f32, y: f32, size: f32, angle: f32) {
        snapshot.save();
        snapshot.translate(&graphene::Point::new(x, y));
        snapshot.rotate(angle);
        snapshot.append_scaled_texture(tex, gsk::ScalingFilter::Linear, &graphene::Rect::new(-size / 2.0, -size / 2.0, size, size));
        snapshot.restore();
    }

    /// The art over the whole page, then its patches, as they are at `t`,
    /// swollen by `swell` (a kick, 0-1).
    fn draw(snapshot: &gtk::Snapshot, art: &Art, w: f32, h: f32, t: f64, swell: f32) {
        let diag = (w * w + h * h).sqrt();
        let wave = |period: f64, phase: f64| (t / period + phase).sin() as f32;
        let grow = 1.0 + SWELL * swell;
        // the whole art: wide enough that no corner shows however it's turned; even swollen, never past MAX_ZOOM
        let rest = MAX_ZOOM - SWELL * 1.1 - 0.02 + 0.02 * wave(7.0, 0.0);
        let size = diag * (rest * grow).min(MAX_ZOOM);
        let (x, y) = (w / 2.0 + 0.012 * w * wave(11.0, 0.0), h / 2.0 + 0.012 * h * wave(9.0, 1.0));
        layer(snapshot, &art.whole, x, y, size, (t * 3.0 % 360.0) as f32);
        // the patches, each wandering around its own part of the page
        let long = w.max(h);
        for (i, &(px, py, part, turn, shows)) in PATCHES.iter().enumerate() {
            let k = i as f64 * 2.1;
            let (x, y) = (w * (px + 0.12 * wave(11.0 + k, k)), h * (py + 0.12 * wave(13.0 - k, k + 0.7)));
            let size = long * part * (1.0 + 0.06 * wave(7.0 + k, k)) * (1.0 + 1.3 * SWELL * swell);
            snapshot.push_opacity(shows as f64);
            layer(snapshot, &art.patch, x, y, size, ((turn * t + 90.0 * i as f64) % 360.0) as f32);
            snapshot.pop();
        }
    }
}

glib::wrapper! {
    pub struct Backdrop(ObjectSubclass<imp::Backdrop>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for Backdrop {
    fn default() -> Self {
        glib::Object::builder().property("can-target", false).property("overflow", gtk::Overflow::Hidden).build()
    }
}

impl Backdrop {
    /// The new art, faded in over the old.
    pub fn set_art(&self, art: Art) {
        let imp = self.imp();
        let old = imp.now.replace(Some(art));
        imp.before.replace(old);
        imp.fade_from.set(self.frame_clock().map(|c| c.frame_time()));
        self.ensure_ticking();
        self.queue_draw();
    }

    /// The song's beat and bass (None: the flow at its own pace).
    pub fn set_pulse(&self, pulse: Option<Rc<Pulse>>) {
        self.imp().pulse.replace(pulse);
    }

    /// Where the song is now (seconds); between calls the backdrop counts on.
    pub fn sync(&self, at: f64) {
        if let Some(clock) = self.frame_clock() {
            self.imp().song_at.set(Some((at, clock.frame_time())));
        }
    }

    /// Flow while true (Now playing on screen, music playing, animations on).
    pub fn set_drifting(&self, on: bool) {
        let on = on && gtk::Settings::default().is_none_or(|s| s.is_gtk_enable_animations());
        self.imp().drifting.set(on);
        self.imp().last.set(None);
        if !on {
            self.imp().bass.set(0.0);
            self.imp().push.set(0.0);
            self.imp().swell.set(0.0);
        }
        self.ensure_ticking();
    }

    fn fading(&self) -> bool {
        let imp = self.imp();
        match (imp.fade_from.get(), self.frame_clock()) {
            (Some(from), Some(clock)) => ((clock.frame_time() - from) as f64) < FADE_US,
            (Some(_), None) => true,
            _ => false,
        }
    }

    /// A frame callback only while something moves.
    fn ensure_ticking(&self) {
        let imp = self.imp();
        if imp.tick.borrow().is_some() || !(imp.drifting.get() || self.fading()) {
            return;
        }
        let id = self.add_tick_callback(|w, clock| {
            let imp = w.imp();
            let now = clock.frame_time();
            if imp.drifting.get() {
                let dt = imp.last.get().map(|last| (now - last) as f64 / 1e6).unwrap_or(0.0).min(0.1);
                imp.last.set(Some(now));
                // the beat and the bass at this moment of the song (counted on from the last sync, at most 1 s)
                let (hit, bass) = match (imp.pulse.borrow().as_ref(), imp.song_at.get()) {
                    (Some(p), Some((at, when))) if now - when < 1_000_000 => {
                        let at = at + (now - when) as f64 / 1e6;
                        (p.at(at), p.bass_at(at) as f64)
                    }
                    _ => (0.0, 0.0),
                };
                // a kick (the beat rising): the flow pushed ahead, the picture swollen at once
                if hit > imp.hit.get() + 0.2 {
                    imp.push.set(imp.push.get() + PUSH * hit as f64);
                }
                imp.hit.set(hit);
                imp.swell.set((hit as f64).max(imp.swell.get() - SWELL_FADES * dt));
                // the bass eased: up within a fifth of a second, down over half a second
                let was = imp.bass.get();
                let ease = if bass > was { 0.2 } else { 0.5 };
                imp.bass.set(was + (bass - was) * (1.0 - (-dt / ease).exp()));
                imp.t.set(imp.t.get() + dt * (1.0 + BASS_SPEEDS * imp.bass.get() + imp.push.get()));
                imp.push.set(imp.push.get() * (-dt / PUSH_SETTLES_S).exp());
            }
            w.queue_draw();
            if imp.drifting.get() || w.fading() {
                glib::ControlFlow::Continue
            } else {
                imp.tick.borrow_mut().take();
                glib::ControlFlow::Break
            }
        });
        imp.tick.replace(Some(id));
    }
}
