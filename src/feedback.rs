//! "Are you enjoying Radiance Music?" (owner, 2026-11-01, radiance-music#24): a
//! pop-up now and then, Yes! or Not really and five ratings, sent to the admin
//! (admin.radianceph.com → Radiance Music → Feedback) with the person's Google
//! sign-in. When: only after 7 days and 30 songs played, then at random, at most
//! once every 30 days; never on the first open and never mid-song (the window
//! asks at a song's end or when paused). Not Now waits 30 days; Don't Ask Again
//! stops it for good. The admin's API is radianceph#42's contract.
//!
//! Sign-in on a computer is the browser's: the app opens the admin's sign-in page
//! with a PKCE challenge, the page sends the browser back to a one-time loopback
//! address here with a code, and the code (with the verifier) becomes the app's
//! token. The token is kept in the system's keyring (Secret Service on Linux,
//! Credential Manager on Windows), or in feedback.json (0600) where there's none.
//! HTTP and the loopback wait run on worker threads only.

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use gtk::glib;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::net;

/// The admin (RADIANCE_FEEDBACK_URL for tests: a fake server).
pub fn base() -> String {
    std::env::var("RADIANCE_FEEDBACK_URL").ok().filter(|s| !s.is_empty()).map(|s| s.trim_end_matches('/').to_string()).unwrap_or_else(|| "https://admin.radianceph.com".into())
}

/// What the form links to: what's kept, and why.
pub const PRIVACY: &str = "https://music.radianceph.com/privacy";
/// What Share Radiance Music shares.
pub const SITE: &str = "https://music.radianceph.com";

/// The five ratings, in the form's order, as the admin names them.
pub const AREAS: [(&str, &str); 5] = [("overall", "Overall"), ("sound", "Sound"), ("mixer", "Splitting & Mixer"), ("lyrics", "Lyrics"), ("sharing", "Sharing")];

/// Longest comment the admin takes.
pub const MOST_COMMENT: usize = 2000;

// ---- when to ask ----

const DAY: u64 = 86_400;
/// Not before a week of using it,
pub const FIRST_AFTER: u64 = 7 * DAY;
/// nor before this many songs,
pub const LEAST_PLAYS: usize = 30;
/// and then at most once a month (Not Now waits as long).
pub const EVERY: u64 = 30 * DAY;
/// Once it may ask: the chance each free moment (a song's end, a pause) does.
pub const CHANCE: f64 = 0.15;

/// What the timing goes by.
#[derive(Clone, Copy, Debug, Default)]
pub struct Use {
    /// The first song played (unix seconds), none yet: never.
    pub first_play: Option<u64>,
    pub plays: usize,
    /// When it last showed by itself, or Not Now was pressed.
    pub asked: Option<u64>,
    /// Don't Ask Again.
    pub never: bool,
    /// An answer was sent for this version already (the admin keeps one per person and version).
    pub sent_here: bool,
}

/// May it ask now (before the roll)?
pub fn due(u: &Use, now: u64) -> bool {
    !u.never
        && !u.sent_here
        && u.plays >= LEAST_PLAYS
        && u.first_play.is_some_and(|f| now.saturating_sub(f) >= FIRST_AFTER)
        && u.asked.is_none_or(|a| now.saturating_sub(a) >= EVERY)
}

/// Does it ask at this free moment? `roll`: 0–1, at random.
pub fn asks(u: &Use, now: u64, roll: f64) -> bool {
    due(u, now) && roll < CHANCE
}

/// This computer's use, as the timing needs it.
pub fn this_use() -> Use {
    let (plays, first) = crate::db::play_stats();
    let s = load();
    Use {
        first_play: first.map(|f| f.max(0) as u64),
        plays,
        asked: s["asked"].as_u64(),
        never: s["never"].as_bool().unwrap_or(false),
        sent_here: s["sent_version"].as_str() == Some(env!("CARGO_PKG_VERSION")),
    }
}

/// Is this free moment the one (the timing, then the roll)?
pub fn time_to_ask() -> bool {
    asks(&this_use(), now(), roll())
}

/// An answer went in: not again for this version, nor for 30 days.
pub fn sent() {
    let mut s = load();
    s["sent_version"] = json!(env!("CARGO_PKG_VERSION"));
    s["asked"] = json!(now());
    save(&s);
}

/// It showed by itself, or Not Now: not again for 30 days.
pub fn asked_now() {
    let mut s = load();
    s["asked"] = json!(now());
    save(&s);
}

/// Don't Ask Again: it never comes up by itself again (the menu still opens it).
pub fn never_again() {
    let mut s = load();
    s["never"] = json!(true);
    save(&s);
}

/// 0–1 at random.
pub fn roll() -> f64 {
    let b = random_bytes(4);
    u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64 / (u32::MAX as f64 + 1.0)
}

// ---- the answer ----

#[derive(Clone, Debug, PartialEq)]
pub struct Answer {
    pub enjoying: bool,
    /// 1–5 each, in AREAS' order.
    pub ratings: [u8; 5],
    pub comment: String,
}

/// Which kind of install this is, as the admin names it.
pub fn platform() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if std::env::var_os("FLATPAK_ID").is_some() || std::path::Path::new("/.flatpak-info").exists() {
        "flatpak"
    } else {
        "linux"
    }
}

/// The body of POST /api/music/feedback: the five ratings and nothing else, an empty comment left out.
pub fn body(a: &Answer, version: &str, platform: &str) -> Value {
    let mut ratings = serde_json::Map::new();
    for ((key, _), n) in AREAS.iter().zip(a.ratings) {
        ratings.insert((*key).into(), json!(n.clamp(1, 5)));
    }
    let mut v = json!({ "enjoying": a.enjoying, "ratings": ratings, "app_version": version, "platform": platform });
    let comment: String = a.comment.trim().chars().take(MOST_COMMENT).collect();
    if !comment.is_empty() {
        v["comment"] = json!(comment);
    }
    v
}

// ---- what went wrong, in words ----

#[derive(Clone, Debug, PartialEq)]
pub enum Failure {
    /// No answer at all: the internet (or the admin) is off.
    Offline,
    /// The sign-in doesn't work there anymore (or never finished): sign in again.
    SignInAgain,
    /// The admin turned the answer down, saying why.
    Refused(String),
    /// Too many tries: again after this many seconds.
    TooMany(u64),
    /// The browser's sign-in: cancelled, or it took too long.
    SignInStopped(String),
    /// The admin isn't taking these right now (switched off: 503).
    Unavailable,
    Other(String),
}

impl Failure {
    pub fn words(&self) -> String {
        match self {
            Failure::Offline => "You're offline, or Radiance Music's site didn't answer. Your answers are kept: try again in a moment.".into(),
            Failure::SignInAgain => "Your sign-in ended. Sign in with Google again to send.".into(),
            Failure::Refused(why) => why.clone(),
            Failure::TooMany(secs) => {
                let mins = secs.div_ceil(60).max(1);
                format!("That's a lot of tries. Try again in {mins} minute{}.", if mins == 1 { "" } else { "s" })
            }
            Failure::SignInStopped(why) => why.clone(),
            Failure::Unavailable => "Radiance Music's site isn't taking these right now. Try again later, or write to hello@radianceph.com.".into(),
            Failure::Other(why) => format!("It didn't go through ({why}). Try again in a moment."),
        }
    }
}

/// The admin's HTTP client (also Report a Problem's, report.rs).
pub(crate) fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::Agent::config_builder()
            .http_status_as_error(false)
            .user_agent(net::USER_AGENT)
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            .build()
            .into()
    })
}

/// The admin's answer: its JSON, or what went wrong in the person's words.
pub(crate) fn answer(resp: Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> Result<Value, Failure> {
    let mut resp = resp.map_err(|_| Failure::Offline)?;
    let status = resp.status().as_u16();
    let retry = resp.headers().get("retry-after").and_then(|v| v.to_str().ok()).and_then(|v| v.trim().parse::<u64>().ok());
    let body: Value = resp.body_mut().with_config().limit(1 << 20).read_json().unwrap_or(Value::Null);
    let said = body["message"].as_str().filter(|s| !s.is_empty()).map(str::to_string);
    match status {
        200..=299 => Ok(body),
        401 => Err(Failure::SignInAgain),
        400 if body["error"] == "invalid_grant" => Err(Failure::SignInStopped("That sign-in didn't finish. Try again.".into())),
        400 | 422 => Err(Failure::Refused(said.unwrap_or_else(|| "Some answers weren't right: check them and send again.".into()))),
        413 => Err(Failure::Refused(said.unwrap_or_else(|| "That's too long to send: make it shorter and send again.".into()))),
        429 => Err(Failure::TooMany(retry.unwrap_or(60))),
        503 => Err(Failure::Unavailable),
        _ => Err(Failure::Other(format!("the site said {status}"))),
    }
}

// ---- sign-in (PKCE, the browser and a loopback address) ----

/// One sign-in's secrets: the verifier stays here, the challenge goes to the page.
#[derive(Clone, Debug)]
pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
    pub state: String,
}

const VERIFIER_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";

pub fn pkce() -> Pkce {
    let verifier: String = random_bytes(64).iter().map(|b| VERIFIER_CHARS[*b as usize % VERIFIER_CHARS.len()] as char).collect();
    let state: String = random_bytes(16).iter().map(|b| format!("{b:02x}")).collect();
    Pkce { challenge: challenge(&verifier), verifier, state }
}

/// S256: the verifier's SHA-256, base64url without padding (43 characters).
pub fn challenge(verifier: &str) -> String {
    b64url(&Sha256::digest(verifier.as_bytes()))
}

fn b64url(bytes: &[u8]) -> String {
    const ABC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        for i in 0..=chunk.len() {
            out.push(ABC[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

fn random_bytes(n: usize) -> Vec<u8> {
    let mut b = vec![0u8; n];
    if getrandom::fill(&mut b).is_err() {
        // never seen; still not guessable from outside in the minutes a sign-in lives
        let mut seed = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos() as u64 ^ std::process::id() as u64;
        for x in b.iter_mut() {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            *x = seed as u8;
        }
    }
    b
}

/// The address the admin's page sends the browser back to: this computer only, its own port.
pub struct Loopback {
    listener: TcpListener,
    pub port: u16,
}

pub fn loopback() -> std::io::Result<Loopback> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let port = listener.local_addr()?.port();
    Ok(Loopback { listener, port })
}

/// The admin's sign-in page for this sign-in.
pub fn sign_in_url(p: &Pkce, port: u16) -> String {
    format!(
        "{}/music/sign-in?code_challenge={}&code_challenge_method=S256&state={}&redirect_uri={}",
        base(),
        p.challenge,
        p.state,
        enc(&format!("http://127.0.0.1:{port}/done"))
    )
}

/// Is the admin's Google sign-in on? Asked before the browser opens (a throwaway sign-in
/// of its own: the admin answers 503 "not ready yet" while Google isn't set up there), so
/// the pop-up says so instead of leaving someone at that page and waiting.
pub fn sign_in_ready(port: u16) -> Result<(), Failure> {
    static NO_FOLLOW: OnceLock<ureq::Agent> = OnceLock::new();
    let agent = NO_FOLLOW.get_or_init(|| {
        ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .user_agent(net::USER_AGENT)
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_recv_response(Some(Duration::from_secs(20)))
            .build()
            .into()
    });
    let resp = agent.get(&sign_in_url(&pkce(), port)).call().map_err(|_| Failure::Offline)?;
    match resp.status().as_u16() {
        503 => Err(Failure::SignInStopped("Signing in to Radiance Music isn't set up yet. Your answers are kept: try again later.".into())),
        429 => Err(Failure::TooMany(resp.headers().get("retry-after").and_then(|v| v.to_str().ok()).and_then(|v| v.trim().parse().ok()).unwrap_or(60))),
        _ => Ok(()),
    }
}

/// How long the browser's sign-in may take (the admin's own lasts 10 minutes).
pub const SIGN_IN_WAIT: Duration = Duration::from_secs(10 * 60);

impl Loopback {
    /// Waits for the browser to come back with the code (or a "no"), then sends it on to the
    /// admin's "you can go back to the app" page. `stop`: Cancel was pressed.
    pub fn wait(&self, state: &str, stop: &AtomicBool, wait: Duration) -> Result<String, Failure> {
        let until = Instant::now() + wait;
        while Instant::now() < until {
            if stop.load(Ordering::Relaxed) {
                return Err(Failure::SignInStopped("Sign-in cancelled.".into()));
            }
            match self.listener.accept() {
                Ok((stream, _)) => {
                    if let Some(got) = self.handle(stream, state) {
                        return got;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(150)),
                Err(_) => std::thread::sleep(Duration::from_millis(150)),
            }
        }
        Err(Failure::SignInStopped("The sign-in took too long. Try again.".into()))
    }

    /// One request to the loopback: the sign-in's answer (Some), or anything else (None: a favicon).
    fn handle(&self, mut stream: TcpStream, state: &str) -> Option<Result<String, Failure>> {
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let mut head = Vec::new();
        let mut buf = [0u8; 1024];
        while !head.windows(4).any(|w| w == b"\r\n\r\n") && head.len() < 16 * 1024 {
            match stream.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => head.extend_from_slice(&buf[..n]),
            }
        }
        let line = String::from_utf8_lossy(&head).lines().next().unwrap_or_default().to_string();
        let target = line.split_whitespace().nth(1).unwrap_or_default().to_string();
        let (path, query) = target.split_once('?').unwrap_or((&target, ""));
        if path != "/done" {
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            return None;
        }
        let q = |k: &str| query.split('&').filter_map(|kv| kv.split_once('=')).find(|(name, _)| *name == k).map(|(_, v)| decode(v));
        let got = if q("state").as_deref() != Some(state) {
            Err(Failure::SignInStopped("That sign-in wasn't this one. Try again.".into()))
        } else if let Some(code) = q("code").filter(|c| !c.is_empty()) {
            Ok(code)
        } else if q("error").as_deref() == Some("access_denied") {
            Err(Failure::SignInStopped("Sign-in cancelled. Your answers are kept.".into()))
        } else {
            Err(Failure::SignInStopped("Google's sign-in didn't finish. Try again in a moment.".into()))
        };
        let next = format!("{}/music/signed-in{}", base(), if got.is_ok() { "" } else { "?error=1" });
        let _ = stream.write_all(format!("HTTP/1.1 302 Found\r\nLocation: {next}\r\nContent-Length: 0\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n").as_bytes());
        Some(got)
    }
}

/// The signed-in person (their token is the app's key to send).
#[derive(Clone, Debug, PartialEq)]
pub struct Account {
    pub token: String,
    pub name: String,
    pub email: String,
}

fn account_of(v: &Value) -> Result<Account, Failure> {
    let token = v["token"].as_str().unwrap_or_default().to_string();
    if token.is_empty() {
        return Err(Failure::Other("no sign-in came back".into()));
    }
    Ok(Account { token, name: v["name"].as_str().unwrap_or_default().into(), email: v["email"].as_str().unwrap_or_default().into() })
}

/// The browser's code and this sign-in's verifier → the app's token.
pub fn exchange(code: &str, verifier: &str) -> Result<Account, Failure> {
    account_of(&answer(agent().post(&format!("{}/api/music/token", base())).send_json(json!({ "code": code, "code_verifier": verifier })))?)
}

/// Sends the answer: Ok(share) (true: four or five stars overall, so Share Radiance Music is offered).
pub fn send(token: &str, a: &Answer) -> Result<bool, Failure> {
    let v = answer(
        agent()
            .post(&format!("{}/api/music/feedback", base()))
            .header("Authorization", &format!("Bearer {token}"))
            .send_json(body(a, env!("CARGO_PKG_VERSION"), platform())),
    )?;
    Ok(v["share"].as_bool().unwrap_or(false))
}

/// Signs this computer out there too (best effort: it's forgotten here either way).
pub fn sign_out_there(token: &str) {
    let _ = agent().delete(&format!("{}/api/music/token", base())).header("Authorization", &format!("Bearer {token}")).call();
}

// ---- what it remembers: feedback.json beside settings.json, the token in the keyring ----

fn state_path() -> PathBuf {
    glib::user_config_dir().join("radiance-music").join("feedback.json")
}

fn load() -> Value {
    fs::read_to_string(state_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(|| json!({}))
}

fn save(v: &Value) {
    let path = state_path();
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("json.part");
    if fs::write(&tmp, serde_json::to_vec_pretty(v).unwrap_or_default()).is_ok() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
        }
        let _ = fs::rename(&tmp, &path);
    }
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

const KEYRING_SERVICE: &str = "io.github.radianceadm.Music";
const KEYRING_USER: &str = "feedback sign-in";

fn keyring() -> Option<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).ok()
}

/// Who's signed in here, if anyone (call off the window's thread: the keyring may take a moment).
pub fn signed_in() -> Option<Account> {
    let s = load();
    let token = keyring().and_then(|k| k.get_password().ok()).or_else(|| s["token"].as_str().map(str::to_string)).filter(|t| !t.is_empty())?;
    Some(Account { token, name: s["name"].as_str().unwrap_or_default().into(), email: s["email"].as_str().unwrap_or_default().into() })
}

/// Keeps a sign-in: the token in the keyring (the file only where there's no keyring).
pub fn keep(a: &Account) {
    let mut s = load();
    s["name"] = json!(a.name);
    s["email"] = json!(a.email);
    match keyring().map(|k| k.set_password(&a.token)) {
        Some(Ok(())) => {
            if let Some(o) = s.as_object_mut() {
                o.remove("token");
            }
        }
        _ => {
            crate::log::note("feedback: no keyring here, the sign-in is kept in feedback.json (this user only)");
            s["token"] = json!(a.token);
        }
    }
    save(&s);
}

/// Forgets the sign-in here (Sign Out, or the admin no longer takes it).
pub fn forget() {
    if let Some(k) = keyring() {
        let _ = k.delete_credential();
    }
    let mut s = load();
    if let Some(o) = s.as_object_mut() {
        for k in ["token", "name", "email"] {
            o.remove(k);
        }
    }
    save(&s);
}

fn enc(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => match (hex(b[i + 1]), hex(b[i + 2])) {
                (Some(hi), Some(lo)) => {
                    out.push(hi << 4 | lo);
                    i += 2;
                }
                _ => out.push(b'%'),
            },
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(c: u8) -> Option<u8> {
    (c as char).to_digit(16).map(|d| d as u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    const NOW: u64 = 1_800_000_000;

    #[test]
    fn asks_only_after_a_week_and_thirty_songs_then_once_a_month() {
        let used = Use { first_play: Some(NOW - 8 * DAY), plays: 30, asked: None, never: false, sent_here: false };
        assert!(due(&used, NOW));
        assert!(!due(&Use { plays: 29, ..used }, NOW), "29 songs");
        assert!(!due(&Use { first_play: Some(NOW - 6 * DAY), ..used }, NOW), "6 days");
        assert!(!due(&Use { first_play: None, ..used }, NOW), "nothing played");
        assert!(!due(&Use { asked: Some(NOW - 29 * DAY), ..used }, NOW), "asked 29 days ago");
        assert!(due(&Use { asked: Some(NOW - 30 * DAY), ..used }, NOW), "asked 30 days ago");
        assert!(!due(&Use { never: true, ..used }, NOW), "Don't Ask Again");
        assert!(!due(&Use { sent_here: true, ..used }, NOW), "answered for this version");
        // at random once due
        assert!(asks(&used, NOW, 0.0) && !asks(&used, NOW, 0.99));
        assert!(!asks(&Use { plays: 3, ..used }, NOW, 0.0));
    }

    #[test]
    fn rolls_are_between_zero_and_one() {
        for _ in 0..200 {
            let r = roll();
            assert!((0.0..1.0).contains(&r));
        }
    }

    #[test]
    fn pkce_as_the_admin_checks_it() {
        // RFC 7636's own example (Appendix B)
        assert_eq!(challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
        let p = pkce();
        assert_eq!(p.verifier.len(), 64);
        assert!(p.verifier.bytes().all(|b| VERIFIER_CHARS.contains(&b)));
        assert_eq!(p.challenge.len(), 43);
        assert_eq!(p.challenge, challenge(&p.verifier));
        assert_ne!(pkce().state, p.state);
        let url = sign_in_url(&p, 41234);
        assert!(url.contains("/music/sign-in?code_challenge="));
        assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A41234%2Fdone"));
        assert!(url.contains(&format!("state={}", p.state)));
    }

    #[test]
    fn the_body_has_exactly_what_the_admin_takes() {
        let a = Answer { enjoying: true, ratings: [5, 4, 9, 0, 3], comment: "  Love the Mixer  ".into() };
        let v = body(&a, "0.7.14", "linux");
        assert_eq!(v["enjoying"], true);
        assert_eq!(v["ratings"], json!({"overall": 5, "sound": 4, "mixer": 5, "lyrics": 1, "sharing": 3}));
        assert_eq!(v["comment"], "Love the Mixer");
        assert_eq!(v["app_version"], "0.7.14");
        assert_eq!(v["platform"], "linux");
        assert_eq!(v.as_object().unwrap().len(), 5);
        let empty = body(&Answer { comment: "   ".into(), ..a.clone() }, "0.7.14", "windows");
        assert!(empty.get("comment").is_none());
        let long = body(&Answer { comment: "é".repeat(2500), ..a }, "0.7.14", "flatpak");
        assert_eq!(long["comment"].as_str().unwrap().chars().count(), MOST_COMMENT);
    }

    /// A fake admin: answers each request in turn with (status, extra headers, body), and keeps what it got.
    fn fake_admin(answers: Vec<(u16, &'static str, Value)>) -> (String, std::sync::mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for (status, headers, body) in answers {
                let Ok((mut s, _)) = listener.accept() else { return };
                let mut got = Vec::new();
                let mut buf = [0u8; 4096];
                loop {
                    let n = s.read(&mut buf).unwrap_or(0);
                    got.extend_from_slice(&buf[..n]);
                    let text = String::from_utf8_lossy(&got).to_string();
                    if let Some(at) = text.find("\r\n\r\n") {
                        let head = text[..at].to_ascii_lowercase();
                        let len = head.lines().find_map(|l| l.strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0))).unwrap_or(0);
                        let chunked = head.contains("transfer-encoding: chunked");
                        if (chunked && text[at + 4..].contains("0\r\n\r\n")) || (!chunked && got.len() >= at + 4 + len) {
                            break;
                        }
                    }
                    if n == 0 {
                        break;
                    }
                }
                let _ = tx.send(String::from_utf8_lossy(&got).to_string());
                let text = body.to_string();
                let _ = s.write_all(format!("HTTP/1.1 {status} X\r\nContent-Type: application/json\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len()).as_bytes());
            }
        });
        (address, rx)
    }

    /// A request's JSON body.
    fn json_of(req: &str) -> Value {
        req.split_once("\r\n\r\n").and_then(|(_, b)| serde_json::from_str(b).ok()).unwrap_or(Value::Null)
    }

    /// RADIANCE_FEEDBACK_URL is process-wide: the tests that set it take turns.
    fn with_admin<T>(address: &str, f: impl FnOnce() -> T) -> T {
        static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
        unsafe { std::env::set_var("RADIANCE_FEEDBACK_URL", address) };
        let out = f();
        unsafe { std::env::remove_var("RADIANCE_FEEDBACK_URL") };
        out
    }

    #[test]
    fn the_whole_computer_sign_in_against_a_fake_admin() {
        let (admin, got) = fake_admin(vec![(200, "", json!({"token": "rmu_abc", "name": "Bassi", "email": "b@example.com"}))]);
        with_admin(&admin, || {
            let p = pkce();
            let lb = loopback().unwrap();
            assert!(sign_in_url(&p, lb.port).starts_with(&format!("{admin}/music/sign-in?")));
            // the browser, coming back from the admin with the code
            let (port, state) = (lb.port, p.state.clone());
            let browser = std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(300));
                let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
                s.write_all(b"GET /favicon.ico HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
                let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
                s.write_all(format!("GET /done?code=C0DE%2D1&state={state} HTTP/1.1\r\nHost: x\r\n\r\n").as_bytes()).unwrap();
                let mut back = String::new();
                let _ = s.read_to_string(&mut back);
                back
            });
            let code = lb.wait(&p.state, &AtomicBool::new(false), Duration::from_secs(10)).unwrap();
            assert_eq!(code, "C0DE-1");
            let back = browser.join().unwrap();
            assert!(back.starts_with("HTTP/1.1 302") && back.contains(&format!("Location: {admin}/music/signed-in\r\n")), "{back}");
            let acct = exchange(&code, &p.verifier).unwrap();
            assert_eq!(acct, Account { token: "rmu_abc".into(), name: "Bassi".into(), email: "b@example.com".into() });
            let req = got.recv().unwrap();
            assert!(req.starts_with("POST /api/music/token "), "{req}");
            assert_eq!(json_of(&req), json!({"code": "C0DE-1", "code_verifier": p.verifier}), "{req}");
        });
    }

    #[test]
    fn a_cancelled_or_foreign_sign_in_and_cancel_and_time_out() {
        let lb = loopback().unwrap();
        let port = lb.port;
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
            s.write_all(b"GET /done?error=access_denied&state=mine HTTP/1.1\r\n\r\n").unwrap();
            let mut back = String::new();
            let _ = s.read_to_string(&mut back);
            assert!(back.contains("/music/signed-in?error=1"), "{back}");
        });
        assert!(matches!(lb.wait("mine", &AtomicBool::new(false), Duration::from_secs(10)), Err(Failure::SignInStopped(w)) if w.contains("cancelled")));
        let lb = loopback().unwrap();
        let port = lb.port;
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
            s.write_all(b"GET /done?code=x&state=someone-elses HTTP/1.1\r\n\r\n").unwrap();
        });
        assert!(matches!(lb.wait("mine", &AtomicBool::new(false), Duration::from_secs(10)), Err(Failure::SignInStopped(w)) if w.contains("wasn't this one")));
        let stop = Arc::new(AtomicBool::new(true));
        assert!(matches!(loopback().unwrap().wait("mine", &stop, Duration::from_secs(10)), Err(Failure::SignInStopped(w)) if w.contains("cancelled")));
        assert!(matches!(loopback().unwrap().wait("mine", &AtomicBool::new(false), Duration::from_millis(300)), Err(Failure::SignInStopped(w)) if w.contains("too long")));
    }

    #[test]
    fn sending_and_what_each_answer_means() {
        let a = Answer { enjoying: true, ratings: [5, 5, 4, 4, 5], comment: String::new() };
        let (admin, got) = fake_admin(vec![
            (200, "", json!({"ok": true, "share": true})),
            (200, "", json!({"ok": true, "share": false})),
            (401, "WWW-Authenticate: Bearer\r\n", json!({"error": "unauthenticated"})),
            (422, "", json!({"message": "The sound rating must be between 1 and 5.", "errors": {"ratings.sound": ["…"]}})),
            (429, "Retry-After: 125\r\n", json!({"message": "Too Many Attempts."})),
            (400, "", json!({"error": "invalid_grant", "message": "The code is used."})),
            (503, "", json!({})),
        ]);
        with_admin(&admin, || {
            assert_eq!(send("rmu_abc", &a), Ok(true));
            let req = got.recv().unwrap();
            assert!(req.starts_with("POST /api/music/feedback "), "{req}");
            assert!(req.to_ascii_lowercase().contains("authorization: bearer rmu_abc"), "{req}");
            assert_eq!(json_of(&req), body(&a, env!("CARGO_PKG_VERSION"), platform()), "{req}");
            assert_eq!(send("rmu_abc", &a), Ok(false));
            assert_eq!(send("rmu_old", &a), Err(Failure::SignInAgain));
            assert_eq!(send("rmu_abc", &a), Err(Failure::Refused("The sound rating must be between 1 and 5.".into())));
            assert_eq!(send("rmu_abc", &a), Err(Failure::TooMany(125)));
            assert!(Failure::TooMany(125).words().contains("3 minutes"));
            assert!(matches!(exchange("used", "v"), Err(Failure::SignInStopped(_))));
            // switched off on the admin (503): said in plain words, not as a status number
            assert_eq!(send("rmu_abc", &a), Err(Failure::Unavailable));
            assert!(Failure::Unavailable.words().contains("isn't taking these right now"));
        });
        // the admin's sign-in still off (503 "not ready yet"), then on (a redirect to Google)
        let (admin, _got) = fake_admin(vec![(503, "", json!({})), (302, "Location: https://accounts.google.com/o/oauth2/v2/auth\r\n", json!({}))]);
        with_admin(&admin, || {
            assert!(matches!(sign_in_ready(41234), Err(Failure::SignInStopped(w)) if w.contains("isn't set up yet")));
            assert_eq!(sign_in_ready(41234), Ok(()));
        });
        // nobody there: offline
        let free = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        with_admin(&format!("http://127.0.0.1:{free}"), || assert_eq!(send("rmu_abc", &a), Err(Failure::Offline)));
    }
}
