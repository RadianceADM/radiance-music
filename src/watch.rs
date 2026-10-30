//! A watch on the window's own thread. If it's ever stuck for 2 s or more (the
//! window stops answering; GNOME then offers to force quit), what it was doing
//! is written down at once in ~/.cache/radiance-music/stalls.log, and how long
//! it lasted once it's over, so a freeze can be found and fixed. When it's
//! stuck 4 s, every thread's stack goes in too (through gdb, when installed;
//! only gdb, for that moment, is let look at the app).

use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use gtk::glib;

/// The window thread's last heartbeat (ms since the watch started).
static BEAT: AtomicU64 = AtomicU64::new(0);
/// What the window thread was last doing.
static DOING: Mutex<&'static str> = Mutex::new("starting");

/// Notes what the window thread is about to do (cheap: call it freely).
pub fn doing(what: &'static str) {
    if let Ok(mut d) = DOING.lock() {
        *d = what;
    }
}

/// Every thread's stack, from gdb (allowed to look at this app just this once).
fn stacks() -> String {
    let me = std::process::id();
    let Ok(child) = std::process::Command::new("sh")
        .args(["-c", &format!("sleep 0.3; exec gdb -p {me} -batch -nx -ex 'thread apply all bt 25'")])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
    else {
        return "(no gdb)".into();
    };
    // Yama: only this one process (it becomes gdb) may attach
    unsafe {
        libc::prctl(libc::PR_SET_PTRACER, child.id() as libc::c_ulong, 0, 0, 0);
    }
    let out = child.wait_with_output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
    unsafe {
        libc::prctl(libc::PR_SET_PTRACER, 0 as libc::c_ulong, 0, 0, 0);
    }
    out.lines().filter(|l| l.starts_with('#') || l.starts_with("Thread")).collect::<Vec<_>>().join("\n")
}

fn note(line: &str) {
    let dir = glib::user_cache_dir().join("radiance-music");
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("stalls.log")) {
        let when = glib::DateTime::now_local().ok().and_then(|d| d.format("%F %T").ok()).map(|s| s.to_string()).unwrap_or_default();
        let _ = writeln!(f, "{when} {line}");
    }
}

pub fn start(window: &impl gtk::prelude::IsA<gtk::Widget>) {
    use gtk::prelude::*;
    let t0 = Instant::now();
    glib::timeout_add_local(Duration::from_millis(100), move || {
        BEAT.store(t0.elapsed().as_millis() as u64, Ordering::Relaxed);
        doing("waiting");
        glib::ControlFlow::Continue
    });
    // the window's own frames: laying out and drawing
    let w = window.clone().upcast::<gtk::Widget>();
    w.connect_realize(|w| {
        if let Some(clock) = w.frame_clock() {
            clock.connect_layout(|_| doing("laying out the window"));
            clock.connect_paint(|_| doing("drawing the window"));
        }
    });
    std::thread::Builder::new()
        .name("watch".into())
        .spawn(move || {
            let mut stuck: Option<(u64, &'static str)> = None;
            let mut traced = false;
            let mut hitch: Option<(u64, &'static str)> = None;
            loop {
                std::thread::sleep(Duration::from_millis(100));
                let now = t0.elapsed().as_millis() as u64;
                let beat = BEAT.load(Ordering::Relaxed);
                let what = DOING.lock().map(|d| *d).unwrap_or("?");
                // a hitch: the window late by 0.3 s or more; written down if it reached 0.6 s
                let late = now.saturating_sub(beat);
                match hitch {
                    None if late >= 300 => hitch = Some((beat, what)),
                    Some((from, was)) if late < 200 => {
                        let took = beat.saturating_sub(from);
                        if (600..2000).contains(&took) {
                            note(&format!("the window hitched {:.1} s while {was}", took as f64 / 1000.0));
                        }
                        hitch = None;
                    }
                    _ => {}
                }
                match (late >= 2000, stuck) {
                    (true, None) => {
                        note(&format!("the window is stuck (2 s and counting) while {what}"));
                        stuck = Some((beat, what));
                        traced = false;
                    }
                    (true, Some(_)) if !traced && now.saturating_sub(beat) >= 4000 => {
                        traced = true;
                        note(&format!("stacks, 4 s in:\n{}", stacks()));
                    }
                    (false, Some((from, what))) => {
                        note(&format!("the window was stuck {:.1} s while {what}", beat.saturating_sub(from) as f64 / 1000.0));
                        stuck = None;
                    }
                    _ => {}
                }
            }
        })
        .ok();
}
