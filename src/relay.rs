//! A tiny local web server between the player and YouTube.
//!
//! YouTube sends an audio stream fast only when the part wanted is in the address
//! ("&range=0-1048575"); a normal request (the whole file, or a Range header like
//! the player's) is slowed to a crawl or refused. So the player asks this relay
//! (http://127.0.0.1:<port>/<n>) like any web server, and the relay fetches the
//! stream from YouTube in 1 MB pieces the fast way. Seeking works too.
//!
//! On Windows it also carries Internet Archive files (`add_web`: ordinary Range
//! requests): GStreamer's own TLS there (GnuTLS) can't check the Archive's
//! certificate on a fresh Windows (its root comes from Windows only on demand), so
//! the player asks the relay over plain local HTTP and the app's own HTTPS fetches.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use ureq::ResponseExt;

use crate::net;

/// Each request starts with a small piece (the player often wants just a few
/// kilobytes there: a header, an index) and doubles up to 1 MB as it keeps reading.
const FIRST_PIECE: u64 = 128 << 10;
const PIECE: u64 = 1 << 20;

struct Upstream {
    url: String,
    /// 0: not known yet (a web file: learned at the first request)
    size: u64,
    mime: String,
    /// YouTube's way: the part wanted in the address (&range=), not a Range header
    in_address: bool,
}

#[derive(Clone)]
pub struct Relay {
    port: u16,
    streams: Arc<Mutex<HashMap<u64, Upstream>>>,
    next: Arc<Mutex<u64>>,
}

impl Relay {
    pub fn start() -> std::io::Result<Relay> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let relay = Relay {
            port: listener.local_addr()?.port(),
            streams: Arc::default(),
            next: Arc::new(Mutex::new(1)),
        };
        let r = relay.clone();
        thread::Builder::new().name("relay".into()).spawn(move || {
            for conn in listener.incoming().flatten() {
                let r = r.clone();
                thread::spawn(move || {
                    let _ = r.serve(conn);
                });
            }
        })?;
        Ok(relay)
    }

    /// The local address to play a YouTube stream from.
    pub fn add(&self, url: String, size: u64, mime: String) -> String {
        let mut next = self.next.lock().unwrap();
        let n = *next;
        *next += 1;
        let mut streams = self.streams.lock().unwrap();
        if streams.len() > 16 {
            // old songs: their addresses expire anyway
            let oldest = *streams.keys().min().unwrap();
            streams.remove(&oldest);
        }
        streams.insert(n, Upstream { url, size, mime, in_address: true });
        format!("http://127.0.0.1:{}/{n}", self.port)
    }

    /// A file on the web served with ordinary Range requests (the Internet Archive),
    /// through the relay: its local address. Its size is learned at the first request.
    pub fn add_web(&self, url: &str) -> String {
        let mut next = self.next.lock().unwrap();
        let n = *next;
        *next += 1;
        let mut streams = self.streams.lock().unwrap();
        if streams.len() > 16 {
            let oldest = *streams.keys().min().unwrap();
            streams.remove(&oldest);
        }
        let mime = if url.to_lowercase().ends_with(".flac") { "audio/flac" } else { "application/octet-stream" };
        streams.insert(n, Upstream { url: url.to_string(), size: 0, mime: mime.into(), in_address: false });
        format!("http://127.0.0.1:{}/{n}", self.port)
    }

    /// A web file's size, from a one-byte Range request ("Content-Range: bytes 0-0/SIZE"),
    /// and where it really is: the Internet Archive redirects every request to one of
    /// its servers, so the pieces after go straight there (a seek: one trip, not two).
    fn learn_size(url: &str) -> Option<(u64, String)> {
        let resp = net::agent().get(url).header("Range", "bytes=0-0").call().ok()?;
        let at = resp.get_uri().to_string();
        let total = resp.headers().get("content-range").and_then(|v| v.to_str().ok()?.rsplit('/').next()?.trim().parse().ok());
        let size = total.or_else(|| resp.headers().get("content-length")?.to_str().ok()?.parse().ok())?;
        Some((size, at))
    }

    fn serve(&self, conn: TcpStream) -> std::io::Result<()> {
        let mut reader = BufReader::new(conn.try_clone()?);
        let mut first = String::new();
        reader.read_line(&mut first)?;
        let mut range: Option<(u64, Option<u64>)> = None;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line)? == 0 || line.trim().is_empty() {
                break;
            }
            if let Some((name, value)) = line.split_once(':') {
                if name.trim().eq_ignore_ascii_case("range") {
                    range = parse_range(value.trim());
                }
            }
        }
        let mut out = conn;
        let mut parts = first.split_whitespace();
        let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
        let n: u64 = path.trim_start_matches('/').parse().unwrap_or(0);
        let (mut url, mut size, mime, in_address) = match self.streams.lock().unwrap().get(&n) {
            Some(u) => (u.url.clone(), u.size, u.mime.clone(), u.in_address),
            None => return out.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"),
        };
        if size == 0 {
            // a web file's size, learned once (here, off the window's thread)
            (size, url) = Self::learn_size(&url).filter(|(s, _)| *s > 0).ok_or_else(|| std::io::Error::other("no size"))?;
            if let Some(u) = self.streams.lock().unwrap().get_mut(&n) {
                u.size = size;
                u.url = url.clone();
            }
        }
        let (start, end) = match range {
            Some((s, e)) => (s, e.unwrap_or(size - 1).min(size - 1)),
            None => (0, size - 1),
        };
        if start >= size {
            let head = format!("HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{size}\r\nConnection: close\r\n\r\n");
            return out.write_all(head.as_bytes());
        }
        let head = if range.is_some() {
            format!(
                "HTTP/1.1 206 Partial Content\r\nContent-Type: {mime}\r\nAccept-Ranges: bytes\r\nContent-Range: bytes {start}-{end}/{size}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                end - start + 1
            )
        } else {
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nAccept-Ranges: bytes\r\nContent-Length: {size}\r\nConnection: close\r\n\r\n"
            )
        };
        if std::env::var_os("RADIANCE_DEBUG").is_some() {
            eprintln!("[relay] {method} {start}-{end} of {size} (range header: {})", range.is_some());
        }
        out.write_all(head.as_bytes())?;
        if method == "HEAD" {
            return Ok(());
        }
        // piece by piece; stops as soon as the player hangs up (a seek, the next song)
        let mut pos = start;
        let mut buf = vec![0u8; 64 * 1024];
        let mut piece = FIRST_PIECE;
        while pos <= end {
            let last = (pos + piece - 1).min(end);
            piece = (piece * 2).min(PIECE);
            let asked = std::time::Instant::now();
            let req = if in_address {
                net::agent().get(&format!("{url}&range={pos}-{last}"))
            } else {
                net::agent().get(&url).header("Range", &format!("bytes={pos}-{last}"))
            };
            let mut resp = req.call().map_err(|e| std::io::Error::other(e.to_string()))?;
            if std::env::var_os("RADIANCE_DEBUG").is_some() {
                eprintln!("[relay] {pos}-{last}: answered in {:.2}s", asked.elapsed().as_secs_f64());
            }
            let mut body = resp.body_mut().as_reader();
            loop {
                let n = body.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                out.write_all(&buf[..n])?;
                pos += n as u64;
            }
            if pos <= last {
                return Err(std::io::Error::other("YouTube sent less than asked"));
            }
        }
        Ok(())
    }
}

/// "bytes=100-" / "bytes=100-199".
fn parse_range(value: &str) -> Option<(u64, Option<u64>)> {
    let spec = value.strip_prefix("bytes=")?;
    let (a, b) = spec.split_once('-')?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()))
}
