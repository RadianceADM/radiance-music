//! Radiance Music Server, from the app's side: your own server, or a friend's
//! (rms/ in this repository). You join one with its owner's invite link
//! (https://HOST/join/CODE): the server gives this computer its own key (a
//! token), kept in settings.json with the server's address and name. Then the
//! app plays the server's library, saves songs from it, adds your liked songs
//! to it, asks it to split songs, and joins its listening party.
//!
//! Everything goes over the server's HTTP API (`Authorization: Bearer TOKEN`;
//! files and the party's WebSocket take `?t=TOKEN`, because the player and the
//! socket can't set headers), on worker threads only. What goes wrong comes
//! back in plain words: the server's own `{"error": …}`, or that it isn't
//! answering.

use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::Duration;

use serde_json::{json, Value};

use crate::net;
use crate::track::{file_name, Source, Track};

/// A server this computer joined: where it is, its key for this computer, and its name.
#[derive(Clone, Debug, PartialEq)]
pub struct Joined {
    /// "https://music.example.com" or "http://203.0.113.5:8080" (no slash at the end).
    pub address: String,
    pub token: String,
    /// "Bassi's server" (from /api/status).
    pub name: String,
}

impl Joined {
    pub fn to_json(&self) -> Value {
        json!({ "address": self.address, "token": self.token, "name": self.name })
    }

    pub fn from_json(v: &Value) -> Option<Joined> {
        let s = |k: &str| v[k].as_str().map(|x| x.trim().to_string()).filter(|x| !x.is_empty());
        Some(Joined { address: origin(&s("address")?), token: s("token")?, name: s("name").unwrap_or_else(|| "A Radiance Music Server".into()) })
    }

    /// The party's WebSocket address (ws:// or wss://, with the key).
    pub fn socket_url(&self) -> String {
        let rest = self.address.strip_prefix("https://").map(|h| format!("wss://{h}")).or_else(|| self.address.strip_prefix("http://").map(|h| format!("ws://{h}"))).unwrap_or_else(|| self.address.clone());
        format!("{rest}/api/party/ws?t={}", self.token)
    }
}

/// The servers joined, for worker threads (a cover's or a song's address gets its key).
static JOINED: RwLock<Vec<Joined>> = RwLock::new(Vec::new());

/// The list as settings.json has it now.
pub fn set_joined(list: &[Joined]) {
    if let Ok(mut j) = JOINED.write() {
        *j = list.to_vec();
    }
}

/// The joined server an address is on (its address starts with the server's).
pub fn of_url(url: &str) -> Option<Joined> {
    let joined = JOINED.read().ok()?;
    joined.iter().find(|s| url.starts_with(&format!("{}/", s.address))).cloned()
}

/// The joined server a song is from.
pub fn of_track(t: &Track) -> Option<Joined> {
    (t.source == Source::Server).then(|| of_url(&t.id)).flatten()
}

/// An address on a joined server, with this computer's key (`?t=`), as the
/// player and cover downloads need it; any other address as it is.
pub fn signed(url: &str) -> String {
    match of_url(url) {
        Some(s) => with_token(url, &s.token),
        None => url.to_string(),
    }
}

fn with_token(url: &str, token: &str) -> String {
    format!("{url}{}t={token}", if url.contains('?') { '&' } else { '?' })
}

/// A server's address as both apps write it (so a song's address, and its key,
/// is the same on the phone): its scheme and host in small letters, its port
/// only when it isn't the scheme's own, nothing after ("HTTPS://Music.Example.com:443/" →
/// "https://music.example.com"), as the phone's Rms.origin (OkHttp's HttpUrl).
pub fn origin(address: &str) -> String {
    let address = address.trim();
    let Some((scheme, rest)) = address.split_once("://") else { return address.trim_end_matches('/').to_string() };
    let scheme = scheme.to_ascii_lowercase();
    let hostport = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let hostport = hostport.rsplit_once('@').map(|(_, h)| h).unwrap_or(hostport);
    let (host, port) = match hostport.strip_prefix('[') {
        // [IPv6]:port
        Some(v6) => match v6.split_once(']') {
            Some((h, p)) => (format!("[{}]", h.to_ascii_lowercase()), p.strip_prefix(':').unwrap_or("")),
            None => (hostport.to_ascii_lowercase(), ""),
        },
        None => match hostport.rsplit_once(':') {
            Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) => (h.to_ascii_lowercase(), p),
            _ => (hostport.to_ascii_lowercase(), ""),
        },
    };
    let default = match scheme.as_str() {
        "https" => "443",
        "http" => "80",
        _ => "",
    };
    match port.parse::<u16>() {
        Ok(p) if p.to_string() != default => format!("{scheme}://{host}:{p}"),
        _ => format!("{scheme}://{host}"),
    }
}

/// An invite link as pasted ("https://HOST/join/CODE", with or without the
/// https://, spaces or a slash at the end): the server's address and the code.
pub fn parse_invite(text: &str) -> Result<(String, String), String> {
    let text = text.trim().trim_end_matches('/');
    const HOW: &str = "Paste the whole invite link, like https://music.example.com/join/ABCD-EFGH-JKMN. The server's owner sends it to you.";
    let (before, code) = text.rsplit_once("/join/").ok_or(HOW)?;
    let code: String = code.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect();
    if code.len() < 8 {
        return Err(HOW.into());
    }
    let lower = before.to_ascii_lowercase();
    let address = if lower.starts_with("http://") || lower.starts_with("https://") {
        before.to_string()
    } else {
        // no scheme: a name is https (Caddy's), an address with a port or an IP plain http
        let host = before.split('/').next().unwrap_or(before);
        let ip = host.split(':').next().is_some_and(|h| h.parse::<std::net::IpAddr>().is_ok() || h == "localhost");
        format!("{}://{before}", if ip || host.contains(':') { "http" } else { "https" })
    };
    let host = address.split("://").nth(1).unwrap_or("");
    if host.is_empty() || host.contains(char::is_whitespace) {
        return Err(HOW.into());
    }
    Ok((origin(&address), code))
}

/// A call that didn't work: the HTTP status (0: no answer at all) and what the server said.
#[derive(Clone, Debug)]
pub struct Failure {
    pub status: u16,
    pub said: String,
}

impl Failure {
    /// What happened, in plain words, with the server's name.
    pub fn words(&self, name: &str) -> String {
        match self.status {
            0 => format!("{name} isn't answering. Check that it's on and that this computer can reach it; your own songs still play."),
            401 => format!("This computer isn't joined to {name} anymore: its owner removed it. Ask them for a new invite, then join again in Servers."),
            _ if !self.said.is_empty() => self.said.clone(),
            s => format!("{name} had a problem ({s}). Try again in a moment."),
        }
    }

    /// The server isn't answering (not "it said no").
    pub fn unreachable(&self) -> bool {
        self.status == 0
    }

    /// This computer's key doesn't work there anymore.
    pub fn signed_out(&self) -> bool {
        self.status == 401
    }
}

fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::Agent::config_builder()
            // a 403 or 404 comes back as an answer: its {"error": …} is what to say
            .http_status_as_error(false)
            .user_agent(net::USER_AGENT)
            .timeout_connect(Some(Duration::from_secs(6)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            .build()
            .into()
    })
}

/// The `Authorization` header's value for a server.
pub fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

fn answer(resp: Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> Result<Value, Failure> {
    let mut resp = resp.map_err(|e| Failure { status: 0, said: e.to_string() })?;
    let status = resp.status().as_u16();
    let body: Value = resp.body_mut().with_config().limit(32 << 20).read_json().unwrap_or(Value::Null);
    if (200..300).contains(&status) {
        Ok(body)
    } else {
        Err(Failure { status, said: body["error"].as_str().unwrap_or_default().to_string() })
    }
}

fn get(server: &Joined, path: &str) -> Result<Value, Failure> {
    answer(agent().get(&format!("{}{path}", server.address)).header("Authorization", &bearer(&server.token)).call())
}

fn post(server: &Joined, path: &str, body: &Value) -> Result<Value, Failure> {
    answer(agent().post(&format!("{}{path}", server.address)).header("Authorization", &bearer(&server.token)).send_json(body))
}

/// What a server says about itself before joining (no key needed): its name, and whether it's set up.
pub fn status(address: &str) -> Result<Value, Failure> {
    answer(agent().get(&format!("{address}/api/status")).call())
}

/// Joins a server with an invite's code: this computer gets its own key there.
/// `device`: the name the server's owner sees for it (the computer's name).
pub fn join(address: &str, code: &str, device: &str) -> Result<Joined, String> {
    let plain = |f: Failure| f.words("The server");
    let about = status(address).map_err(plain)?;
    if about["needs_setup"].as_bool() == Some(true) {
        return Err("That server isn't set up yet: its owner opens its page first and types their name.".into());
    }
    let said = answer(agent().post(&format!("{address}/api/join")).send_json(json!({ "code": code, "device_name": device }))).map_err(plain)?;
    let token = said["token"].as_str().filter(|t| !t.is_empty()).ok_or("The server didn't give this computer a key. Try again in a moment.")?.to_string();
    let name = about["server"].as_str().or(said["server"]["name"].as_str()).unwrap_or("A Radiance Music Server").to_string();
    Ok(Joined { address: address.to_string(), token, name })
}

/// Who this computer is on a server, and what its owner lets you do there.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Me {
    pub person: i64,
    pub name: String,
    pub owner: bool,
    pub device: i64,
    /// The server's name now (its owner may have been renamed).
    pub server: String,
    pub can_add: bool,
    pub can_split: bool,
    pub can_host: bool,
}

pub fn parse_me(v: &Value) -> Me {
    Me {
        person: v["person"]["id"].as_i64().unwrap_or(0),
        name: v["person"]["name"].as_str().unwrap_or_default().to_string(),
        owner: v["person"]["role"].as_str() == Some("owner"),
        device: v["device"]["id"].as_i64().unwrap_or(0),
        server: v["server"]["name"].as_str().unwrap_or_default().to_string(),
        can_add: v["can"]["add"].as_bool().unwrap_or(false),
        can_split: v["can"]["split"].as_bool().unwrap_or(false),
        can_host: v["can"]["host"].as_bool().unwrap_or(false),
    }
}

pub fn me(server: &Joined) -> Result<Me, Failure> {
    get(server, "/api/me").map(|v| parse_me(&v))
}

/// One of a song's files on the server.
#[derive(Clone, Debug, PartialEq)]
pub struct File {
    pub name: String,
    pub size: u64,
    /// "/files/…" (the key goes on when it's fetched).
    pub url: String,
}

/// A song in a server's library.
#[derive(Clone, Debug, PartialEq)]
pub struct Song {
    pub id: i64,
    pub artist: String,
    pub title: String,
    /// Its own file's name ("Artist - Title.opus").
    pub file: String,
    pub url: String,
    pub duration_ms: Option<i64>,
    /// All four parts are there (drums, bass, other, vocals).
    pub split: bool,
    pub parts: Vec<String>,
    pub lyrics: bool,
    pub word_times: bool,
    pub cover: Option<String>,
    /// By kind: song, drums, bass, other, vocals, music, lyrics, cover, video.
    pub files: BTreeMap<String, File>,
    pub added_by: Option<String>,
}

pub fn parse_library(v: &Value) -> Vec<Song> {
    let list = v["songs"].as_array().cloned().unwrap_or_default();
    list.iter()
        .filter_map(|s| {
            let text = |k: &str| s[k].as_str().unwrap_or_default().to_string();
            let files = s["files"]
                .as_object()
                .map(|o| {
                    o.iter()
                        .filter_map(|(kind, f)| Some((kind.clone(), File { name: f["name"].as_str()?.to_string(), size: f["size"].as_u64().unwrap_or(0), url: f["url"].as_str()?.to_string() })))
                        .collect()
                })
                .unwrap_or_default();
            Some(Song {
                id: s["id"].as_i64()?,
                artist: text("artist"),
                title: text("title"),
                file: text("file"),
                url: s["url"].as_str()?.to_string(),
                duration_ms: s["duration_ms"].as_i64(),
                split: s["split"].as_bool().unwrap_or(false),
                parts: s["parts"].as_array().map(|p| p.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default(),
                lyrics: s["lyrics"].as_bool().unwrap_or(false),
                word_times: s["word_times"].as_bool().unwrap_or(false),
                cover: s["cover"].as_str().map(String::from),
                files,
                added_by: s["added_by"]["name"].as_str().map(String::from),
            })
        })
        .collect()
}

impl Song {
    /// The song as the app plays it: its address on the server (without the
    /// key: that's added as it plays) is its id.
    pub fn track(&self, server: &Joined) -> Track {
        let ext = self.file.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
        Track {
            source: Source::Server,
            id: format!("{}{}", server.address, self.url),
            title: if self.title.is_empty() { self.file.clone() } else { self.title.clone() },
            artist: self.artist.clone(),
            album: String::new(),
            seconds: (self.duration_ms.unwrap_or(0).max(0) / 1000) as u32,
            art: self.cover.as_ref().map(|c| format!("{}{c}", server.address)).unwrap_or_default(),
            format: match ext.as_str() {
                "flac" => "FLAC".into(),
                "m4a" => "AAC".into(),
                _ => "Opus".into(),
            },
            file: None,
            video: false,
            genres: None,
        }
    }
}

/// Each server's library as last fetched (by address): a song's parts and
/// lyrics are found there by its address.
static LIBRARIES: Mutex<Option<HashMap<String, Vec<Song>>>> = Mutex::new(None);

pub fn library(server: &Joined) -> Result<Vec<Song>, Failure> {
    let songs = parse_library(&get(server, "/api/library")?);
    if let Ok(mut l) = LIBRARIES.lock() {
        l.get_or_insert_with(HashMap::new).insert(server.address.clone(), songs.clone());
    }
    Ok(songs)
}

/// A server's songs as last fetched (none yet: empty).
pub fn cached(address: &str) -> Vec<Song> {
    LIBRARIES.lock().ok().and_then(|l| l.as_ref()?.get(address).cloned()).unwrap_or_default()
}

/// A song from a server, as last fetched.
pub fn song_of(t: &Track) -> Option<(Joined, Song)> {
    let server = of_track(t)?;
    let song = cached(&server.address).into_iter().find(|s| format!("{}{}", server.address, s.url) == t.id)?;
    Some((server, song))
}

/// The parts the Mixer plays for a server's song: its four, or (split before
/// there were four) its music and vocals; else none.
pub fn mix_parts(t: &Track) -> Vec<&'static str> {
    let Some((_, song)) = song_of(t) else { return vec![] };
    let has = |p: &str| song.files.contains_key(p);
    if crate::split::PARTS.iter().all(|p| has(p)) {
        crate::split::PARTS.to_vec()
    } else if has("music") && has("vocals") {
        vec!["music", "vocals"]
    } else {
        vec![]
    }
}

/// A server song's parts, fetched into the cache for the Mixer (worker thread;
/// one there at the same size isn't fetched again). The Mixer plays them from
/// there: over HTTP, GStreamer jumps in an Ogg file only to the start of a page
/// (up to a second early), and the parts would no longer line up sample for
/// sample. How many came.
pub fn fetch_parts(t: &Track) -> Result<usize, String> {
    let (server, song) = song_of(t).ok_or("its server's songs aren't known yet")?;
    let mut got = 0;
    for part in mix_parts(t) {
        let Some(f) = song.files.get(part) else { continue };
        let ext = f.name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_else(|| "ogg".into());
        let to = crate::split::cache_part(t, &format!("{part}.{ext}")).ok_or("no place for it")?;
        if let Some(dir) = to.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        download(&server, f, &to)?;
        got += 1;
    }
    Ok(got)
}

/// A server song's lyrics file (a Lyricsfile, or .lrc text), fetched (worker thread).
pub fn lyrics_text(t: &Track) -> Option<(bool, String)> {
    let (server, song) = song_of(t)?;
    let f = song.files.get("lyrics")?;
    let mut resp = agent().get(&format!("{}{}", server.address, f.url)).header("Authorization", &bearer(&server.token)).call().ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let text = resp.body_mut().with_config().limit(4 << 20).read_to_string().ok()?;
    Some((f.name.ends_with(".lyrics.yaml"), text))
}

/// A file's name from its address on the server ("/files/Artist%20-%20Title.opus" → "Artist - Title.opus").
pub fn name_of(url: &str) -> String {
    let path = url.split('?').next().unwrap_or(url);
    let enc = path.rsplit_once("/files/").map(|(_, n)| n).unwrap_or(path);
    let bytes = enc.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        match (bytes[i], bytes.get(i + 1).copied().and_then(hex), bytes.get(i + 2).copied().and_then(hex)) {
            (b'%', Some(a), Some(b)) => {
                out.push((a * 16 + b) as u8);
                i += 3;
            }
            (c, ..) => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A file from a server into `to` (through a .part file; one already there at
/// the same size isn't fetched again).
fn download(server: &Joined, f: &File, to: &Path) -> Result<(), String> {
    if std::fs::metadata(to).is_ok_and(|m| m.len() == f.size && f.size > 0) {
        return Ok(());
    }
    let mut resp = agent().get(&format!("{}{}", server.address, f.url)).header("Authorization", &bearer(&server.token)).call().map_err(|_| format!("{} isn't answering", server.name))?;
    if !resp.status().is_success() {
        return Err(format!("{} didn't send {} ({})", server.name, f.name, resp.status().as_u16()));
    }
    let part = to.with_extension("part");
    let mut file = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut body = resp.body_mut().as_reader();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = body.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
    }
    drop(file);
    std::fs::rename(&part, to).map_err(|e| e.to_string())
}

/// Saves a server's song into your library (worker thread), as liking a song
/// saves it: the song, its parts, its lyrics and its cover, each named as the
/// app names them ("Artist - Title.drums.ogg" beside "Artist - Title.opus").
/// The song's path.
pub fn save(t: &Track, dir: &Path) -> Result<String, String> {
    let server = of_track(t).ok_or("This song's server isn't joined anymore, so it can't be saved from there.")?;
    let songs = library(&server).map_err(|f| f.words(&server.name))?;
    let song = songs.into_iter().find(|s| format!("{}{}", server.address, s.url) == t.id).ok_or_else(|| format!("“{}” isn't on {} anymore.", t.title, server.name))?;
    let own = song.files.get("song").cloned().unwrap_or(File { name: song.file.clone(), size: 0, url: song.url.clone() });
    let (stem, ext) = own.name.rsplit_once('.').map(|(s, e)| (file_name(s), e.to_ascii_lowercase())).ok_or("the song's file has no type")?;
    let path = dir.join(format!("{stem}.{ext}"));
    download(&server, &own, &path)?;
    // its extras (one that doesn't come is left out: the song is what matters)
    for (kind, f) in &song.files {
        let fext = f.name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
        let name = match kind.as_str() {
            "song" | "video" => continue,
            "cover" => format!("{stem}.jpg"),
            "lyrics" if f.name.ends_with(".lyrics.yaml") => format!("{stem}.lyrics.yaml"),
            "lyrics" => format!("{stem}.lrc"),
            part => format!("{stem}.{part}.{fext}"),
        };
        if let Err(e) = download(&server, f, &dir.join(name)) {
            crate::log::note(&format!("saving “{}” from {}: {kind}: {e}", t.title, server.name));
        }
    }
    Ok(path.display().to_string())
}

/// What adding a song to a server did.
#[derive(Clone, Debug, PartialEq)]
pub struct Added {
    /// The server had it already (same artist and title): only the extras it lacked went.
    pub skipped: bool,
    pub extras: Vec<String>,
}

/// One form field of a multipart upload: (name, file name, bytes) or (name, text).
enum Field {
    File(&'static str, String, Vec<u8>),
    Text(&'static str, String),
}

fn multipart(fields: &[Field], boundary: &str) -> Vec<u8> {
    let mut body = vec![];
    for f in fields {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        match f {
            Field::File(name, file, bytes) => {
                let file = file.replace(['"', '\r', '\n'], "_");
                body.extend_from_slice(format!("Content-Disposition: form-data; name=\"{name}\"; filename=\"{file}\"\r\nContent-Type: application/octet-stream\r\n\r\n").as_bytes());
                body.extend_from_slice(bytes);
            }
            Field::Text(name, text) => {
                body.extend_from_slice(format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes());
                body.extend_from_slice(text.as_bytes());
            }
        }
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    body
}

/// Adds one of your saved songs to a server (POST /api/songs; worker thread):
/// its file, with its cover and lyrics when it has them.
pub fn upload(server: &Joined, t: &Track) -> Result<Added, String> {
    let file = t.file.as_deref().filter(|f| Path::new(f).exists()).ok_or("Only saved songs can go to a server: like it first, so it's saved.")?;
    let song = std::fs::read(file).map_err(|e| e.to_string())?;
    let name = Path::new(file).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "song".into());
    let mut fields = vec![Field::File("song", name, song), Field::Text("artist", t.artist.clone()), Field::Text("title", t.title.clone())];
    if t.seconds > 0 {
        fields.push(Field::Text("seconds", t.seconds.to_string()));
    }
    for (kind, ext) in [("cover", "jpg"), ("lyrics", "lyrics.yaml"), ("lyrics", "lrc")] {
        if fields.iter().any(|f| matches!(f, Field::File(k, ..) if *k == kind)) {
            continue;
        }
        let p = crate::library::beside(file, ext);
        if let Ok(bytes) = std::fs::read(&p) {
            fields.push(Field::File(kind, p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), bytes));
        }
    }
    let boundary = format!("radiance-music-{:016x}", now_ms().to_bits() ^ 0x5eed_cafe);
    let body = multipart(&fields, &boundary);
    let said = answer(
        agent()
            .post(&format!("{}/api/songs", server.address))
            .header("Authorization", &bearer(&server.token))
            .header("Content-Type", &format!("multipart/form-data; boundary={boundary}"))
            .config()
            .timeout_global(Some(Duration::from_secs(600)))
            .build()
            .send(&body[..]),
    )
    .map_err(|f| f.words(&server.name))?;
    Ok(Added { skipped: said["skipped"].as_bool().unwrap_or(false), extras: said["extras"].as_array().map(|e| e.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default() })
}

/// A song in a server's split queue.
#[derive(Clone, Debug, PartialEq)]
pub struct Job {
    pub id: i64,
    pub song: i64,
    pub title: String,
    pub artist: String,
    pub asked_by: Option<String>,
    /// waiting | splitting | timing | done | failed | removed
    pub state: String,
    pub progress: f64,
    /// The server's own words: "Waiting: 2 songs before it.", "Splitting (40 %)."…
    pub note: String,
}

fn parse_job(v: &Value) -> Option<Job> {
    Some(Job {
        id: v["id"].as_i64()?,
        song: v["song"]["id"].as_i64().unwrap_or(0),
        title: v["song"]["title"].as_str().unwrap_or_default().to_string(),
        artist: v["song"]["artist"].as_str().unwrap_or_default().to_string(),
        asked_by: v["asked_by"]["name"].as_str().map(String::from),
        state: v["state"].as_str().unwrap_or_default().to_string(),
        progress: v["progress"].as_f64().unwrap_or(0.0),
        note: v["note"].as_str().unwrap_or_default().to_string(),
    })
}

impl Job {
    pub fn finished(&self) -> bool {
        matches!(self.state.as_str(), "done" | "failed" | "removed")
    }
}

/// What asking a server to split a song did.
#[derive(Clone, Debug, PartialEq)]
pub enum Asked {
    /// Its parts are there already.
    Split,
    /// In the queue (just now, or it was already): its job.
    Queued(Job),
}

/// Asks a server to split one of its songs (POST /api/split; worker thread).
/// Leaving `server`: this computer's key stops working there (as the phone does), so
/// it leaves the owner's People list too. (The owner's last device can't: 409.)
pub fn leave(server: &Joined) -> Result<(), Failure> {
    let me = me(server)?;
    post(server, &format!("/api/devices/{}/revoke", me.device), &json!({})).map(|_| ())
}

pub fn split(server: &Joined, t: &Track) -> Result<Asked, Failure> {
    let said = post(server, "/api/split", &json!({ "song": song_ref(t) }))?;
    parse_asked(&said).ok_or(Failure { status: 500, said: "The server's answer didn't say where the song is in its queue.".into() })
}

/// A server's song in a request: its id there, or (its library not fetched
/// yet) its file's name, which the API takes too.
pub fn song_ref(t: &Track) -> Value {
    match song_of(t) {
        Some((_, s)) => json!(s.id),
        None => json!(name_of(&t.id)),
    }
}

pub fn parse_asked(v: &Value) -> Option<Asked> {
    if v["already_split"].as_bool() == Some(true) {
        return Some(Asked::Split);
    }
    parse_job(&v["job"]).map(Asked::Queued)
}

/// A server's split queue: the songs at work, the waiting ones in their turns, the last day's finished.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Queue {
    pub jobs: Vec<Job>,
    /// Whose turn is next.
    pub next_turn: Option<String>,
}

impl Queue {
    /// Where a waiting job is in line (0: next).
    pub fn ahead(&self, job: i64) -> Option<usize> {
        self.jobs.iter().filter(|j| j.state == "waiting").position(|j| j.id == job)
    }

    pub fn job(&self, id: i64) -> Option<&Job> {
        self.jobs.iter().find(|j| j.id == id)
    }
}

pub fn parse_queue(v: &Value) -> Queue {
    Queue { jobs: v["jobs"].as_array().map(|a| a.iter().filter_map(parse_job).collect()).unwrap_or_default(), next_turn: v["next_turn"]["name"].as_str().map(String::from) }
}

pub fn queue(server: &Joined) -> Result<Queue, Failure> {
    get(server, "/api/queue").map(|v| parse_queue(&v))
}

// ------------------------------------------------------------------ the listening party

/// A song in the party (playing, or in its queue).
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// Its place in this party (the same song twice is two entries).
    pub entry: u64,
    pub song: i64,
    pub artist: String,
    pub title: String,
    pub url: String,
    pub cover: Option<String>,
    pub duration_ms: Option<f64>,
    pub added_by: (i64, String),
}

impl Entry {
    /// As the app plays it (like a library song: its address is its id).
    pub fn track(&self, server: &Joined) -> Track {
        let ext = self.url.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
        Track {
            source: Source::Server,
            id: format!("{}{}", server.address, self.url),
            title: self.title.clone(),
            artist: self.artist.clone(),
            album: String::new(),
            seconds: (self.duration_ms.unwrap_or(0.0).max(0.0) / 1000.0) as u32,
            art: self.cover.as_ref().map(|c| format!("{}{c}", server.address)).unwrap_or_default(),
            format: if ext == "flac" { "FLAC".into() } else if ext == "m4a" { "AAC".into() } else { "Opus".into() },
            file: None,
            video: false,
            genres: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Member {
    pub id: i64,
    pub name: String,
    pub host: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Reaction {
    pub emoji: String,
    pub by: String,
    pub at_server_ms: f64,
}

/// The party as the server sends it (on every change, over the WebSocket).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Party {
    pub on: bool,
    pub host: (i64, String),
    pub everyone_controls: bool,
    pub song: Option<Entry>,
    pub playing: bool,
    /// The server time the playing song's start was (or will be) at.
    pub started_at_server_ms: Option<f64>,
    /// Where a paused song stopped.
    pub paused_at_ms: Option<f64>,
    /// A change starts this long after it's made (so everyone has it in time).
    pub lead_ms: f64,
    pub queue: Vec<Entry>,
    pub members: Vec<Member>,
    pub reactions: Vec<Reaction>,
}

fn parse_entry(v: &Value) -> Option<Entry> {
    Some(Entry {
        entry: v["entry"].as_u64()?,
        song: v["id"].as_i64().unwrap_or(0),
        artist: v["artist"].as_str().unwrap_or_default().to_string(),
        title: v["title"].as_str().unwrap_or_default().to_string(),
        url: v["url"].as_str()?.to_string(),
        cover: v["cover"].as_str().map(String::from),
        duration_ms: v["duration_ms"].as_f64(),
        added_by: (v["added_by"]["id"].as_i64().unwrap_or(0), v["added_by"]["name"].as_str().unwrap_or_default().to_string()),
    })
}

pub fn parse_party(v: &Value) -> Party {
    Party {
        on: v["on"].as_bool().unwrap_or(false),
        host: (v["host"]["id"].as_i64().unwrap_or(0), v["host"]["name"].as_str().unwrap_or_default().to_string()),
        everyone_controls: v["everyone_controls"].as_bool().unwrap_or(false),
        song: parse_entry(&v["song"]),
        playing: v["playing"].as_bool().unwrap_or(false),
        started_at_server_ms: v["started_at_server_ms"].as_f64(),
        paused_at_ms: v["paused_at_ms"].as_f64(),
        lead_ms: v["lead_ms"].as_f64().unwrap_or(800.0),
        queue: v["queue"].as_array().map(|a| a.iter().filter_map(parse_entry).collect()).unwrap_or_default(),
        members: v["members"]
            .as_array()
            .map(|a| a.iter().filter_map(|m| Some(Member { id: m["id"].as_i64()?, name: m["name"].as_str().unwrap_or_default().to_string(), host: m["host"].as_bool().unwrap_or(false) })).collect())
            .unwrap_or_default(),
        reactions: v["reactions"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|r| Some(Reaction { emoji: r["emoji"].as_str()?.to_string(), by: r["by"]["name"].as_str().unwrap_or_default().to_string(), at_server_ms: r["at_server_ms"].as_f64().unwrap_or(0.0) }))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

impl Party {
    /// Where the party's song is at server time `server_now` (ms; negative: it
    /// starts in a moment), as party.rs counts it:
    /// position = server now − started_at_server_ms; paused: where it stopped.
    pub fn position_ms(&self, server_now: f64) -> Option<f64> {
        self.song.as_ref()?;
        if self.playing {
            self.started_at_server_ms.map(|s| server_now - s)
        } else {
            Some(self.paused_at_ms.unwrap_or(0.0))
        }
    }

    /// Whether `person` plays, pauses and skips (the host, or everyone when the host allows).
    pub fn can_control(&self, person: i64) -> bool {
        self.on && (self.host.0 == person || self.everyone_controls)
    }
}

pub fn party(server: &Joined) -> Result<Party, Failure> {
    get(server, "/api/party").map(|v| parse_party(&v))
}

pub fn party_start(server: &Joined, everyone_controls: bool) -> Result<Party, Failure> {
    post(server, "/api/party/start", &json!({ "everyone_controls": everyone_controls })).map(|v| parse_party(&v))
}

pub fn party_add(server: &Joined, t: &Track) -> Result<Party, Failure> {
    post(server, "/api/party/add", &json!({ "song": song_ref(t) })).map(|v| parse_party(&v))
}

/// play | pause | skip | seek {position_ms} | reorder {entry, to} | remove {entry} | allow {everyone}.
pub fn party_control(server: &Joined, action: &str, extra: Value) -> Result<Party, Failure> {
    let mut body = json!({ "action": action });
    if let (Some(b), Some(e)) = (body.as_object_mut(), extra.as_object()) {
        b.extend(e.clone());
    }
    post(server, "/api/party/control", &body).map(|v| parse_party(&v))
}

pub fn party_react(server: &Joined, emoji: &str) -> Result<(), Failure> {
    post(server, "/api/party/react", &json!({ "emoji": emoji })).map(|_| ())
}

pub fn party_end(server: &Joined) -> Result<(), Failure> {
    post(server, "/api/party/end", &json!({})).map(|_| ())
}

/// The computer's clock in ms since 1970 (the party's clock math).
pub fn now_ms() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64() * 1000.0).unwrap_or(0.0)
}

/// The clock offset to a server from pings (sent, the server's time, received;
/// all ms), NTP-style: offset = server − (sent + received) / 2, from the
/// quickest ping (its answer waited least on the way). (offset, round trip).
pub fn best_offset(pings: &[(f64, f64, f64)]) -> Option<(f64, f64)> {
    pings.iter().filter(|(s, _, r)| r >= s).min_by(|a, b| (a.2 - a.0).total_cmp(&(b.2 - b.0))).map(|(s, m, r)| (m - (s + r) / 2.0, r - s))
}

/// The server's clock less this computer's (ms), from five GET /api/time pings (worker thread).
pub fn clock_offset(server: &Joined) -> Option<(f64, f64)> {
    let mut pings = vec![];
    for _ in 0..5 {
        let sent = now_ms();
        let said = answer(agent().get(&format!("{}/api/time", server.address)).call());
        let got = now_ms();
        if let Some(m) = said.ok().and_then(|v| v["now_ms"].as_f64()) {
            pings.push((sent, m, got));
        }
    }
    best_offset(&pings)
}

/// How far this computer may play from the party before it does something
/// about it (s): the ear doesn't mind less. (The playback rate is never
/// nudged: GStreamer's rate changes glitch.)
pub const IN_STEP: f64 = 0.08;
/// Ahead by up to this (s), it waits (a pause just that long) instead of jumping.
pub const HOLD_MOST: f64 = 1.5;

/// What to do about playing `drift` seconds off the party (+: ahead).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    /// In step.
    Keep,
    /// A little ahead: a pause just this long (s), and it's in step.
    Wait(f64),
    /// Behind, or far ahead: a jump to where the party is.
    Jump,
}

pub fn step(drift: f64) -> Step {
    if drift.abs() <= IN_STEP {
        Step::Keep
    } else if drift > 0.0 && drift <= HOLD_MOST {
        Step::Wait(drift)
    } else {
        Step::Jump
    }
}

/// What the party's WebSocket says.
#[derive(Clone, Debug)]
pub enum Live {
    State(Value),
    /// This device was removed from the server (the party goes with it).
    Removed(String),
    /// The line dropped (it joins again by itself).
    Lost,
    Back,
}

/// The party's WebSocket, on its own thread: every state the server sends
/// goes to `tx`; a dropped line joins again (1, 2, 4… up to 15 s apart).
/// Dropped: the thread closes the line and ends.
pub struct Socket {
    stop: Arc<AtomicBool>,
}

impl Drop for Socket {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

pub fn listen(server: &Joined, tx: async_channel::Sender<Live>) -> Socket {
    let stop = Arc::new(AtomicBool::new(false));
    let (url, flag) = (server.socket_url(), stop.clone());
    let _ = std::thread::Builder::new().name("party".into()).spawn(move || {
        use tungstenite::stream::MaybeTlsStream;
        use tungstenite::{Error, Message};
        let mut wait = 1u64;
        let mut lost = false;
        while !flag.load(Ordering::Relaxed) {
            let mut socket = match tungstenite::connect(url.as_str()) {
                Ok((s, _)) => s,
                Err(_) => {
                    if !lost {
                        lost = true;
                        let _ = tx.send_blocking(Live::Lost);
                    }
                    for _ in 0..wait * 4 {
                        if flag.load(Ordering::Relaxed) {
                            return;
                        }
                        std::thread::sleep(Duration::from_millis(250));
                    }
                    wait = (wait * 2).min(15);
                    continue;
                }
            };
            // a short wait for each read, so a leave is noticed at once
            let timeout = Some(Duration::from_millis(250));
            match socket.get_mut() {
                MaybeTlsStream::Plain(s) => {
                    let _ = s.set_read_timeout(timeout);
                }
                MaybeTlsStream::Rustls(s) => {
                    let _ = s.get_mut().set_read_timeout(timeout);
                }
                _ => {}
            }
            if lost {
                lost = false;
                let _ = tx.send_blocking(Live::Back);
            }
            wait = 1;
            loop {
                if flag.load(Ordering::Relaxed) {
                    let _ = socket.close(None);
                    let _ = socket.flush();
                    return;
                }
                match socket.read() {
                    Ok(Message::Text(text)) => {
                        let v: Value = serde_json::from_str(text.as_str()).unwrap_or(Value::Null);
                        match v["type"].as_str() {
                            Some("state") => {
                                if tx.send_blocking(Live::State(v)).is_err() {
                                    return;
                                }
                            }
                            Some("removed") => {
                                let _ = tx.send_blocking(Live::Removed(v["error"].as_str().unwrap_or("This device was removed from the server.").to_string()));
                                return;
                            }
                            _ => {}
                        }
                    }
                    Ok(Message::Close(_)) => break,
                    Ok(_) => {}
                    Err(Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {
                        let _ = socket.flush(); // (its answers to the server's pings go out)
                    }
                    Err(_) => break,
                }
            }
            if !flag.load(Ordering::Relaxed) {
                lost = true;
                let _ = tx.send_blocking(Live::Lost);
            }
        }
    });
    Socket { stop }
}

/// The reactions the server's party page draws (its own pictures: no emoji
/// font needed): (emoji, name, the app's icon for it).
pub const REACTIONS: [(&str, &str, &str); 6] = [
    ("🔥", "Fire", "rms-react-fire"),
    ("❤️", "Love", "rms-react-love"),
    ("😂", "Laugh", "rms-react-laugh"),
    ("👏", "Clap", "rms-react-clap"),
    ("🎉", "Party", "rms-react-party"),
    ("😮", "Wow", "rms-react-wow"),
];

/// A reaction's icon and name (one the app doesn't know: a soft smile).
pub fn reaction_icon(emoji: &str) -> (&'static str, &'static str) {
    REACTIONS.iter().find(|(e, ..)| *e == emoji).map(|(_, name, icon)| (*icon, *name)).unwrap_or(("face-smile-symbolic", "A reaction"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tests that set the joined servers (they're the app's, one list) take turns.
    static JOINED_TESTS: Mutex<()> = Mutex::new(());

    fn server() -> Joined {
        Joined { address: "https://music.example.com".into(), token: "ab12".into(), name: "Bassi's server".into() }
    }

    #[test]
    fn invite_links_as_pasted() {
        assert_eq!(parse_invite("https://music.example.com/join/ABCD-EFGH-JKMN").unwrap(), ("https://music.example.com".into(), "ABCDEFGHJKMN".into()));
        assert_eq!(parse_invite("  http://203.0.113.5:8080/join/abcd-efgh-jkmn/ \n").unwrap(), ("http://203.0.113.5:8080".into(), "ABCDEFGHJKMN".into()));
        // no scheme: a name is https, an address with a port plain http
        assert_eq!(parse_invite("music.example.com/join/ABCDEFGHJKMN").unwrap().0, "https://music.example.com");
        assert_eq!(parse_invite("127.0.0.1:18321/join/ABCDEFGHJKMN").unwrap().0, "http://127.0.0.1:18321");
        assert!(parse_invite("https://music.example.com/").is_err());
        // the address as the phone writes it (a song's address, so its key, the same on both)
        assert_eq!(parse_invite("HTTPS://Music.Example.com:443/join/ABCDEFGHJKMN").unwrap().0, "https://music.example.com");
        assert_eq!(origin("http://127.0.0.1:80"), "http://127.0.0.1");
        assert_eq!(origin("https://music.example.com:8443/x"), "https://music.example.com:8443");
        assert_eq!(origin("http://[::1]:18321/"), "http://[::1]:18321");
        let local = Joined { address: origin("http://127.0.0.1:18321/"), token: "k".into(), name: "x".into() };
        let song = &parse_library(&json!({ "songs": [{ "id": 1, "title": "Evening Glow", "url": "/files/Ana%20-%20Evening%20Glow.flac", "file": "Ana - Evening Glow.flac", "files": {} }] }))[0];
        assert_eq!(song.track(&local).key(), "rms:ca419abfd9ba52f1");
        assert!(parse_invite("https://music.example.com/join/AB").is_err());
    }

    #[test]
    fn the_key_goes_where_headers_cant() {
        let _turn = JOINED_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let s = server();
        assert_eq!(bearer(&s.token), "Bearer ab12");
        set_joined(std::slice::from_ref(&s));
        assert_eq!(signed("https://music.example.com/files/A%20-%20B.opus"), "https://music.example.com/files/A%20-%20B.opus?t=ab12");
        assert_eq!(signed("https://music.example.com/files/x.jpg?v=2"), "https://music.example.com/files/x.jpg?v=2&t=ab12");
        // another site's address keeps no key of ours (nor a look-alike name)
        assert_eq!(signed("https://music.example.com.evil.net/files/x.jpg"), "https://music.example.com.evil.net/files/x.jpg");
        assert_eq!(signed("https://archive.org/download/x.flac"), "https://archive.org/download/x.flac");
        assert_eq!(s.socket_url(), "wss://music.example.com/api/party/ws?t=ab12");
        let plain = Joined { address: "http://127.0.0.1:8080".into(), ..s };
        assert_eq!(plain.socket_url(), "ws://127.0.0.1:8080/api/party/ws?t=ab12");
        set_joined(&[]);
    }

    #[test]
    fn joined_servers_round_trip() {
        let s = server();
        assert_eq!(Joined::from_json(&s.to_json()), Some(s));
        assert_eq!(Joined::from_json(&json!({ "address": "https://x.org/", "token": "t" })).unwrap().address, "https://x.org");
        assert_eq!(Joined::from_json(&json!({ "address": "https://x.org" })), None);
    }

    #[test]
    fn failures_in_plain_words() {
        let name = "Bassi's server";
        assert!(Failure { status: 0, said: "connection refused".into() }.words(name).starts_with("Bassi's server isn't answering"));
        assert!(Failure { status: 401, said: String::new() }.words(name).contains("isn't joined to Bassi's server anymore"));
        let off = Failure { status: 403, said: "The server's owner (Bassi) turned off splitting songs for you.".into() };
        assert_eq!(off.words(name), off.said);
    }

    #[test]
    fn me_and_what_you_can_do() {
        let v = json!({ "person": { "id": 3, "name": "Ana", "role": "friend" }, "device": { "id": 7, "name": "laptop" }, "server": { "name": "Bassi's server" }, "can": { "add": true, "split": false, "host": true } });
        let me = parse_me(&v);
        assert_eq!((me.person, me.owner, me.device), (3, false, 7));
        assert_eq!((me.can_add, me.can_split, me.can_host), (true, false, true));
        assert_eq!(me.server, "Bassi's server");
    }

    #[test]
    fn the_library_as_the_api_sends_it() {
        let v = json!({ "server": "Bassi's server", "count": 2, "songs": [
            { "id": 4, "artist": "Hev Abi", "title": "Para Sa Streets", "file": "Hev Abi - Para Sa Streets.opus", "url": "/files/Hev%20Abi%20-%20Para%20Sa%20Streets.opus",
              "duration_ms": 201500, "split": true, "parts": ["drums", "bass", "other", "vocals", "music"], "lyrics": true, "word_times": true,
              "cover": "/files/Hev%20Abi%20-%20Para%20Sa%20Streets.jpg",
              "files": {
                "song": { "name": "Hev Abi - Para Sa Streets.opus", "size": 3000000, "url": "/files/Hev%20Abi%20-%20Para%20Sa%20Streets.opus" },
                "drums": { "name": "Hev Abi - Para Sa Streets.drums.ogg", "size": 900000, "url": "/files/Hev%20Abi%20-%20Para%20Sa%20Streets.drums.ogg" },
                "bass": { "name": "Hev Abi - Para Sa Streets.bass.ogg", "size": 1, "url": "/files/Hev%20Abi%20-%20Para%20Sa%20Streets.bass.ogg" },
                "other": { "name": "Hev Abi - Para Sa Streets.other.ogg", "size": 1, "url": "/files/Hev%20Abi%20-%20Para%20Sa%20Streets.other.ogg" },
                "vocals": { "name": "Hev Abi - Para Sa Streets.vocals.ogg", "size": 1, "url": "/files/Hev%20Abi%20-%20Para%20Sa%20Streets.vocals.ogg" },
                "lyrics": { "name": "Hev Abi - Para Sa Streets.lyrics.yaml", "size": 1, "url": "/files/Hev%20Abi%20-%20Para%20Sa%20Streets.lyrics.yaml" }
              }, "added_by": { "id": 1, "name": "Bassi" }, "added_ms": 1 },
            { "id": 5, "artist": "", "title": "Untitled", "file": "Untitled.flac", "url": "/files/Untitled.flac", "duration_ms": null, "split": false, "parts": [], "files": {}, "added_by": null }
        ]});
        let songs = parse_library(&v);
        assert_eq!(songs.len(), 2);
        let s = &songs[0];
        assert_eq!((s.id, s.split, s.word_times, s.added_by.as_deref()), (4, true, true, Some("Bassi")));
        assert_eq!(s.files["drums"].name, "Hev Abi - Para Sa Streets.drums.ogg");
        let t = s.track(&server());
        assert_eq!(t.source, Source::Server);
        assert_eq!(t.id, "https://music.example.com/files/Hev%20Abi%20-%20Para%20Sa%20Streets.opus");
        // its key: the phone app's (model/Track.kt): "rms:" and the first 16 hex digits of the address's SHA-256
        assert_eq!(t.key(), "rms:a3f938a0ad353093");
        assert_eq!((t.seconds, t.format.as_str()), (201, "Opus"));
        assert_eq!(t.art, "https://music.example.com/files/Hev%20Abi%20-%20Para%20Sa%20Streets.jpg");
        assert!(!t.lossless());
        let flac = songs[1].track(&server());
        assert!(flac.lossless());
        assert_eq!((flac.seconds, flac.art.as_str()), (0, ""));
        // the song back from its JSON (history, the queue)
        let back = Track::from_json(&t.to_json()).unwrap();
        assert_eq!((back.source, back.key()), (Source::Server, t.key()));
        // its parts and lyrics, from the library as fetched
        let _turn = JOINED_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        set_joined(&[server()]);
        LIBRARIES.lock().unwrap().get_or_insert_with(HashMap::new).insert(server().address, songs.clone());
        assert_eq!(mix_parts(&t), vec!["drums", "bass", "other", "vocals"]);
        assert!(mix_parts(&flac).is_empty());
        set_joined(&[]);
        assert_eq!(name_of(&t.id), "Hev Abi - Para Sa Streets.opus");
        assert_eq!(name_of("/files/%C3%91ino%20%28feat.%20Lo%20ki%29.opus?t=x"), "Ñino (feat. Lo ki).opus");
    }

    #[test]
    fn the_split_request_and_its_turn() {
        // the song by its id once the library is fetched, else by its file's name
        let _turn = JOINED_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let s = Joined { address: "http://127.0.0.1:9".into(), ..server() };
        set_joined(std::slice::from_ref(&s));
        let songs = parse_library(&json!({ "songs": [{ "id": 9, "title": "A", "url": "/files/A%20-%20B.opus", "file": "A - B.opus", "files": {} }] }));
        let t = songs[0].track(&s);
        assert_eq!(song_ref(&t), json!("A - B.opus"));
        LIBRARIES.lock().unwrap().get_or_insert_with(HashMap::new).insert(s.address.clone(), songs.clone());
        assert_eq!(song_ref(&t), json!(9));
        set_joined(&[]);
        assert_eq!(parse_asked(&json!({ "already_split": true, "queued": false, "song": { "id": 9 } })), Some(Asked::Split));
        let job = json!({ "already_split": false, "queued": true, "job": { "id": 12, "song": { "id": 9, "artist": "", "title": "A" }, "asked_by": { "id": 3, "name": "Ana" }, "state": "waiting", "progress": 0, "note": "Waiting: 1 song before it." } });
        let Some(Asked::Queued(j)) = parse_asked(&job) else { panic!("not queued") };
        assert_eq!((j.id, j.song, j.state.as_str(), j.asked_by.as_deref()), (12, 9, "waiting", Some("Ana")));
        let q = parse_queue(&json!({ "next_turn": { "id": 1, "name": "Bassi" }, "jobs": [
            { "id": 10, "song": { "id": 1, "title": "X" }, "state": "splitting", "progress": 0.4, "note": "Splitting (40 %)." },
            { "id": 11, "song": { "id": 2, "title": "Y" }, "state": "waiting", "progress": 0, "note": "Next up." },
            { "id": 12, "song": { "id": 9, "title": "A" }, "state": "waiting", "progress": 0, "note": "Waiting: 1 song before it." },
            { "id": 8, "song": { "id": 3, "title": "Z" }, "state": "done", "progress": 1, "note": "Split into drums, bass, other and vocals." }
        ]}));
        assert_eq!(q.next_turn.as_deref(), Some("Bassi"));
        assert_eq!((q.ahead(11), q.ahead(12), q.ahead(10)), (Some(0), Some(1), None));
        assert!(q.job(8).unwrap().finished());
        assert!(!q.job(10).unwrap().finished());
    }

    #[test]
    fn the_party_clock() {
        // the quickest ping counts: sent at 1000, the server said 51010, back at 1020 → offset 50000
        let (offset, rtt) = best_offset(&[(0.0, 50100.0, 300.0), (1000.0, 51010.0, 1020.0), (2000.0, 52050.0, 2090.0)]).unwrap();
        assert_eq!((offset, rtt), (50000.0, 20.0));
        assert_eq!(best_offset(&[]), None);
        let v = json!({ "type": "state", "on": true, "host": { "id": 1, "name": "Bassi" }, "everyone_controls": false,
            "song": { "entry": 3, "id": 4, "artist": "Hev Abi", "title": "Para Sa Streets", "file": "x.opus", "url": "/files/x.opus", "cover": null, "duration_ms": 200000, "added_by": { "id": 2, "name": "Ana" } },
            "playing": true, "started_at_server_ms": 1_000_000, "paused_at_ms": null, "position_ms": 0, "server_ms": 1_000_000, "lead_ms": 800,
            "queue": [], "members": [{ "id": 1, "name": "Bassi", "devices": 1, "host": true }], "reactions": [{ "emoji": "🔥", "by": { "id": 2, "name": "Ana" }, "at_server_ms": 999_000 }] });
        let mut p = parse_party(&v);
        assert!(p.on && p.playing);
        assert_eq!(p.song.as_ref().unwrap().entry, 3);
        // position = server now − started_at; this computer's now + offset is the server's now
        let local_now = 1_000_000.0 - 50_000.0 + 12_345.0;
        assert_eq!(p.position_ms(local_now + 50_000.0), Some(12_345.0));
        // a change starts lead_ms after it's made: just after, the song is still to start
        assert_eq!(p.position_ms(1_000_000.0 - p.lead_ms), Some(-800.0));
        assert!(p.can_control(1) && !p.can_control(2));
        p.everyone_controls = true;
        assert!(p.can_control(2));
        // paused: where it stopped, whatever the time
        p.playing = false;
        p.paused_at_ms = Some(42_000.0);
        assert_eq!(p.position_ms(9e12), Some(42_000.0));
        // ~80 ms off is fine; a little ahead, a wait; behind or far ahead, a jump
        assert_eq!(step(0.05), Step::Keep);
        assert_eq!(step(-0.08), Step::Keep);
        assert_eq!(step(0.4), Step::Wait(0.4));
        assert_eq!(step(-0.1), Step::Jump);
        assert_eq!(step(2.5), Step::Jump);
        assert_eq!(reaction_icon("🔥"), ("rms-react-fire", "Fire"));
        assert_eq!(reaction_icon("🦄").0, "face-smile-symbolic");
        assert!(!parse_party(&json!({ "type": "state", "on": false })).on);
    }

    /// A tiny stand-in server: answers each request with `answer(method, path,
    /// headers, body)`, one connection at a time; the requests come back at the end.
    fn stand_in(n: usize, answer: fn(&str, &str, &str, &str) -> (u16, String)) -> (String, std::thread::JoinHandle<Vec<(String, String, String)>>) {
        use std::io::{BufRead, BufReader};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let seen = std::thread::spawn(move || {
            let mut seen = vec![];
            for conn in listener.incoming().take(n) {
                let mut conn = conn.unwrap();
                let mut reader = BufReader::new(conn.try_clone().unwrap());
                let mut first = String::new();
                reader.read_line(&mut first).unwrap();
                let (mut headers, mut length) = (String::new(), 0usize);
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line.trim().is_empty() {
                        break;
                    }
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap_or(0);
                    }
                    headers += &line;
                }
                let mut body = vec![0u8; length];
                reader.read_exact(&mut body).unwrap();
                let body = String::from_utf8_lossy(&body).into_owned();
                let mut parts = first.split_whitespace();
                let (method, path) = (parts.next().unwrap_or("").to_string(), parts.next().unwrap_or("").to_string());
                let (status, json) = answer(&method, &path, &headers, &body);
                let reply = format!("HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{json}", json.len());
                conn.write_all(reply.as_bytes()).unwrap();
                seen.push((format!("{method} {path}"), headers, body));
            }
            seen
        });
        (address, seen)
    }

    #[test]
    fn joining_and_the_key_on_every_call() {
        let (address, seen) = stand_in(4, |method, path, headers, body| {
            let keyed = headers.lines().any(|l| l.trim().eq_ignore_ascii_case("authorization: Bearer tok-123"));
            let sent: Value = serde_json::from_str(body).unwrap_or(Value::Null);
            match (method, path) {
                ("GET", "/api/status") => (200, r#"{"server":"Bassi's server","needs_setup":false}"#.into()),
                ("POST", "/api/join") if sent == json!({ "code": "ABCDEFGHJKMN", "device_name": "laptop" }) => {
                    (201, r#"{"token":"tok-123","person":{"id":3,"name":"Ana","role":"friend"},"device":{"id":7,"name":"laptop"},"server":{"name":"Bassi's server"}}"#.into())
                }
                ("GET", "/api/me") if keyed => (200, r#"{"person":{"id":3,"name":"Ana","role":"friend"},"device":{"id":7,"name":"laptop"},"server":{"name":"Bassi's server"},"can":{"add":true,"split":true,"host":false}}"#.into()),
                ("POST", "/api/split") if keyed => (403, r#"{"error":"The server's owner (Bassi) turned off splitting songs for you. Ask them if you'd like it back on."}"#.into()),
                _ => (401, r#"{"error":"This needs your device's key"}"#.into()),
            }
        });
        let (at, code) = parse_invite(&format!("{address}/join/ABCD-EFGH-JKMN")).unwrap();
        let joined = join(&at, &code, "laptop").unwrap();
        assert_eq!(joined, Joined { address: address.clone(), token: "tok-123".into(), name: "Bassi's server".into() });
        let who = me(&joined).unwrap();
        assert_eq!((who.name.as_str(), who.can_split, who.can_host), ("Ana", true, false));
        // a 403 comes back as the server's own words
        let t = Track { source: Source::Server, id: format!("{address}/files/A.opus"), title: "A".into(), artist: String::new(), album: String::new(), seconds: 0, art: String::new(), format: "Opus".into(), file: None, video: false, genres: None };
        let no = split(&joined, &t).unwrap_err();
        assert_eq!(no.status, 403);
        assert!(no.words(&joined.name).starts_with("The server's owner (Bassi) turned off splitting"));
        let seen = seen.join().unwrap();
        assert_eq!(seen.iter().map(|s| s.0.as_str()).collect::<Vec<_>>(), ["GET /api/status", "POST /api/join", "GET /api/me", "POST /api/split"]);
        // the key goes on every call after joining, never before
        assert!(!seen[1].1.to_ascii_lowercase().contains("authorization"));
        assert!(seen[3].1.contains("Bearer tok-123"));
        assert_eq!(serde_json::from_str::<Value>(&seen[3].2).unwrap(), json!({ "song": "A.opus" }));
        // a server that isn't there: "isn't answering"
        let gone = me(&Joined { address: "http://127.0.0.1:9".into(), ..joined }).unwrap_err();
        assert!(gone.unreachable());
    }

    #[test]
    fn a_multipart_upload() {
        let body = multipart(&[Field::File("song", "A \"x\".opus".into(), b"OPUS".to_vec()), Field::Text("title", "A".into())], "B0");
        let text = String::from_utf8(body).unwrap();
        assert_eq!(
            text,
            "--B0\r\nContent-Disposition: form-data; name=\"song\"; filename=\"A _x_.opus\"\r\nContent-Type: application/octet-stream\r\n\r\nOPUS\r\n--B0\r\nContent-Disposition: form-data; name=\"title\"\r\n\r\nA\r\n--B0--\r\n"
        );
    }
}
