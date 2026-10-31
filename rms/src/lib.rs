//! Radiance Music Server (RMS): your own server for Radiance Music. It splits
//! songs (drums, bass, other, vocals, and every word's time), keeps your
//! library (songs, parts, lyrics, covers, videos) and holds listening parties,
//! for you and the friends you invite (owner, 2026-10-31: "share across
//! friends"; friends play and split, plus a listening party).
//!
//! The app is what everyone downloads; the server is what *you* host. Nothing
//! here is public: no sign-up, no listing, only invites.
//!
//! One data folder (RMS_DATA, /data in Docker) holds everything:
//! `library/` (files named as the apps save them), `server.db` (SQLite:
//! people, invites, devices, songs, the split queue) and `models/` (the
//! splitter's models, downloaded at the first start: never in the image).

pub mod auth;
pub mod db;
pub mod events;
pub mod library;
pub mod lyrics;
pub mod media;
pub mod party;
pub mod people;
pub mod split;
pub mod web;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::extract::DefaultBodyLimit;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Everything the server is told by its environment.
#[derive(Clone, Debug)]
pub struct Config {
    /// RMS_DATA: the one folder with the library, the database and the models.
    pub data: PathBuf,
    /// RMS_BIND: where it listens (Caddy talks to it there).
    pub bind: String,
    /// RMS_PYTHON: the Python with Demucs and PyTorch (the splitter).
    pub python: String,
    /// RMS_SCRIPTS: the folder with split.py and align.py (the app's own).
    pub scripts: PathBuf,
    /// RMS_DOMAIN: the server's domain (Caddy gets its certificate); links use https:// then.
    pub domain: Option<String>,
    /// RMS_PUBLIC_URL: the address links use, when it's neither of the above.
    pub public_url: Option<String>,
    /// RMS_SETUP_KEY: the setup page asks for it, so nobody else claims a fresh server (owner,
    /// 2026-11-01: required). None at the first start: the server makes one (setup-key.txt, the log).
    pub setup_key: Option<String>,
    /// RMS_THREADS: the cores the splitter may use (default: all of them, at the lowest priority).
    pub threads: usize,
}

impl Config {
    pub fn from_env() -> Config {
        let var = |k: &str| std::env::var(k).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        Config {
            data: var("RMS_DATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/data")),
            bind: var("RMS_BIND").unwrap_or_else(|| "0.0.0.0:8080".into()),
            python: var("RMS_PYTHON").unwrap_or_else(|| "python3".into()),
            scripts: var("RMS_SCRIPTS").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/opt/rms/scripts")),
            domain: var("RMS_DOMAIN").map(|d| d.trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/').to_string()),
            public_url: var("RMS_PUBLIC_URL").map(|u| u.trim_end_matches('/').to_string()),
            setup_key: var("RMS_SETUP_KEY"),
            threads: var("RMS_THREADS").and_then(|t| t.parse().ok()).filter(|&t| t > 0).unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2)),
        }
    }

    pub fn library(&self) -> PathBuf {
        self.data.join("library")
    }

    pub fn models(&self) -> PathBuf {
        self.data.join("models")
    }

    pub fn work(&self) -> PathBuf {
        self.data.join("work")
    }
}

/// The running server: its settings, its database and its three helpers.
pub struct App {
    pub cfg: Config,
    pub db: Mutex<rusqlite::Connection>,
    pub splitter: split::Splitter,
    pub party: party::Party,
    /// Change events for the apps (/api/events).
    pub events: events::Events,
    /// The server's own id (db::server_id).
    pub id: String,
}

pub type Shared = Arc<App>;

impl App {
    /// The database, for one short step (never held across an await).
    pub fn db(&self) -> std::sync::MutexGuard<'_, rusqlite::Connection> {
        self.db.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn server_id(&self) -> &str {
        &self.id
    }

    /// "Bassi's server" (owner's ruling: the owner's name from the setup page); None before setup.
    pub fn server_name(&self) -> Option<String> {
        db::owner(&self.db()).map(|(_, name)| server_name(&name))
    }
}

pub fn server_name(owner: &str) -> String {
    format!("{owner}'s server")
}

/// The server's clock, ms since 1970 (the party runs on it).
pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// A line in the server's log (Docker and journald add the time).
pub fn log(what: &str) {
    eprintln!("[rms] {what}");
}

/// An answer that went wrong: its status, and what happened in plain words.
#[derive(Debug)]
pub struct ApiError(pub StatusCode, pub String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

impl From<rusqlite::Error> for ApiError {
    fn from(e: rusqlite::Error) -> Self {
        log(&format!("database: {e}"));
        ApiError(StatusCode::INTERNAL_SERVER_ERROR, format!("The server's database had a problem: {e}"))
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

pub fn bad(msg: impl Into<String>) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, msg.into())
}

pub fn not_found(msg: impl Into<String>) -> ApiError {
    ApiError(StatusCode::NOT_FOUND, msg.into())
}

pub fn forbidden(msg: impl Into<String>) -> ApiError {
    ApiError(StatusCode::FORBIDDEN, msg.into())
}

pub fn conflict(msg: impl Into<String>) -> ApiError {
    ApiError(StatusCode::CONFLICT, msg.into())
}

pub fn broken(msg: impl Into<String>) -> ApiError {
    ApiError(StatusCode::INTERNAL_SERVER_ERROR, msg.into())
}

/// Opens the data folder and the database, and starts the helpers: the split
/// worker (which first downloads the models), and the party's clock.
pub fn open(mut cfg: Config) -> Result<Shared, String> {
    for dir in [cfg.data.clone(), cfg.library(), cfg.models(), cfg.work(), cfg.data.join("logs")] {
        std::fs::create_dir_all(&dir).map_err(|e| format!("can't make {}: {e}", dir.display()))?;
    }
    let conn = db::open(&cfg.data.join("server.db"))?;
    if db::owner(&conn).is_none() {
        cfg.setup_key = Some(setup_key(&cfg)?);
    }
    let id = db::server_id(&conn)?;
    let app = Arc::new(App { cfg, db: Mutex::new(conn), splitter: split::Splitter::default(), party: party::Party::default(), events: events::Events::default(), id });
    // a crashed upload's leftovers
    if let Ok(dir) = std::fs::read_dir(app.cfg.work()) {
        for e in dir.flatten() {
            if e.file_name().to_string_lossy().starts_with("upload-") {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
    tokio::spawn(split::worker(app.clone()));
    tokio::spawn(party::clock(app.clone()));
    Ok(app)
}

/// The key the setup page asks for: the one given when deploying (RMS_SETUP_KEY),
/// else one made here once, readable like an invite code, kept in setup-key.txt
/// and printed in the log (Hostinger: Docker Manager → your project → Logs).
fn setup_key(cfg: &Config) -> Result<String, String> {
    if let Some(k) = &cfg.setup_key {
        log("waiting for setup: the setup page asks for the setup key given when deploying (RMS_SETUP_KEY)");
        return Ok(k.clone());
    }
    let file = cfg.data.join("setup-key.txt");
    let key = match std::fs::read_to_string(&file) {
        Ok(k) if !k.trim().is_empty() => k.trim().to_string(),
        _ => {
            let k = auth::show_code(&auth::new_code());
            std::fs::write(&file, format!("{k}\n")).map_err(|e| format!("can't keep the setup key in {}: {e}", file.display()))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600));
            }
            k
        }
    };
    log(&format!("waiting for setup. The setup key is {key} (also in {})", file.display()));
    Ok(key)
}

/// Every address the server answers.
pub fn router(app: Shared) -> Router {
    Router::new()
        // the web pages
        .route("/", get(web::home))
        .route("/party", get(web::party_page))
        .route("/join/{code}", get(web::join_page))
        .route("/assets/{name}", get(web::asset))
        // setup and people
        .route("/api/status", get(people::status))
        .route("/api/setup", post(people::setup))
        .route("/api/join", post(people::join))
        .route("/api/me", get(people::me))
        .route("/api/invites", get(people::invites).post(people::invite))
        .route("/api/invites/{code}", axum::routing::delete(people::take_back))
        .route("/api/people", get(people::people))
        .route("/api/people/{id}", axum::routing::patch(people::switches))
        .route("/api/people/{id}/revoke", post(people::revoke_person))
        .route("/api/devices/{id}/revoke", post(people::revoke_device))
        // the library
        .route("/api/library", get(library::list))
        .route("/api/songs", post(library::upload).layer(DefaultBodyLimit::max(library::MOST_UPLOAD as usize)))
        .route("/api/songs/{id}", get(library::one))
        .route("/api/songs/{id}/parts", post(library::upload_parts).layer(DefaultBodyLimit::max(library::MOST_UPLOAD as usize)))
        .route("/api/events", get(events::ws))
        .route("/files/{*name}", get(library::file))
        // the split queue
        .route("/api/split", post(split::ask))
        .route("/api/queue", get(split::queue))
        .route("/api/queue/{id}", axum::routing::delete(split::remove))
        // the listening party
        .route("/api/time", get(party::time))
        .route("/api/party", get(party::state))
        .route("/api/party/start", post(party::start))
        .route("/api/party/add", post(party::add))
        .route("/api/party/control", post(party::control))
        .route("/api/party/react", post(party::react))
        .route("/api/party/end", post(party::end))
        .route("/api/party/ws", get(party::ws))
        .with_state(app)
}

/// Serves until the process is stopped.
pub async fn serve(app: Shared, listener: tokio::net::TcpListener) -> std::io::Result<()> {
    axum::serve(listener, router(app)).with_graceful_shutdown(async {
        let ctrl_c = async {
            let _ = tokio::signal::ctrl_c().await;
        };
        #[cfg(unix)]
        let term = async {
            if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                s.recv().await;
            }
        };
        #[cfg(not(unix))]
        let term = std::future::pending::<()>();
        tokio::select! { _ = ctrl_c => {}, _ = term => {} }
    })
    .await
}
