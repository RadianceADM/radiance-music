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
    let hidden = hide_keys(line);
    let line = hidden.as_str();
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

/// A Radiance Music Server's key in an address ("?t=KEY", "&t=KEY") is never written down.
fn hide_keys(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(at) = rest.find("t=") {
        let before = &rest[..at];
        let key = before.ends_with('?') || before.ends_with('&');
        out.push_str(&rest[..at + 2]);
        rest = &rest[at + 2..];
        if key {
            let n = rest.find(|c: char| !c.is_ascii_alphanumeric()).unwrap_or(rest.len());
            if n > 0 {
                out.push('…');
            }
            rest = &rest[n..];
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn server_keys_stay_out_of_the_log() {
        assert_eq!(super::hide_keys("load http://h:8080/files/A%20-%20B.opus?t=0a1b2c3d from 0 s"), "load http://h:8080/files/A%20-%20B.opus?t=… from 0 s");
        assert_eq!(super::hide_keys("x.jpg?v=2&t=ff00 and art=1"), "x.jpg?v=2&t=… and art=1");
        assert_eq!(super::hide_keys("nothing secret: start=5"), "nothing secret: start=5");
    }
}
