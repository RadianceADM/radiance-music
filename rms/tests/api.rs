//! The server end to end, over real HTTP on 127.0.0.1: setup → invite → join
//! → tokens, removing people and devices, the library and its files (Range),
//! the split queue (with a stand-in for the splitter's Python), the listening
//! party (its states, a late joiner's position, the WebSocket) and /api/time.

use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};

/// A stand-in for the splitter's Python: answers the checks, "downloads" the
/// models, and splits and aligns as the real scripts say they do.
const FAKE_PYTHON: &str = r#"#!/bin/bash
here="$(dirname "$0")"
case "$1" in
  -c)
    echo "progress 6"
    echo ok ;;
  */split.py)
    out="$3"; mkdir -p "$out"
    echo "progress 5"; sleep 0.2; echo "progress 50"; sleep 0.2
    for p in vocals music drums bass other; do head -c 4096 /dev/urandom > "$out/$p.ogg"; done
    echo "args $*" > "$here/split-args.txt"
    echo "progress 100"
    echo "files vocals.ogg music.ogg drums.ogg bass.ogg other.ogg" ;;
  */align.py)
    cat > "$here/given.json"
    echo "progress 40" >&2
    printf 'version: "1.0"\n# word times: MMS forced alignment (Meta, torchaudio MMS_FA) on the song Vocals part\nlines:\n  - text: "hello there"\n    start_ms: 1000\n    end_ms: 2000\n    words:\n      - text: "hello "\n        start_ms: 1000\n        end_ms: 1500\n      - text: "there"\n        start_ms: 1500\n        end_ms: 2000\n' > "$3"
    cat "$3"
    echo "progress 100" >&2 ;;
  *) echo "the fake python doesn't know $*" >&2; exit 2 ;;
esac
"#;

struct Server {
    base: String,
    dir: PathBuf,
    http: Client,
}

impl Server {
    fn library(&self) -> PathBuf {
        self.dir.join("data/library")
    }

    async fn get(&self, path: &str, token: Option<&str>) -> (StatusCode, Value) {
        let mut req = self.http.get(format!("{}{path}", self.base));
        if let Some(t) = token {
            req = req.bearer_auth(t);
        }
        let res = req.send().await.unwrap();
        let status = res.status();
        (status, res.json().await.unwrap_or(Value::Null))
    }

    async fn post(&self, path: &str, token: Option<&str>, body: Value) -> (StatusCode, Value) {
        let mut req = self.http.post(format!("{}{path}", self.base)).json(&body);
        if let Some(t) = token {
            req = req.bearer_auth(t);
        }
        let res = req.send().await.unwrap();
        let status = res.status();
        (status, res.json().await.unwrap_or(Value::Null))
    }

    async fn delete(&self, path: &str, token: &str) -> (StatusCode, Value) {
        let res = self.http.delete(format!("{}{path}", self.base)).bearer_auth(token).send().await.unwrap();
        let status = res.status();
        (status, res.json().await.unwrap_or(Value::Null))
    }

    /// The owner set up: their browser's token.
    async fn setup(&self, name: &str) -> String {
        let (s, v) = self.post("/api/setup", None, json!({ "name": name })).await;
        assert_eq!(s, StatusCode::CREATED, "{v}");
        v["token"].as_str().unwrap().to_string()
    }

    /// A friend invited and joined: (their token, their person id).
    async fn friend(&self, owner: &str, name: &str) -> (String, i64) {
        let (s, inv) = self.post("/api/invites", Some(owner), json!({ "name": name })).await;
        assert_eq!(s, StatusCode::CREATED, "{inv}");
        let (s, j) = self.post("/api/join", None, json!({ "code": inv["code"], "device_name": format!("{name}'s phone") })).await;
        assert_eq!(s, StatusCode::CREATED, "{j}");
        (j["token"].as_str().unwrap().to_string(), j["person"]["id"].as_i64().unwrap())
    }
}

/// A fresh server on its own port and data folder. `python`: None for the
/// stand-in splitter, or a path (one that isn't there: no splitter).
async fn server(python: Option<&str>) -> Server {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("rms-test-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let scripts = dir.join("scripts");
    std::fs::create_dir_all(&scripts).unwrap();
    std::fs::write(scripts.join("split.py"), "# the stand-in reads only its name\n").unwrap();
    std::fs::write(scripts.join("align.py"), "# the stand-in reads only its name\n").unwrap();
    let fake = dir.join("fakepy");
    std::fs::write(&fake, FAKE_PYTHON).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let cfg = rms::Config {
        data: dir.join("data"),
        bind: "127.0.0.1:0".into(),
        python: python.map(String::from).unwrap_or_else(|| fake.display().to_string()),
        scripts,
        domain: None,
        public_url: None,
        setup_key: None,
        threads: 2,
    };
    let app = rms::open(cfg).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, rms::router(app)).await.unwrap() });
    Server { base, dir, http: Client::new() }
}

/// A FLAC file's header (44.1 kHz) saying it's `ms` long, and some bytes.
fn flac(ms: u64) -> Vec<u8> {
    let (rate, samples) = (44_100u64, 44_100 * ms / 1000);
    let mut v = b"fLaC".to_vec();
    v.extend([0x80, 0, 0, 34]);
    let mut si = [0u8; 34];
    si[10] = (rate >> 12) as u8;
    si[11] = (rate >> 4) as u8;
    si[12] = (((rate & 0xf) << 4) as u8) | 2;
    si[13] = 0xf0 | ((samples >> 32) & 0xf) as u8;
    si[14..18].copy_from_slice(&(samples as u32).to_be_bytes());
    v.extend(si);
    v.extend((0..3000u32).map(|i| (i % 251) as u8));
    v
}

fn put(dir: &Path, name: &str, bytes: &[u8]) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join(name), bytes).unwrap();
}

async fn wait_for<F: Fn(&Value) -> bool>(s: &Server, path: &str, token: Option<&str>, ok: F, secs: u64) -> Value {
    let until = std::time::Instant::now() + Duration::from_secs(secs);
    loop {
        let (_, v) = s.get(path, token).await;
        if ok(&v) {
            return v;
        }
        assert!(std::time::Instant::now() < until, "waited {secs} s for {path}: {v}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn setup_invite_join_and_tokens() {
    let s = server(None).await;
    let (st, v) = s.get("/api/status", None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["needs_setup"], true);
    assert!(v["server"].is_null());
    let page = s.http.get(format!("{}/", s.base)).send().await.unwrap();
    assert_eq!(page.status(), StatusCode::OK);
    assert!(page.text().await.unwrap().contains("Radiance Music Server"));

    // the setup page: a name is needed
    let (st, _) = s.post("/api/setup", None, json!({ "name": "   " })).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, setup) = s.post("/api/setup", None, json!({ "name": "Bassi" })).await;
    assert_eq!(st, StatusCode::CREATED);
    assert_eq!(setup["server"]["name"], "Bassi's server");
    let owner = setup["token"].as_str().unwrap().to_string();
    assert_eq!(owner.len(), 64);
    let link = setup["invite"]["link"].as_str().unwrap();
    assert!(link.starts_with(&format!("{}/join/", s.base)), "{link}");
    assert!(setup["invite"]["qr_svg"].as_str().unwrap().starts_with("<svg"));
    // only once
    let (st, _) = s.post("/api/setup", None, json!({ "name": "Someone Else" })).await;
    assert_eq!(st, StatusCode::CONFLICT);
    let (_, v) = s.get("/api/status", None).await;
    assert_eq!(v["server"], "Bassi's server");
    assert_eq!(v["needs_setup"], false);

    // tokens: none, a wrong one, the owner's
    let (st, v) = s.get("/api/me", None).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
    assert!(v["error"].as_str().unwrap().contains("key"));
    let (st, _) = s.get("/api/me", Some("not-a-token")).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
    let (st, me) = s.get("/api/me", Some(&owner)).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(me["person"]["role"], "owner");
    assert_eq!(me["server"]["name"], "Bassi's server");
    // the database keeps only the token's hash
    let db = rusqlite::Connection::open(s.dir.join("data/server.db")).unwrap();
    let kept: String = db.query_row("SELECT token_sha256 FROM devices LIMIT 1", [], |r| r.get(0)).unwrap();
    assert_ne!(kept, owner);
    assert_eq!(kept.len(), 64);

    // an invite for a friend: its page, then the app joins with its code
    let (st, inv) = s.post("/api/invites", Some(&owner), json!({ "name": "Mika" })).await;
    assert_eq!(st, StatusCode::CREATED);
    let code = inv["code"].as_str().unwrap().to_string();
    assert_eq!(code.len(), 14, "XXXX-XXXX-XXXX: {code}");
    let page = s.http.get(format!("{}/join/{code}", s.base)).send().await.unwrap();
    assert_eq!(page.status(), StatusCode::OK);
    let html = page.text().await.unwrap();
    assert!(html.contains("Bassi&#39;s server") && html.contains("Mika") && html.contains("<svg"), "{html}");
    let typed = code.replace('-', "").to_lowercase();
    let (st, joined) = s.post("/api/join", None, json!({ "code": typed, "device_name": "Mika's phone" })).await;
    assert_eq!(st, StatusCode::CREATED, "{joined}");
    let mika = joined["token"].as_str().unwrap().to_string();
    assert_eq!(joined["person"]["name"], "Mika");
    assert_eq!(joined["server"]["name"], "Bassi's server");
    // one-time
    let (st, v) = s.post("/api/join", None, json!({ "code": code })).await;
    assert_eq!(st, StatusCode::GONE, "{v}");
    let page = s.http.get(format!("{}/join/{code}", s.base)).send().await.unwrap();
    assert_eq!(page.status(), StatusCode::NOT_FOUND);
    let (st, _) = s.post("/api/join", None, json!({ "code": "AAAA-BBBB-CCCC" })).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (_, me) = s.get("/api/me", Some(&mika)).await;
    assert_eq!(me["person"]["role"], "friend");
    assert_eq!(me["device"]["name"], "Mika's phone");
    // friends don't manage the server
    let (st, _) = s.post("/api/invites", Some(&mika), json!({ "name": "Someone" })).await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (st, _) = s.get("/api/people", Some(&mika)).await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    // the owner's own app joins with the link from the setup page
    let (st, app) = s.post("/api/join", None, json!({ "code": setup["invite"]["code"], "device_name": "Bassi's S24" })).await;
    assert_eq!(st, StatusCode::CREATED);
    assert_eq!(app["person"]["role"], "owner");
    let (_, people) = s.get("/api/people", Some(&owner)).await;
    let list = people["people"].as_array().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["name"], "Bassi");
    assert_eq!(list[0]["devices"].as_array().unwrap().len(), 2);
    assert_eq!(list[1]["devices"][0]["name"], "Mika's phone");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn removing_devices_friends_and_invites() {
    let s = server(None).await;
    let owner = s.setup("Bassi").await;
    let (mika, mika_id) = s.friend(&owner, "Mika").await;
    let (_, me) = s.get("/api/me", Some(&mika)).await;
    let device = me["device"]["id"].as_i64().unwrap();

    // a device removed stops working at once
    let (st, _) = s.post(&format!("/api/devices/{device}/revoke"), Some(&owner), json!({})).await;
    assert_eq!(st, StatusCode::OK);
    let (st, v) = s.get("/api/me", Some(&mika)).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
    assert!(v["error"].as_str().unwrap().contains("removed"));

    // a new invite for the same friend (their next device)
    let (st, inv) = s.post("/api/invites", Some(&owner), json!({ "person": mika_id })).await;
    assert_eq!(st, StatusCode::CREATED);
    let (_, j) = s.post("/api/join", None, json!({ "code": inv["code"], "device_name": "Mika's laptop" })).await;
    let laptop = j["token"].as_str().unwrap().to_string();
    let (st, _) = s.get("/api/me", Some(&laptop)).await;
    assert_eq!(st, StatusCode::OK);

    // an invite taken back can't be used
    let (_, inv) = s.post("/api/invites", Some(&owner), json!({ "person": mika_id })).await;
    let open_codes = |v: &Value| v["invites"].as_array().unwrap().iter().map(|i| i["code"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    let (_, open) = s.get("/api/invites", Some(&owner)).await;
    // this one, and the owner's own link from the setup page (not used yet)
    assert_eq!(open_codes(&open).len(), 2);
    assert!(open_codes(&open).contains(&inv["code"].as_str().unwrap().to_string()));
    let (st, _) = s.delete(&format!("/api/invites/{}", inv["code"].as_str().unwrap()), &owner).await;
    assert_eq!(st, StatusCode::OK);
    let (st, _) = s.post("/api/join", None, json!({ "code": inv["code"] })).await;
    assert_eq!(st, StatusCode::GONE);
    let (_, open) = s.get("/api/invites", Some(&owner)).await;
    assert!(!open_codes(&open).contains(&inv["code"].as_str().unwrap().to_string()));
    assert_eq!(open_codes(&open).len(), 1);

    // the owner can't remove themselves, nor their last device
    let (_, me) = s.get("/api/me", Some(&owner)).await;
    let (st, _) = s.post(&format!("/api/people/{}/revoke", me["person"]["id"]), Some(&owner), json!({})).await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (st, _) = s.post(&format!("/api/devices/{}/revoke", me["device"]["id"]), Some(&owner), json!({})).await;
    assert_eq!(st, StatusCode::CONFLICT);
    // a friend can't remove someone else's device
    let (st, _) = s.post(&format!("/api/devices/{}/revoke", me["device"]["id"]), Some(&laptop), json!({})).await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    // a friend removed: every device of theirs stops, and their open invites go
    let (_, inv) = s.post("/api/invites", Some(&owner), json!({ "person": mika_id })).await;
    let (st, _) = s.post(&format!("/api/people/{mika_id}/revoke"), Some(&owner), json!({})).await;
    assert_eq!(st, StatusCode::OK);
    let (st, _) = s.get("/api/me", Some(&laptop)).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
    let (st, _) = s.post("/api/join", None, json!({ "code": inv["code"] })).await;
    assert_eq!(st, StatusCode::GONE);
    let (_, people) = s.get("/api/people", Some(&owner)).await;
    let mika_row = people["people"].as_array().unwrap().iter().find(|p| p["id"] == mika_id).unwrap().clone();
    assert_eq!(mika_row["removed"], true);
    // and can't be invited by id again
    let (st, _) = s.post("/api/invites", Some(&owner), json!({ "person": mika_id })).await;
    assert_eq!(st, StatusCode::CONFLICT);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn library_listing_files_and_uploads() {
    let s = server(None).await;
    let owner = s.setup("Bassi").await;
    let (mika, _) = s.friend(&owner, "Mika").await;
    let lib = s.library();
    // as the app saves them
    put(&lib, "Ann - One.flac", &flac(61_000));
    for p in ["drums", "bass", "other", "vocals", "music"] {
        put(&lib, &format!("Ann - One.{p}.flac"), b"part");
    }
    put(&lib, "Ann - One.lyrics.yaml", b"version: '1.0'\nlines:\n  - text: 'hi'\n    start_ms: 0\n    end_ms: 900\n    words:\n      - text: 'hi'\n        start_ms: 0\n        end_ms: 900\n");
    put(&lib, "Ann - One.jpg", b"0123456789abcdef");
    put(&lib, "Ann - One.video.mkv", b"video");
    put(&lib, "Bob - Two.flac", &flac(5_000));
    put(&lib, "Bob - Two.vocals.ogg", b"half a split");
    put(&lib, "notes.txt", b"not a song");

    let (st, v) = s.get("/api/library", None).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
    let (st, v2) = s.get("/api/library", Some(&mika)).await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(v2["server"], "Bassi's server");
    let songs = v2["songs"].as_array().unwrap();
    assert_eq!(songs.len(), 2, "{v2}");
    let ann = &songs[0];
    assert_eq!(ann["artist"], "Ann");
    assert_eq!(ann["title"], "One");
    assert_eq!(ann["duration_ms"], 61_000);
    assert_eq!(ann["split"], true);
    assert_eq!(ann["parts"], json!(["drums", "bass", "other", "vocals", "music"]));
    assert_eq!(ann["lyrics"], true);
    assert_eq!(ann["word_times"], true);
    assert_eq!(ann["cover"], "/files/Ann%20-%20One.jpg");
    assert_eq!(ann["files"]["video"]["name"], "Ann - One.video.mkv");
    assert_eq!(ann["files"]["drums"]["size"], 4);
    assert!(ann["added_by"].is_null(), "put there by hand");
    let bob = &songs[1];
    assert_eq!(bob["split"], false);
    assert_eq!(bob["parts"], json!(["vocals"]));
    assert_eq!(bob["lyrics"], false);

    // files: the header or ?t=, nothing outside the folder
    let cover = format!("{}{}", s.base, ann["cover"].as_str().unwrap());
    assert_eq!(s.http.get(&cover).send().await.unwrap().status(), StatusCode::UNAUTHORIZED);
    let res = s.http.get(format!("{cover}?t={mika}")).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()["content-type"], "image/jpeg");
    assert_eq!(res.bytes().await.unwrap().as_ref(), b"0123456789abcdef");
    let res = s.http.get(format!("{}/files/..%2Fserver.db?t={mika}", s.base)).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let res = s.http.get(format!("{}/files/%2E%2E/server.db?t={mika}", s.base)).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // a Range request (seeking)
    let res = s.http.get(&cover).bearer_auth(&mika).header("Range", "bytes=2-5").send().await.unwrap();
    assert_eq!(res.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(res.headers()["content-range"], "bytes 2-5/16");
    assert_eq!(res.bytes().await.unwrap().as_ref(), b"2345");
    let song = format!("{}{}", s.base, ann["url"].as_str().unwrap());
    let res = s.http.get(format!("{song}?t={mika}")).header("Range", "bytes=-10").send().await.unwrap();
    assert_eq!(res.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(res.headers()["content-type"], "audio/flac");
    assert_eq!(res.bytes().await.unwrap().len(), 10);

    // an upload, by a friend: their name on it
    let form = reqwest::multipart::Form::new()
        .text("artist", "Cee")
        .text("title", "Three: Live")
        .part("song", reqwest::multipart::Part::bytes(flac(30_000)).file_name("whatever.flac"))
        .part("cover", reqwest::multipart::Part::bytes(b"jpeg".to_vec()).file_name("cover.jpg"));
    let res = s.http.post(format!("{}/api/songs", s.base)).bearer_auth(&mika).multipart(form).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let up: Value = res.json().await.unwrap();
    assert_eq!(up["added"], true);
    assert_eq!(up["song"]["file"], "Cee - Three_ Live.flac");
    assert_eq!(up["song"]["title"], "Three: Live");
    assert_eq!(up["song"]["added_by"]["name"], "Mika");
    assert_eq!(up["song"]["duration_ms"], 30_000);
    assert!(lib.join("Cee - Three_ Live.jpg").exists());
    // the same song again: skipped; only the extra it lacks is kept
    let form = reqwest::multipart::Form::new()
        .part("song", reqwest::multipart::Part::bytes(flac(30_000)).file_name("Cee - Three_ Live.opus"))
        .part("lyrics", reqwest::multipart::Part::bytes(b"[00:01.00]la la".to_vec()).file_name("x.lrc"))
        .part("cover", reqwest::multipart::Part::bytes(b"other".to_vec()).file_name("c.jpg"));
    let res = s.http.post(format!("{}/api/songs", s.base)).bearer_auth(&owner).multipart(form).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let up: Value = res.json().await.unwrap();
    assert_eq!(up["skipped"], true);
    assert_eq!(up["extras"], json!(["lyrics"]));
    assert_eq!(std::fs::read(lib.join("Cee - Three_ Live.jpg")).unwrap(), b"jpeg");
    assert!(lib.join("Cee - Three_ Live.lrc").exists());
    assert!(!lib.join("Cee - Three_ Live.opus").exists());
    // only songs as the apps save them
    let form = reqwest::multipart::Form::new().part("song", reqwest::multipart::Part::bytes(b"x".to_vec()).file_name("a.mp3"));
    let res = s.http.post(format!("{}/api/songs", s.base)).bearer_auth(&owner).multipart(form).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let (_, v) = s.get("/api/library", Some(&owner)).await;
    assert_eq!(v["count"], 3);

    // a song taken out of the folder leaves the list
    std::fs::remove_file(lib.join("Bob - Two.flac")).unwrap();
    let (_, v) = s.get("/api/library", Some(&owner)).await;
    assert_eq!(v["count"], 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn split_queue_runs_the_splitter() {
    let s = server(None).await;
    let owner = s.setup("Bassi").await;
    let (mika, _) = s.friend(&owner, "Mika").await;
    let lib = s.library();
    put(&lib, "Dee - Four.flac", &flac(10_000));
    put(&lib, "Dee - Four.lyrics.yaml", b"version: '1.0'\nlines:\n  - text: 'hello there'\n    start_ms: 1000\n    end_ms: 2000\n");
    put(&lib, "Eve - Five.flac", &flac(10_000));
    // the models download at the first start (here: the stand-in, at once)
    let v = wait_for(&s, "/api/status", None, |v| v["models"]["state"] == "ready", 20).await;
    assert_eq!(v["splitter"]["ready"], true);
    assert!(s.dir.join("data/models/.ready").exists());

    let (_, lib_now) = s.get("/api/library", Some(&mika)).await;
    let dee = lib_now["songs"].as_array().unwrap().iter().find(|x| x["artist"] == "Dee").unwrap()["id"].clone();
    let (st, v) = s.post("/api/split", Some(&mika), json!({ "song": dee })).await;
    assert_eq!(st, StatusCode::CREATED, "{v}");
    assert_eq!(v["queued"], true);
    assert_eq!(v["job"]["asked_by"]["name"], "Mika");
    // asking again doesn't queue it twice
    let (st, v) = s.post("/api/split", Some(&owner), json!({ "song": dee })).await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(v["queued"], false);
    // a song by its file's name
    let (st, v) = s.post("/api/split", Some(&owner), json!({ "song": "Eve - Five.flac" })).await;
    assert_eq!(st, StatusCode::CREATED, "{v}");
    let (st, _) = s.post("/api/split", Some(&owner), json!({ "song": 999 })).await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    let q = wait_for(&s, "/api/queue", Some(&mika), |v| v["jobs"].as_array().is_some_and(|j| j.len() == 2 && j.iter().all(|j| j["state"] == "done")), 30).await;
    assert_eq!(q["splitter"]["ready"], true);
    let job = q["jobs"].as_array().unwrap().iter().find(|j| j["song"]["artist"] == "Dee").unwrap().clone();
    assert_eq!(job["progress"], 1.0);
    assert!(job["note"].as_str().unwrap().contains("every word is timed"), "{job}");
    // the parts landed beside the song, with the apps' names
    for p in ["vocals", "music", "drums", "bass", "other"] {
        assert!(lib.join(format!("Dee - Four.{p}.ogg")).exists(), "{p}");
    }
    // the splitter was given the song and a work folder; the aligner the lyrics' lines
    let args = std::fs::read_to_string(s.dir.join("split-args.txt")).unwrap();
    assert!(args.contains("/split.py") && (args.contains("Eve - Five.flac") || args.contains("Dee - Four.flac")), "{args}");
    let given: Value = serde_json::from_str(&std::fs::read_to_string(s.dir.join("given.json")).unwrap()).unwrap();
    assert_eq!(given, json!({ "lines": [{ "start": 1.0, "end": 2.0, "text": "hello there" }] }));
    let lyrics = std::fs::read_to_string(lib.join("Dee - Four.lyrics.yaml")).unwrap();
    assert!(lyrics.contains("MMS forced alignment"));
    let eve = q["jobs"].as_array().unwrap().iter().find(|j| j["song"]["artist"] == "Eve").unwrap().clone();
    assert!(eve["note"].as_str().unwrap().contains("no lyrics"), "{eve}");

    let (_, v) = s.get("/api/library", Some(&mika)).await;
    let dee_now = v["songs"].as_array().unwrap().iter().find(|x| x["artist"] == "Dee").unwrap().clone();
    assert_eq!(dee_now["split"], true);
    assert_eq!(dee_now["word_times"], true);
    // split already
    let (st, v) = s.post("/api/split", Some(&mika), json!({ "song": dee })).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["already_split"], true);
    // finished songs can't be taken out of the queue
    let (st, _) = s.delete(&format!("/api/queue/{}", job["id"]), &owner).await;
    assert_eq!(st, StatusCode::CONFLICT);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn queue_says_when_the_splitter_is_missing() {
    let s = server(Some("/nowhere/python3")).await;
    let owner = s.setup("Bassi").await;
    let (mika, _) = s.friend(&owner, "Mika").await;
    put(&s.library(), "Fay - Six.flac", &flac(3_000));
    let v = wait_for(&s, "/api/status", None, |v| v["splitter"]["checked"] == true, 20).await;
    assert_eq!(v["splitter"]["ready"], false);
    assert!(v["splitter"]["message"].as_str().unwrap().contains("isn't installed"), "{v}");
    assert_eq!(v["models"]["state"], "waiting");
    let (_, lib) = s.get("/api/library", Some(&owner)).await;
    let (st, v) = s.post("/api/split", Some(&mika), json!({ "song": lib["songs"][0]["id"] })).await;
    assert_eq!(st, StatusCode::CREATED, "{v}");
    let (_, q) = s.get("/api/queue", Some(&mika)).await;
    assert_eq!(q["jobs"][0]["state"], "waiting");
    assert_eq!(q["jobs"][0]["note"], "Waiting: the splitter isn't installed on this server yet.");
    // the one who asked (or the owner) takes it out
    let (st, _) = s.delete(&format!("/api/queue/{}", q["jobs"][0]["id"]), &owner).await;
    assert_eq!(st, StatusCode::OK);
    let (_, q) = s.get("/api/queue", Some(&mika)).await;
    assert_eq!(q["jobs"].as_array().unwrap().len(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn time_is_the_servers_clock() {
    let s = server(None).await;
    let before = now_ms();
    let (st, v) = s.get("/api/time", None).await;
    let after = now_ms();
    assert_eq!(st, StatusCode::OK);
    let t = v["now_ms"].as_i64().unwrap();
    assert!(t >= before - 5 && t <= after + 5, "{before} {t} {after}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn listening_party() {
    let s = server(None).await;
    let owner = s.setup("Bassi").await;
    let (mika, mika_id) = s.friend(&owner, "Mika").await;
    let lib = s.library();
    put(&lib, "A - Long.flac", &flac(120_000));
    put(&lib, "A - Long.jpg", b"cover");
    put(&lib, "B - Short.flac", &flac(1_000));
    put(&lib, "C - Mid.flac", &flac(60_000));
    let (_, v) = s.get("/api/library", Some(&owner)).await;
    let id = |artist: &str| v["songs"].as_array().unwrap().iter().find(|x| x["artist"] == artist).unwrap()["id"].clone();
    let (a, b, c) = (id("A"), id("B"), id("C"));

    let (st, p) = s.get("/api/party", Some(&mika)).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(p["on"], false);
    let (st, _) = s.post("/api/party/add", Some(&mika), json!({ "song": a })).await;
    assert_eq!(st, StatusCode::NOT_FOUND, "no party yet");

    // Mika starts it (anyone can); one at a time
    let (st, p) = s.post("/api/party/start", Some(&mika), json!({})).await;
    assert_eq!(st, StatusCode::CREATED);
    assert_eq!(p["host"]["name"], "Mika");
    assert_eq!(p["everyone_controls"], false);
    assert!(p["song"].is_null());
    let (st, v) = s.post("/api/party/start", Some(&owner), json!({})).await;
    assert_eq!(st, StatusCode::CONFLICT);
    assert!(v["error"].as_str().unwrap().contains("Mika"));

    // a listener on the WebSocket (as a browser: ?t=)
    let ws_url = format!("{}/api/party/ws?t={mika}", s.base.replace("http://", "ws://"));
    let (mut ws, _) = tokio_tungstenite::connect_async(ws_url).await.unwrap();
    let first: Value = serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(first["type"], "state");
    assert_eq!(first["on"], true);
    // no key, no listening
    assert!(tokio_tungstenite::connect_async(format!("{}/api/party/ws", s.base.replace("http://", "ws://"))).await.is_err());

    // anyone adds; nothing playing: it starts LEAD_MS from now
    let t0 = now_ms();
    let (st, p) = s.post("/api/party/add", Some(&owner), json!({ "song": a })).await;
    assert_eq!(st, StatusCode::OK, "{p}");
    assert_eq!(p["song"]["title"], "Long");
    assert_eq!(p["song"]["added_by"]["name"], "Bassi");
    assert_eq!(p["song"]["duration_ms"], 120_000);
    assert_eq!(p["song"]["cover"], "/files/A%20-%20Long.jpg");
    assert_eq!(p["playing"], true);
    let started = p["started_at_server_ms"].as_i64().unwrap();
    assert!(started >= t0 + rms::party::LEAD_MS - 50 && started <= now_ms() + rms::party::LEAD_MS + 50, "{started} {t0}");
    assert!(p["paused_at_ms"].is_null());
    // the WebSocket pushes every change (members joining, the song)
    let mut pushed = Value::Null;
    for _ in 0..10 {
        let m: Value = serde_json::from_str(tokio::time::timeout(Duration::from_secs(5), ws.next()).await.unwrap().unwrap().unwrap().to_text().unwrap()).unwrap();
        if m["song"]["title"] == "Long" {
            pushed = m;
            break;
        }
    }
    assert_eq!(pushed["members"][0]["name"], "Mika");
    assert_eq!(pushed["members"][0]["host"], true);
    // clock pings on the same line
    ws.send(tokio_tungstenite::tungstenite::Message::Text(json!({ "type": "time", "client_ms": 42 }).to_string().into())).await.unwrap();
    let mut pong = Value::Null;
    for _ in 0..10 {
        let m: Value = serde_json::from_str(tokio::time::timeout(Duration::from_secs(5), ws.next()).await.unwrap().unwrap().unwrap().to_text().unwrap()).unwrap();
        if m["type"] == "time" {
            pong = m;
            break;
        }
    }
    assert_eq!(pong["client_ms"], 42);
    assert!((pong["server_ms"].as_i64().unwrap() - now_ms()).abs() < 2000);

    // only the host controls (until they allow everyone)
    let (st, v) = s.post("/api/party/control", Some(&owner), json!({ "action": "pause" })).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "{v}");
    tokio::time::sleep(Duration::from_millis(1200)).await;
    let (st, p) = s.post("/api/party/control", Some(&mika), json!({ "action": "pause" })).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(p["playing"], false);
    assert!(p["started_at_server_ms"].is_null());
    let paused = p["paused_at_ms"].as_i64().unwrap();
    assert!((300..=1500).contains(&paused), "{paused}");
    // paused, the position holds
    tokio::time::sleep(Duration::from_millis(300)).await;
    let (_, p) = s.get("/api/party", Some(&owner)).await;
    assert_eq!(p["position_ms"], paused);

    // seek while paused, then play: a late joiner computes where everyone is
    let (_, p) = s.post("/api/party/control", Some(&mika), json!({ "action": "seek", "position_ms": 30_000 })).await;
    assert_eq!(p["paused_at_ms"], 30_000);
    let (_, p) = s.post("/api/party/control", Some(&mika), json!({ "action": "play" })).await;
    assert_eq!(p["playing"], true);
    let started = p["started_at_server_ms"].as_i64().unwrap();
    assert!(((now_ms() - started) - (30_000 - rms::party::LEAD_MS)).abs() < 300);
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let (_, late) = s.get("/api/party", Some(&owner)).await;
    let server_now = late["server_ms"].as_i64().unwrap();
    let where_to_be = server_now - late["started_at_server_ms"].as_i64().unwrap();
    assert!((where_to_be - (30_000 - rms::party::LEAD_MS + 1500)).abs() < 400, "{where_to_be}");
    assert_eq!(late["position_ms"], where_to_be);
    // seeking while playing starts there after the lead
    let (_, p) = s.post("/api/party/control", Some(&mika), json!({ "action": "seek", "position_ms": 60_000 })).await;
    assert!((p["position_ms"].as_i64().unwrap() - (60_000 - rms::party::LEAD_MS)).abs() < 300);

    // the queue: add two, reorder, allow everyone, react
    s.post("/api/party/add", Some(&mika), json!({ "song": c })).await;
    let (_, p) = s.post("/api/party/add", Some(&owner), json!({ "song": b })).await;
    let queue: Vec<String> = p["queue"].as_array().unwrap().iter().map(|e| e["title"].as_str().unwrap().to_string()).collect();
    assert_eq!(queue, ["Mid", "Short"]);
    let b_entry = p["queue"][1]["entry"].clone();
    let (st, _) = s.post("/api/party/control", Some(&owner), json!({ "action": "reorder", "entry": b_entry, "to": 0 })).await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (st, _) = s.post("/api/party/control", Some(&owner), json!({ "action": "allow", "everyone": true })).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "only the host allows");
    let (_, p) = s.post("/api/party/control", Some(&mika), json!({ "action": "allow", "everyone": true })).await;
    assert_eq!(p["everyone_controls"], true);
    let (st, p) = s.post("/api/party/control", Some(&owner), json!({ "action": "reorder", "entry": b_entry, "to": 0 })).await;
    assert_eq!(st, StatusCode::OK);
    let queue: Vec<String> = p["queue"].as_array().unwrap().iter().map(|e| e["title"].as_str().unwrap().to_string()).collect();
    assert_eq!(queue, ["Short", "Mid"]);
    let (st, p) = s.post("/api/party/react", Some(&owner), json!({ "emoji": "🔥" })).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(p["reactions"][0]["emoji"], "🔥");
    assert_eq!(p["reactions"][0]["by"]["name"], "Bassi");
    let (st, _) = s.post("/api/party/react", Some(&owner), json!({ "emoji": "hello" })).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, _) = s.post("/api/party/control", Some(&owner), json!({ "action": "dance" })).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // skip to the short song; when it ends, the next starts by itself
    let (_, p) = s.post("/api/party/control", Some(&owner), json!({ "action": "skip" })).await;
    assert_eq!(p["song"]["title"], "Short");
    let p = wait_for(&s, "/api/party", Some(&mika), |p| p["song"]["title"] == "Mid", 5).await;
    assert_eq!(p["playing"], true);
    assert_eq!(p["queue"].as_array().unwrap().len(), 0);
    let late = p["server_ms"].as_i64().unwrap() - p["started_at_server_ms"].as_i64().unwrap();
    assert!(late < 1000, "the next song starts from its beginning: {late}");
    // the last one skipped: quiet, the party stays on
    let (_, p) = s.post("/api/party/control", Some(&mika), json!({ "action": "skip" })).await;
    assert!(p["song"].is_null());
    assert_eq!(p["playing"], false);
    assert_eq!(p["on"], true);
    let (st, _) = s.post("/api/party/control", Some(&mika), json!({ "action": "play" })).await;
    assert_eq!(st, StatusCode::CONFLICT, "nothing to play");

    // a removed friend leaves at once (their WebSocket is closed)
    let (_, me) = s.get("/api/me", Some(&owner)).await;
    assert_ne!(me["person"]["id"], mika_id);
    s.post(&format!("/api/people/{mika_id}/revoke"), Some(&owner), json!({})).await;
    let mut closed = false;
    for _ in 0..20 {
        match tokio::time::timeout(Duration::from_secs(5), ws.next()).await.unwrap() {
            Some(Ok(m)) if m.is_close() => {
                closed = true;
                break;
            }
            Some(Ok(m)) if m.to_text().unwrap_or("").contains("\"removed\"") => continue,
            Some(Ok(_)) => continue,
            _ => {
                closed = true;
                break;
            }
        }
    }
    assert!(closed);
    let p = wait_for(&s, "/api/party", Some(&owner), |p| p["members"].as_array().is_some_and(|m| m.is_empty()), 5).await;
    assert_eq!(p["host"]["name"], "Mika", "the party goes on");
    // the server's owner can end it
    let (st, p) = s.post("/api/party/end", Some(&owner), json!({})).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(p["on"], false);
}
