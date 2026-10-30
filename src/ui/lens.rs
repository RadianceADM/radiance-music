//! The lyrics' column: lines one under another, each drawn larger the closer
//! it is to the middle of the view. A line grows as it rises from below, is
//! full size in the middle while it's sung, and shrinks as it moves up into
//! the lines already sung. Drawing only: scrolling lays nothing out again.
//!
//! The line being sung can be drawn bigger still (set_grow: 2.5 times in Now
//! Playing). It's wrapped again to fit the column at that size, and its room
//! in the column swells as the last line's shrinks back, over GROW_US: its
//! words swell from their own size, the small text fading into the big. Only
//! when the line changes is the column laid out again (for those moments).

use std::cell::{Cell, RefCell};

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, graphene, gsk, pango};

/// The smallest a line gets: above the middle (sung) and below it (to come).
const SUNG: f32 = 0.58;
const COMING: f32 = 0.7;

/// How long the line being sung takes to swell (and the last one to shrink back).
const GROW_US: f64 = 400_000.0;

/// The most of the view the line being sung may fill: a long line on a small
/// screen grows less (down to 1.5 times).
const ROOM: f32 = 0.6;
const LEAST: f32 = 1.5;

/// A line swelling or shrinking back: how big it was (0 its own size, 1 grown),
/// how big it's going to be and when it started (frame time, µs).
#[derive(Clone, Copy)]
pub struct Swell {
    line: usize,
    from: f32,
    to: f32,
    start: Option<i64>,
}

impl Swell {
    fn at(&self, now: Option<i64>) -> f32 {
        let x = match (self.start, now) {
            (Some(s), Some(n)) => ((n - s) as f64 / GROW_US).clamp(0.0, 1.0) as f32,
            _ => 1.0,
        };
        let ease = 1.0 - (1.0 - x).powi(3);
        self.from + (self.to - self.from) * ease
    }

    fn done(&self, now: Option<i64>) -> bool {
        (self.at(now) - self.to).abs() < 1e-3
    }
}

/// A line's text as it's drawn grown: laid out again to fit `width` at
/// `grow` times its size (less when that won't fit `room`).
struct Grown {
    layout: pango::Layout,
    grow: f32,
    /// The room it takes in the column, its padding included.
    height: f32,
    left: f32,
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn grown(child: &gtk::Widget, width: i32, room: f32, most: f32) -> Option<Grown> {
    let label = child.downcast_ref::<gtk::Label>()?;
    #[allow(deprecated)] // the label's own padding (CSS), to wrap and place the text as the label does
    let pad = label.style_context().padding();
    let inner = (width - pad.left() as i32 - pad.right() as i32).max(1) as f32;
    let pads = (pad.top() + pad.bottom()) as f32;
    let base = label.layout();
    let mut grow = most;
    loop {
        let layout = base.copy();
        layout.set_width((inner / grow * pango::SCALE as f32) as i32);
        let height = layout.pixel_size().1 as f32 * grow + pads;
        if height <= room || grow <= LEAST {
            return Some(Grown { layout, grow, height, left: pad.left() as f32 });
        }
        grow = (grow - 0.25).max(LEAST);
    }
}

mod imp {
    use super::*;

    pub struct Lens {
        /// Lines grow and shrink (timed lyrics); off: drawn as laid out (plain text, notes).
        pub on: Cell<bool>,
        /// How much bigger the line being sung is drawn (1: no bigger).
        pub grow: Cell<f32>,
        /// The line being sung, and the lines swelling or shrinking back.
        pub big: Cell<Option<usize>>,
        pub swells: RefCell<Vec<Swell>>,
        pub tick: RefCell<Option<gtk::TickCallbackId>>,
        /// Each line's room in the column as laid out: (top, height).
        pub slots: RefCell<Vec<(f32, f32)>>,
    }

    impl Default for Lens {
        fn default() -> Self {
            Lens { on: Cell::new(false), grow: Cell::new(1.0), big: Cell::new(None), swells: RefCell::default(), tick: RefCell::default(), slots: RefCell::default() }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Lens {
        const NAME: &'static str = "RadianceLens";
        type Type = super::Lens;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for Lens {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl Lens {
        pub fn view(&self) -> Option<gtk::Widget> {
            self.obj().ancestor(gtk::ScrolledWindow::static_type())
        }

        /// How much of the view the line being sung may fill.
        pub fn room(&self) -> f32 {
            ROOM * self.view().map(|v| v.height() as f32).filter(|h| *h > 0.0).unwrap_or(800.0)
        }

        /// How far line `i` has swollen now (0-1).
        pub fn swell(&self, i: usize, now: Option<i64>) -> f32 {
            self.swells.borrow().iter().find(|s| s.line == i).map(|s| s.at(now)).unwrap_or(0.0)
        }

        pub fn now(&self) -> Option<i64> {
            self.obj().frame_clock().map(|c| c.frame_time())
        }

        /// Each line's height at `width`: as it's going to be (`settled`), or as it is now.
        pub fn heights(&self, width: i32, settled: bool) -> Vec<f32> {
            let obj = self.obj();
            let (now, room, most) = (self.now(), self.room(), self.grow.get());
            let mut out = Vec::new();
            let mut next = obj.first_child();
            let mut i = 0;
            while let Some(child) = next {
                next = child.next_sibling();
                let h = if child.should_layout() { child.measure(gtk::Orientation::Vertical, width).1 as f32 } else { 0.0 };
                let v = if settled { if self.big.get() == Some(i) { 1.0 } else { 0.0 } } else { self.swell(i, now) };
                let h = match (v > 0.0, self.on.get() && most > 1.0) {
                    (true, true) => grown(&child, width, room, most).map(|g| h + (g.height - h) * v).unwrap_or(h),
                    _ => h,
                };
                out.push(h);
                i += 1;
            }
            out
        }
    }

    impl WidgetImpl for Lens {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let obj = self.obj();
            if orientation == gtk::Orientation::Horizontal {
                let (mut min, mut nat) = (0, 0);
                let mut next = obj.first_child();
                while let Some(child) = next {
                    next = child.next_sibling();
                    if child.should_layout() {
                        let (a, b, _, _) = child.measure(orientation, -1);
                        (min, nat) = (min.max(a), nat.max(b));
                    }
                }
                return (min, nat, -1, -1);
            }
            // as it's going to be (a line shrinking back may run past the end for a moment)
            let total = self.heights(for_size, true).iter().sum::<f32>().ceil() as i32;
            (total, total, -1, -1)
        }

        fn size_allocate(&self, width: i32, _height: i32, _baseline: i32) {
            let obj = self.obj();
            let heights = self.heights(width, false);
            let mut slots = Vec::with_capacity(heights.len());
            let (mut y, mut next, mut i) = (0.0f32, obj.first_child(), 0);
            while let Some(child) = next {
                next = child.next_sibling();
                let slot = heights.get(i).copied().unwrap_or(0.0);
                if child.should_layout() {
                    let own = child.measure(gtk::Orientation::Vertical, width).1;
                    // the label in the middle of its room (the text is drawn grown over it)
                    let top = (y + (slot - own as f32) / 2.0).round();
                    child.allocate(width, own, -1, Some(gsk::Transform::new().translate(&graphene::Point::new(0.0, top))));
                }
                slots.push((y, slot));
                y += slot;
                i += 1;
            }
            self.slots.replace(slots);
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let view = self.view();
            let middle = view.as_ref().and_then(|v| v.compute_point(&*obj, &graphene::Point::new(0.0, v.height() as f32 / 2.0)));
            let mut next = obj.first_child();
            let (Some(view), Some(middle), true) = (view, middle, self.on.get()) else {
                while let Some(child) = next {
                    obj.snapshot_child(&child, snapshot);
                    next = child.next_sibling();
                }
                return;
            };
            let half = (view.height() as f32 / 2.0).max(1.0);
            let (now, width, room, most) = (self.now(), obj.width(), self.room(), self.grow.get());
            let slots = self.slots.borrow();
            let mut i = 0;
            while let Some(child) = next {
                next = child.next_sibling();
                let Some(&(top, h)) = slots.get(i) else { break };
                i += 1;
                // grown from the line's left edge (left-aligned lyrics stay in place)
                let cy = top + h / 2.0;
                let d = (cy - middle.y()) / half; // -1 the top edge, 0 the middle, 1 the bottom edge
                if d.abs() > 1.6 + h / half {
                    continue; // out of sight
                }
                let x = d.abs().min(1.0);
                let ease = x * x * (3.0 - 2.0 * x);
                let scale = 1.0 - ease * (1.0 - if d < 0.0 { SUNG } else { COMING });
                snapshot.save();
                snapshot.translate(&graphene::Point::new(0.0, cy));
                snapshot.scale(scale, scale);
                snapshot.translate(&graphene::Point::new(0.0, -cy));
                let v = self.swell(i - 1, now);
                match (v > 0.0 && most > 1.0).then(|| grown(&child, width, room, most)).flatten() {
                    None => obj.snapshot_child(&child, snapshot),
                    Some(g) => {
                        // the words at the same size in both: the line as laid out fading, the line wrapped for its new size coming in
                        let s = 1.0 + (g.grow - 1.0) * v;
                        let small = 1.0 - smooth(0.0, 0.4, v);
                        if small > 0.01 {
                            snapshot.push_opacity(small as f64);
                            snapshot.save();
                            snapshot.translate(&graphene::Point::new(g.left, cy));
                            snapshot.scale(s, s);
                            snapshot.translate(&graphene::Point::new(-g.left, -cy));
                            obj.snapshot_child(&child, snapshot);
                            snapshot.restore();
                            snapshot.pop();
                        }
                        let big = smooth(0.2, 0.6, v);
                        if big > 0.01 {
                            let text_h = g.layout.pixel_size().1 as f32;
                            snapshot.push_opacity(big as f64);
                            snapshot.save();
                            snapshot.translate(&graphene::Point::new(g.left, cy - text_h * s / 2.0));
                            snapshot.scale(s, s);
                            snapshot.append_layout(&g.layout, &child.color());
                            snapshot.restore();
                            snapshot.pop();
                        }
                    }
                }
                snapshot.restore();
            }
        }
    }
}

glib::wrapper! {
    pub struct Lens(ObjectSubclass<imp::Lens>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for Lens {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl Lens {
    pub fn append(&self, child: &impl IsA<gtk::Widget>) {
        child.set_parent(self);
    }

    pub fn clear(&self) {
        while let Some(child) = self.first_child() {
            child.unparent();
        }
        let imp = self.imp();
        imp.big.set(None);
        imp.swells.borrow_mut().clear();
        imp.slots.borrow_mut().clear();
    }

    /// Lines grow towards the middle (true) or are drawn as laid out.
    pub fn set_lensing(&self, on: bool) {
        self.imp().on.set(on);
        self.queue_resize();
    }

    /// The line being sung is drawn this much bigger (1: no bigger).
    pub fn set_grow(&self, grow: f32) {
        self.imp().grow.set(grow.max(1.0));
        self.queue_resize();
    }

    /// The line being sung (it swells, the last one shrinks back).
    pub fn set_big(&self, line: Option<usize>) {
        let imp = self.imp();
        if imp.big.get() == line || imp.grow.get() <= 1.0 {
            return;
        }
        imp.big.set(line);
        let now = imp.now();
        let mut swells = imp.swells.borrow_mut();
        let mut moving: Vec<Swell> = swells.iter().map(|s| Swell { line: s.line, from: s.at(now), to: 0.0, start: now }).filter(|s| s.from > 1e-3).collect();
        if let Some(i) = line {
            let from = moving.iter().position(|s| s.line == i).map(|k| moving.remove(k).from).unwrap_or(0.0);
            moving.push(Swell { line: i, from, to: 1.0, start: now });
        }
        *swells = moving;
        drop(swells);
        self.queue_resize();
        self.ensure_ticking();
    }

    /// Where line `child` will sit once it's done swelling (its middle, the
    /// column's own coordinates): where to scroll to.
    pub fn settled_middle(&self, child: &impl IsA<gtk::Widget>) -> Option<f32> {
        let heights = self.imp().heights(self.width(), true);
        let mut next = self.first_child();
        let mut y = 0.0;
        for h in heights {
            let c = next?;
            if &c == child.upcast_ref() {
                return Some(y + h / 2.0);
            }
            y += h;
            next = c.next_sibling();
        }
        None
    }

    /// A frame callback only while a line swells or shrinks.
    fn ensure_ticking(&self) {
        let imp = self.imp();
        if imp.tick.borrow().is_some() {
            return;
        }
        let id = self.add_tick_callback(|lens, clock| {
            let imp = lens.imp();
            let now = Some(clock.frame_time());
            lens.queue_allocate();
            lens.queue_draw();
            let mut swells = imp.swells.borrow_mut();
            if swells.iter().all(|s| s.done(now)) {
                swells.retain(|s| s.to > 0.0);
                for s in swells.iter_mut() {
                    s.from = s.to;
                }
                imp.tick.borrow_mut().take();
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
        imp.tick.replace(Some(id));
    }

    /// Redraws as `adj` scrolls (the lines' sizes follow where they are).
    pub fn follow(&self, adj: &gtk::Adjustment) {
        let lens = self.downgrade();
        adj.connect_value_changed(move |_| {
            if let Some(lens) = lens.upgrade() {
                lens.queue_draw();
            }
        });
    }

    /// Which line is at `y` (the column's own coordinates, as laid out).
    pub fn index_at(&self, y: f64) -> Option<usize> {
        let y = y as f32;
        self.imp().slots.borrow().iter().position(|&(top, h)| (top..top + h).contains(&y))
    }
}
