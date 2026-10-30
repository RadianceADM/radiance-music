//! Songs to and from a phone by QR code (Radiance Music for Android:
//! Library → Share songs → Scan a QR), straight over the local network: no
//! server in between.
//!
//! The computer shows the QR and listens; the phone scans it and connects.
//! "get": the phone takes this computer's liked songs. "put": the phone
//! sends its songs here. Either way the receiving side says first what it has
//! (songs saved, parts split, lyrics, covers, videos) and only what's missing
//! goes: nothing is sent twice. The same protocol as the phone's (Wire.kt).
//!
//! Wi-Fi and Bluetooth share one radio on many laptops: while music plays to
//! Bluetooth and this computer is on Wi-Fi, songs move slowly (256 KB/s) so the
//! music never stutters; on a cable, at full speed.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::track::{file_name, Track};
use crate::{library, split};

pub const VERSION: u32 = 1;
/// What a song can come with.
pub const KINDS: [&str; 8] = ["song", "drums", "bass", "other", "vocals", "lyrics", "cover", "video"];
/// The ports it listens on (the first free one): a firewall lets these in for the phone.
pub const PORTS: std::ops::RangeInclusive<u16> = 47800..=47809;
/// Bigger than any song or video would be: refused.
const MOST_BYTES: u64 = 4 << 30;
/// While Bluetooth music plays and this computer is on Wi-Fi.
const GENTLE: u64 = 256 * 1024;

/// What the share window hears.
pub enum Event {
    /// The phone asked for something: bytes done of all (0: not known), songs done of all, the song now.
    Progress { bytes: u64, total: u64, songs: usize, of: usize, now: Option<String> },
    /// A song from the phone is all here (its file saved): it joins Liked Songs.
    Received(Track),
    /// Moving slowly for Bluetooth's sake, or not.
    Gentle(bool),
}

struct File {
    name: String,
    size: u64,
    path: PathBuf,
}

struct Item {
    key: String,
    track: Value,
    files: Vec<(String, File)>,
}

impl Item {
    fn json(&self) -> Value {
        let files: serde_json::Map<String, Value> = self.files.iter().map(|(k, f)| (k.clone(), json!({ "name": f.name, "size": f.size }))).collect();
        json!({ "key": self.key, "track": self.track, "files": files })
    }
}

/// A song's details as the phone reads them (not where it sits here).
fn track_json(t: &Track) -> Value {
    let v = t.to_json();
    json!({
        "source": v["source"], "id": v["id"], "title": v["title"], "artist": v["artist"],
        "album": v["album"], "seconds": v["seconds"], "art": v["art"], "format": v["format"],
    })
}

fn file_at(path: PathBuf) -> Option<File> {
    let size = fs::metadata(&path).ok()?.len();
    let name = path.file_name()?.to_string_lossy().into_owned();
    Some(File { name, size, path })
}

/// [songs] as offered: each saved song with its parts, lyrics, cover (and video).
fn offer(songs: &[Track], videos: bool) -> Vec<Item> {
    songs
        .iter()
        .filter_map(|t| {
            let song = t.file.as_ref()?;
            let mut files = vec![("song".to_string(), file_at(PathBuf::from(song))?)];
            for p in split::PARTS {
                if let Some(f) = split::ready_part(t, p).and_then(file_at) {
                    files.push((p.to_string(), f));
                }
            }
            if let Some(f) = file_at(library::beside(song, "lrc")) {
                files.push(("lyrics".into(), f));
            }
            if let Some(f) = file_at(library::beside(song, "jpg")) {
                files.push(("cover".into(), f));
            }
            if videos {
                if let Some(f) = ["video.mkv", "video.webm", "video.mp4"].iter().find_map(|e| file_at(library::beside(song, e))) {
                    files.push(("video".into(), f));
                }
            }
            Some(Item { key: t.key(), track: track_json(t), files })
        })
        .collect()
}

/// This computer's address on the local network (the way out it uses), if any.
pub fn address() -> Option<IpAddr> {
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("8.8.8.8:53").ok()?; // no packet goes: it only picks the way
    Some(s.local_addr().ok()?.ip())
}

/// Bluetooth music playing while this computer's way out is Wi-Fi (they share a radio).
fn bluetooth_busy() -> bool {
    let wifi = fs::read_to_string("/proc/net/route").ok().and_then(|r| {
        r.lines().skip(1).find(|l| l.split_whitespace().nth(1) == Some("00000000")).and_then(|l| l.split_whitespace().next().map(String::from))
    });
    let Some(iface) = wifi else { return false };
    if !Path::new(&format!("/sys/class/net/{iface}/wireless")).exists() {
        return false;
    }
    let Ok(out) = std::process::Command::new("pw-dump").output() else { return false };
    let Ok(v) = serde_json::from_slice::<Value>(&out.stdout) else { return false };
    v.as_array().into_iter().flatten().any(|o| {
        let p = &o["info"]["props"];
        p["media.class"] == "Audio/Sink" && p["node.name"].as_str().is_some_and(|n| n.starts_with("bluez_output")) && o["info"]["state"] == "running"
    })
}

/// How fast it may go now: checked every few seconds.
struct Pace {
    checked: Instant,
    busy: bool,
    events: async_channel::Sender<Event>,
}

impl Pace {
    fn limit(&mut self) -> u64 {
        if self.checked.elapsed() > Duration::from_secs(3) {
            let busy = bluetooth_busy();
            if busy != self.busy {
                let _ = self.events.send_blocking(Event::Gentle(busy));
            }
            self.busy = busy;
            self.checked = Instant::now();
        }
        if self.busy { GENTLE } else { 0 }
    }
}

fn copy(from: &mut impl Read, to: &mut impl Write, size: u64, pace: &Mutex<Pace>, moved: &mut dyn FnMut(u64)) -> std::io::Result<()> {
    let mut buf = vec![0u8; 64 * 1024];
    let mut left = size;
    let (mut start, mut window) = (Instant::now(), 0u64);
    while left > 0 {
        let want = left.min(buf.len() as u64) as usize;
        let n = from.read(&mut buf[..want])?;
        if n == 0 {
            return Err(std::io::Error::other("it stopped coming"));
        }
        to.write_all(&buf[..n])?;
        left -= n as u64;
        moved(n as u64);
        let limit = pace.lock().map(|mut p| p.limit()).unwrap_or(0);
        if limit > 0 {
            window += n as u64;
            let should = Duration::from_secs_f64(window as f64 / limit as f64);
            let took = start.elapsed();
            if should > took {
                std::thread::sleep(should - took);
            }
            if took > Duration::from_secs(1) {
                start = Instant::now();
                window = 0;
            }
        }
    }
    Ok(())
}

fn new_key() -> String {
    let mut b = [0u8; 16];
    if fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b)).is_err() {
        // never happens on Linux; still a key nobody guesses easily
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
        b.copy_from_slice(&t.to_le_bytes());
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

fn dec(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = if b[i] == b'%' { b.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u8::from_str_radix(h, 16).ok()) } else { None };
        match (b[i], hex) {
            (b'%', Some(v)) => {
                out.push(v);
                i += 2;
            }
            (b'+', _) => out.push(b' '),
            (c, _) => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn query(q: &str) -> HashMap<String, String> {
    q.split('&').filter_map(|p| p.split_once('=')).map(|(k, v)| (dec(k), dec(v))).collect()
}

/// Songs coming in: beside the others in the Music folder (parts, lyrics, cover, video too).
struct Store {
    /// What each song coming is called here (its file's name without the ending).
    bases: HashMap<String, PathBuf>,
    /// The song file each one came as (or had).
    files: HashMap<String, String>,
}

impl Store {
    /// The same song liked here: the same key, or the same title by the same artist.
    fn mine(item_key: &str, t: &Track) -> Option<Track> {
        let liked = library::load();
        liked.iter().find(|x| x.key() == item_key).or_else(|| liked.iter().find(|x| x.same_song(t))).cloned()
    }

    fn have(&mut self, key: &str, track: &Value, kinds: &HashMap<String, (String, u64)>) -> HashSet<String> {
        let mut here = HashSet::new();
        let Some(t) = Track::from_json(track) else { return here };
        let mine = Self::mine(key, &t);
        let base = match mine.as_ref().and_then(|m| m.file.clone()) {
            Some(f) if Path::new(&f).exists() => {
                here.insert("song".to_string());
                self.files.insert(key.into(), f.clone());
                Path::new(&f).with_extension("")
            }
            _ => {
                let name = kinds.get("song").map(|(n, _)| n.clone()).unwrap_or_default();
                let stem = Path::new(&name).file_stem().map(|s| s.to_string_lossy().into_owned()).filter(|s| !s.is_empty());
                library::music_dir().join(file_name(&stem.unwrap_or_else(|| format!("{} - {}", t.artist, t.title))))
            }
        };
        let beside = |ext: &str| PathBuf::from(format!("{}.{ext}", base.display()));
        for p in split::PARTS {
            if ["flac", "ogg", "m4a"].iter().any(|e| beside(&format!("{p}.{e}")).exists()) {
                here.insert(p.to_string());
            }
        }
        if beside("lrc").exists() || beside("lyrics.yaml").exists() {
            here.insert("lyrics".into());
        }
        if beside("jpg").exists() {
            here.insert("cover".into());
        }
        if ["video.mkv", "video.webm", "video.mp4"].iter().any(|e| beside(e).exists()) {
            here.insert("video".into());
        }
        self.bases.insert(key.into(), base);
        here
    }

    fn write(&mut self, key: &str, kind: &str, name: &str, body: &mut impl Read, size: u64, pace: &Mutex<Pace>, moved: &mut dyn FnMut(u64)) -> Result<(), String> {
        let base = self.bases.get(key).ok_or("not asked for")?.clone();
        let ext: String = Path::new(name).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default().chars().filter(|c| c.is_ascii_alphanumeric()).take(5).collect();
        let ext = if ext.is_empty() { "m4a".to_string() } else { ext };
        let to = match kind {
            "song" => PathBuf::from(format!("{}.{ext}", base.display())),
            "lyrics" => PathBuf::from(format!("{}.lrc", base.display())),
            "cover" => PathBuf::from(format!("{}.jpg", base.display())),
            "video" => PathBuf::from(format!("{}.video.{ext}", base.display())),
            part if split::PARTS.contains(&part) => PathBuf::from(format!("{}.{part}.{ext}", base.display())),
            _ => return Err("no such kind".into()),
        };
        if let Some(d) = to.parent() {
            fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        // written whole, then put in place (a stopped transfer leaves nothing half-there)
        let part = PathBuf::from(format!("{}.part", to.display()));
        let mut f = fs::File::create(&part).map_err(|e| e.to_string())?;
        copy(body, &mut f, size, pace, moved).map_err(|e| e.to_string())?;
        drop(f);
        fs::rename(&part, &to).map_err(|e| e.to_string())?;
        if kind == "song" {
            self.files.insert(key.into(), to.to_string_lossy().into_owned());
        }
        Ok(())
    }

    /// All of a song is here: the song as Liked Songs will hold it.
    fn done(&mut self, key: &str, track: &Value) -> Option<Track> {
        let mut t = Track::from_json(track)?;
        let file = self.files.get(key).cloned()?;
        if let Some(mine) = Self::mine(key, &t) {
            t = mine;
        }
        t.file = Some(file);
        Some(t)
    }
}

/// A share being shown: its QR's words, and how to stop it.
pub struct Share {
    pub port: u16,
    pub key: String,
    pub mode: &'static str,
    stop: Arc<AtomicBool>,
}

impl Share {
    /// Shows [songs] (with their videos if [videos]) for a phone to take.
    pub fn send(songs: Vec<Track>, videos: bool, events: async_channel::Sender<Event>) -> Result<Share, String> {
        Self::open("get", Some(Arc::new(offer(&songs, videos))), events)
    }

    /// Lets a phone send its songs here.
    pub fn receive(events: async_channel::Sender<Event>) -> Result<Share, String> {
        Self::open("put", None, events)
    }

    fn open(mode: &'static str, items: Option<Arc<Vec<Item>>>, events: async_channel::Sender<Event>) -> Result<Share, String> {
        let listener = PORTS.clone().find_map(|p| TcpListener::bind(("0.0.0.0", p)).ok()).ok_or("the ports for sharing (47800 to 47809) are all taken")?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let key = new_key();
        let stop = Arc::new(AtomicBool::new(false));
        let pace = Arc::new(Mutex::new(Pace { checked: Instant::now() - Duration::from_secs(10), busy: false, events: events.clone() }));
        let store = Arc::new(Mutex::new(Store { bases: HashMap::new(), files: HashMap::new() }));
        let plan: Arc<Mutex<HashMap<String, Value>>> = Arc::new(Mutex::new(HashMap::new()));
        let counts = Arc::new((AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0))); // bytes, total, songs, of
        let (k, s) = (key.clone(), stop.clone());
        std::thread::Builder::new()
            .name("radiance-share".into())
            .spawn(move || {
                for conn in listener.incoming() {
                    if s.load(Ordering::Relaxed) {
                        break;
                    }
                    let Ok(conn) = conn else { continue };
                    let c = Conn { key: k.clone(), mode, items: items.clone(), store: store.clone(), plan: plan.clone(), pace: pace.clone(), events: events.clone(), counts: counts.clone() };
                    std::thread::spawn(move || {
                        let _ = c.serve(conn);
                    });
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Share { port, key, mode, stop })
    }

    /// The QR's words.
    pub fn ticket(&self, name: &str) -> Option<String> {
        let host = address()?;
        Some(format!("radiancemusic://share?v={VERSION}&m={}&k={}&p={}&n={}&h={host}", self.mode, self.key, self.port, enc(name)))
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect_timeout(&([127, 0, 0, 1], self.port).into(), Duration::from_millis(300)); // wakes it to go
    }
}

impl Drop for Share {
    fn drop(&mut self) {
        self.stop();
    }
}

struct Conn {
    key: String,
    mode: &'static str,
    items: Option<Arc<Vec<Item>>>,
    store: Arc<Mutex<Store>>,
    plan: Arc<Mutex<HashMap<String, Value>>>,
    pace: Arc<Mutex<Pace>>,
    events: async_channel::Sender<Event>,
    counts: Arc<(AtomicU64, AtomicU64, AtomicU64, AtomicU64)>,
}

impl Conn {
    fn progress(&self, now: Option<String>) {
        let c = &self.counts;
        let _ = self.events.send_blocking(Event::Progress {
            bytes: c.0.load(Ordering::Relaxed),
            total: c.1.load(Ordering::Relaxed),
            songs: c.2.load(Ordering::Relaxed) as usize,
            of: c.3.load(Ordering::Relaxed) as usize,
            now,
        });
    }

    fn serve(&self, conn: TcpStream) -> std::io::Result<()> {
        conn.set_read_timeout(Some(Duration::from_secs(30)))?;
        let mut out = conn.try_clone()?;
        let mut input = BufReader::new(conn);
        let mut line = String::new();
        input.read_line(&mut line)?;
        let mut words = line.split_whitespace();
        let (method, target) = (words.next().unwrap_or_default().to_string(), words.next().unwrap_or_default().to_string());
        let mut length = 0u64;
        loop {
            let mut h = String::new();
            if input.read_line(&mut h)? == 0 || h.trim().is_empty() {
                break;
            }
            if h.len() > 8192 {
                return Ok(());
            }
            if let Some((name, value)) = h.split_once(':') {
                if name.trim().eq_ignore_ascii_case("content-length") {
                    length = value.trim().parse().unwrap_or(0);
                }
            }
        }
        let (path, q) = match target.split_once('?') {
            Some((p, q)) => (p.to_string(), query(q)),
            None => (target.clone(), HashMap::new()),
        };
        let given = q.get("k").map(String::as_str).unwrap_or_default();
        // the same length and every byte the same (no early way out to time)
        let ok = given.len() == self.key.len() && given.bytes().zip(self.key.bytes()).fold(0u8, |a, (x, y)| a | (x ^ y)) == 0;
        if !ok {
            return reply(&mut out, 403, "not this share");
        }
        match (self.mode, method.as_str(), path.as_str()) {
            ("get", "GET", "/list") => {
                let songs: Vec<Value> = self.items.iter().flat_map(|i| i.iter()).map(Item::json).collect();
                reply(&mut out, 200, &json!({ "v": VERSION, "songs": songs }).to_string())
            }
            ("get", "GET", "/file") => {
                let items = self.items.as_ref().expect("get has songs");
                let Some(item) = items.iter().find(|i| Some(&i.key) == q.get("key")) else { return reply(&mut out, 404, "no such song") };
                let Some((_, f)) = item.files.iter().find(|(k, _)| Some(k) == q.get("kind")) else { return reply(&mut out, 404, "no such file") };
                let mut src = fs::File::open(&f.path)?;
                write!(out, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n", f.size)?;
                let title = item.track["title"].as_str().map(String::from);
                let mut last = Instant::now();
                copy(&mut src, &mut out, f.size, &self.pace, &mut |n| {
                    self.counts.0.fetch_add(n, Ordering::Relaxed);
                    if last.elapsed() > Duration::from_millis(400) {
                        last = Instant::now();
                        self.progress(title.clone());
                    }
                })?;
                self.progress(title);
                out.flush()
            }
            ("put", "POST", "/plan") => {
                if length == 0 || length > 64 << 20 {
                    return reply(&mut out, 413, "too big");
                }
                let mut body = vec![0u8; length as usize];
                input.read_exact(&mut body)?;
                let v: Value = serde_json::from_slice(&body).unwrap_or_default();
                let mut want = serde_json::Map::new();
                let mut plan = self.plan.lock().unwrap();
                let mut store = self.store.lock().unwrap();
                let mut total = 0u64;
                for item in v["songs"].as_array().into_iter().flatten() {
                    let Some(key) = item["key"].as_str() else { continue };
                    let files: HashMap<String, (String, u64)> = item["files"]
                        .as_object()
                        .into_iter()
                        .flatten()
                        .filter(|(k, _)| KINDS.contains(&k.as_str()))
                        .map(|(k, f)| (k.clone(), (f["name"].as_str().unwrap_or_default().to_string(), f["size"].as_u64().unwrap_or(0))))
                        .filter(|(_, (_, s))| *s <= MOST_BYTES)
                        .collect();
                    let have = store.have(key, &item["track"], &files);
                    let missing: Vec<&String> = files.keys().filter(|k| !have.contains(*k)).collect();
                    if missing.is_empty() {
                        continue;
                    }
                    total += missing.iter().map(|k| files[*k].1).sum::<u64>();
                    want.insert(key.into(), json!(missing));
                    plan.insert(key.into(), item.clone());
                }
                self.counts.1.store(total, Ordering::Relaxed);
                self.counts.3.store(plan.len() as u64, Ordering::Relaxed);
                drop((plan, store));
                self.progress(None);
                reply(&mut out, 200, &json!({ "want": want }).to_string())
            }
            ("put", "PUT", "/file") => {
                let key = q.get("key").cloned().unwrap_or_default();
                let kind = q.get("kind").cloned().unwrap_or_default();
                let Some(item) = self.plan.lock().unwrap().get(&key).cloned() else { return reply(&mut out, 404, "not asked for") };
                let f = &item["files"][&kind];
                if f.is_null() || f["size"].as_u64() != Some(length) || length > MOST_BYTES {
                    return reply(&mut out, 400, "wrong size");
                }
                let title = item["track"]["title"].as_str().map(String::from);
                self.progress(title.clone());
                let mut body = (&mut input).take(length);
                let mut last = Instant::now();
                let written = self.store.lock().unwrap().write(&key, &kind, f["name"].as_str().unwrap_or_default(), &mut body, length, &self.pace, &mut |n| {
                    self.counts.0.fetch_add(n, Ordering::Relaxed);
                    if last.elapsed() > Duration::from_millis(400) {
                        last = Instant::now();
                        self.progress(title.clone());
                    }
                });
                match written {
                    Ok(()) => reply(&mut out, 200, "ok"),
                    Err(e) => reply(&mut out, 500, &e),
                }
            }
            ("put", "POST", "/done") => {
                let key = q.get("key").cloned().unwrap_or_default();
                let Some(item) = self.plan.lock().unwrap().get(&key).cloned() else { return reply(&mut out, 404, "not asked for") };
                if let Some(t) = self.store.lock().unwrap().done(&key, &item["track"]) {
                    let _ = self.events.send_blocking(Event::Received(t));
                }
                self.counts.2.fetch_add(1, Ordering::Relaxed);
                self.progress(None);
                reply(&mut out, 200, "ok")
            }
            _ => reply(&mut out, 404, "no"),
        }
    }
}

fn reply(out: &mut impl Write, code: u16, body: &str) -> std::io::Result<()> {
    let kind = if body.starts_with('{') { "application/json" } else { "text/plain" };
    write!(out, "HTTP/1.1 {code} {}\r\nContent-Length: {}\r\nContent-Type: {kind}; charset=utf-8\r\nConnection: close\r\n\r\n{body}", if code == 200 { "OK" } else { "No" }, body.len())?;
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(port: u16, request: &str, body: &[u8]) -> (u16, Vec<u8>) {
        let mut c = TcpStream::connect(("127.0.0.1", port)).unwrap();
        c.write_all(request.as_bytes()).unwrap();
        c.write_all(body).unwrap();
        let mut all = Vec::new();
        c.read_to_end(&mut all).unwrap();
        let split = all.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        let head = String::from_utf8_lossy(&all[..split]).to_string();
        let code = head.split_whitespace().nth(1).unwrap().parse().unwrap();
        (code, all[split + 4..].to_vec())
    }

    /// The phone's requests (as OkHttp makes them) against the computer's songs.
    #[test]
    fn a_phone_takes_a_song_and_its_parts() {
        let dir = std::env::temp_dir().join(format!("radiance-share-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let song = dir.join("Artist - Song.opus");
        let bytes: Vec<u8> = (0..300_000u32).map(|i| (i * 7 % 251) as u8).collect();
        fs::write(&song, &bytes).unwrap();
        for p in split::PARTS {
            fs::write(dir.join(format!("Artist - Song.{p}.ogg")), p.repeat(1000)).unwrap();
        }
        fs::write(dir.join("Artist - Song.lrc"), "[00:01.00]Hello").unwrap();
        let t = Track::from_json(&json!({ "source": "youtube", "id": "abc123", "title": "Song", "artist": "Artist", "file": song.to_string_lossy() })).unwrap();
        let (tx, _rx) = async_channel::unbounded();
        let share = Share::send(vec![t], false, tx).unwrap();
        let (port, key) = (share.port, share.key.clone());

        let (code, _) = ask(port, "GET /list?k=0000 HTTP/1.1\r\nHost: x\r\n\r\n", b"");
        assert_eq!(code, 403, "a wrong key gets nothing");

        let (code, body) = ask(port, &format!("GET /list?k={key} HTTP/1.1\r\nHost: x\r\nUser-Agent: okhttp/5.5.1\r\nAccept-Encoding: gzip\r\n\r\n"), b"");
        assert_eq!(code, 200);
        let v: Value = serde_json::from_slice(&body).unwrap();
        let item = &v["songs"][0];
        assert_eq!(item["key"], "yt:abc123");
        assert_eq!(item["track"]["title"], "Song");
        assert!(item["track"].get("file").is_none(), "where it sits here isn't sent");
        let files = item["files"].as_object().unwrap();
        let mut kinds: Vec<&str> = files.keys().map(String::as_str).collect();
        kinds.sort();
        assert_eq!(kinds, ["bass", "drums", "lyrics", "other", "song", "vocals"]);
        assert_eq!(files["song"]["size"], 300_000);

        let (code, got) = ask(port, &format!("GET /file?k={key}&key=yt%3Aabc123&kind=song HTTP/1.1\r\nHost: x\r\n\r\n"), b"");
        assert_eq!(code, 200);
        assert_eq!(got, bytes);
        let (code, got) = ask(port, &format!("GET /file?k={key}&key=yt%3Aabc123&kind=vocals HTTP/1.1\r\nHost: x\r\n\r\n"), b"");
        assert_eq!((code, got), (200, "vocals".repeat(1000).into_bytes()));
        let (code, _) = ask(port, &format!("GET /file?k={key}&key=yt%3Aother&kind=song HTTP/1.1\r\nHost: x\r\n\r\n"), b"");
        assert_eq!(code, 404, "only what's on the list");

        let ticket = share.ticket("Test");
        if let Some(t) = ticket {
            assert!(t.starts_with(&format!("radiancemusic://share?v=1&m=get&k={key}&p={port}&n=Test&h=")));
        }
        drop(share);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn words_go_both_ways() {
        let s = "Galaxy S24 Ultra ni Bassi & co=1";
        assert_eq!(dec(&enc(s)), s);
        assert_eq!(query("a=1&n=Hi%20there&x")["n"], "Hi there");
    }
}
