//! A tiny local web server between the player and YouTube.
//!
//! YouTube sends an audio stream fast only when the part wanted is in the address
//! ("&range=0-1048575"); a normal request (the whole file, or a Range header like
//! the player's) is slowed to a crawl or refused. So the player asks this relay
//! (http://127.0.0.1:<port>/<n>) like any web server, and the relay fetches the
//! stream from YouTube in 1 MB pieces the fast way. Seeking works too.
//!
//! On Windows it also carries Internet Archive files and a Radiance Music
//! Server's (`add_web`: ordinary Range requests): GStreamer's own TLS there (GnuTLS) can't check the Archive's
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
        // by the file's type (a Radiance Music Server's address ends in ?t=KEY)
        let path = url.split('?').next().unwrap_or(url).to_lowercase();
        let mime = match path.rsplit_once('.').map(|(_, e)| e) {
            Some("flac") => "audio/flac",
            Some("opus" | "ogg") => "audio/ogg",
            Some("m4a") => "audio/mp4",
            _ => "application/octet-stream",
        };
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
            let mut tries = 0;
            let mut resp = loop {
                let req = if in_address {
                    net::agent().get(&format!("{url}&range={pos}-{last}"))
                } else {
                    net::agent().get(&url).header("Range", &format!("bytes={pos}-{last}"))
                };
                match req.call() {
                    Ok(resp) => break resp,
                    Err(e) if tries < ASK_AGAIN_AFTER_MS.len() && worth_asking_again(&e) => {
                        crate::log::note(&format!("relay: {pos}-{last} of {size} ({mime}) refused ({e}): asking again"));
                        thread::sleep(std::time::Duration::from_millis(ASK_AGAIN_AFTER_MS[tries]));
                        tries += 1;
                    }
                    Err(e) => {
                        crate::log::note(&format!("relay: {pos}-{last} of {size} ({mime}) refused: {e}"));
                        return Err(std::io::Error::other(e.to_string()));
                    }
                }
            };
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

/// YouTube now and then refuses a stream's piece (403) or is busy (429, 5xx) and answers the same ask a moment
/// later: asked again after these waits before the song is given up (the beta skipped such songs to the next one;
/// Previous then played them, 2026-11-07).
const ASK_AGAIN_AFTER_MS: [u64; 2] = [400, 1000];

fn worth_asking_again(e: &ureq::Error) -> bool {
    matches!(e, ureq::Error::StatusCode(403 | 429 | 500..=599) | ureq::Error::Io(_) | ureq::Error::Timeout(_))
}

/// "bytes=100-" / "bytes=100-199".
fn parse_range(value: &str) -> Option<(u64, Option<u64>)> {
    let spec = value.strip_prefix("bytes=")?;
    let (a, b) = spec.split_once('-')?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A YouTube stand-in on 127.0.0.1: its first `refusals` asks get 403 (as googlevideo answers a stream's first
    /// piece now and then), the rest the part in their address (&range=a-b) of `body`.
    fn fake_youtube(body: Vec<u8>, refusals: usize) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let asked = Arc::new(AtomicUsize::new(0));
        let count = asked.clone();
        thread::spawn(move || {
            for conn in listener.incoming().flatten() {
                let n = count.fetch_add(1, Ordering::SeqCst);
                let mut reader = BufReader::new(conn.try_clone().unwrap());
                let mut first = String::new();
                let _ = reader.read_line(&mut first);
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
                        break;
                    }
                }
                let mut out = conn;
                if n < refusals {
                    let _ = out.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    continue;
                }
                let range = first.split("range=").nth(1).and_then(|r| r.split_whitespace().next()).unwrap_or("0-0");
                let (a, b) = range.split_once('-').unwrap();
                let (a, b): (usize, usize) = (a.parse().unwrap(), b.parse::<usize>().unwrap().min(body.len() - 1));
                let part = &body[a..=b];
                let _ = out.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", part.len()).as_bytes());
                let _ = out.write_all(part);
            }
        });
        (format!("http://127.0.0.1:{port}/videoplayback?id=1"), asked)
    }

    fn song(len: u32) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    /// The owner's beta skipped songs YouTube refused once (403 on the first piece: "won't play", the next song
    /// started; Previous played it). The relay asks again before giving up.
    #[test]
    fn a_refused_piece_is_asked_again() {
        let body = song(300_000);
        let (url, asked) = fake_youtube(body.clone(), 1);
        let relay = Relay::start().unwrap();
        let address = relay.add(url, body.len() as u64, "audio/webm".into());
        let got = net::agent().get(&address).call().unwrap().body_mut().read_to_vec().unwrap();
        assert_eq!(got.len(), body.len());
        assert!(got == body, "the song came back changed");
        assert!(asked.load(Ordering::SeqCst) >= 2, "it was never asked again");
    }

    /// A stream YouTube refuses every time still fails (the song is given up, not waited on forever).
    #[test]
    fn a_stream_refused_every_time_still_fails() {
        let body = song(200_000);
        let (url, asked) = fake_youtube(body.clone(), usize::MAX);
        let relay = Relay::start().unwrap();
        let address = relay.add(url, body.len() as u64, "audio/webm".into());
        let got = net::agent().get(&address).call().and_then(|mut r| r.body_mut().read_to_vec());
        assert!(got.map(|b| b.len() < body.len()).unwrap_or(true), "a refused stream came back whole");
        assert!(asked.load(Ordering::SeqCst) <= 3, "asked {} times", asked.load(Ordering::SeqCst));
    }
}
