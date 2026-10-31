//! Change events, so the apps stop polling: a WebSocket (`/api/events`, the
//! header or `?t=TOKEN`) that pushes small JSON events.
//!
//!   {"type":"hello","server":{"id","name"}}     on connecting
//!   {"type":"library"}                          songs or their parts changed: fetch again
//!   {"type":"queue", …GET /api/queue…}          the split queue changed (progress, done…)
//!   {"type":"me"}                               your switches changed, or you were removed
//!                                               (then the line closes)
//!
//! The queue goes out at most twice a second while a song is being split.

use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::{broadcast, watch};

use crate::auth::{Listener, Who};
use crate::Shared;

/// The changes the server tells its listeners about.
pub struct Events {
    library: watch::Sender<u64>,
    queue: watch::Sender<u64>,
    /// A person whose switches changed, or who (or one of whose devices) was removed.
    me: broadcast::Sender<i64>,
}

impl Default for Events {
    fn default() -> Self {
        Events { library: watch::channel(0).0, queue: watch::channel(0).0, me: broadcast::channel(64).0 }
    }
}

impl Events {
    pub fn library(&self) {
        self.library.send_modify(|v| *v += 1);
    }

    pub fn queue(&self) {
        self.queue.send_modify(|v| *v += 1);
    }

    pub fn me(&self, person: i64) {
        let _ = self.me.send(person);
    }
}

/// GET /api/events: the change events for this device.
pub async fn ws(State(app): State<Shared>, Listener(who): Listener, upgrade: WebSocketUpgrade) -> Response {
    upgrade.on_upgrade(move |socket| listen(app, who, socket))
}

async fn listen(app: Shared, who: Who, socket: WebSocket) {
    let (mut tx, mut rx) = socket.split();
    let mut library = app.events.library.subscribe();
    let mut queue = app.events.queue.subscribe();
    let mut me = app.events.me.subscribe();
    library.borrow_and_update();
    queue.borrow_and_update();
    let text = |v: Value| Message::Text(v.to_string().into());
    let hello = json!({ "type": "hello", "server": { "id": app.server_id(), "name": app.server_name() } });
    if tx.send(text(hello)).await.is_err() {
        return;
    }
    let mut ping = tokio::time::interval(Duration::from_secs(25));
    ping.tick().await;
    // the queue: at most every half second
    let mut queue_due = false;
    let mut queue_next = tokio::time::Instant::now();
    loop {
        tokio::select! {
            changed = library.changed() => {
                if changed.is_err() { break; }
                library.borrow_and_update();
                if tx.send(text(json!({ "type": "library" }))).await.is_err() { break; }
            }
            changed = queue.changed() => {
                if changed.is_err() { break; }
                queue.borrow_and_update();
                queue_due = true;
            }
            _ = tokio::time::sleep_until(queue_next), if queue_due => {
                queue_due = false;
                queue_next = tokio::time::Instant::now() + Duration::from_millis(500);
                let Ok(mut q) = crate::split::queue_json(&app) else { continue };
                q["type"] = json!("queue");
                if tx.send(text(q)).await.is_err() { break; }
            }
            person = me.recv() => {
                let mine = match person {
                    Ok(p) => p == who.person,
                    // missed some: tell it anyway (it only fetches /api/me again)
                    Err(broadcast::error::RecvError::Lagged(_)) => true,
                    Err(broadcast::error::RecvError::Closed) => break,
                };
                if mine {
                    if tx.send(text(json!({ "type": "me" }))).await.is_err() { break; }
                    if !crate::party::still_joined(&app, who.device) {
                        let _ = tx.send(Message::Close(None)).await;
                        break;
                    }
                }
            }
            msg = rx.next() => match msg {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                _ => {}
            },
            _ = ping.tick() => {
                if tx.send(Message::Ping(Default::default())).await.is_err() { break; }
            }
        }
    }
}
