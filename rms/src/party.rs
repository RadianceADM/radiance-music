//! The listening party (owner's choice, 2026-10-31): everyone in it hears the
//! same song at the same moment, with a shared queue that people add to. The
//! host plays, pauses and skips (or everyone, as the host allows); others react.
//!
//! The server keeps the clock and the queue. A playing song has the server
//! time its start was (or will be) at, `started_at_server_ms`; a paused one,
//! where it stopped, `paused_at_ms`. Each listener keeps its own offset to the
//! server's clock (a few /api/time pings, NTP-style), plays the same file from
//! the library and nudges itself to stay together. A late joiner reads the
//! state and starts mid-song where everyone is:
//!
//!   position_ms = (local now + offset) - started_at_server_ms
//!
//! A change starts LEAD_MS after it's made (the start can be a little ahead),
//! so every listener has it in time and they start together.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;
use axum::Json;
use futures_util::{SinkExt, StreamExt};
use rusqlite::params;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::{watch, Notify};

use crate::auth::{Listener, Who};
use crate::{bad, conflict, forbidden, library, not_found, now_ms, ApiError, ApiResult, Shared};

/// A change (play, skip, seek…) starts this long after it's made.
pub const LEAD_MS: i64 = 800;
/// Reactions kept in the state (and for how long).
const REACTIONS: usize = 20;
const REACTION_MS: i64 = 60_000;

#[derive(Clone, Debug)]
struct Person {
    id: i64,
    name: String,
}

impl Person {
    fn of(who: &Who) -> Person {
        Person { id: who.person, name: who.name.clone() }
    }

    fn json(&self) -> Value {
        json!({ "id": self.id, "name": self.name })
    }
}

#[derive(Clone, Debug)]
struct Entry {
    id: u64,
    song: i64,
    artist: String,
    title: String,
    file: String,
    cover: Option<String>,
    duration_ms: Option<i64>,
    added_by: Person,
}

impl Entry {
    fn json(&self) -> Value {
        json!({
            "entry": self.id,
            "id": self.song,
            "artist": self.artist,
            "title": self.title,
            "file": self.file,
            "url": library::url(&self.file),
            "cover": self.cover.as_deref().map(library::url),
            "duration_ms": self.duration_ms,
            "added_by": self.added_by.json(),
        })
    }
}

struct Reaction {
    emoji: String,
    by: Person,
    at: i64,
}

struct PartyState {
    id: String,
    host: Person,
    everyone_controls: bool,
    song: Option<Entry>,
    playing: bool,
    started_at: i64,
    paused_at: i64,
    queue: Vec<Entry>,
    reactions: VecDeque<Reaction>,
    next_entry: u64,
    started_ms: i64,
}

impl PartyState {
    fn position(&self, now: i64) -> i64 {
        if self.playing { now - self.started_at } else { self.paused_at }
    }

    /// The next song in the queue starts (in LEAD_MS); none left: quiet, the party stays on.
    fn advance(&mut self, now: i64) {
        if self.queue.is_empty() {
            self.song = None;
            self.playing = false;
            self.paused_at = 0;
        } else {
            self.song = Some(self.queue.remove(0));
            self.playing = true;
            self.started_at = now + LEAD_MS;
            self.paused_at = 0;
        }
    }

    fn can_control(&self, who: &Who) -> bool {
        who.person == self.host.id || self.everyone_controls
    }
}

struct Member {
    person: i64,
    name: String,
}

/// The one party a server holds at a time, and who's listening.
pub struct Party {
    state: Mutex<Option<PartyState>>,
    members: Mutex<HashMap<u64, Member>>,
    next_conn: AtomicU64,
    /// Every change bumps it; each listener's WebSocket sends the state again.
    version: watch::Sender<u64>,
    /// A device or friend was removed: listeners check they're still allowed.
    recheck: watch::Sender<u64>,
    /// The song clock looks again (something changed).
    clock: Notify,
    last_reaction: Mutex<HashMap<i64, i64>>,
}

impl Default for Party {
    fn default() -> Self {
        Party {
            state: Mutex::new(None),
            members: Mutex::new(HashMap::new()),
            next_conn: AtomicU64::new(1),
            version: watch::channel(0).0,
            recheck: watch::channel(0).0,
            clock: Notify::new(),
            last_reaction: Mutex::new(HashMap::new()),
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Party {
    fn changed(&self) {
        self.version.send_modify(|v| *v += 1);
        self.clock.notify_one();
    }

    /// Removed devices and friends leave the party at once.
    pub fn recheck_listeners(&self, _app: &Shared) {
        self.recheck.send_modify(|v| *v += 1);
    }

    /// The party as everyone sees it, now.
    pub fn snapshot(&self) -> Value {
        let now = now_ms();
        let members: Vec<Value> = {
            let m = lock(&self.members);
            let mut seen = std::collections::BTreeMap::new();
            for x in m.values() {
                seen.entry(x.person).or_insert_with(|| (x.name.clone(), 0)).1 += 1;
            }
            let host = lock(&self.state).as_ref().map(|s| s.host.id);
            seen.into_iter().map(|(id, (name, devices))| json!({ "id": id, "name": name, "devices": devices, "host": Some(id) == host })).collect()
        };
        let state = lock(&self.state);
        let Some(s) = state.as_ref() else {
            return json!({ "type": "state", "on": false, "server_ms": now, "members": members, "version": *self.version.borrow() });
        };
        json!({
            "type": "state",
            "on": true,
            "id": s.id,
            "host": s.host.json(),
            "everyone_controls": s.everyone_controls,
            "song": s.song.as_ref().map(Entry::json),
            "playing": s.playing && s.song.is_some(),
            "started_at_server_ms": (s.playing && s.song.is_some()).then_some(s.started_at),
            "paused_at_ms": (!s.playing && s.song.is_some()).then_some(s.paused_at),
            "position_ms": s.song.as_ref().map(|_| s.position(now)),
            "server_ms": now,
            "lead_ms": LEAD_MS,
            "queue": s.queue.iter().map(Entry::json).collect::<Vec<_>>(),
            "members": members,
            "reactions": s.reactions.iter().filter(|r| now - r.at < REACTION_MS).map(|r| json!({ "emoji": r.emoji, "by": r.by.json(), "at_server_ms": r.at })).collect::<Vec<_>>(),
            "started_ms": s.started_ms,
            "version": *self.version.borrow(),
        })
    }

    /// When the song playing now ends (server time), if it's known.
    fn song_end(&self) -> Option<(i64, u64)> {
        let state = lock(&self.state);
        let s = state.as_ref()?;
        let song = s.song.as_ref()?;
        (s.playing).then_some(())?;
        Some((s.started_at + song.duration_ms.filter(|d| *d > 0)?, song.id))
    }

    /// The song `entry` reached its end: the next one starts.
    fn ended(&self, entry: u64) {
        {
            let mut state = lock(&self.state);
            let Some(s) = state.as_mut() else { return };
            if s.song.as_ref().map(|e| e.id) != Some(entry) || !s.playing {
                return;
            }
            s.advance(now_ms());
        }
        self.changed();
    }

    fn join(&self, who: &Who) -> u64 {
        let id = self.next_conn.fetch_add(1, Ordering::Relaxed);
        lock(&self.members).insert(id, Member { person: who.person, name: who.name.clone() });
        self.changed();
        id
    }

    fn leave(&self, conn: u64) {
        lock(&self.members).remove(&conn);
        self.changed();
    }

    fn with<T>(&self, f: impl FnOnce(&mut PartyState) -> Result<T, ApiError>) -> Result<T, ApiError> {
        let out = {
            let mut state = lock(&self.state);
            let s = state.as_mut().ok_or_else(|| not_found("No party is on right now: start one first."))?;
            f(s)?
        };
        self.changed();
        Ok(out)
    }
}

/// The song clock: when a playing song reaches its end, the next one starts.
pub async fn clock(app: Shared) {
    loop {
        match app.party.song_end() {
            Some((at, entry)) => {
                let wait = (at - now_ms()).max(0) as u64;
                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_millis(wait)) => app.party.ended(entry),
                    _ = app.party.clock.notified() => {}
                }
            }
            None => app.party.clock.notified().await,
        }
    }
}

/// GET /api/time: the server's clock in ms, for the apps' clock-offset pings
/// (no key needed: it says nothing but the time).
pub async fn time() -> Json<Value> {
    Json(json!({ "now_ms": now_ms() }))
}

/// GET /api/party: the party now (`on: false` when there's none).
pub async fn state(State(app): State<Shared>, _who: Who) -> Json<Value> {
    Json(app.party.snapshot())
}

#[derive(Deserialize, Default)]
pub struct StartBody {
    #[serde(default)]
    everyone_controls: bool,
}

/// POST /api/party/start: anyone can start one (one at a time on a server).
pub async fn start(State(app): State<Shared>, who: Who, body: axum::body::Bytes) -> ApiResult<(StatusCode, Json<Value>)> {
    if !who.can_host {
        return Err(crate::auth::switched_off(&app, "hosting parties"));
    }
    // the body may be left out: {everyone_controls: false}
    let body: StartBody = if body.iter().all(u8::is_ascii_whitespace) { StartBody::default() } else { serde_json::from_slice(&body).map_err(|e| bad(format!("That isn't the party's settings: {e}")))? };
    {
        let mut state = lock(&app.party.state);
        if let Some(s) = state.as_ref() {
            return Err(conflict(format!("A party is on already, hosted by {}: join it.", s.host.name)));
        }
        let now = now_ms();
        *state = Some(PartyState {
            id: format!("p{now:x}"),
            host: Person::of(&who),
            everyone_controls: body.everyone_controls,
            song: None,
            playing: false,
            started_at: 0,
            paused_at: 0,
            queue: vec![],
            reactions: VecDeque::new(),
            next_entry: 1,
            started_ms: now,
        });
    }
    crate::log(&format!("{} started a listening party", who.name));
    app.party.changed();
    Ok((StatusCode::CREATED, Json(app.party.snapshot())))
}

#[derive(Deserialize)]
pub struct AddBody {
    song: Value,
}

/// POST /api/party/add {song}: anyone in the party adds a song from the
/// library to the queue (it starts at once when nothing is playing).
pub async fn add(State(app): State<Shared>, who: Who, Json(body): Json<AddBody>) -> ApiResult<Json<Value>> {
    let song = library::resolve(&app, &body.song)?;
    let dir = app.cfg.library();
    let file = song.file.clone();
    let found = tokio::task::spawn_blocking(move || library::found(&dir, &file)).await.ok().flatten().ok_or_else(|| not_found("That song's file isn't in the library anymore."))?;
    let duration = match song.duration_ms {
        Some(d) => Some(d),
        None => {
            let p = app.cfg.library().join(&song.file);
            let d = tokio::task::spawn_blocking(move || crate::media::duration_ms(&p)).await.ok().flatten();
            if let Some(d) = d {
                let _ = app.db().execute("UPDATE songs SET duration_ms = ?1 WHERE id = ?2", params![d, song.id]);
            }
            d
        }
    };
    app.party.with(|s| {
        let entry = Entry {
            id: s.next_entry,
            song: song.id,
            artist: song.artist.clone(),
            title: song.title.clone(),
            file: song.file.clone(),
            cover: found.files.get("cover").map(|(n, _)| n.clone()),
            duration_ms: duration,
            added_by: Person::of(&who),
        };
        s.next_entry += 1;
        s.queue.push(entry);
        if s.song.is_none() {
            s.advance(now_ms());
        }
        Ok(())
    })?;
    Ok(Json(app.party.snapshot()))
}

#[derive(Deserialize)]
pub struct ControlBody {
    action: String,
    #[serde(default)]
    position_ms: Option<i64>,
    #[serde(default)]
    entry: Option<u64>,
    #[serde(default)]
    to: Option<usize>,
    #[serde(default)]
    everyone: Option<bool>,
}

/// POST /api/party/control {action}: play | pause | skip | seek {position_ms}
/// | reorder {entry, to} | remove {entry} — the host, or everyone when the
/// host allows; allow {everyone: true|false} — the host only.
pub async fn control(State(app): State<Shared>, who: Who, Json(body): Json<ControlBody>) -> ApiResult<Json<Value>> {
    app.party.with(|s| {
        let now = now_ms();
        let host_only = body.action == "allow";
        // a song you added you can take out yourself
        let own_entry = body.action == "remove" && s.queue.iter().any(|e| Some(e.id) == body.entry && e.added_by.id == who.person);
        if host_only && who.person != s.host.id {
            return Err(forbidden(format!("Only the host ({}) can choose who controls the party.", s.host.name)));
        }
        if !s.can_control(&who) && !own_entry {
            return Err(forbidden(format!("Only the host ({}) controls the party; they can let everyone control it.", s.host.name)));
        }
        let length = s.song.as_ref().and_then(|e| e.duration_ms);
        match body.action.as_str() {
            "play" => {
                if s.song.is_none() {
                    if s.queue.is_empty() {
                        return Err(conflict("The queue is empty: add a song first."));
                    }
                    s.advance(now);
                } else if !s.playing {
                    s.started_at = now + LEAD_MS - s.paused_at;
                    s.playing = true;
                }
            }
            "pause" => {
                if s.playing && s.song.is_some() {
                    let at = (now - s.started_at).max(0);
                    s.paused_at = length.map_or(at, |l| at.min(l));
                    s.playing = false;
                }
            }
            "skip" => s.advance(now),
            "seek" => {
                if s.song.is_none() {
                    return Err(conflict("Nothing is playing."));
                }
                let pos = body.position_ms.ok_or_else(|| bad("Say where to (position_ms)."))?.max(0);
                let pos = length.map_or(pos, |l| pos.min(l));
                if s.playing {
                    s.started_at = now + LEAD_MS - pos;
                } else {
                    s.paused_at = pos;
                }
            }
            "reorder" => {
                let entry = body.entry.ok_or_else(|| bad("Say which song (entry) and where to (to)."))?;
                let from = s.queue.iter().position(|e| e.id == entry).ok_or_else(|| not_found("That song isn't in the queue."))?;
                let item = s.queue.remove(from);
                let to = body.to.unwrap_or(0).min(s.queue.len());
                s.queue.insert(to, item);
            }
            "remove" => {
                let entry = body.entry.ok_or_else(|| bad("Say which song (entry)."))?;
                let from = s.queue.iter().position(|e| e.id == entry).ok_or_else(|| not_found("That song isn't in the queue."))?;
                s.queue.remove(from);
            }
            "allow" => s.everyone_controls = body.everyone.unwrap_or(!s.everyone_controls),
            other => return Err(bad(format!("There's no \"{other}\": play, pause, skip, seek, reorder, remove or allow."))),
        }
        Ok(())
    })?;
    Ok(Json(app.party.snapshot()))
}

#[derive(Deserialize)]
pub struct ReactBody {
    emoji: String,
}

/// POST /api/party/react {emoji}: shows on everyone's Now Playing.
pub async fn react(State(app): State<Shared>, who: Who, Json(body): Json<ReactBody>) -> ApiResult<Json<Value>> {
    let emoji = body.emoji.trim().to_string();
    if emoji.is_empty() || emoji.chars().count() > 8 || emoji.len() > 32 || emoji.chars().any(|c| c.is_control() || c.is_ascii_alphanumeric()) {
        return Err(bad("A reaction is one emoji."));
    }
    let now = now_ms();
    {
        let mut last = lock(&app.party.last_reaction);
        if last.get(&who.person).is_some_and(|t| now - t < 250) {
            return Ok(Json(app.party.snapshot()));
        }
        last.insert(who.person, now);
    }
    app.party.with(|s| {
        s.reactions.push_back(Reaction { emoji, by: Person::of(&who), at: now });
        while s.reactions.len() > REACTIONS {
            s.reactions.pop_front();
        }
        Ok(())
    })?;
    Ok(Json(app.party.snapshot()))
}

/// POST /api/party/end: the host (or the server's owner) ends it.
pub async fn end(State(app): State<Shared>, who: Who) -> ApiResult<Json<Value>> {
    {
        let mut state = lock(&app.party.state);
        let s = state.as_ref().ok_or_else(|| not_found("No party is on right now."))?;
        if who.person != s.host.id && !who.owner {
            return Err(forbidden(format!("Only the host ({}) can end the party.", s.host.name)));
        }
        *state = None;
    }
    crate::log(&format!("{} ended the listening party", who.name));
    app.party.changed();
    Ok(Json(app.party.snapshot()))
}

fn still_joined(app: &Shared, device: i64) -> bool {
    app.db()
        .query_row("SELECT 1 FROM devices d JOIN people p ON p.id = d.person_id WHERE d.id = ?1 AND d.revoked_ms IS NULL AND p.revoked_ms IS NULL", params![device], |_| Ok(()))
        .is_ok()
}

/// GET /api/party/ws (the header, or ?t=TOKEN): the full state on every
/// change. A listener may send {"type":"time","client_ms":N} and gets
/// {"type":"time","client_ms":N,"server_ms":M} back (clock pings on the same line).
pub async fn ws(State(app): State<Shared>, Listener(who): Listener, upgrade: WebSocketUpgrade) -> Response {
    upgrade.on_upgrade(move |socket| listen(app, who, socket))
}

async fn listen(app: Shared, who: Who, socket: WebSocket) {
    let (mut tx, mut rx) = socket.split();
    let mut version = app.party.version.subscribe();
    let mut recheck = app.party.recheck.subscribe();
    let conn = app.party.join(&who);
    version.borrow_and_update();
    let mut ping = tokio::time::interval(std::time::Duration::from_secs(25));
    ping.tick().await;
    let send = |v: Value| Message::Text(v.to_string().into());
    if tx.send(send(app.party.snapshot())).await.is_ok() {
        loop {
            tokio::select! {
                changed = version.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    version.borrow_and_update();
                    if tx.send(send(app.party.snapshot())).await.is_err() {
                        break;
                    }
                }
                _ = recheck.changed() => {
                    recheck.borrow_and_update();
                    if !still_joined(&app, who.device) {
                        let _ = tx.send(send(json!({ "type": "removed", "error": "This device was removed from the server." }))).await;
                        let _ = tx.send(Message::Close(None)).await;
                        break;
                    }
                }
                msg = rx.next() => match msg {
                    Some(Ok(Message::Text(text))) => {
                        let v: Value = serde_json::from_str(text.as_str()).unwrap_or(Value::Null);
                        if v["type"] == "time" {
                            let answer = json!({ "type": "time", "client_ms": v["client_ms"], "server_ms": now_ms() });
                            if tx.send(send(answer)).await.is_err() {
                                break;
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    _ => {}
                },
                _ = ping.tick() => {
                    if tx.send(Message::Ping(Default::default())).await.is_err() {
                        break;
                    }
                }
            }
        }
    }
    app.party.leave(conn);
}
