//! Rule 2 (owner, 2026-11-09, "applies to all platforms or os"): a song split on this computer goes up
//! to the servers it joined, with its parts, by itself; offline or away from home, it waits here and
//! goes up when the server answers again. Only joined devices ("pag ka ininvite ko nalang sila sa
//! server"). As the phone's ServerUploads: silent, logs only.
//!
//! The queue is kept in `radiance-music/server-uploads.json` (the config folder):
//! `{"waiting":[{"key","server"}],"done":["<server>|<key>"],"aside":["<server>|<key>"]}`, the server
//! by the address it was first joined at. One worker drains it at a time.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, RwLock};

use serde_json::{json, Value};

use crate::servers::{self, Failure, Joined};
use crate::track::{Source, Track};

/// The queue as kept.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Queue {
    /// (the song's key, the server's first address), in turn.
    pub waiting: Vec<(String, String)>,
    /// "<server>|<key>": went up, or the server had it split already.
    pub done: Vec<String>,
    /// "<server>|<key>": never going (the server said no, the file is gone, a type servers don't take).
    pub aside: Vec<String>,
}

impl Queue {
    fn to_json(&self) -> Value {
        json!({
            "waiting": self.waiting.iter().map(|(k, s)| json!({ "key": k, "server": s })).collect::<Vec<_>>(),
            "done": self.done,
            "aside": self.aside,
        })
    }

    fn from_json(v: &Value) -> Queue {
        let list = |k: &str| v[k].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
        Queue {
            waiting: v["waiting"].as_array().map(|a| a.iter().filter_map(|w| Some((w["key"].as_str()?.to_string(), w["server"].as_str()?.to_string()))).collect()).unwrap_or_default(),
            done: list("done"),
            aside: list("aside"),
        }
    }

    fn settled(&self, mark: &str) -> bool {
        self.done.iter().any(|d| d == mark) || self.aside.iter().any(|a| a == mark)
    }
}

static FILE: RwLock<Option<PathBuf>> = RwLock::new(None);
static QUEUE: Mutex<Option<Queue>> = Mutex::new(None);
/// The saved songs by key (the queue keeps keys; their files are found here).
static SONGS: Mutex<Option<HashMap<String, Track>>> = Mutex::new(None);
static RUNNING: AtomicBool = AtomicBool::new(false);
static AGAIN: AtomicBool = AtomicBool::new(false);

fn file() -> PathBuf {
    FILE.read().ok().and_then(|f| f.clone()).unwrap_or_else(|| glib::user_config_dir().join("radiance-music/server-uploads.json"))
}

/// Where the queue is kept (tests: their own; None: the config folder's). The queue is read again from there.
pub fn set_file(path: Option<PathBuf>) {
    if let Ok(mut f) = FILE.write() {
        *f = path;
    }
    if let Ok(mut q) = QUEUE.lock() {
        *q = None;
    }
}

fn with_queue<R>(f: impl FnOnce(&mut Queue) -> R) -> R {
    let mut q = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    let q = q.get_or_insert_with(|| std::fs::read_to_string(file()).ok().and_then(|t| serde_json::from_str(&t).ok()).map(|v| Queue::from_json(&v)).unwrap_or_default());
    f(q)
}

fn store(q: &Queue) {
    let path = file();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let part = path.with_extension("json.part");
    if std::fs::write(&part, q.to_json().to_string()).is_ok() {
        let _ = std::fs::rename(&part, &path);
    }
}

/// The queue as it is now.
pub fn state() -> Queue {
    with_queue(|q| q.clone())
}

fn mark(server: &str, key: &str) -> String {
    format!("{server}|{key}")
}

fn saved(t: &Track) -> bool {
    t.source != Source::Server && t.file.as_deref().is_some_and(|f| Path::new(f).exists())
}

/// The liked songs as they are now (their files): a song waiting from before is found among them.
pub fn set_songs(liked: &[Track]) {
    let mut s = SONGS.lock().unwrap_or_else(|e| e.into_inner());
    let songs = s.get_or_insert_with(HashMap::new);
    for t in liked.iter().filter(|t| saved(t)) {
        songs.insert(t.key(), t.clone());
    }
}

/// A song split here goes up to every server joined (a saved song; a server's own song never).
pub fn enqueue(t: &Track) {
    if !saved(t) {
        return;
    }
    set_songs(std::slice::from_ref(t));
    let key = t.key();
    let servers = servers::joined();
    if servers.is_empty() {
        return;
    }
    with_queue(|q| {
        let mut changed = false;
        for s in &servers {
            if !q.settled(&mark(&s.first, &key)) && !q.waiting.iter().any(|(k, f)| *k == key && *f == s.first) {
                q.waiting.push((key.clone(), s.first.clone()));
                changed = true;
            }
        }
        if changed {
            store(q);
        }
    });
}

/// The liked songs split here before (all four parts here) go up too, once.
pub fn backfill(liked: &[Track]) {
    for t in liked.iter().filter(|t| saved(t) && crate::split::has_parts(t)) {
        enqueue(t);
    }
}

/// The queue drained on a worker thread of its own (one at a time; asked again while it runs: once more after).
pub fn kick() {
    if RUNNING.swap(true, Ordering::SeqCst) {
        AGAIN.store(true, Ordering::SeqCst);
        return;
    }
    let started = std::thread::Builder::new().name("server-uploads".into()).spawn(|| loop {
        AGAIN.store(false, Ordering::SeqCst);
        drain();
        RUNNING.store(false, Ordering::SeqCst);
        if !AGAIN.load(Ordering::SeqCst) || RUNNING.swap(true, Ordering::SeqCst) {
            break;
        }
    });
    if started.is_err() {
        RUNNING.store(false, Ordering::SeqCst);
    }
}

/// At the app's start: the liked songs known; what waited goes up after 20 s, the liked songs split
/// here before after 30 s, and the queue is gone through every 10 minutes (the phone's times). Once.
pub fn start(liked: &[Track]) {
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    set_songs(liked);
    let liked = liked.to_vec();
    let _ = std::thread::Builder::new().name("server-uploads-clock".into()).spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(20));
        kick();
        std::thread::sleep(std::time::Duration::from_secs(10));
        backfill(&liked);
        kick();
        loop {
            std::thread::sleep(std::time::Duration::from_secs(600));
            kick();
        }
    });
}

/// A server just joined: the liked songs split here go up to it too (in 30 s, as at the start).
pub fn joined_now(liked: &[Track]) {
    let liked = liked.to_vec();
    let _ = std::thread::Builder::new().name("server-uploads-join".into()).spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(30));
        set_songs(&liked);
        backfill(&liked);
        kick();
    });
}

/// What sending one song to one server came to.
#[derive(Clone, Debug, PartialEq)]
pub enum Sent {
    /// It went up, or the server had it split already.
    Done,
    /// Not now (offline, away, busy): the next time.
    Later,
    /// Never (the server said no, the file is gone, a type servers don't take).
    Aside(String),
}

fn later_or_aside(f: Failure) -> Sent {
    if f.status == 0 || f.status == 408 || f.status == 429 || f.status >= 500 {
        Sent::Later
    } else {
        Sent::Aside(f.said)
    }
}

/// One song to one server: not there → the song (with `parts=coming` when its four parts are here),
/// then its parts; there, not split → its four parts if here, else the server asked to split it;
/// there and split → nothing. The two halves (music + vocals) never go.
pub fn send(server: &Joined, t: &Track) -> Sent {
    let Some(file) = t.file.as_deref().filter(|f| Path::new(f).exists()) else { return Sent::Aside("the song's file is gone".into()) };
    let ext = Path::new(file).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    if !["opus", "flac", "m4a"].contains(&ext.as_str()) {
        return Sent::Aside(format!("servers don't take .{ext} files"));
    }
    let four = crate::split::PARTS.iter().all(|p| crate::split::ready_part(t, p).is_some());
    let there = match servers::find_on(server, t) {
        Ok(s) => s,
        Err(f) => return later_or_aside(f),
    };
    let song = match there {
        Some(s) if s.split => return Sent::Done,
        Some(s) => s,
        None => match servers::upload_with(server, t, four) {
            Ok(added) => match added.song {
                Some(s) if s.split => return Sent::Done,
                // no parts from here: the server splits it (rms 0.2.2+ does by itself; an older one is asked)
                Some(s) if !four => {
                    let _ = servers::split_by_id(server, s.id);
                    return Sent::Done;
                }
                Some(s) => s,
                None => return Sent::Later,
            },
            Err(f) => return later_or_aside(f),
        },
    };
    let done = if four { servers::upload_parts_with(server, song.id, t).map(|_| ()) } else { servers::split_by_id(server, song.id).map(|_| ()) };
    match done {
        Ok(()) => Sent::Done,
        Err(f) => later_or_aside(f),
    }
}

/// Everything waiting, in turn: a server that isn't answering is left for the next time.
pub fn drain() {
    let waiting = with_queue(|q| q.waiting.clone());
    let joined = servers::joined();
    let mut not_now: Vec<String> = vec![];
    for (key, first) in waiting {
        if not_now.contains(&first) {
            continue;
        }
        let server = joined.iter().find(|s| s.first == first).cloned();
        let song = SONGS.lock().ok().and_then(|s| s.as_ref()?.get(&key).cloned());
        let outcome = match (&server, &song) {
            (None, _) => Sent::Aside("that server isn't joined anymore".into()),
            (_, None) => continue, // its song isn't known yet (the liked songs come at the start)
            (Some(s), Some(t)) => send(s, t),
        };
        let name = server.as_ref().map(|s| s.name.clone()).unwrap_or(first.clone());
        let title = song.as_ref().map(|t| t.title.clone()).unwrap_or(key.clone());
        match &outcome {
            Sent::Done => crate::log::note(&format!("“{title}” is on {name} with its parts (rule 2)")),
            Sent::Later => {
                crate::log::note(&format!("“{title}” waits for {name} (not answering): it goes up later"));
                not_now.push(first.clone());
                continue;
            }
            Sent::Aside(why) => crate::log::note(&format!("“{title}” won't go up to {name}: {why}")),
        }
        with_queue(|q| {
            q.waiting.retain(|(k, f)| !(*k == key && *f == first));
            let m = mark(&first, &key);
            match outcome {
                Sent::Done => q.done.push(m),
                Sent::Aside(_) => q.aside.push(m),
                Sent::Later => {}
            }
            store(q);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::servers::tests::{copy_json, saved as saved_in, scratch, stand_in, JOINED_TESTS};
    use crate::servers::set_joined;

    fn with_parts(dir: &Path, id: &str, parts: &[&str]) -> Track {
        let t = saved_in(dir, id);
        for p in parts {
            std::fs::write(dir.join(format!("Maki - Dilaw.{p}.ogg")), b"PART").unwrap();
        }
        t
    }

    /// Not on the server, split here in four: the song goes with parts=coming, then its parts.
    #[test]
    fn a_song_split_here_goes_up_with_its_parts() {
        let (address, seen) = stand_in(3, |method, path, _, _| match (method, path) {
            (_, p) if p.starts_with("/api/library?") => (200, r#"{"songs":[]}"#.into()),
            ("POST", "/api/songs?parts=coming") => (201, json!({ "song": copy_json(8, false) }).to_string()),
            ("POST", "/api/songs/8/parts") => (200, json!({ "added": ["drums", "bass", "other", "vocals"], "song": copy_json(8, true) }).to_string()),
            _ => (404, "{}".into()),
        });
        let dir = scratch("up-four");
        let t = with_parts(&dir, "yt-up-four", &crate::split::PARTS);
        assert_eq!(send(&Joined::new(&address, "t", "S", None), &t), Sent::Done);
        let seen = seen.join().unwrap();
        assert_eq!(seen.iter().map(|s| s.0.split('?').next().unwrap()).collect::<Vec<_>>(), ["GET /api/library", "POST /api/songs", "POST /api/songs/8/parts"]);
        assert!(seen[1].0.ends_with("?parts=coming"));
        assert!(seen[1].2.contains("name=\"song\"") && seen[2].2.contains("name=\"vocals\""));
    }

    /// Split here in two only: the song goes (the server splits it itself), its halves never do.
    #[test]
    fn the_two_halves_never_go() {
        let (address, seen) = stand_in(3, |method, path, _, _| match (method, path) {
            (_, p) if p.starts_with("/api/library?") => (200, r#"{"songs":[]}"#.into()),
            ("POST", "/api/songs") => (201, json!({ "song": copy_json(9, false), "split_job": { "id": 1 } }).to_string()),
            ("POST", "/api/split") => (200, r#"{"queued":false,"job":{"id":1,"song":{"id":9},"state":"waiting"}}"#.into()),
            _ => (404, "{}".into()),
        });
        let dir = scratch("up-two");
        let t = with_parts(&dir, "yt-up-two", &["music", "vocals"]);
        assert_eq!(send(&Joined::new(&address, "t", "S", None), &t), Sent::Done);
        let seen = seen.join().unwrap();
        assert_eq!(seen[1].0, "POST /api/songs");
        assert!(!seen[1].2.contains("name=\"music\""));
        assert_eq!(seen[2].0, "POST /api/split");
    }

    /// On the server, not split: its four parts go when they're here, else the server is asked to split it;
    /// on the server and split: nothing goes.
    #[test]
    fn a_copy_there_gets_its_parts_or_a_split() {
        let (address, seen) = stand_in(4, |method, path, _, _| match (method, path) {
            (_, p) if p.starts_with("/api/library?q=Dilaw") => (200, json!({ "songs": [copy_json(5, false)] }).to_string()),
            ("POST", "/api/songs/5/parts") => (200, json!({ "added": ["drums"], "song": copy_json(5, true) }).to_string()),
            ("POST", "/api/split") => (200, r#"{"queued":true,"job":{"id":2,"song":{"id":5},"state":"waiting"}}"#.into()),
            _ => (404, "{}".into()),
        });
        let s = Joined::new(&address, "t", "S", None);
        let (four, none) = (scratch("there-four"), scratch("there-none"));
        assert_eq!(send(&s, &with_parts(&four, "yt-there-four", &crate::split::PARTS)), Sent::Done);
        assert_eq!(send(&s, &with_parts(&none, "yt-there-none", &[])), Sent::Done);
        let seen = seen.join().unwrap();
        assert_eq!(seen.iter().map(|s| s.0.split('?').next().unwrap()).collect::<Vec<_>>(), ["GET /api/library", "POST /api/songs/5/parts", "GET /api/library", "POST /api/split"]);
        let (address, seen) = stand_in(1, |_, _, _, _| (200, json!({ "songs": [copy_json(6, true)] }).to_string()));
        assert_eq!(send(&Joined::new(&address, "t", "S", None), &with_parts(&scratch("there-split"), "yt-there-split", &crate::split::PARTS)), Sent::Done);
        assert_eq!(seen.join().unwrap().len(), 1);
    }

    /// Busy or away: later. Said no (a friend who can't add songs), the file gone, an .ogg: never.
    #[test]
    fn later_or_never() {
        let (busy, _b) = stand_in(1, |_, _, _, _| (503, r#"{"error":"busy"}"#.into()));
        let (no, _n) = stand_in(2, |method, _, _, _| if method == "GET" { (200, r#"{"songs":[]}"#.into()) } else { (403, r#"{"error":"adding songs is off"}"#.into()) });
        let dir = scratch("later-never");
        let t = with_parts(&dir, "yt-later", &crate::split::PARTS);
        assert_eq!(send(&Joined::new(&busy, "t", "S", None), &t), Sent::Later);
        assert_eq!(send(&Joined::new(&no, "t", "S", None), &t), Sent::Aside("adding songs is off".into()));
        assert_eq!(send(&Joined::new("http://127.0.0.1:9", "t", "S", None), &t), Sent::Later, "not answering");
        let mut gone = t.clone();
        gone.file = Some(dir.join("nope.opus").display().to_string());
        assert!(matches!(send(&Joined::new(&busy, "t", "S", None), &gone), Sent::Aside(_)));
        let ogg = dir.join("Maki - Dilaw.ogg");
        std::fs::write(&ogg, b"OGG").unwrap();
        let mut odd = t.clone();
        odd.file = Some(ogg.display().to_string());
        assert!(matches!(send(&Joined::new(&busy, "t", "S", None), &odd), Sent::Aside(_)));
    }

    /// The queue: a split song waits per server joined, is kept across a restart, goes once, and a
    /// server that isn't answering keeps it waiting.
    #[test]
    fn the_queue_keeps_what_waits() {
        let _turn = JOINED_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let dir = scratch("queue");
        set_file(Some(dir.join("server-uploads.json")));
        let (up, seen) = stand_in(1, |_, _, _, _| (200, json!({ "songs": [copy_json(6, true)] }).to_string()));
        let away = "http://127.0.0.1:9".to_string();
        set_joined(&[Joined::new(&up, "t", "Up", None), Joined::new(&away, "t", "Away", None)]);
        let t = with_parts(&dir, "yt-queue", &crate::split::PARTS);
        enqueue(&t);
        enqueue(&t); // twice: once
        assert_eq!(state().waiting, vec![(t.key(), up.clone()), (t.key(), away.clone())]);
        set_file(Some(dir.join("server-uploads.json"))); // read again from the file: as after a restart
        assert_eq!(state().waiting.len(), 2);
        drain();
        set_joined(&[]);
        let q = state();
        set_file(None);
        assert_eq!(q.waiting, vec![(t.key(), away)], "the server away keeps it waiting");
        assert_eq!(q.done, vec![format!("{up}|{}", t.key())]);
        assert_eq!(seen.join().unwrap().len(), 1);
    }
}
