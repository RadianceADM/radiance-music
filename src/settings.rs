//! What the app remembers between runs (~/.config/radiance-music/settings.json):
//! how one song goes into the next.

use std::fs;
use std::path::PathBuf;

use gtk::glib;
use serde_json::{json, Value};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Transition {
    /// One song ends, the next starts.
    Off,
    /// The end of one fades into the start of the next.
    Crossfade,
    /// Beats lined up, tempo matched, the bass swapped halfway (a smooth crossfade
    /// when the two songs' tempos are too far apart).
    DjMix,
}

impl Transition {
    pub fn id(self) -> &'static str {
        match self {
            Transition::Off => "off",
            Transition::Crossfade => "crossfade",
            Transition::DjMix => "dj",
        }
    }

    pub fn from_id(id: &str) -> Transition {
        match id {
            "crossfade" => Transition::Crossfade,
            "dj" => Transition::DjMix,
            _ => Transition::Off,
        }
    }
}

/// Where songs are split into vocals and music (over SSH; see server/split.py).
#[derive(Clone, Debug)]
pub struct SplitServer {
    /// An SSH host (an alias from ~/.ssh/config works).
    pub host: String,
    /// The account the split runs as there.
    pub user: String,
    pub python: String,
    pub script: String,
    /// A folder there for the jobs' temporary files.
    pub work: String,
    /// A folder there that keeps every finished split (parts, lyrics, details),
    /// one folder per song, ready to be hosted.
    pub library: String,
}

/// This computer's own splitter: it takes part of each song (server/split.py
/// in a Python with Demucs), using `threads` threads.
#[derive(Clone, Debug)]
pub struct LocalSplitter {
    pub python: String,
    pub script: String,
    pub threads: u32,
}

#[derive(Clone, Debug)]
pub struct Settings {
    pub transition: Transition,
    /// Crossfade length, seconds.
    pub fade: f64,
    pub split: Option<SplitServer>,
    pub local_split: Option<LocalSplitter>,
    /// The app's volume, 0-1 (1 = as loud as the system plays it).
    pub volume: f64,
    /// A browser whose YouTube sign-in yt-dlp may use ("chrome", "firefox"…).
    pub youtube_sign_in: Option<String>,
    /// The dark look: "stone" or "black".
    pub look: String,
    /// Where liked songs are kept (None: ~/Music/Radiance Music).
    pub music_dir: Option<String>,
    /// When the queue's last song plays, songs like it are added after it.
    pub autoplay: bool,
    /// The firewall lets a phone in for Share Songs with a Phone (done once).
    pub share_firewall_open: bool,
    /// How to Use It was shown (the first time the app opened).
    pub guide_seen: bool,
}

pub const FADE_RANGE: (f64, f64) = (1.0, 12.0);

fn path() -> PathBuf {
    glib::user_config_dir().join("radiance-music/settings.json")
}

pub fn load() -> Settings {
    let v: Value = fs::read_to_string(path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
    let s = &v["split"];
    let text = |k: &str| s[k].as_str().map(String::from);
    Settings {
        transition: Transition::from_id(v["transition"].as_str().unwrap_or("off")),
        fade: v["fade"].as_f64().unwrap_or(6.0).clamp(FADE_RANGE.0, FADE_RANGE.1),
        volume: v["volume"].as_f64().unwrap_or(1.0).clamp(0.0, 1.0),
        local_split: (|| {
            let l = &v["local_split"];
            let threads = l["threads"].as_u64().map(|n| n.clamp(1, 256) as u32).unwrap_or_else(crate::setup::half_the_cores);
            let path = |k: &str| l[k].as_str().map(|p| p.replacen('~', &glib::home_dir().display().to_string(), 1));
            match (path("python"), path("script")) {
                (Some(python), Some(script)) if std::path::Path::new(&python).exists() && std::path::Path::new(&script).exists() => Some(LocalSplitter { python, script, threads }),
                // set up in the app's own folder (Splitting…), even if the settings forgot it
                _ => crate::setup::installed(threads),
            }
        })(),
        youtube_sign_in: v["youtube_sign_in"].as_str().filter(|b| !b.is_empty()).map(String::from),
        music_dir: v["music_dir"].as_str().filter(|d| !d.is_empty()).map(String::from),
        look: if v["look"].as_str() == Some("black") { "black".into() } else { "stone".into() },
        autoplay: v["autoplay"].as_bool().unwrap_or(true),
        share_firewall_open: v["share_firewall_open"].as_bool().unwrap_or(false),
        guide_seen: v["guide_seen"].as_bool().unwrap_or(false),
        split: (|| {
            Some(SplitServer {
                host: text("host").filter(|h| !h.is_empty())?,
                user: text("user").unwrap_or_else(|| "root".into()),
                python: text("python")?,
                script: text("script")?,
                work: text("work").unwrap_or_else(|| "/tmp/radiance-split".into()),
                library: text("library").unwrap_or_else(|| "/srv/data/radiance-library".into()),
            })
        })(),
    }
}

pub fn save(s: &Settings) {
    let p = path();
    if let Some(dir) = p.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let mut v = json!({ "transition": s.transition.id(), "fade": s.fade, "volume": s.volume, "look": s.look, "autoplay": s.autoplay, "share_firewall_open": s.share_firewall_open, "guide_seen": s.guide_seen });
    if let Some(l) = &s.local_split {
        v["local_split"] = json!({ "python": l.python, "script": l.script, "threads": l.threads });
    }
    if let Some(b) = &s.youtube_sign_in {
        v["youtube_sign_in"] = json!(b);
    }
    if let Some(sp) = &s.split {
        v["split"] = json!({ "host": sp.host, "user": sp.user, "python": sp.python, "script": sp.script, "work": sp.work, "library": sp.library });
    }
    if let Some(d) = &s.music_dir {
        v["music_dir"] = json!(d);
    }
    let _ = fs::write(&p, serde_json::to_string_pretty(&v).unwrap_or_default());
}
