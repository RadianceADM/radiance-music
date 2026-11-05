//! YouTube Music: every song, compressed (Opus, about 130-160 kbps). The fallback
//! when the free lossless libraries don't have a song.
//!
//! Search goes to YouTube Music's own search (songs only); the stream address and
//! saving a liked song go through yt-dlp, which keeps up with YouTube's changes.

use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use serde_json::{json, Value};

use crate::net;
use crate::track::{parse_clock, Source, Track};

static COOKIES_FROM: OnceLock<Option<String>> = OnceLock::new();

/// Lets yt-dlp use the YouTube sign-in of a browser on this computer (e.g.
/// "chrome"), so YouTube doesn't take it for a bot. Set once, at start.
pub fn use_sign_in_from(browser: Option<String>) {
    let _ = COOKIES_FROM.set(browser);
}

/// yt-dlp, with the browser sign-in if one was set.
fn yt_dlp(nice: bool) -> Command {
    // nice: a Linux program (Windows has none; there it runs at its usual priority)
    let mut cmd = if nice && cfg!(unix) {
        let mut c = Command::new("nice");
        c.args(["-n", "10", "yt-dlp"]);
        c
    } else {
        Command::new("yt-dlp")
    };
    crate::setup::quiet_window(&mut cmd, false); // Windows: no console window
    if let Some(Some(browser)) = COOKIES_FROM.get() {
        cmd.args(["--cookies-from-browser", browser]);
    }
    cmd
}

/// yt-dlp is there and runs (YouTube songs need it; worker thread: it takes a second).
pub fn installed() -> bool {
    yt_dlp(false).arg("--version").output().is_ok_and(|o| o.status.success())
}

const CLIENT_VERSION: &str = "1.20251020.01.00";
/// YouTube Music's "Songs" and "Videos" filters.
const SONGS: &str = "EgWKAQIIAWoKEAkQBRAKEAMQBA%3D%3D";
const VIDEOS: &str = "EgWKAQIQAWoKEAkQChAFEAMQBA%3D%3D";
const MAX: usize = 12;

pub fn search(query: &str) -> Vec<Track> {
    search_kind(query, false)
}

/// Music videos (official videos, live performances).
pub fn search_videos(query: &str) -> Vec<Track> {
    search_kind(query, true).into_iter().take(8).collect()
}

fn search_kind(query: &str, videos: bool) -> Vec<Track> {
    let body = json!({
        "context": { "client": { "clientName": "WEB_REMIX", "clientVersion": CLIENT_VERSION, "hl": "en" } },
        "query": query,
        "params": if videos { VIDEOS } else { SONGS },
    });
    let Some(v) = net::post_json(
        "https://music.youtube.com/youtubei/v1/search?prettyPrint=false",
        &body,
        &[("Origin", "https://music.youtube.com"), ("Referer", "https://music.youtube.com/")],
    ) else {
        return vec![];
    };
    let mut rows = vec![];
    collect(&v, &mut rows);
    rows.iter().filter_map(|r| song(r, videos)).take(MAX).collect()
}

fn collect<'a>(v: &'a Value, out: &mut Vec<&'a Value>) {
    match v {
        Value::Object(o) => {
            if let Some(r) = o.get("musicResponsiveListItemRenderer") {
                out.push(r);
            }
            o.values().for_each(|x| collect(x, out));
        }
        Value::Array(a) => a.iter().for_each(|x| collect(x, out)),
        _ => {}
    }
}

fn column(r: &Value, i: usize) -> String {
    r["flexColumns"][i]["musicResponsiveListItemFlexColumnRenderer"]["text"]["runs"]
        .as_array()
        .map(|runs| runs.iter().filter_map(|x| x["text"].as_str()).collect())
        .unwrap_or_default()
}

/// One search result row: "Title" / "Artist • Album • 4:56" (a video:
/// "Artist • 1.2B views • 4:12").
fn song(r: &Value, video: bool) -> Option<Track> {
    let id = r["playlistItemData"]["videoId"].as_str()?.to_string();
    let title = column(r, 0);
    let parts: Vec<String> = column(r, 1).split(" • ").map(|s| s.trim().to_string()).collect();
    let seconds = parts.last().and_then(|p| parse_clock(p)).unwrap_or(0);
    let (artist, album) = match parts.len() {
        _ if video => (parts.first().cloned().unwrap_or_default(), String::new()),
        n if n >= 3 => (parts[0].clone(), parts[1..n - 1].join(" • ")),
        2 => (parts[0].clone(), String::new()),
        _ => (parts.first().cloned().unwrap_or_default(), String::new()),
    };
    let thumbs = r["thumbnail"]["musicThumbnailRenderer"]["thumbnail"]["thumbnails"].as_array()?;
    let art = thumbs.last()?["url"].as_str().map(big_art).unwrap_or_default();
    Some(Track { source: Source::YouTube, id, title, artist, album, seconds, art, format: "Opus".into(), file: None, video, genres: None })
}

/// A song's radio: YouTube Music's endless mix of songs like it (about 50),
/// the song itself first.
pub fn radio(id: &str) -> Vec<Track> {
    let body = json!({
        "context": { "client": { "clientName": "WEB_REMIX", "clientVersion": CLIENT_VERSION, "hl": "en" } },
        "videoId": id,
        "playlistId": format!("RDAMVM{id}"),
        "isAudioOnly": true,
    });
    let Some(v) = net::post_json(
        "https://music.youtube.com/youtubei/v1/next?prettyPrint=false",
        &body,
        &[("Origin", "https://music.youtube.com"), ("Referer", "https://music.youtube.com/")],
    ) else {
        return vec![];
    };
    let mut rows = vec![];
    collect_key(&v, "playlistPanelVideoRenderer", &mut rows);
    rows.iter().filter_map(|r| radio_song(r)).collect()
}

fn collect_key<'a>(v: &'a Value, key: &str, out: &mut Vec<&'a Value>) {
    match v {
        Value::Object(o) => {
            if let Some(r) = o.get(key) {
                out.push(r);
            }
            o.values().for_each(|x| collect_key(x, key, out));
        }
        Value::Array(a) => a.iter().for_each(|x| collect_key(x, key, out)),
        _ => {}
    }
}

fn runs(v: &Value) -> String {
    v["runs"].as_array().map(|r| r.iter().filter_map(|x| x["text"].as_str()).collect()).unwrap_or_default()
}

/// A radio row: "Title" / "Artist • Album • 2015" / "4:56".
fn radio_song(r: &Value) -> Option<Track> {
    let id = r["videoId"].as_str()?.to_string();
    let mut parts: Vec<String> = runs(&r["longBylineText"]).split(" • ").map(|s| s.trim().to_string()).collect();
    if parts.len() > 1 && parts.last().is_some_and(|y| y.len() == 4 && y.chars().all(|c| c.is_ascii_digit())) {
        parts.pop(); // the year
    }
    let art = r["thumbnail"]["thumbnails"].as_array()?.last()?["url"].as_str().map(big_art).unwrap_or_default();
    Some(Track {
        source: Source::YouTube,
        id,
        title: runs(&r["title"]),
        artist: parts.first().cloned().unwrap_or_default(),
        album: parts.get(1..).map(|p| p.join(" • ")).unwrap_or_default(),
        seconds: parse_clock(&runs(&r["lengthText"])).unwrap_or(0),
        art,
        format: "Opus".into(),
        file: None,
        video: false,
        genres: None,
    })
}

/// Google's image addresses carry their size ("=w120-h120-..."): ask for a big one.
fn big_art(url: &str) -> String {
    match url.rfind("=w") {
        Some(i) => format!("{}=w544-h544-l90-rj", &url[..i]),
        None => url.to_string(),
    }
}

fn watch_url(id: &str) -> String {
    format!("https://music.youtube.com/watch?v={id}")
}

/// What to play: the audio stream's address, its size in bytes, its type and its
/// bitrate. Takes a few seconds (yt-dlp works out YouTube's signature).
pub struct Stream {
    pub url: String,
    pub size: u64,
    pub mime: String,
    pub kbps: u32,
}

pub fn stream(id: &str) -> Result<Stream, String> {
    let out = yt_dlp(false)
        .args(["--no-update", "--no-warnings", "--no-playlist", "-4", "-f", "bestaudio[acodec=opus]/bestaudio"])
        .args(["--print", "%(url)s\n%(filesize)s\n%(ext)s\n%(abr)s"])
        .arg(watch_url(id))
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("yt-dlp isn't installed ({e})"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut lines = text.lines();
    let (Some(url), Some(size), Some(ext), abr) = (lines.next(), lines.next(), lines.next(), lines.next()) else {
        let err = String::from_utf8_lossy(&out.stderr);
        let last = err.lines().rev().find(|l| l.contains("ERROR")).unwrap_or("YouTube didn't give this song");
        return Err(last.trim_start_matches("ERROR: ").to_string());
    };
    // the size is also in the address ("clen=")
    let size = size.parse().ok().or_else(|| {
        url.split(['?', '&']).find_map(|p| p.strip_prefix("clen=")).and_then(|n| n.parse().ok())
    });
    Ok(Stream {
        url: url.to_string(),
        size: size.ok_or("YouTube didn't say how long the song is")?,
        mime: if ext == "webm" { "audio/webm".into() } else { "audio/mp4".into() },
        kbps: abr.and_then(|a| a.parse::<f64>().ok()).map(|a| a.round() as u32).unwrap_or(0),
    })
}

/// The other version of a song: its official music video (for a song), or the
/// song itself (for a video). Found by search, kept only when the title and
/// the artist match (worker thread).
pub fn counterpart(t: &Track) -> Option<Track> {
    use crate::track::plain;
    let artist = crate::genre::artists(&t.artist).into_iter().next().unwrap_or_default();
    let (a, title) = (plain(&artist), plain(&t.title));
    if a.is_empty() || title.is_empty() {
        return None;
    }
    let found = if t.video { search(&format!("{artist} {}", t.title)) } else { search_videos(&format!("{artist} {}", t.title)) };
    found.into_iter().find(|c| {
        let (ct, who) = (plain(&c.title), plain(&format!("{} {}", c.artist, c.title)));
        let same_title = if t.video { title.contains(&ct) && !ct.is_empty() } else { ct.contains(&title) };
        c.id != t.id && same_title && who.contains(&a)
    })
}

/// A video's picture (H.264, up to 720p) and its sound, as two streams.
pub fn video_streams(id: &str) -> Result<(Stream, Stream), String> {
    let out = yt_dlp(false)
        .args(["--no-update", "--no-warnings", "--no-playlist", "-4"])
        .args(["-f", "bestvideo[height<=720][vcodec^=avc1][protocol=https]+bestaudio[acodec=opus]/bestvideo[height<=720][protocol=https]+bestaudio"])
        .args(["--print", "%(requested_formats.0.url)s\n%(requested_formats.0.filesize)s\n%(requested_formats.0.height)s"])
        .args(["--print", "%(requested_formats.1.url)s\n%(requested_formats.1.filesize)s\n%(requested_formats.1.ext)s\n%(requested_formats.1.abr)s"])
        .arg(format!("https://www.youtube.com/watch?v={id}"))
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("yt-dlp isn't installed ({e})"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let l: Vec<&str> = text.lines().collect();
    if l.len() < 7 {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(err.lines().rev().find(|x| x.contains("ERROR")).unwrap_or("YouTube didn't give this video").trim_start_matches("ERROR: ").to_string());
    }
    let size = |url: &str, given: &str| {
        given.parse().ok().or_else(|| url.split(['?', '&']).find_map(|p| p.strip_prefix("clen=")).and_then(|n| n.parse().ok()))
    };
    let video = Stream {
        url: l[0].into(),
        size: size(l[0], l[1]).ok_or("YouTube didn't say how big the video is")?,
        mime: "video/mp4".into(),
        kbps: l[2].parse().unwrap_or(0), // the height, for a video
    };
    let audio = Stream {
        url: l[3].into(),
        size: size(l[3], l[4]).ok_or("YouTube didn't say how big the sound is")?,
        mime: if l[5] == "webm" { "audio/webm".into() } else { "audio/mp4".into() },
        kbps: l[6].parse::<f64>().map(|a| a.round() as u32).unwrap_or(0),
    };
    Ok((video, audio))
}

/// Saves a liked song into `dir` as "<base>.opus" (no re-encoding: the stream as
/// YouTube sends it, with its tags). Runs for a few seconds to half a minute.
pub fn save(id: &str, dir: &Path, base: &str) -> Result<String, String> {
    let template = dir.join(format!("{base}.%(ext)s"));
    let out = yt_dlp(true)
        .args(["--no-update", "--no-warnings", "--no-playlist", "-q", "-4"])
        .args(["-f", "bestaudio[acodec=opus]/bestaudio", "--http-chunk-size", "1M"])
        .args(["-x", "--audio-format", "opus", "--embed-metadata"])
        .args(["--print", "after_move:filepath", "-o"])
        .arg(&template)
        .arg(watch_url(id))
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    let path = String::from_utf8_lossy(&out.stdout).lines().last().unwrap_or_default().trim().to_string();
    if out.status.success() && Path::new(&path).exists() {
        Ok(path)
    } else {
        Err(String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("saving failed").to_string())
    }
}

/// Saves a music video at 720p (H.264 when there is, cheap to play) as
/// "<base>.video.mkv" in `dir`, at most 1.5 MB/s so the connection stays usable.
/// Returns the file (worker thread; minutes).
pub fn save_video(id: &str, dir: &Path, base: &str) -> Result<String, String> {
    let template = dir.join(format!("{base}.video.%(ext)s"));
    let out = yt_dlp(true)
        .args(["--no-update", "--no-warnings", "--no-playlist", "-q", "-4", "--limit-rate", "1.5M"])
        .args(["-f", "bv*[height<=720][vcodec^=avc1]+ba[acodec=opus]/bv*[height<=720]+ba/b[height<=720]"])
        .args(["--merge-output-format", "mkv", "--http-chunk-size", "4M"])
        .args(["--print", "after_move:filepath", "-o"])
        .arg(&template)
        .arg(watch_url(id))
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    let path = String::from_utf8_lossy(&out.stdout).lines().last().unwrap_or_default().trim().to_string();
    if out.status.success() && Path::new(&path).exists() {
        Ok(path)
    } else {
        Err(String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("saving the video failed").to_string())
    }
}

/// What YouTube Music suggests for a search being typed (worker thread).
pub fn suggestions(q: &str) -> Vec<String> {
    let body = json!({ "context": { "client": { "clientName": "WEB_REMIX", "clientVersion": CLIENT_VERSION, "hl": "en" } }, "input": q });
    let Some(v) = net::post_json(
        "https://music.youtube.com/youtubei/v1/music/get_search_suggestions?prettyPrint=false",
        &body,
        &[("Origin", "https://music.youtube.com"), ("Referer", "https://music.youtube.com/")],
    ) else {
        return vec![];
    };
    let mut found = vec![];
    collect_key(&v, "searchSuggestionRenderer", &mut found);
    found
        .iter()
        .filter_map(|r| r["suggestion"]["runs"].as_array().map(|runs| runs.iter().filter_map(|x| x["text"].as_str()).collect::<String>()))
        .filter(|s| !s.trim().is_empty())
        .take(8)
        .collect()
}

/// The lyrics YouTube Music shows for a song (plain text, from LyricFind;
/// worker thread): its Lyrics tab, if it has one.
pub fn lyrics(id: &str) -> Option<String> {
    let context = json!({ "client": { "clientName": "WEB_REMIX", "clientVersion": CLIENT_VERSION, "hl": "en" } });
    let headers = [("Origin", "https://music.youtube.com"), ("Referer", "https://music.youtube.com/")];
    let next = net::post_json("https://music.youtube.com/youtubei/v1/next?prettyPrint=false", &json!({ "context": context, "videoId": id }), &headers)?;
    let mut ends = vec![];
    collect_key(&next, "browseEndpoint", &mut ends);
    let tab = ends.iter().filter_map(|e| e["browseId"].as_str()).find(|b| b.starts_with("MPLYt"))?.to_string();
    let page = net::post_json("https://music.youtube.com/youtubei/v1/browse?prettyPrint=false", &json!({ "context": context, "browseId": tab }), &headers)?;
    let mut shelves = vec![];
    collect_key(&page, "musicDescriptionShelfRenderer", &mut shelves);
    let text: String = shelves.first()?["description"]["runs"].as_array()?.iter().filter_map(|r| r["text"].as_str()).collect();
    (!text.trim().is_empty()).then_some(text)
}
