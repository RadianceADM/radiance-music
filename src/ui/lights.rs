//! Home's banner lights (owner, 2026-11-01: "gumagalaw yung back draft non like with
//! gradients"): two soft lights drifting slowly over the cover, under the scrim. They
//! were a CSS animation (`.hero-drift`, 18 s each way, for ever); on GTK 4.24 every one
//! of its frames had the window's styles worked out again, about 1.5 cores all the time
//! on Windows 0.7.16/0.7.17 (with animations off: 0 %), and the PC lagged (owner,
//! 2026-11-04). Now a widget that only draws: the same two radial gradients, moved 12
//! times a second while the banner is on screen and the window is the one in front, not
//! minimized (working in another app, they stand still and cost nothing); nothing else is touched.

use std::cell::{Cell, RefCell};

use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, graphene, gsk};

/// One way across (then back), as the CSS had it: 18s ease-in-out infinite alternate.
const LEG_S: f64 = 18.0;
/// 12 frames a second: soft lights 18 s on their way across move a few pixels a frame, which
/// doesn't show; on GTK 4.24 each frame redraws the whole window (20 a second: ~22 % of a core
/// on ribs03 while in front).
const FRAME_US: i64 = 83_333;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Lights {
        pub tick: RefCell<Option<gtk::TickCallbackId>>,
        /// The window, and our handlers of its being in front and of its being minimized (while on screen).
        pub front: RefCell<Option<(gtk::Window, glib::SignalHandlerId, glib::SignalHandlerId)>>,
        /// Windows: a look once a second at whether the window is minimized or behind (GTK doesn't say there).
        pub look: RefCell<Option<glib::SourceId>>,
        /// When they started moving (frame time, µs).
        pub start: Cell<Option<i64>>,
        pub last: Cell<i64>,
        /// Where they are on their way across, eased (0-1).
        pub at: Cell<f64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Lights {
        const NAME: &'static str = "RadianceHeroLights";
        type Type = super::Lights;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for Lights {}

    impl WidgetImpl for Lights {
        fn map(&self) {
            self.parent_map();
            let obj = self.obj();
            match obj.root().and_downcast::<gtk::Window>() {
                Some(win) => {
                    // in front and not minimized (Windows keeps a minimized window "active")
                    let seen = |w: &gtk::Window| w.is_active() && !w.is_suspended();
                    let weak = obj.downgrade();
                    let active = win.connect_is_active_notify(move |w| {
                        if let Some(l) = weak.upgrade() {
                            l.follow(seen(w));
                        }
                    });
                    let weak = obj.downgrade();
                    let suspended = win.connect_suspended_notify(move |w| {
                        if let Some(l) = weak.upgrade() {
                            l.follow(seen(w));
                        }
                    });
                    obj.follow(seen(&win));
                    #[cfg(windows)]
                    {
                        let weak = obj.downgrade();
                        let look = glib::timeout_add_local(std::time::Duration::from_secs(1), move || {
                            let Some(l) = weak.upgrade() else { return glib::ControlFlow::Break };
                            if let Some(w) = l.root().and_downcast::<gtk::Window>() {
                                l.follow(seen(&w) && super::in_front(&w));
                            }
                            glib::ControlFlow::Continue
                        });
                        self.look.replace(Some(look));
                    }
                    self.front.replace(Some((win, active, suspended)));
                }
                None => obj.follow(true),
            }
        }

        fn unmap(&self) {
            if let Some((win, active, suspended)) = self.front.take() {
                win.disconnect(active);
                win.disconnect(suspended);
            }
            if let Some(look) = self.look.take() {
                look.remove();
            }
            self.obj().follow(false); // off screen: no frames at all
            self.parent_unmap();
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let w = self.obj();
            let (width, height) = (w.width() as f32, w.height() as f32);
            if width <= 0.0 || height <= 0.0 {
                return;
            }
            let e = self.at.get() as f32;
            let bounds = graphene::Rect::new(0.0, 0.0, width, height);
            // the second first: CSS draws its first layer on top
            light(snapshot, &bounds, 1.6, (0.70, 0.65), (1.0 - e, 1.0 - 0.7 * e), rgba(0xFF, 0xAE, 0x63, 0.22));
            light(snapshot, &bounds, 1.8, (0.35, 0.45), (e, 0.7 * e), rgba(0xF3, 0x8A, 0x35, 0.42));
        }
    }
}

glib::wrapper! {
    pub struct Lights(ObjectSubclass<imp::Lights>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for Lights {
    fn default() -> Self {
        Self::new()
    }
}

impl Lights {
    pub fn new() -> Self {
        glib::Object::builder().property("can-target", false).build()
    }

    /// Moves them (`on`: on screen, the window in front) or stops them where they are. With
    /// animations off in the system's settings, they stay put.
    fn follow(&self, on: bool) {
        let imp = self.imp();
        if !on {
            if let Some(id) = imp.tick.take() {
                id.remove();
            }
            return;
        }
        if imp.tick.borrow().is_some() || !self.settings().is_gtk_enable_animations() {
            return;
        }
        let id = self.add_tick_callback(|w, clock| {
            let imp = w.imp();
            let now = clock.frame_time();
            if now - imp.last.get() >= FRAME_US {
                imp.last.set(now);
                let start = imp.start.get().unwrap_or(now);
                imp.start.set(Some(start));
                imp.at.set(eased((now - start) as f64 / 1e6 / LEG_S));
                w.queue_draw();
            }
            glib::ControlFlow::Continue
        });
        imp.tick.replace(Some(id));
    }
}

/// Windows: the window is the one in front and not minimized, as Windows itself says (GTK's
/// own "active" stays on there when it's minimized or behind another app).
#[cfg(windows)]
fn in_front(win: &gtk::Window) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, IsIconic};
    let Some(hwnd) = win.surface().and_downcast::<gdk4_win32::Win32Surface>().map(|s| s.handle().0) else { return true };
    unsafe { GetForegroundWindow() == hwnd as _ && IsIconic(hwnd as _) == 0 }
}

/// Where the lights are after `legs` trips across (there and back again, eased at both ends).
fn eased(legs: f64) -> f64 {
    let leg = legs.floor();
    let u = legs - leg;
    let u = if leg as i64 % 2 == 1 { 1.0 - u } else { u };
    0.5 - 0.5 * (std::f64::consts::PI * u).cos()
}

fn rgba(r: u8, g: u8, b: u8, a: f32) -> gdk::RGBA {
    gdk::RGBA::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a)
}

/// One light as the CSS drew it: `radial-gradient(circle farthest-side at cx cy, colour,
/// transparent)` on a picture `scale` times the banner's size, at `background-position:
/// px py` (parts of 1). The picture always covers the banner, so nothing needs clipping.
fn light(snapshot: &gtk::Snapshot, bounds: &graphene::Rect, scale: f32, (cx, cy): (f32, f32), (px, py): (f32, f32), colour: gdk::RGBA) {
    let (w, h) = (bounds.width(), bounds.height());
    let (iw, ih) = (w * scale, h * scale);
    let (ox, oy) = ((w - iw) * px, (h - ih) * py);
    let centre = graphene::Point::new(ox + cx * iw, oy + cy * ih);
    let radius = (cx * iw).max((1.0 - cx) * iw).max(cy * ih).max((1.0 - cy) * ih);
    let clear = gdk::RGBA::new(colour.red(), colour.green(), colour.blue(), 0.0);
    snapshot.append_radial_gradient(bounds, &centre, radius, radius, 0.0, 1.0, &[gsk::ColorStop::new(0.0, colour), gsk::ColorStop::new(1.0, clear)]);
}

#[cfg(test)]
mod tests {
    use super::eased;

    #[test]
    fn the_lights_go_there_and_back() {
        assert!(eased(0.0).abs() < 1e-9);
        assert!((eased(0.5) - 0.5).abs() < 1e-9);
        assert!((eased(1.0) - 1.0).abs() < 1e-9); // across
        assert!((eased(1.5) - 0.5).abs() < 1e-9);
        assert!(eased(2.0).abs() < 1e-9); // and back
    }
}
