//! Report a Problem (owner 2026-11-02, chosen in a dialog): the person sees
//! exactly what will be sent (the app's recent log with their name, home
//! folder and email addresses taken out, the app's version, the kind of
//! install, the system), may add a note, and presses Send. Nothing leaves
//! without that press. It lands in the admin (admin.radianceph.com, see
//! RadianceADM/radianceph#43): POST /api/music/reports.

use serde_json::{json, Value};

use crate::feedback::{self, Failure};

/// The limits, as the admin takes them (radianceph#43): the note in characters,
/// the log in bytes (the admin's is 204,800), the system in characters.
pub const MOST_NOTE: usize = 2000;
pub const MOST_LOG: usize = 200_000;
pub const MOST_SYSTEM: usize = 500;
/// Sent instead of an empty log (the admin needs one).
pub const NO_LOG: &str = "(the app's log is empty)\n";

/// What will be sent, shown to the person before they send it.
#[derive(Clone, Debug)]
pub struct Report {
    pub platform: &'static str,
    pub version: String,
    pub system: String,
    pub install: String,
    pub log: String,
}

impl Report {
    /// Everything but the note, gathered now (worker thread: it reads the log).
    pub fn gather() -> Report {
        let dir = glib::user_cache_dir().join("radiance-music");
        let mut raw = String::new();
        for name in ["app.log.1", "app.log"] {
            if let Ok(t) = std::fs::read_to_string(dir.join(name)) {
                raw.push_str(&t);
            }
        }
        // the Windows beta's own log (beta-crash.log, beside its data) goes along, after the engine's
        let beta = crate::update::is_beta().then(|| glib::user_data_dir().join("radiance-music").join("beta-crash.log"));
        let home = glib::home_dir().to_string_lossy().to_string();
        let user = glib::user_name().to_string_lossy().to_string();
        gather_with(&crate::update::version_here(), &raw, beta.as_deref(), &home, &user)
    }

    /// The body of POST /api/music/reports (an empty note left out).
    pub fn body(&self, note: &str) -> Value {
        let system: String = self.system.chars().take(MOST_SYSTEM).collect();
        let log = if self.log.trim().is_empty() { NO_LOG } else { self.log.as_str() };
        let mut v = json!({ "platform": self.platform, "version": self.version, "system": system, "log": log, "install": self.install });
        let note: String = note.trim().chars().take(MOST_NOTE).collect();
        if !note.is_empty() {
            v["note"] = json!(note);
        }
        v
    }
}

/// Where the beta's own log starts in what's sent.
pub const BETA_LOG_HEAD: &str = "--- the beta's own log ---";

/// A report of `version`, with `engine_log` (and the beta's own log after it), scrubbed and
/// cut to its newest MOST_LOG bytes.
pub fn gather_with(version: &str, engine_log: &str, beta_log: Option<&std::path::Path>, home: &str, user: &str) -> Report {
    let mut text = engine_log.to_string();
    if let Some(t) = beta_log.and_then(|p| std::fs::read_to_string(p).ok()) {
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(BETA_LOG_HEAD);
        text.push('\n');
        text.push_str(&t);
    }
    Report { platform: feedback::platform(), version: version.into(), system: system(), install: install_id(), log: newest(&scrub(&text, home, user), MOST_LOG) }
}

/// Sends it: the report's code ("RM-0123") to quote, or why not (worker thread).
pub fn send(report: &Report, note: &str) -> Result<String, Failure> {
    let url = format!("{}/api/music/reports", feedback::base());
    let body = feedback::answer(feedback::agent().post(&url).send_json(report.body(note)))?;
    Ok(body["code"].as_str().map(str::to_string).or_else(|| body["id"].as_u64().map(|n| format!("RM-{n:04}"))).unwrap_or_else(|| "sent".into()))
}

/// The system it runs on, in a few words ("Ubuntu 26.04 LTS, GNOME", "Windows 11 (26100)").
pub fn system() -> String {
    #[cfg(windows)]
    {
        let mut cmd = std::process::Command::new("cmd");
        cmd.args(["/c", "ver"]);
        crate::setup::quiet_window(&mut cmd, false);
        let ver = cmd.output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).filter(|s| !s.is_empty());
        ver.unwrap_or_else(|| "Windows".into())
    }
    #[cfg(not(windows))]
    {
        // in the Flatpak, /etc is the runtime's: the computer's own is under /run/host
        let os = ["/run/host/os-release", "/etc/os-release"]
            .iter()
            .find_map(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| t.lines().find_map(|l| l.strip_prefix("PRETTY_NAME=").map(|v| v.trim_matches('"').to_string())))
            .unwrap_or_else(|| "Linux".into());
        // in the snap, /etc is the snap's base ("Ubuntu Core 26") and the computer's own isn't
        // readable without another permission: the kernel's version says enough
        let os = if crate::update::in_snap() {
            let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease").map(|k| k.trim().to_string()).unwrap_or_default();
            format!("Linux {kernel} (Snap)")
        } else {
            os
        };
        match std::env::var("XDG_CURRENT_DESKTOP").ok().filter(|d| !d.is_empty()) {
            Some(desktop) => format!("{os}, {}", desktop.replace(':', " ")),
            None => os,
        }
    }
}

/// A random id made once for this install (no personal data): reports from the same
/// install can be told apart from others, and the admin can limit how many come.
pub fn install_id() -> String {
    let path = glib::user_data_dir().join("radiance-music/install-id");
    if let Some(id) = std::fs::read_to_string(&path).ok().map(|s| s.trim().to_string()).filter(|s| s.len() == 36) {
        return id;
    }
    let mut b = [0u8; 16];
    let _ = getrandom::fill(&mut b);
    b[6] = (b[6] & 0x0f) | 0x40; // version 4
    b[8] = (b[8] & 0x3f) | 0x80; // variant
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    let id = format!("{}-{}-{}-{}-{}", &hex[0..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..32]);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&path, &id);
    id
}

/// The app's log, its newest part (up to MOST_LOG, whole lines), with the person's
/// name, home folder and email addresses taken out.
pub fn recent_log() -> String {
    let dir = glib::user_cache_dir().join("radiance-music");
    let mut text = String::new();
    for name in ["app.log.1", "app.log"] {
        if let Ok(t) = std::fs::read_to_string(dir.join(name)) {
            text.push_str(&t);
        }
    }
    let home = glib::home_dir().to_string_lossy().to_string();
    let user = glib::user_name().to_string_lossy().to_string();
    let clean = scrub(&text, &home, &user);
    newest(&clean, MOST_LOG)
}

/// Home folder → "~", the user's name → "you", email addresses → "(email)".
pub fn scrub(text: &str, home: &str, user: &str) -> String {
    let mut out = text.to_string();
    // the folder as written, as a file:// address writes it (spaces as %20, letters like ñ as
    // %C3%B1), and on Windows with forward slashes too (C:/Users/…, as GStreamer's addresses);
    // in any case (Windows' c:\users\… is the same folder)
    if home.len() > 1 {
        let forward = home.replace('\\', "/");
        for form in [home.to_string(), home.replace(' ', "%20"), forward.clone(), forward.replace(' ', "%20"), percent(&forward)] {
            out = replace_any_case(&out, &form, "~");
        }
    }
    if user.len() >= 3 {
        for form in [user.to_string(), percent(user)] {
            out = replace_any_case(&out, &form, "you");
        }
    }
    out.split_inclusive(char::is_whitespace)
        .map(|word| {
            let w = word.trim_end();
            let at = w.find('@');
            let looks_like_mail = at.is_some_and(|i| i > 0 && w[i + 1..].contains('.') && !w[i + 1..].starts_with('.'));
            if looks_like_mail {
                format!("(email){}", &word[w.len()..])
            } else {
                word.to_string()
            }
        })
        .collect()
}

/// `s` as a file:// address writes it (GLib's g_filename_to_uri: these kept, the rest %XX).
fn percent(s: &str) -> String {
    s.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-._~!$&'()*+,;=:@/".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

/// Every `find` in `text` (letters in either case) as `with`.
fn replace_any_case(text: &str, find: &str, with: &str) -> String {
    let (t, f) = (text.as_bytes(), find.as_bytes());
    if f.is_empty() {
        return text.to_string();
    }
    let (mut out, mut i, mut last) = (String::with_capacity(text.len()), 0, 0);
    while i + f.len() <= t.len() {
        if t[i..i + f.len()].eq_ignore_ascii_case(f) && text.is_char_boundary(i) && text.is_char_boundary(i + f.len()) {
            out.push_str(&text[last..i]);
            out.push_str(with);
            i += f.len();
            last = i;
        } else {
            i += 1;
        }
    }
    out.push_str(&text[last..]);
    out
}

/// The last `most` bytes of `text`, starting at a whole line.
fn newest(text: &str, most: usize) -> String {
    if text.len() <= most {
        return text.to_string();
    }
    let mut from = text.len() - most;
    while !text.is_char_boundary(from) {
        from += 1;
    }
    let tail = &text[from..];
    tail.find('\n').map(|i| tail[i + 1..].to_string()).unwrap_or_else(|| tail.to_string())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    /// Final plan Review Focus 4: on Windows the home folder is written both ways (C:\\Users\\… in
    /// paths, file:///C:/Users/… in GStreamer's addresses, spaces as %20): neither leaves.
    #[test]
    fn windows_paths_are_scrubbed_both_ways() {
        let log = "load file:///C:/Users/Bassi%20Saba/Music/Radiance%20Music/a.flac\nsaved C:\\Users\\Bassi Saba\\Music\\x.flac\nC:/Users/Bassi Saba/AppData/y\n";
        let out = scrub(log, "C:\\Users\\Bassi Saba", "Bassi Saba");
        assert!(!out.contains("Bassi"), "{out}");
        assert!(out.contains("file:///~/Music/Radiance%20Music/a.flac"), "{out}");
        assert!(out.contains("saved ~\\Music\\x.flac") && out.contains("~/AppData/y"), "{out}");
    }

    /// Final review: a name with ñ (common here) left the report in file:// addresses (written
    /// Ni%C3%B1o), and the home folder written in another case (c:\users\…) did too.
    #[test]
    fn names_with_accents_and_other_cases_are_scrubbed() {
        let log = "load file:///C:/Users/Ni%C3%B1o%20Cruz/Music/a.flac\nopen c:\\users\\niño cruz\\Music\\b.flac\nuser Niño Cruz here\nlower ni%c3%b1o%20cruz\nnamed O'Neil: file:///C:/Users/Jos%C3%A9%20O'Neil/x\n";
        let out = scrub(log, "C:\\Users\\Niño Cruz", "Niño Cruz");
        assert!(!out.to_lowercase().contains("ni%c3%b1o") && !out.to_lowercase().contains("niño"), "{out}");
        assert!(out.contains("file:///~/Music/a.flac") && out.contains("open ~\\Music\\b.flac"), "{out}");
        let out = scrub(log, "C:\\Users\\José O'Neil", "José O'Neil");
        assert!(out.contains("file:///~/x") && !out.contains("Jos%C3%A9"), "{out}");
    }

    /// The beta's report: its own version, and its own log (beta-crash.log) after the engine's, scrubbed.
    #[test]
    fn the_beta_report_carries_its_version_and_its_log() {
        let dir = std::env::temp_dir().join(format!("rm-report-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let crash: PathBuf = dir.join("beta-crash.log");
        std::fs::write(&crash, "2026-11-07 shot: C:\\Users\\Bassi Saba\\x\nmail me at a@b.co\n").unwrap();
        let r = gather_with("0.8.0-beta.4", "engine line\n", Some(&crash), "C:\\Users\\Bassi Saba", "Bassi Saba");
        assert_eq!(r.version, "0.8.0-beta.4");
        assert!(r.log.starts_with("engine line\n"), "{}", r.log);
        assert!(r.log.contains(BETA_LOG_HEAD) && r.log.contains("~\\x") && r.log.contains("(email)"), "{}", r.log);
        assert!(!r.log.contains("Bassi"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    use super::*;

    #[test]
    fn names_and_folders_are_taken_out() {
        let log = "2026-11-02 12:27:23 load file:///home/maria%20cruz/Music/Radiance%20Music/song.opus from 0 s\n\
                   saved to /home/maria cruz/Music/Radiance Music/song.opus by maria cruz, write to maria.cruz@example.com\n";
        let s = scrub(log, "/home/maria cruz", "maria cruz");
        assert!(!s.contains("maria"), "{s}");
        assert!(s.contains("file://~/Music/Radiance%20Music/song.opus"), "{s}");
        assert!(s.contains("saved to ~/Music/Radiance Music/song.opus by you"), "{s}");
        assert!(s.contains("write to (email)\n"), "{s}");
    }

    #[test]
    fn short_or_empty_names_are_left_alone() {
        assert_eq!(scrub("a b c", "/", "ab"), "a b c");
        assert_eq!(scrub("ends with @ sign", "", ""), "ends with @ sign");
    }

    #[test]
    fn only_the_newest_whole_lines_go() {
        let text = "one\ntwo\nthree\nfour\n";
        assert_eq!(newest(text, 100), text);
        assert_eq!(newest(text, 9), "four\n");
    }

    #[test]
    fn the_body_is_what_the_admin_takes() {
        let r = Report { platform: "linux", version: "0.7.15".into(), system: "Ubuntu 26.04 LTS, GNOME".into(), install: "00000000-0000-4000-8000-000000000000".into(), log: "x\n".into() };
        let empty = r.body("   ");
        assert!(empty.get("note").is_none());
        for k in ["platform", "version", "system", "log", "install"] {
            assert!(empty.get(k).is_some(), "{k}");
        }
        let long = r.body(&"a".repeat(3000));
        assert_eq!(long["note"].as_str().unwrap().chars().count(), MOST_NOTE);
        // the admin needs a log and a short system
        let bare = Report { log: " \n".into(), system: "é".repeat(600), ..r };
        assert_eq!(bare.body("")["log"], NO_LOG);
        assert_eq!(bare.body("")["system"].as_str().unwrap().chars().count(), MOST_SYSTEM);
    }
}
