//! Rules 1 and 2 against a real Radiance Music Server (radiance-music#75), not a stand-in. Ignored by default;
//! run on the build machine with a server whose library has "Bassi - Sunset Walk" split, and two songs to send:
//!
//!   RMS_E2E=http://127.0.0.1:18334 RMS_E2E_TOKEN=… RMS_E2E_SONGS=/dir/with/flac+parts \
//!     cargo test -p radiance-engine --test server_rules_e2e -- --ignored --nocapture
//!
//! RMS_E2E_SONGS holds "Ana - Evening Glow.flac" and its four parts (copied under new names here).

use std::path::{Path, PathBuf};

use radiance_engine::servers::{self, Joined};
use radiance_engine::track::{Source, Track};
use radiance_engine::uploads::{self, Sent};

fn env(k: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| panic!("{k} isn't set"))
}

fn yt(artist: &str, title: &str, id: &str) -> Track {
    Track { source: Source::YouTube, id: id.into(), title: title.into(), artist: artist.into(), album: String::new(), seconds: 180, art: String::new(), format: String::new(), file: None, video: false, genres: None }
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("rules-e2e-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A saved song here named "ARTIST - TITLE.flac", with the given parts beside it (from RMS_E2E_SONGS).
fn saved_here(dir: &Path, artist: &str, title: &str, parts: &[&str]) -> Track {
    let from = PathBuf::from(env("RMS_E2E_SONGS"));
    let stem = format!("{artist} - {title}");
    std::fs::copy(from.join("Ana - Evening Glow.flac"), dir.join(format!("{stem}.flac"))).unwrap();
    for p in parts {
        std::fs::copy(from.join(format!("Ana - Evening Glow.{p}.ogg")), dir.join(format!("{stem}.{p}.ogg"))).unwrap();
    }
    let mut t = yt(artist, title, &format!("yt-e2e-{}", title.replace(' ', "-")));
    t.file = Some(dir.join(format!("{stem}.flac")).display().to_string());
    t
}

fn get(server: &Joined, path: &str) -> serde_json::Value {
    let mut r = ureq::get(&format!("{}{path}", server.address)).header("Authorization", &format!("Bearer {}", server.token)).call().unwrap();
    r.body_mut().read_json().unwrap()
}

#[test]
#[ignore]
fn rules_one_and_two_on_a_real_server() {
    let server = Joined::new(&env("RMS_E2E"), &env("RMS_E2E_TOKEN"), "e2e", None);
    servers::set_joined(std::slice::from_ref(&server));
    let stamp = std::process::id();

    // rule 1: the server's copy is found, played from, and saved (with its parts)
    let t = yt("Bassi", "Sunset Walk", "yt-e2e-sunset");
    let (_, song) = servers::copy_of(&t).expect("the server's copy of Sunset Walk");
    assert!(song.split, "{song:?}");
    let relay = radiance_engine::relay::Relay::start().unwrap();
    let (uri, _) = radiance_engine::playing::stream_of(&t, &relay, || Err("YouTube mustn't be asked".into())).unwrap();
    let r = ureq::get(&uri).call().unwrap();
    assert_eq!(r.status().as_u16(), 200, "{uri}");
    let music = scratch("music");
    radiance_engine::library::set_music_dir(Some(music.clone()));
    let path = radiance_engine::library::save_with(&t, &|_, _| Err("no".into()), &|_| radiance_engine::lyrics::Lyrics::None).unwrap();
    let mut saved = t.clone();
    saved.file = Some(path.clone());
    assert!(radiance_engine::split::has_parts(&saved), "parts beside {path}");
    println!("rule 1: played and saved from the server: {path}");

    // rule 2: split here in four → up with parts=coming, then its parts; no split job there
    let dir = scratch("here");
    let four = saved_here(&dir, "E2E Artist", &format!("Four {stamp}"), &["drums", "bass", "other", "vocals"]);
    assert_eq!(uploads::send(&server, &four), Sent::Done);
    let there = servers::find_on(&server, &four).unwrap().expect("sent");
    assert!(there.split, "its parts went with it: {there:?}");
    let q = get(&server, "/api/queue");
    let job = q["jobs"].as_array().unwrap().iter().find(|j| j["song"]["id"] == there.id).cloned();
    assert!(job.as_ref().is_none_or(|j| j["state"] == "done"), "the server didn't split it again: {job:?}");
    println!("rule 2: four parts went up, no split there ({job:?})");

    // halves only: the song goes, the halves don't, and the server splits it itself
    let two = saved_here(&scratch("two"), "E2E Artist", &format!("Two {stamp}"), &["vocals"]);
    std::fs::copy(PathBuf::from(env("RMS_E2E_SONGS")).join("Ana - Evening Glow.music.ogg"), Path::new(two.file.as_deref().unwrap()).with_extension("music.ogg")).unwrap();
    assert_eq!(uploads::send(&server, &two), Sent::Done);
    let there = servers::find_on(&server, &two).unwrap().expect("sent");
    assert!(!there.files.contains_key("music"), "the halves stay here: {there:?}");
    let q = get(&server, "/api/queue");
    assert!(q["jobs"].as_array().unwrap().iter().any(|j| j["song"]["id"] == there.id), "the server splits it: {q}");
    println!("rule 2: halves only → the song went, the server splits it");
    servers::set_joined(&[]);
}
