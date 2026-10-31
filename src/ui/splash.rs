//! The start: while the app gets ready, the icon's sun rises out of the sea and
//! its four bars (drums, bass, other, vocals) play like an equalizer; once
//! everything is ready the bars come to rest, the sun sets again and the logo
//! writes itself (the R, its leg rolling on into the m, the wave fading out),
//! with the name under it, then it all fades into the app. In the app's own
//! look (Stone or True Black). One widget over the whole window that only
//! redraws (the GPU paints it each frame). The start's work is handed out one
//! job a frame, the few that can't be split before the sun moves at all, and
//! the window behind stays hidden (nothing laid out) until the logo is written:
//! so nothing it does shows as a stutter. A click goes straight in. Never there
//! for screenshots, or with animations off.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;

use adw::prelude::*;
use gtk::glib;
use gtk::subclass::prelude::*;
use gtk::{gdk, graphene, gsk};

/// The sun coming up, at least this long on screen, the bars coming to rest.
const RISE_S: f64 = 1.0;
const SHOWN_S: f64 = 1.4;
const SETTLE_S: f64 = 0.35;
/// After they settle: the sun goes down, the logo writes itself (from WRITE_S
/// on, WRITTEN_S long), it stays a moment, and it fades into the app.
const SET_S: f64 = 0.5;
const WRITE_S: f64 = 0.3;
const WRITTEN_S: f64 = 1.75;
const HOLD_S: f64 = 0.35;
/// The dark opening from the middle out (the app showing through), the logo going last.
const FADE_S: f64 = 0.9;
/// The opening's soft edge (part of the window's half-diagonal).
const SOFT: f32 = 0.3;
/// Never longer than this, ready or not.
const LONGEST_S: f64 = 10.0;

/// The icon's own drawing (data/icons/io.github.radianceadm.Music.svg, 128 across):
/// the sun, the bars (x, resting top) and the sea's fade.
const SUN: (f32, f32, f32) = (64.0, 67.5, 34.8);
const BARS: [(f32, f32); 4] = [(50.7, 73.3), (59.6, 60.5), (68.4, 67.5), (77.3, 77.9)];
const BAR_W: f32 = 5.8;
const BAR_FOOT: f32 = 112.7;
const SEA: (f32, f32) = (87.2, 105.8);
/// How far below its place the sun starts (under the sea), in the icon's units.
const RISE_FROM: f32 = 45.0;

/// The logo (data/brand/radiance-music-logo.svg, its 0..100 units, the R's middle at 50).
const LOGO_R: &str = "M38 71V29H51.5A10.5 10.5 0 0 1 51.5 50H38";
const LOGO_LEG: &str = "M50.5 50C56.5 51.5 61 59 61 71C62.5467 60 63.4167 52.5 68.25 52.5C73.0833 52.5 73.9533 60 75.5 70.6C77.0467 60 77.9167 52.5 82.75 52.5C87.5833 52.5 88.0667 71 90.5 71";
const LOGO_TAIL: &str = "M90.5 71C94.5 71 97.5 66 100.5 61";
const PEN: f32 = 8.8;

fn rgba(hex: u32, a: f32) -> gdk::RGBA {
    gdk::RGBA::new(((hex >> 16) & 0xff) as f32 / 255.0, ((hex >> 8) & 0xff) as f32 / 255.0, (hex & 0xff) as f32 / 255.0, a)
}
const WHITE: u32 = 0xF5F7FA;
const SUN_TOP: u32 = 0xFFDB94;
const ORANGE: u32 = 0xF38A35;
const GLOW: u32 = 0xFFAE63;

/// The app's look: the sky at the top and at the bottom (the sea, the bars).
#[derive(Clone, Copy, Default)]
pub struct Look {
    top: u32,
    bottom: u32,
}

impl Look {
    /// "black" (True Black) or anything else (Stone), as the menu has them.
    pub fn of(name: &str) -> Look {
        if name == "black" { Look { top: 0x000000, bottom: 0x000000 } } else { Look { top: 0x1C1917, bottom: 0x0C0A09 } }
    }
}

fn ease(x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// `path` drawn `part` (0..1) of the way along (None: not started).
fn written(path: &gsk::Path, part: f64) -> Option<gsk::Path> {
    if part <= 0.001 {
        return None;
    }
    if part >= 0.999 {
        return Some(path.clone());
    }
    let measure = gsk::PathMeasure::new(path);
    let (from, to) = (measure.point(0.0)?, measure.point(measure.length() * part as f32)?);
    let b = gsk::PathBuilder::new();
    b.add_segment(path, &from, &to);
    Some(b.to_path())
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Splash {
        pub look: Cell<Look>,
        /// The first frame's time (µs): the animation counts from it.
        pub start: Cell<Option<i64>>,
        /// Seconds since then.
        pub t: Cell<f64>,
        pub ready: Cell<bool>,
        pub settle_from: Cell<Option<f64>>,
        pub fade_from: Cell<Option<f64>>,
        /// The start's work, one job after each frame, and one running now;
        /// `early`: those before the sun moves (they can't be split up).
        pub jobs: RefCell<VecDeque<(&'static str, Box<dyn FnOnce()>)>>,
        pub early: RefCell<VecDeque<(&'static str, Box<dyn FnOnce()>)>>,
        /// The window behind, hidden until the logo is written, and the frames drawn since it's shown.
        pub content: RefCell<Option<gtk::Widget>>,
        pub shown_frames: Cell<Option<u32>>,
        pub busy: Cell<bool>,
        /// The last job run, and the slow frames (ms late, and the job before them).
        pub last_job: Cell<&'static str>,
        pub last_frame: Cell<Option<i64>>,
        pub frames: Cell<u32>,
        pub slow: RefCell<Vec<(u32, &'static str)>>,
        /// The logo's strokes, in its own units.
        pub logo: RefCell<Option<(gsk::Path, gsk::Path, gsk::Path)>>,
        /// Something the app needs first (the splitter, the first time): the
        /// sun keeps playing until each hold is let go; what's happening, and how far.
        pub holds: Cell<u32>,
        pub progress: RefCell<Option<(String, f64)>>,
        /// Buttons under the progress (getting ready failed: try again, or go on now).
        pub offered: RefCell<Option<gtk::Widget>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Splash {
        const NAME: &'static str = "RadianceSplash";
        type Type = super::Splash;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for Splash {}

    impl WidgetImpl for Splash {
        fn measure(&self, _: gtk::Orientation, _: i32) -> (i32, i32, i32, i32) {
            (0, 0, -1, -1) // takes whatever it's given
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let (w, h) = (obj.width() as f32, obj.height() as f32);
            if w <= 0.0 || h <= 0.0 {
                return;
            }
            let (t, look) = (self.t.get(), self.look.get());
            let rise = ease(t / RISE_S) as f32;
            let settle = self.settle_from.get();
            let settled = settle.map(|s| ease((t - s) / SETTLE_S)).unwrap_or(0.0);
            let play = (ease((t - 0.8) / 0.3) * (1.0 - settled)) as f32;
            let set = settle.map(|s| ease((t - s - 0.1) / SET_S)).unwrap_or(0.0) as f32;
            // the end: the dark opens from the middle out, the app showing through
            let reveal = self.fade_from.get().map(|f| ease((t - f) / FADE_S)).unwrap_or(0.0) as f32;
            // the icon's 128 units, half the window's shorter side, in the middle
            let u = w.min(h) * 0.5 / 128.0;
            let at = |x: f32, y: f32| graphene::Point::new(w / 2.0 + (x - 64.0) * u, h / 2.0 + (y - 64.0) * u);
            let whole = graphene::Rect::new(0.0, 0.0, w, h);
            if reveal > 0.0 {
                // what's left of the dark: none inside the opening, all of it past its soft edge
                snapshot.push_mask(gsk::MaskMode::Alpha);
                let inner = (reveal * (1.0 + SOFT) - SOFT).clamp(0.0, 1.0);
                let outer = (reveal * (1.0 + SOFT)).clamp(inner, 1.0);
                let r = (w * w + h * h).sqrt() / 2.0;
                let (none, all) = (gdk::RGBA::new(0.0, 0.0, 0.0, 0.0), gdk::RGBA::new(0.0, 0.0, 0.0, 1.0));
                snapshot.append_radial_gradient(&whole, &graphene::Point::new(w / 2.0, h / 2.0), r, r, 0.0, 1.0, &[gsk::ColorStop::new(0.0, none), gsk::ColorStop::new(inner, none), gsk::ColorStop::new(outer, all), gsk::ColorStop::new(1.0, all)]);
                snapshot.pop();
            }
            // the sky (the app's own look), and the sun's warm glow
            snapshot.append_linear_gradient(&whole, &graphene::Point::new(0.0, 0.0), &graphene::Point::new(w * 0.35, h), &[gsk::ColorStop::new(0.0, rgba(look.top, 1.0)), gsk::ColorStop::new(1.0, rgba(look.bottom, 1.0))]);
            let glow = at(64.0, 69.8);
            let r = 100.0 * u;
            snapshot.append_radial_gradient(&whole, &glow, r, r, 0.0, 1.0, &[gsk::ColorStop::new(0.0, rgba(GLOW, 0.4 * rise)), gsk::ColorStop::new(1.0, rgba(GLOW, 0.0))]);
            if set < 1.0 {
                // the sun, rising (and setting again), its bars playing
                snapshot.push_opacity(1.0 - set as f64 * 0.6);
                let lift = (1.0 - rise) * RISE_FROM + set * RISE_FROM;
                let (sx, sy, sr) = SUN;
                let c = at(sx, sy + lift);
                let sun = graphene::Rect::new(c.x() - sr * u, c.y() - sr * u, 2.0 * sr * u, 2.0 * sr * u);
                snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(sun, sr * u));
                snapshot.append_linear_gradient(&sun, &graphene::Point::new(0.0, sun.y()), &graphene::Point::new(0.0, sun.y() + sun.height()), &[gsk::ColorStop::new(0.0, rgba(SUN_TOP, 1.0)), gsk::ColorStop::new(1.0, rgba(ORANGE, 1.0))]);
                snapshot.pop();
                for (i, &(x, top)) in BARS.iter().enumerate() {
                    let bounce = (t * std::f64::consts::TAU * 1.6 + i as f64 * 1.7).sin() as f32 * 7.0 * play;
                    let (a, b) = (at(x - BAR_W / 2.0, top + bounce + lift), at(x + BAR_W / 2.0, BAR_FOOT + lift));
                    let bar = graphene::Rect::new(a.x(), a.y() - BAR_W / 2.0 * u, b.x() - a.x(), b.y() - a.y() + BAR_W * u);
                    snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(bar, BAR_W / 2.0 * u));
                    snapshot.append_color(&rgba(look.bottom, 1.0), &bar);
                    snapshot.pop();
                }
                // the sea: the sun's foot and the bars' fade into the dark
                let (y0, y1) = (at(0.0, SEA.0).y(), at(0.0, SEA.1).y());
                let fade = graphene::Rect::new(0.0, y0, w, y1 - y0);
                snapshot.append_linear_gradient(&fade, &graphene::Point::new(0.0, y0), &graphene::Point::new(0.0, y1), &[gsk::ColorStop::new(0.0, rgba(look.bottom, 0.0)), gsk::ColorStop::new(1.0, rgba(look.bottom, 1.0))]);
                snapshot.append_color(&rgba(look.bottom, 1.0), &graphene::Rect::new(0.0, y1 - 0.5, w, h - y1 + 0.5));
                // what the app is getting ready first, and how far along
                if let Some((what, done)) = self.progress.borrow().as_ref() {
                    let layout = obj.create_pango_layout(Some(what));
                    let mut font = gtk::pango::FontDescription::from_string("Lato, Cantarell, sans-serif");
                    font.set_absolute_size((5.0 * u).max(13.0) as f64 * gtk::pango::SCALE as f64);
                    layout.set_font_description(Some(&font));
                    let (_, size) = layout.pixel_extents();
                    let top = y1 + 12.0 * u;
                    snapshot.save();
                    snapshot.translate(&graphene::Point::new(w / 2.0 - size.width() as f32 / 2.0, top));
                    snapshot.append_layout(&layout, &rgba(WHITE, 0.8));
                    snapshot.restore();
                    let (bw, bh) = (w.min(h) * 0.36, 4.0f32);
                    let track = graphene::Rect::new(w / 2.0 - bw / 2.0, top + size.height() as f32 + 10.0, bw, bh);
                    snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(track, bh / 2.0));
                    snapshot.append_color(&rgba(WHITE, 0.14), &track);
                    snapshot.append_color(&rgba(ORANGE, 1.0), &graphene::Rect::new(track.x(), track.y(), bw * done.clamp(0.0, 1.0) as f32, bh));
                    snapshot.pop();
                }
                snapshot.pop();
            }
            if reveal > 0.0 {
                snapshot.pop(); // (the dark, through its opening)
            }
            // then the logo writes itself: the R, its leg into the m, the wave fading out
            // (and at the end it stays a moment over the opening, then goes)
            snapshot.push_opacity(1.0 - ease((reveal as f64 - 0.35) / 0.55));
            if let Some(s) = settle {
                let k = t - s - SETTLE_S - WRITE_S;
                let parts = [ease(k / 0.65), ease((k - 0.55) / 0.75), ease((k - 1.25) / 0.3)];
                if parts[0] > 0.0 {
                    if self.logo.borrow().is_none() {
                        let p = |d: &str| gsk::Path::parse(d).ok();
                        if let (Some(r), Some(leg), Some(tail)) = (p(LOGO_R), p(LOGO_LEG), p(LOGO_TAIL)) {
                            self.logo.replace(Some((r, leg, tail)));
                        }
                    }
                    if let Some((r, leg, tail)) = self.logo.borrow().as_ref() {
                        // its 100 units, the R's middle in the window's middle
                        let v = w.min(h) * 0.55 / 100.0;
                        snapshot.save();
                        snapshot.translate(&graphene::Point::new(w / 2.0 - 50.0 * v, h / 2.0 - 50.0 * v));
                        snapshot.scale(v, v);
                        let round = gsk::Stroke::new(PEN);
                        round.set_line_cap(gsk::LineCap::Round);
                        round.set_line_join(gsk::LineJoin::Round);
                        if let Some(p) = written(r, parts[0]) {
                            snapshot.append_stroke(&p, &round, &rgba(WHITE, 1.0));
                        }
                        let area = graphene::Rect::new(20.0, 10.0, 100.0, 80.0);
                        if let Some(p) = written(leg, parts[1]) {
                            snapshot.push_stroke(&p, &round);
                            snapshot.append_linear_gradient(&area, &graphene::Point::new(50.5, 0.0), &graphene::Point::new(90.5, 0.0), &[gsk::ColorStop::new(0.0, rgba(WHITE, 1.0)), gsk::ColorStop::new(0.24, rgba(SUN_TOP, 1.0)), gsk::ColorStop::new(1.0, rgba(ORANGE, 1.0))]);
                            snapshot.pop();
                        }
                        if let Some(p) = written(tail, parts[2]) {
                            let butt = gsk::Stroke::new(PEN);
                            butt.set_line_cap(gsk::LineCap::Butt);
                            snapshot.push_stroke(&p, &butt);
                            snapshot.append_linear_gradient(&area, &graphene::Point::new(90.5, 71.0), &graphene::Point::new(100.5, 61.0), &[gsk::ColorStop::new(0.0, rgba(ORANGE, 1.0)), gsk::ColorStop::new(1.0, rgba(ORANGE, 0.0))]);
                            snapshot.pop();
                        }
                        snapshot.restore();
                    }
                    // and the name under it, coming up softly as the wave goes out
                    let name = ease((k - 1.1) / 0.55) as f32;
                    if name > 0.0 {
                        let v = w.min(h) * 0.55 / 100.0;
                        let layout = obj.create_pango_layout(None);
                        let mut font = gtk::pango::FontDescription::from_string("Lato, Cantarell, sans-serif Bold");
                        font.set_absolute_size((11.0 * v) as f64 * gtk::pango::SCALE as f64);
                        layout.set_font_description(Some(&font));
                        layout.set_markup(&format!("<span foreground='#{WHITE:06X}'>Radiance</span> <span foreground='#{ORANGE:06X}'>Music</span>"));
                        let (_, size) = layout.pixel_extents();
                        snapshot.save();
                        snapshot.translate(&graphene::Point::new(w / 2.0 - size.width() as f32 / 2.0, h / 2.0 + (36.0 + 4.0 * (1.0 - name)) * v));
                        snapshot.push_opacity(name as f64);
                        snapshot.append_layout(&layout, &rgba(WHITE, 1.0));
                        snapshot.pop();
                        snapshot.restore();
                    }
                }
            }
            snapshot.pop();
        }
    }
}

glib::wrapper! {
    pub struct Splash(ObjectSubclass<imp::Splash>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Splash {
    /// Over everything in `window` until [`Splash::ready`], in `look`; None
    /// (nothing shown) for screenshots (unless RADIANCE_SHOT_SPLASH) and when animations are off.
    pub fn cover(window: &adw::ApplicationWindow, look: Look) -> Option<Splash> {
        let shot = std::env::var_os("RADIANCE_SHOT").is_some() && std::env::var_os("RADIANCE_SHOT_SPLASH").is_none();
        if shot || gtk::Settings::default().is_some_and(|s| !s.is_gtk_enable_animations()) {
            return None;
        }
        let splash: Splash = glib::Object::builder().property("can-target", true).property("hexpand", true).property("vexpand", true).build();
        splash.imp().look.set(look);
        let content = window.content();
        window.set_content(None::<&gtk::Widget>);
        let overlay = gtk::Overlay::new();
        overlay.set_child(content.as_ref());
        // hidden until the logo is written: nothing behind is laid out or drawn meanwhile
        if let Some(c) = &content {
            c.set_visible(false);
        }
        splash.imp().content.replace(content);
        overlay.add_overlay(&splash);
        window.set_content(Some(&overlay));
        // a click: straight into the app (once it's ready)
        let click = gtk::GestureClick::new();
        let me = splash.downgrade();
        click.connect_pressed(move |_, _, _, _| {
            if let Some(s) = me.upgrade() {
                s.skip();
            }
        });
        splash.add_controller(click);
        splash.add_tick_callback(|s, clock| {
            let imp = s.imp();
            let now = clock.frame_time();
            // how late this frame is (the window's thread was busy: with what); counted below, if it showed
            let late = imp.last_frame.replace(Some(now)).map(|last| ((now - last) / 1000) as u32).filter(|ms| *ms > 50);
            let held_up_by = imp.last_job.get();
            imp.frames.set(imp.frames.get() + 1);
            // the next job, once this frame is painted (one a frame: it never stops);
            // the early ones first, before the sun moves (the clock waits for them)
            let early = !imp.early.borrow().is_empty();
            if !imp.busy.get() {
                let job = if early { imp.early.borrow_mut().pop_front() } else { imp.jobs.borrow_mut().pop_front() };
                if let Some((what, job)) = job {
                    imp.busy.set(true);
                    imp.last_job.set(what);
                    let weak = s.downgrade();
                    glib::idle_add_local_once(move || {
                        let t = std::time::Instant::now();
                        job();
                        // RADIANCE_SHOT_SPLASH: the jobs that held a frame up, to find them
                        if std::env::var_os("RADIANCE_SHOT_SPLASH").is_some() && t.elapsed().as_millis() > 25 {
                            eprintln!("splash: {what} took {} ms", t.elapsed().as_millis());
                        }
                        if let Some(s) = weak.upgrade() {
                            s.imp().busy.set(false);
                        }
                    });
                } else {
                    imp.last_job.set("nothing (the app's own work)");
                }
            }
            if early || imp.busy.get() && imp.start.get().is_none() {
                s.queue_draw();
                return glib::ControlFlow::Continue;
            }
            let start = imp.start.get().unwrap_or(now);
            imp.start.set(Some(start));
            let t = (now - start) as f64 / 1e6;
            imp.t.set(t);
            // a late frame only shows while something moves (not with the logo written and standing still)
            let still = imp.settle_from.get().is_some_and(|s| t >= s + SETTLE_S + WRITE_S + WRITTEN_S) && imp.fade_from.get().is_none();
            if let (Some(ms), false) = (late, still) {
                imp.slow.borrow_mut().push((ms, held_up_by));
            }
            if imp.settle_from.get().is_none() && imp.holds.get() == 0 && ((imp.ready.get() && t >= SHOWN_S) || t >= LONGEST_S) {
                imp.settle_from.set(Some(t));
            }
            if let (Some(settle), None) = (imp.settle_from.get(), imp.fade_from.get()) {
                // written: the window behind is shown (laid out while the logo stands still)
                if t >= settle + SETTLE_S + WRITE_S + WRITTEN_S {
                    s.show_content();
                }
                // and once it's been drawn a couple of times, the fade
                if t >= settle + SETTLE_S + WRITE_S + WRITTEN_S + HOLD_S && imp.shown_frames.get().is_some_and(|n| n >= 2) {
                    imp.fade_from.set(Some(t));
                    s.set_can_target(false); // the app answers clicks from here on
                }
            }
            if let Some(n) = imp.shown_frames.get() {
                imp.shown_frames.set(Some(n + 1));
            }
            if imp.fade_from.get().is_some_and(|f| t >= f + FADE_S) {
                // (gone after LONGEST_S with work left: it's done at once)
                let left: Vec<_> = imp.early.borrow_mut().drain(..).chain(imp.jobs.borrow_mut().drain(..)).collect();
                left.into_iter().for_each(|(_, job)| job());
                s.show_content();
                s.report();
                s.clear_offer();
                if let Some(o) = s.parent().and_downcast::<gtk::Overlay>() {
                    o.remove_overlay(s);
                }
                return glib::ControlFlow::Break;
            }
            s.queue_draw();
            glib::ControlFlow::Continue
        });
        Some(splash)
    }

    /// Work for the start: run after a frame of its own, in turn (the first once
    /// the splash is on screen). `what`: what it is, for the slow-frame report.
    pub fn then(&self, what: &'static str, job: impl FnOnce() + 'static) {
        self.imp().jobs.borrow_mut().push_back((what, Box::new(job)));
    }

    /// Work that can't be split up: done before the sun moves at all (a still
    /// moment), in turn, a frame each.
    pub fn first(&self, what: &'static str, job: impl FnOnce() + 'static) {
        self.imp().early.borrow_mut().push_back((what, Box::new(job)));
    }

    /// The window behind it, shown (once).
    fn show_content(&self) {
        let imp = self.imp();
        if imp.shown_frames.get().is_none() {
            if let Some(c) = imp.content.borrow().as_ref() {
                c.set_visible(true);
            }
            imp.shown_frames.set(Some(0));
        }
    }

    /// The app needs something first (the splitter, the first time): the sun
    /// keeps playing (with `progress`) until it's let go.
    pub fn hold(&self) {
        self.imp().holds.set(self.imp().holds.get() + 1);
    }

    pub fn release(&self) {
        let imp = self.imp();
        imp.holds.set(imp.holds.get().saturating_sub(1));
        if imp.holds.get() == 0 {
            imp.progress.replace(None);
            self.clear_offer();
        }
    }

    /// Buttons under what's being got ready (owner, 2026-10-31 audit: offline, the first
    /// start had no way past the sunrise): real buttons over the drawing, near the bottom.
    pub fn offer(&self, buttons: &impl IsA<gtk::Widget>) {
        self.clear_offer();
        if let Some(o) = self.parent().and_downcast::<gtk::Overlay>() {
            o.add_overlay(buttons);
            self.imp().offered.replace(Some(buttons.clone().upcast()));
        }
    }

    pub fn clear_offer(&self) {
        if let Some(w) = self.imp().offered.take() {
            if let Some(o) = w.parent().and_downcast::<gtk::Overlay>() {
                o.remove_overlay(&w);
            }
        }
    }

    /// What's being got ready, and how far along (0..1), under the sun.
    pub fn progress(&self, what: &str, done: f64) {
        self.imp().progress.replace(Some((what.to_string(), done)));
    }

    /// Everything's ready: the bars come to rest and the logo writes itself (once it has shown whole).
    pub fn ready(&self) {
        self.imp().ready.set(true);
    }

    /// A click: once ready, it fades into the app now.
    fn skip(&self) {
        let imp = self.imp();
        if imp.ready.get() && imp.holds.get() == 0 && imp.jobs.borrow().is_empty() && imp.fade_from.get().is_none() {
            self.show_content();
            imp.fade_from.set(Some(imp.t.get()));
            self.set_can_target(false);
        }
    }

    /// How smooth it was, in the app log (the slowest frames, and what held them up).
    fn report(&self) {
        let imp = self.imp();
        let mut slow = imp.slow.borrow().clone();
        slow.sort_by(|a, b| b.0.cmp(&a.0));
        let worst: Vec<String> = slow.iter().take(4).map(|(ms, what)| format!("{ms} ms after {what}")).collect();
        let line = format!(
            "splash: {} frames in {:.1} s, {} late{}",
            imp.frames.get(),
            imp.t.get(),
            slow.len(),
            if worst.is_empty() { String::new() } else { format!(" (the slowest: {})", worst.join(", ")) }
        );
        if std::env::var_os("RADIANCE_SHOT").is_some() {
            eprintln!("{line}");
        }
        crate::log::note(&line);
    }
}
