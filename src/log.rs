//! The app's own log, ~/.cache/radiance-music/app.log: songs loaded, started,
//! ended, failed; crossfades; the mixer; where the sound went; anything fixed
//! on the way. Kept short (a new file past 1 MB, the last one as app.log.1).

use std::io::Write;
use std::sync::Mutex;

use gtk::glib;

/// The last line written and when: the same line again within 10 s isn't.
static LAST: Mutex<Option<(String, std::time::Instant)>> = Mutex::new(None);

pub fn note(line: &str) {
    if std::env::var_os("RADIANCE_SHOT").is_some() {
        return; // screenshot copies keep out of it
    }
    let Ok(mut last) = LAST.lock() else { return };
    if last.as_ref().is_some_and(|(l, at)| l == line && at.elapsed().as_secs() < 10) {
        return;
    }
    *last = Some((line.to_string(), std::time::Instant::now()));
    let dir = glib::user_cache_dir().join("radiance-music");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("app.log");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 1_000_000) {
        let _ = std::fs::rename(&path, dir.join("app.log.1"));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let when = glib::DateTime::now_local().ok().and_then(|d| d.format("%F %T").ok()).map(|s| s.to_string()).unwrap_or_default();
        let _ = writeln!(f, "{when} {line}");
    }
}
