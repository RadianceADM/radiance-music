//! Updates (owner, 2026-10-31: "a modal opening when there's an update avail ...
//! with a remind me later option"): the newest release on GitHub, looked up a
//! little after the start (at most every 6 hours); on "Update now" its file for
//! this kind of install is downloaded while the music plays and checked against
//! the SHA-256 GitHub gives for it; "Restart to update" (pressed by you, never by
//! itself) puts it on and opens the new version.
//!
//! How each kind of install updates:
//! - Windows (the installer's per-user copy): the new RadianceMusic-Setup.exe runs
//!   quietly once this app has closed, then opens the app again.
//! - Linux (the tarball's ~/.local/bin copy): the new tarball's install.sh, then
//!   the new app opens once this one has closed.
//! - Our Flatpak: Flatpak's own way (owner: "official update process"), through
//!   the Flatpak portal: it updates from our signed Flatpak repo on GitHub Pages
//!   (https://radianceadm.github.io/radiance-music-flatpak/: the bundle carries
//!   its address and key, see packaging/flatpak), and opens the newest version.
//! - Anything else (a build from source): the release page.
//!
//! Runs on worker threads only, except where noted.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use glib::prelude::*;
use serde_json::{json, Value};

use crate::net;

const LATEST: &str = "https://api.github.com/repos/RadianceADM/radiance-music/releases/latest";
/// Our own copy of the newest release (owner, 2026-11-08): GitHub's JSON with its files on downloads.radianceph.com
/// (Hostinger), mirrored at every release. Asked first: GitHub limits update checks to 60 an hour per IP, and
/// everyone behind one Converge IP shares them (403), and its downloads timed out (504).
const OWN_LATEST: &str = "https://downloads.radianceph.com/radiance-music/latest.json";
/// The beta looks through the newest releases for the one carrying its installer.
const RELEASES: &str = "https://api.github.com/repos/RadianceADM/radiance-music/releases?per_page=20";
/// "Remind me later": a day (owner, 2026-10-31).
pub const LATER: Duration = Duration::from_secs(24 * 60 * 60);
/// The count of updates (no name, no address: the platform and the version).
const COUNT: &str = "https://admin.radianceph.com/api/music/count";

/// How this copy of the app was installed (so how it updates).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Install {
    /// RadianceMusic-Setup.exe's per-user copy: its folder (with Uninstall.exe).
    Windows(PathBuf),
    /// The tarball's install.sh copy in ~/.local/bin.
    Tarball,
    /// Our own Flatpak.
    Flatpak,
    /// The Snap Store's snap: the store updates it by itself (packaging/snap).
    Snap,
    /// A build from source, or a copy somewhere else: updated where it came from.
    Other,
    /// The Windows beta's per-user copy (RadianceMusic-Beta-Setup.exe): its folder, with
    /// RadianceMusicBeta.exe and Uninstall.exe side by side (stage 4a, #45).
    Beta(PathBuf),
}

/// The Windows beta's installer, a file of each computer release (the spec: the beta's pop-up
/// offers only beta installers), labelled with its version when uploaded:
/// `gh release upload vX 'RadianceMusic-Beta-Setup.exe#Radiance Music (beta) 0.8.0-beta.N'`.
pub const BETA_ASSET: &str = "RadianceMusic-Beta-Setup.exe";
pub const BETA_LABEL: &str = "Radiance Music (beta) ";

/// The beta's version, once its app says it is the beta (`set_beta`): its update state, its
/// installer and its version are then its own; the GTK app's process never sets it.
static BETA: std::sync::RwLock<Option<String>> = std::sync::RwLock::new(None);

/// This process is the Windows beta, at `version` (its ffi says so at its start).
pub fn set_beta(version: &str) {
    *BETA.write().unwrap() = Some(version.to_string());
}

fn beta() -> Option<String> {
    BETA.read().unwrap().clone()
}

/// This process is the Windows beta (its report and rating say so, its log goes along).
pub fn is_beta() -> bool {
    beta().is_some()
}

/// The version this app is: the beta's own, or this build's.
pub fn version_here() -> String {
    beta().unwrap_or_else(|| current().to_string())
}

/// The beta's install from where its exe is: its folder when the installer's Uninstall.exe is
/// beside it, else a copy run from somewhere else.
pub fn beta_install(exe: &Path) -> Install {
    match exe.parent() {
        Some(root) if root.join("Uninstall.exe").exists() => Install::Beta(root.to_path_buf()),
        _ => Install::Other,
    }
}

impl Install {
    pub fn here() -> Install {
        // a screenshot copy may play another kind (design checks of the pop-up)
        if std::env::var_os("RADIANCE_SHOT").is_some() {
            match std::env::var("RADIANCE_SHOT_INSTALL").as_deref() {
                Ok("windows") => return Install::Windows(PathBuf::from("C:/Radiance Music")),
                Ok("tarball") => return Install::Tarball,
                Ok("flatpak") => return Install::Flatpak,
                _ => {}
            }
        }
        if std::env::var_os("FLATPAK_ID").is_some() || Path::new("/.flatpak-info").exists() {
            return Install::Flatpak;
        }
        if in_snap() {
            return Install::Snap;
        }
        let Ok(exe) = std::env::current_exe() else { return Install::Other };
        if beta().is_some() {
            return beta_install(&exe);
        }
        if cfg!(windows) {
            // <folder>\bin\radiance-music.exe, the installer's Uninstall.exe in <folder>
            if let Some(root) = exe.parent().and_then(|b| b.parent()) {
                if root.join("Uninstall.exe").exists() {
                    return Install::Windows(root.to_path_buf());
                }
            }
            return Install::Other;
        }
        if exe == glib::home_dir().join(".local/bin/radiance-music") {
            return Install::Tarball;
        }
        Install::Other
    }

    /// The release's file this kind of install takes.
    pub fn asset(&self) -> Option<&'static str> {
        match self {
            Install::Windows(_) => Some("RadianceMusic-Setup.exe"),
            Install::Tarball => Some("RadianceMusic-linux-x86_64.tar.gz"),
            Install::Flatpak => Some("RadianceMusic.flatpak"),
            Install::Beta(_) => Some(BETA_ASSET),
            Install::Snap | Install::Other => None,
        }
    }

    /// For the count of updates.
    fn platform(&self) -> &'static str {
        match self {
            Install::Windows(_) | Install::Beta(_) => "windows",
            Install::Tarball => "linux",
            Install::Flatpak => "flatpak",
            Install::Snap => "snap",
            Install::Other => "other",
        }
    }
}

/// This copy runs as Radiance Music's snap (its launcher sets SNAP_NAME).
pub fn in_snap() -> bool {
    std::env::var_os("SNAP").is_some() && std::env::var("SNAP_NAME").is_ok_and(|n| n == "radiance-music")
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    /// The release's notes (GitHub's markdown).
    pub notes: String,
    /// The release's page on GitHub.
    pub page: String,
    /// This install's file: where, how big, its SHA-256 (empty: none for it).
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

impl Release {
    /// Its notes' "- " lines, the first few, plainly.
    pub fn bullets(&self, most: usize) -> Vec<String> {
        self.notes.lines().map(str::trim).filter_map(|l| l.strip_prefix("- ")).map(|l| l.replace("**", "")).take(most).collect()
    }

    fn to_json(&self) -> Value {
        json!({"version": self.version, "notes": self.notes, "page": self.page, "url": self.url, "size": self.size, "sha256": self.sha256})
    }

    fn from_json(v: &Value) -> Option<Release> {
        Some(Release {
            version: v["version"].as_str()?.to_string(),
            notes: v["notes"].as_str().unwrap_or_default().to_string(),
            page: v["page"].as_str().unwrap_or_default().to_string(),
            url: v["url"].as_str().unwrap_or_default().to_string(),
            size: v["size"].as_u64().unwrap_or(0),
            sha256: v["sha256"].as_str().unwrap_or_default().to_string(),
        })
    }
}

/// This version.
pub fn current() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Whether version `a` comes after `b` (0.7.13 after 0.7.9).
pub fn newer(a: &str, b: &str) -> bool {
    let parts = |v: &str| v.trim_start_matches('v').split(['.', '-']).map(|p| p.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    let (x, y) = (parts(a), parts(b));
    for i in 0..x.len().max(y.len()) {
        let (p, q) = (x.get(i).copied().unwrap_or(0), y.get(i).copied().unwrap_or(0));
        if p != q {
            return p > q;
        }
    }
    false
}

/// Our own site's host: the only one a test machine's key goes to.
const OWN_HOST: &str = "https://downloads.radianceph.com/";

/// The header that lets our own testing past downloads.radianceph.com's limit (owner, 2026-11-08: "may exception
/// tayo dahil tayo ay nag tetest"): the test machine's key (RADIANCE_TESTER_KEY), on our own site's requests only.
fn tester_header(url: &str, key: Option<&str>) -> Option<(&'static str, String)> {
    let key = key?.trim();
    (url.starts_with(OWN_HOST) && !key.is_empty()).then(|| ("X-Radiance-Tester", key.to_string()))
}

/// GitHub's copy of a file from our own site (the computer app's releases only), for when ours refuses or fails.
fn github_copy(url: &str, version: &str) -> Option<String> {
    let rest = url.strip_prefix(OWN_HOST)?.strip_prefix("radiance-music/")?;
    let name = rest.rsplit('/').next().filter(|n| !n.is_empty())?;
    Some(format!("https://github.com/RadianceADM/radiance-music/releases/download/v{version}/{name}"))
}

fn tester_key() -> Option<String> {
    std::env::var("RADIANCE_TESTER_KEY").ok()
}

/// Where the newest release is asked for, in turn: our own copy first, then GitHub's; the beta: GitHub's list.
fn feeds(beta: bool) -> Vec<&'static str> {
    if beta {
        vec![RELEASES]
    } else {
        vec![OWN_LATEST, LATEST]
    }
}

/// The first feed that answers and reads, in turn (the last one's failure when none does).
fn ask_feeds(feeds: &[String]) -> Result<Value, String> {
    let mut last = String::from("no feed to ask");
    for feed in feeds {
        let mut req = net::agent().get(feed).header("Accept", "application/vnd.github+json");
        if let Some((name, value)) = tester_header(feed, tester_key().as_deref()) {
            req = req.header(name, &value);
        }
        let answer = req.call();
        match answer {
            Ok(mut resp) => match resp.body_mut().read_json::<Value>() {
                Ok(j) => return Ok(j),
                Err(e) => last = format!("the answer didn't read ({e})"),
            },
            Err(e) => last = format!("couldn't reach {} ({e})", if feed.contains("github.com") { "GitHub" } else { "radianceph.com" }),
        }
        crate::log::note(&format!("update check: {feed}: {last}"));
    }
    Err(last)
}

/// The newest release, with this install's file. (Worker thread.)
pub fn latest(install: &Install) -> Result<Release, String> {
    // RADIANCE_UPDATE_FEED: another answer in GitHub's shape (end-to-end tests of updating)
    let feeds: Vec<String> = match std::env::var("RADIANCE_UPDATE_FEED") {
        Ok(f) => vec![f],
        Err(_) => feeds(beta().is_some()).into_iter().map(String::from).collect(),
    };
    let j = ask_feeds(&feeds)?;
    if beta().is_some() {
        // no beta on the newest releases: nothing newer (the version on now)
        return Ok(beta_from(&j).unwrap_or_else(|| Release { version: version_here(), ..Default::default() }));
    }
    release_from(&j, install).ok_or_else(|| "the newest release didn't read".into())
}

/// The beta's release: the newest of `j` (GitHub's list, newest first; or one release) that
/// carries its labelled installer.
fn beta_from(j: &Value) -> Option<Release> {
    match j.as_array() {
        Some(list) => list.iter().find_map(release_from_beta),
        None => release_from_beta(j),
    }
}

fn release_from(j: &Value, install: &Install) -> Option<Release> {
    let version = j["tag_name"].as_str()?.trim_start_matches('v').to_string();
    let asset = install.asset().and_then(|name| j["assets"].as_array()?.iter().find(|a| a["name"].as_str() == Some(name)));
    Some(Release {
        version,
        notes: j["body"].as_str().unwrap_or_default().to_string(),
        page: j["html_url"].as_str().unwrap_or_default().to_string(),
        url: asset.and_then(|a| a["browser_download_url"].as_str()).unwrap_or_default().to_string(),
        size: asset.and_then(|a| a["size"].as_u64()).unwrap_or(0),
        sha256: asset.and_then(|a| a["digest"].as_str()).and_then(|d| d.strip_prefix("sha256:")).unwrap_or_default().to_string(),
    })
}

/// The beta's release: the computer release's beta installer, its version from its label;
/// none without the installer or its label (nothing is offered then).
fn release_from_beta(j: &Value) -> Option<Release> {
    let asset = j["assets"].as_array()?.iter().find(|a| a["name"].as_str() == Some(BETA_ASSET))?;
    let version = asset["label"].as_str()?.strip_prefix(BETA_LABEL)?.trim().to_string();
    if version.is_empty() {
        return None;
    }
    Some(Release {
        version,
        notes: j["body"].as_str().unwrap_or_default().to_string(),
        page: j["html_url"].as_str().unwrap_or_default().to_string(),
        url: asset["browser_download_url"].as_str().unwrap_or_default().to_string(),
        size: asset["size"].as_u64().unwrap_or(0),
        sha256: asset["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).unwrap_or_default().to_string(),
    })
}

// ---- what it remembers (update.json beside settings.json; the beta's own update-beta.json:
// the two apps share the folder, and the GTK app must never be offered the beta's installer) ----

fn state_path() -> PathBuf {
    let name = if beta().is_some() { "update-beta.json" } else { "update.json" };
    glib::user_config_dir().join("radiance-music").join(name)
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
        let _ = fs::rename(&tmp, &path);
    }
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Forgets what the looks found and what was decided (Remind Me Later, Skip This Version): a
/// fresh start for the tests (each starts from nothing).
pub fn forget() {
    let _ = fs::remove_file(state_path());
}

/// What the last look found (shown at once at the next start, before looking again).
pub fn remembered() -> Option<Release> {
    Release::from_json(&load()["release"])
}

/// A look done: when, and what it found.
pub fn looked(r: &Release) {
    let mut v = load();
    v["checked"] = json!(now());
    v["release"] = r.to_json();
    save(&v);
}

/// Whether the pop-up asks about `version` now: not the version skipped, not
/// within a day of "Remind me later".
pub fn asks(version: &str) -> bool {
    let v = load();
    asks_at(version, v["skipped"].as_str(), v["later"].as_u64().unwrap_or(0), now())
}

pub fn asks_at(version: &str, skipped: Option<&str>, later_until: u64, now: u64) -> bool {
    skipped != Some(version) && now >= later_until
}

/// "Remind me later": the pop-up again in a day.
pub fn remind_later() {
    let mut v = load();
    v["later"] = json!(now() + LATER.as_secs());
    save(&v);
}

/// "Skip this version": no pop-up about it (the next version asks again).
pub fn skip(version: &str) {
    let mut v = load();
    v["skipped"] = json!(version);
    save(&v);
}

// ---- downloading and checking ----

/// Where downloads are kept. On Windows not in GLib's cache folder: that's the old Internet
/// Explorer cache (INetCache), and Windows treats an installer started from there as from the
/// internet ("The publisher could not be verified", found on a real Windows 11): the app's
/// own folder in AppData\Local instead.
fn folder() -> PathBuf {
    let base = if cfg!(windows) { glib::user_data_dir() } else { glib::user_cache_dir() };
    base.join("radiance-music").join("update")
}

/// Where release `r`'s file is kept once it's whole and checked.
pub fn file(r: &Release, install: &Install) -> PathBuf {
    folder().join(format!("{}-{}", r.version, install.asset().unwrap_or("download")))
}

/// Downloads of versions that are on now (or older) aren't needed any more, nor ones older
/// than the newest the last look found (a newer one replaces them).
pub fn clean() {
    let Ok(dir) = fs::read_dir(folder()) else { return };
    let newest = remembered().map(|r| r.version);
    let asset = Install::here().asset().unwrap_or("download");
    let here = version_here();
    for f in dir.flatten() {
        let name = f.file_name().to_string_lossy().to_string();
        if stale(&name, asset, &here, newest.as_deref()) {
            if f.path().is_dir() {
                let _ = fs::remove_dir_all(f.path());
            } else {
                let _ = fs::remove_file(f.path());
            }
        }
    }
}

/// A download that isn't needed any more: this app's own (its installer's name, or what's
/// made from it), of the version on now or older, or older than the newest found. The other
/// app's (the GTK app and the beta share the folder) is never touched.
fn stale(name: &str, asset: &str, here: &str, newest: Option<&str>) -> bool {
    let Some(version) = own_version(name, asset) else { return false };
    !newer(version, here) || newest.is_some_and(|n| newer(n, version))
}

/// The version of this app's own download `name` (its installer, half downloaded, or its log:
/// "{version}-{asset's name}…"); None for the other app's.
fn own_version<'a>(name: &'a str, asset: &str) -> Option<&'a str> {
    let stem = asset.rsplit_once('.').map(|(s, _)| s).unwrap_or(asset);
    name.find(&format!("-{stem}.")).map(|at| &name[..at])
}

/// Before a download: this app's files of other versions go (the other app's stay).
fn leftover(name: &str, version: &str, asset: &str) -> bool {
    own_version(name, asset).is_some_and(|v| v != version)
}

/// Already downloaded and checked (from an earlier "Update now").
pub fn ready(r: &Release, install: &Install) -> bool {
    !r.url.is_empty() && file(r, install).exists()
}

/// Downloads `r`'s file (picking up where it stopped), telling how far along
/// (bytes, of), and checks it: its SHA-256 must be the one GitHub gives. Stops
/// when `stop` is set. (Worker thread; uses the app's own HTTPS, never
/// GStreamer's: Windows' GnuTLS can't check GitHub's certificate there.)
pub fn download(r: &Release, install: &Install, stop: &AtomicBool, mut progress: impl FnMut(u64, u64)) -> Result<PathBuf, String> {
    if r.url.is_empty() {
        return Err("the newest release has no file for this computer".into());
    }
    let done = file(r, install);
    if done.exists() {
        return Ok(done);
    }
    fs::create_dir_all(folder()).map_err(|e| format!("couldn't make room for it ({e})"))?;
    // this app's leftovers of other versions aren't needed any more (the other app's stay)
    if let Ok(dir) = fs::read_dir(folder()) {
        let asset = install.asset().unwrap_or("download");
        for f in dir.flatten() {
            if leftover(&f.file_name().to_string_lossy(), &r.version, asset) {
                let _ = fs::remove_file(f.path());
            }
        }
    }
    let part = done.with_extension("part");
    let mut tries = 0;
    // our own copy first; refused or failed, GitHub's copy of the same file, on from the bytes already here
    let mut url = r.url.clone();
    loop {
        let from = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
        progress(from, r.size);
        let got = fetch(&url, &part, from, r.size, stop, &mut progress);
        if got.is_err() && !stop.load(Ordering::Relaxed) {
            if let Some(theirs) = github_copy(&url, &r.version) {
                crate::log::note(&format!("update download: {} ({}); on from GitHub's copy", url, got.as_ref().err().map(String::as_str).unwrap_or("")));
                url = theirs;
                continue;
            }
        }
        if stop.load(Ordering::Relaxed) {
            return Err("stopped".into());
        }
        let have = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
        if got.is_ok() && (r.size == 0 || have == r.size) {
            break;
        }
        tries += 1;
        if tries >= 5 {
            return Err(got.err().unwrap_or_else(|| "the download came short".into()));
        }
        std::thread::sleep(Duration::from_secs(2 * tries)); // a dropped connection: go on from here
    }
    if !r.sha256.is_empty() {
        let sum = sha256(&part).map_err(|e| format!("couldn't read the download ({e})"))?;
        if !sum.eq_ignore_ascii_case(&r.sha256) {
            let _ = fs::remove_file(&part);
            return Err("the download isn't the release's own file; try again".into());
        }
    }
    fs::rename(&part, &done).map_err(|e| format!("couldn't keep the download ({e})"))?;
    Ok(done)
}

fn fetch(url: &str, part: &Path, from: u64, size: u64, stop: &AtomicBool, progress: &mut impl FnMut(u64, u64)) -> Result<(), String> {
    let mut req = net::agent().get(url);
    if from > 0 {
        req = req.header("Range", &format!("bytes={from}-"));
    }
    if let Some((name, value)) = tester_header(url, tester_key().as_deref()) {
        req = req.header(name, &value);
    }
    let mut resp = match req.call() {
        Ok(r) => r,
        Err(ureq::Error::StatusCode(416)) => return Ok(()), // all here already
        Err(e) => return Err(format!("the download stopped ({e})")),
    };
    let append = resp.status().as_u16() == 206;
    let mut out = fs::OpenOptions::new().create(true).write(true).append(append).truncate(!append).open(part).map_err(|e| e.to_string())?;
    let mut have = if append { from } else { 0 };
    let mut body = resp.body_mut().with_config().limit(u64::MAX).reader();
    let mut buf = vec![0u8; 256 << 10];
    let mut shown = have;
    loop {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        let n = body.read(&mut buf).map_err(|e| format!("the download stopped ({e})"))?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n]).map_err(|e| format!("couldn't save the download ({e})"))?;
        have += n as u64;
        if have - shown >= 1 << 20 {
            shown = have;
            progress(have, size);
        }
    }
    out.flush().map_err(|e| e.to_string())?;
    progress(have, size);
    Ok(())
}

pub fn sha256(path: &Path) -> std::io::Result<String> {
    let mut sum = glib::Checksum::new(glib::ChecksumType::Sha256).expect("SHA-256");
    let mut f = fs::File::open(path)?;
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        sum.update(&buf[..n]);
    }
    Ok(sum.string().unwrap_or_default().to_string())
}

/// Adds one to the count of updates. Never in the way.
pub fn counted(r: &Release, install: &Install) {
    let _ = net::agent().get(COUNT).query("event", "update").query("platform", install.platform()).query("version", &r.version).call();
}

// ---- putting it on ----

/// The environment variable a just-updated copy is started with: it waits for
/// this one to have closed before it opens (one Radiance Music at a time).
pub const AFTER_UPDATE: &str = "RADIANCE_AFTER_UPDATE";

/// Puts the downloaded `file` on, for when this app has closed, and opens the
/// new version then (the Flatpak's new version is already on: it just opens).
/// Returns once that's arranged: the app quits right after.
/// (Window thread is fine: it only starts things.)
pub fn put_on(install: &Install, file: &Path) -> Result<(), String> {
    match install {
        Install::Windows(root) => windows_put_on(root, file, &version_of(file, "RadianceMusic-Setup.exe"), false),
        Install::Beta(root) => windows_put_on(root, file, &version_of(file, BETA_ASSET), true),
        Install::Tarball => tarball_put_on(file),
        Install::Flatpak => flatpak_open_newest(),
        Install::Snap => Err("the Snap Store updates this copy by itself".into()),
        Install::Other => Err("this copy updates where it came from".into()),
    }
}

/// The new installer, quietly, once this app (and anything it started from its
/// folder: the splitter, yt-dlp; given 30 s, then stopped: an interrupted split
/// runs again later) has closed; then the app again. PowerShell,
/// which every Windows 10 and 11 has, waits and runs it, hidden.
/// The version a download is of: its name is "{version}-{asset}" (a beta's version has its own
/// hyphens: "0.8.0-beta.4-RadianceMusic-Beta-Setup.exe").
fn version_of(file: &Path, asset: &str) -> String {
    let name = file.to_string_lossy();
    let name = name.rsplit(['\\', '/']).next().unwrap_or_default();
    match name.strip_suffix(&format!("-{asset}")) {
        Some(v) => v.to_string(),
        None => name.split('-').next().unwrap_or_default().to_string(),
    }
}

fn windows_put_on(root: &Path, setup: &Path, version: &str, beta: bool) -> Result<(), String> {
    let script = windows_script(std::process::id(), root, setup, version, beta);
    let mut cmd = std::process::Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-Command", &script]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console flash
    }
    cmd.spawn().map(|_| ()).map_err(|e| format!("couldn't start the installer ({e})"))
}

fn windows_script(pid: u32, root: &Path, setup: &Path, version: &str, beta: bool) -> String {
    let q = |p: &str| p.replace('\'', "''");
    let root = root.display().to_string();
    // the beta: its exe in its folder, its own uninstall key (its folder is where it reinstalls:
    // D:\Apps\Radiance Music (beta) stays there, never the installer's default on C:)
    let (exe, key) = if beta { (format!("{root}\\RadianceMusicBeta.exe"), "RadianceMusicBeta") } else { (format!("{root}\\bin\\radiance-music.exe"), "RadianceMusic") };
    // What opens once it's in: the beta itself; today's app, whichever app the installer left in its folder. The
    // release that switches to the WinUI app installs it there as RadianceMusic.exe and takes the GTK files away
    // (owner, 2026-11-08: "Patch now, delete at switch"), so the WinUI app opens; before it, the GTK app as always.
    let open = if beta {
        format!("Start-Process -FilePath '{}'; L 'opened the app'", q(&exe))
    } else {
        format!(
            "$app = '{}'; if (-not (Test-Path $app)) {{ $app = '{}' }}; Start-Process -FilePath $app; L ('opened ' + $app)",
            q(&format!("{root}\\RadianceMusic.exe")),
            q(&exe)
        )
    };
    let log = setup.with_extension("log").display().to_string();
    // The installer starts straight (CreateProcess, not the shell: no "unknown publisher" check),
    // its /D= last and unquoted, spaces and all, in one argument string as is. It's done when its
    // uninstall entry says the new version (it can end before its files are all there), then the
    // app opens. Each step goes in a log beside the installer, for when something goes wrong.
    format!(
        "$ErrorActionPreference='SilentlyContinue'; $log='{log}'; function L($m) {{ Add-Content -Path $log -Value ((Get-Date -Format o) + ' ' + $m) }}; \
         L 'waiting for the app to close'; Wait-Process -Id {pid} -Timeout 60; \
         $root='{root}'; $mine={{ Get-Process | Where-Object {{ $_.Path -like ($root + '\\*') }} }}; \
         $t=0; while ((& $mine) -and $t -lt 60) {{ Start-Sleep -Milliseconds 500; $t++ }}; & $mine | Stop-Process -Force; Start-Sleep 1; \
         L 'installing'; $si = New-Object System.Diagnostics.ProcessStartInfo; $si.FileName = '{setup}'; $si.Arguments = '/S /D={root}'; $si.UseShellExecute = $false; \
         $p = [System.Diagnostics.Process]::Start($si); $p.WaitForExit(); L ('installer done: ' + $p.ExitCode); \
         $k = 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{key}'; $t=0; \
         while (((Get-ItemProperty $k).DisplayVersion -ne '{version}') -and $t -lt 300) {{ Start-Sleep 1; $t++ }}; L ('installed: ' + (Get-ItemProperty $k).DisplayVersion); \
         {open}",
        log = q(&log),
        root = q(&root),
        setup = q(&setup.display().to_string()),
        open = open,
        version = q(version),
        key = key,
    )
}

/// The new tarball's install.sh (it replaces ~/.local/bin/radiance-music; this
/// app keeps running from the file it started from), then the new app, which
/// waits for this one to close.
fn tarball_put_on(tarball: &Path) -> Result<(), String> {
    let into = folder().join("unpacked");
    let _ = fs::remove_dir_all(&into);
    fs::create_dir_all(&into).map_err(|e| e.to_string())?;
    let ok = std::process::Command::new("tar").arg("-xzf").arg(tarball).arg("-C").arg(&into).status().map_err(|e| format!("couldn't unpack it ({e})"))?;
    if !ok.success() {
        return Err("couldn't unpack it".into());
    }
    let install = fs::read_dir(&into).map_err(|e| e.to_string())?.flatten().map(|d| d.path().join("install.sh")).find(|p| p.exists()).ok_or("the download has no install.sh")?;
    let out = std::process::Command::new("sh").arg(&install).output().map_err(|e| format!("couldn't run install.sh ({e})"))?;
    if !out.status.success() {
        return Err(format!("install.sh didn't finish: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    let _ = fs::remove_dir_all(&into);
    let bin = glib::home_dir().join(".local/bin/radiance-music");
    let mut cmd = std::process::Command::new(&bin);
    cmd.env(AFTER_UPDATE, "1").stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0); // its own: it lives on after this app quits
    }
    cmd.spawn().map(|_| ()).map_err(|e| format!("couldn't open the new version ({e})"))
}

/// A just-updated copy: wait (up to 20 s) for the old one to let go of the app's
/// name on the session bus, so this one opens as the app rather than handing
/// over to the closing one. Window thread, before the app starts.
pub fn wait_for_old() {
    if std::env::var_os(AFTER_UPDATE).is_none() {
        return;
    }
    std::env::remove_var(AFTER_UPDATE);
    let Ok(bus) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) else { return };
    for _ in 0..100 {
        let owned = bus
            .call_sync(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", "org.freedesktop.DBus", "NameHasOwner", Some(&(crate::APP_ID,).to_variant()), Some(glib::VariantTy::new("(b)").unwrap()), gio::DBusCallFlags::NONE, 1000, gio::Cancellable::NONE)
            .ok()
            .and_then(|v| v.child_value(0).get::<bool>())
            .unwrap_or(false);
        if !owned {
            return;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

// ---- our Flatpak: Flatpak's own update, through its portal ----

const PORTAL: &str = "org.freedesktop.portal.Flatpak";
const PORTAL_PATH: &str = "/org/freedesktop/portal/Flatpak";
const MONITOR: &str = "org.freedesktop.portal.Flatpak.UpdateMonitor";

/// How Flatpak's update goes.
#[derive(Clone, Debug, PartialEq)]
pub enum FlatpakStep {
    /// How far along, 0 to 1.
    Going(f64),
    /// The new version is on (beside this one, which goes on running).
    Done,
    /// Flatpak found nothing newer in our repo (yet).
    NothingNew,
    Failed(String),
}

/// Asks Flatpak to update this app from our repo, like the Software app would
/// (the first time, the computer asks once whether Radiance Music may update
/// itself). `step` hears how it goes. Window thread.
pub fn flatpak_update(step: impl Fn(FlatpakStep) + 'static) {
    let step = std::rc::Rc::new(step);
    glib::spawn_future_local(async move {
        let bus = match gio::bus_get_future(gio::BusType::Session).await {
            Ok(b) => b,
            Err(e) => return step(FlatpakStep::Failed(format!("couldn't reach Flatpak ({})", e.message()))),
        };
        let monitor = match bus.call_future(Some(PORTAL), PORTAL_PATH, PORTAL, "CreateUpdateMonitor", Some(&monitor_args()), Some(glib::VariantTy::new("(o)").unwrap()), gio::DBusCallFlags::NONE, 30_000).await {
            Ok(v) => v.child_value(0).str().unwrap_or_default().to_string(),
            Err(e) => return step(FlatpakStep::Failed(format!("Flatpak can't update apps here ({})", e.message()))),
        };
        // how it goes: Progress signals, until done, nothing to do, or failed
        let keep: std::rc::Rc<std::cell::RefCell<Option<gio::SignalSubscription>>> = Default::default();
        let (s, k, b) = (step.clone(), keep.clone(), bus.clone());
        let path = monitor.clone();
        let sub = bus.subscribe_to_signal(None, Some(MONITOR), Some("Progress"), Some(&monitor), None, gio::DBusSignalFlags::NONE, move |sig| {
            let info = glib::VariantDict::new(Some(&sig.parameters.child_value(0)));
            let num = |key: &str| info.lookup::<u32>(key).ok().flatten().unwrap_or(0);
            let (n_ops, op, progress, status) = (num("n_ops").max(1), num("op"), num("progress"), num("status"));
            let end = match status {
                0 => {
                    s(FlatpakStep::Going(((op as f64) + progress as f64 / 100.0) / n_ops as f64));
                    false
                }
                1 => {
                    s(FlatpakStep::NothingNew);
                    true
                }
                2 => {
                    s(FlatpakStep::Done);
                    true
                }
                _ => {
                    let why = info.lookup::<String>("error_message").ok().flatten().filter(|m| !m.is_empty()).unwrap_or_else(|| "Flatpak didn't update it".into());
                    s(FlatpakStep::Failed(why));
                    true
                }
            };
            if end {
                k.borrow_mut().take(); // no more signals
                let _ = b.call(Some(PORTAL), &path, MONITOR, "Close", None, None, gio::DBusCallFlags::NONE, -1, gio::Cancellable::NONE, |_| {});
            }
        });
        *keep.borrow_mut() = Some(sub);
        step(FlatpakStep::Going(0.0));
        if let Err(e) = bus.call_future(Some(PORTAL), &monitor, MONITOR, "Update", Some(&update_args()), None, gio::DBusCallFlags::NONE, 30_000).await {
            keep.borrow_mut().take();
            step(FlatpakStep::Failed(format!("Flatpak didn't start the update ({})", e.message())));
        }
    });
}

/// CreateUpdateMonitor's (a{sv}): no options.
fn monitor_args() -> glib::Variant {
    glib::Variant::tuple_from_iter([glib::VariantDict::new(None).end()])
}

/// UpdateMonitor.Update's (sa{sv}): no parent window, no options.
fn update_args() -> glib::Variant {
    glib::Variant::tuple_from_iter(["".to_variant(), glib::VariantDict::new(None).end()])
}

/// Spawn's (ayaaya{uh}a{ss}ua{sv}): cwd, argv, no fds, the env saying "wait for
/// the old one", LATEST_VERSION (1 << 1), no options.
fn spawn_args() -> Result<glib::Variant, String> {
    let ty = glib::VariantTy::new("(ayaaya{uh}a{ss}ua{sv})").unwrap();
    glib::Variant::parse(Some(ty), &format!("(b'/', [b'radiance-music'], {{}}, {{'{AFTER_UPDATE}': '1'}}, 2, {{}})")).map_err(|e| e.to_string())
}

/// Opens the newest installed version of this Flatpak (it waits for this one to
/// close). Window thread.
pub fn flatpak_open_newest() -> Result<(), String> {
    let args = spawn_args()?;
    let bus = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).map_err(|e| e.to_string())?;
    bus.call_sync(Some(PORTAL), PORTAL_PATH, PORTAL, "Spawn", Some(&args), Some(glib::VariantTy::new("(u)").unwrap()), gio::DBusCallFlags::NONE, 10_000, gio::Cancellable::NONE)
        .map(|_| ())
        .map_err(|e| format!("couldn't open the new version ({})", e.message()))
}

/// `--probe-update [apply]`: the whole update without the window (end-to-end
/// tests): looks, downloads and checks it (the Flatpak: Flatpak's update), and
/// with `apply` puts it on and opens the new version, as Restart to Update does.
pub fn probe(apply: bool) -> glib::ExitCode {
    let install = Install::here();
    println!("this copy: {} ({install:?})", current());
    let r = match latest(&install) {
        Ok(r) => r,
        Err(e) => {
            println!("couldn't look: {e}");
            return glib::ExitCode::FAILURE;
        }
    };
    println!("newest: {} (file {:?}, {} bytes, sha256 {:?})", r.version, r.url, r.size, r.sha256);
    if !newer(&r.version, current()) {
        println!("up to date");
        return glib::ExitCode::SUCCESS;
    }
    let ok = if install == Install::Flatpak {
        let main = glib::MainLoop::new(None, false);
        let (m, result) = (main.clone(), std::rc::Rc::new(std::cell::Cell::new(false)));
        let res = result.clone();
        flatpak_update(move |s| {
            println!("flatpak: {s:?}");
            if !matches!(s, FlatpakStep::Going(_)) {
                res.set(s == FlatpakStep::Done);
                m.quit();
            }
        });
        main.run();
        result.get()
    } else {
        let stop = AtomicBool::new(false);
        match download(&r, &install, &stop, |have, of| println!("downloaded {have} of {of}")) {
            Ok(f) => {
                println!("downloaded and checked: {}", f.display());
                true
            }
            Err(e) => {
                println!("couldn't download: {e}");
                false
            }
        }
    };
    if !ok {
        return glib::ExitCode::FAILURE;
    }
    if apply {
        match put_on(&install, &file(&r, &install)) {
            Ok(()) => println!("putting it on: this copy closes now, the new one opens"),
            Err(e) => {
                println!("couldn't put it on: {e}");
                return glib::ExitCode::FAILURE;
            }
        }
    }
    glib::ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- the Windows beta's updates (stage 4a, #45)

    #[test]
    fn version_of_reads_hyphenated_versions() {
        assert_eq!(version_of(Path::new("C:/x/update/0.8.0-beta.4-RadianceMusic-Beta-Setup.exe"), BETA_ASSET), "0.8.0-beta.4");
        assert_eq!(version_of(Path::new("/x/update/0.7.21-RadianceMusic-Setup.exe"), "RadianceMusic-Setup.exe"), "0.7.21");
    }

    #[test]
    fn the_beta_install_is_found_beside_its_uninstaller() {
        let dir = std::env::temp_dir().join(format!("rm-beta-install-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("RadianceMusicBeta.exe");
        fs::write(&exe, b"").unwrap();
        assert_eq!(beta_install(&exe), Install::Other, "no uninstaller: a copy run from elsewhere");
        fs::write(dir.join("Uninstall.exe"), b"").unwrap();
        assert_eq!(beta_install(&exe), Install::Beta(dir.clone()));
        assert_eq!(Install::Beta(dir.clone()).asset(), Some(BETA_ASSET));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_beta_puts_its_update_where_it_runs() {
        let script = windows_script(42, Path::new("D:\\Apps\\Radiance Music (beta)"), Path::new("C:\\u\\0.8.0-beta.4-RadianceMusic-Beta-Setup.exe"), "0.8.0-beta.4", true);
        assert!(script.contains("$si.Arguments = '/S /D=D:\\Apps\\Radiance Music (beta)'"), "{script}");
        assert!(script.contains("Uninstall\\RadianceMusicBeta'"), "its own uninstall key");
        assert!(script.contains("Radiance Music (beta)\\RadianceMusicBeta.exe"), "it opens the beta again");
        assert!(!script.contains("bin\\radiance-music.exe"));
        let gtk = windows_script(42, Path::new("C:\\Radiance Music"), Path::new("C:\\u\\0.7.21-RadianceMusic-Setup.exe"), "0.7.21", false);
        assert!(gtk.contains("Uninstall\\RadianceMusic'") && gtk.contains("bin\\radiance-music.exe"), "the GTK app's as before");
    }

    /// Owner, 2026-11-08 ("Patch now, delete at switch"): today's Windows app, updated by the release that switches
    /// to the WinUI app, opens what that installer left in its folder (RadianceMusic.exe, the WinUI app's name there)
    /// and, before the switch, itself as always. The WinUI installer removes the GTK files (stage 5).
    #[test]
    fn the_gtk_app_opens_the_winui_app_after_the_switch() {
        let gtk = windows_script(42, Path::new("C:\\Users\\Ana\\AppData\\Local\\Programs\\Radiance Music"), Path::new("C:\\u\\0.9.0-RadianceMusic-Setup.exe"), "0.9.0", false);
        let switched = gtk.find("Radiance Music\\RadianceMusic.exe").expect("it looks for the WinUI app first");
        let as_before = gtk.find("Radiance Music\\bin\\radiance-music.exe").expect("else itself, as before");
        assert!(switched < as_before, "the WinUI app first: {gtk}");
        assert!(gtk.contains("Test-Path"), "the one that's there: {gtk}");
        let beta = windows_script(42, Path::new("D:\\Apps\\Radiance Music (beta)"), Path::new("C:\\u\\b.exe"), "0.8.0-beta.7", true);
        assert!(!beta.contains("Test-Path '"), "the beta opens itself, nothing to choose: {beta}");
    }

    #[test]
    fn a_release_without_the_beta_label_offers_nothing() {
        let asset = |label: &str| serde_json::json!({ "name": BETA_ASSET, "label": label, "browser_download_url": "https://x/b.exe", "size": 149, "digest": "sha256:abc" });
        let release = |assets: Vec<Value>| serde_json::json!({ "tag_name": "v0.7.21", "body": "- one", "html_url": "https://x/r", "assets": assets });
        assert!(release_from_beta(&release(vec![])).is_none(), "no beta installer");
        assert!(release_from_beta(&release(vec![asset("")])).is_none(), "no label");
        let r = release_from_beta(&release(vec![asset("Radiance Music (beta) 0.8.0-beta.4")])).unwrap();
        assert_eq!((r.version.as_str(), r.url.as_str(), r.size, r.sha256.as_str()), ("0.8.0-beta.4", "https://x/b.exe", 149, "abc"));
    }

    /// Final review: the beta read only the newest release; one without the beta's installer hid a
    /// beta on the release before it (and Look for Updates said the release "didn't read").
    #[test]
    fn the_beta_finds_its_installer_on_an_older_release() {
        let asset = |label: &str| serde_json::json!({ "name": BETA_ASSET, "label": label, "browser_download_url": "https://x/b.exe", "size": 149, "digest": "sha256:abc" });
        let release = |tag: &str, assets: Vec<Value>| serde_json::json!({ "tag_name": tag, "body": "- one", "html_url": "https://x/r", "assets": assets });
        let list = serde_json::json!([release("v0.7.22", vec![]), release("v0.7.21", vec![asset("Radiance Music (beta) 0.8.0-beta.5")])]);
        assert_eq!(beta_from(&list).map(|r| r.version), Some("0.8.0-beta.5".to_string()));
        assert!(beta_from(&serde_json::json!([release("v0.7.22", vec![])])).is_none(), "none yet: nothing newer");
        // one release on its own (a test feed's answer)
        assert_eq!(beta_from(&release("v0.7.22", vec![asset("Radiance Music (beta) 0.8.0-beta.6")])).map(|r| r.version), Some("0.8.0-beta.6".to_string()));
    }

    /// Final review: a download cleared the shared folder of everything not its version, the other
    /// app's waiting installer too (the beta's took the GTK app's 0.7.21, and the other way round).
    #[test]
    fn a_download_leaves_the_other_apps_files() {
        assert!(!leftover("0.7.21-RadianceMusic-Setup.exe", "0.8.0-beta.5", BETA_ASSET), "the GTK app's");
        assert!(leftover("0.8.0-beta.4-RadianceMusic-Beta-Setup.exe", "0.8.0-beta.5", BETA_ASSET));
        assert!(leftover("0.8.0-beta.4-RadianceMusic-Beta-Setup.part", "0.8.0-beta.5", BETA_ASSET), "a half download of another version");
        assert!(!leftover("0.8.0-beta.5-RadianceMusic-Beta-Setup.part", "0.8.0-beta.5", BETA_ASSET), "its own, picked up where it stopped");
        assert!(!leftover("0.8.0-beta.4-RadianceMusic-Beta-Setup.exe", "0.7.22", "RadianceMusic-Setup.exe"), "the beta's");
        assert!(leftover("0.7.21-RadianceMusic-Setup.exe", "0.7.22", "RadianceMusic-Setup.exe"));
        // and the clean-up at the start takes this app's old half downloads and logs too
        assert!(stale("0.8.0-beta.3-RadianceMusic-Beta-Setup.part", BETA_ASSET, "0.8.0-beta.3", None));
        assert!(stale("0.8.0-beta.3-RadianceMusic-Beta-Setup.log", BETA_ASSET, "0.8.0-beta.3", None));
        assert!(!stale("0.7.21-RadianceMusic-Setup.log", BETA_ASSET, "0.8.0-beta.3", None));
    }

    #[test]
    fn each_app_cleans_only_its_own_downloads() {
        // the beta (0.8.0-beta.3) never removes the GTK app's waiting installer, nor the GTK app the beta's
        assert!(!stale("0.7.21-RadianceMusic-Setup.exe", BETA_ASSET, "0.8.0-beta.3", None));
        assert!(stale("0.8.0-beta.3-RadianceMusic-Beta-Setup.exe", BETA_ASSET, "0.8.0-beta.3", None), "the one now on");
        assert!(!stale("0.8.0-beta.4-RadianceMusic-Beta-Setup.exe", BETA_ASSET, "0.8.0-beta.3", Some("0.8.0-beta.4")));
        assert!(!stale("0.8.0-beta.4-RadianceMusic-Beta-Setup.exe", "RadianceMusic-Setup.exe", "0.7.20", None));
    }

    #[test]
    fn versions_compare_as_numbers() {
        assert!(newer("0.7.13", "0.7.12"));
        assert!(newer("0.7.10", "0.7.9"));
        assert!(newer("v1.0", "0.9.9"));
        assert!(!newer("0.7.12", "0.7.12"));
        assert!(!newer("0.7.11", "0.7.12"));
        assert!(!newer("0.7", "0.7.0"));
    }

    #[test]
    fn the_pop_up_waits_a_day_after_remind_me_later_and_skips_the_skipped_version() {
        let now = 1_000_000;
        let day = LATER.as_secs();
        assert!(asks_at("0.7.13", None, 0, now));
        assert!(!asks_at("0.7.13", None, now + day, now));
        assert!(!asks_at("0.7.13", None, now + day, now + day - 1));
        assert!(asks_at("0.7.13", None, now + day, now + day));
        assert!(!asks_at("0.7.13", Some("0.7.13"), 0, now));
        assert!(asks_at("0.7.14", Some("0.7.13"), 0, now));
    }

    /// Owner, 2026-11-08: GitHub refused update checks (403: its 60-an-hour limit per IP, shared by everyone behind
    /// one Converge IP) and its downloads timed out (504). The newest release is asked of our own copy on Hostinger
    /// first (downloads.radianceph.com, the same JSON with our addresses), GitHub only after it; the beta's list stays
    /// GitHub's (the beta isn't public).
    #[test]
    fn the_newest_release_is_asked_of_our_own_site_first() {
        assert_eq!(feeds(false), ["https://downloads.radianceph.com/radiance-music/latest.json", LATEST]);
        assert_eq!(feeds(true), [RELEASES]);
        let ours = json!({
            "tag_name": "v0.7.20", "body": "- one", "html_url": "https://github.com/RadianceADM/radiance-music/releases/tag/v0.7.20",
            "assets": [{"name": "RadianceMusic-Setup.exe", "size": 9, "digest": "sha256:aa",
                        "browser_download_url": "https://downloads.radianceph.com/radiance-music/v0.7.20/RadianceMusic-Setup.exe"}]
        });
        let w = release_from(&ours, &Install::Windows(PathBuf::from("C:/x"))).unwrap();
        assert_eq!((w.version.as_str(), w.url.as_str(), w.sha256.as_str()), ("0.7.20", "https://downloads.radianceph.com/radiance-music/v0.7.20/RadianceMusic-Setup.exe", "aa"));
    }

    /// Our own testing isn't limited on downloads.radianceph.com (owner, 2026-11-08): a test machine's key
    /// (RADIANCE_TESTER_KEY) goes on our own site's requests only, never to GitHub or anyone else.
    #[test]
    fn the_tester_key_goes_to_our_site_only() {
        assert_eq!(tester_header("https://downloads.radianceph.com/radiance-music/latest.json", Some("k1")), Some(("X-Radiance-Tester", "k1".to_string())));
        assert_eq!(tester_header("https://downloads.radianceph.com/radiance-music/v0.7.20/RadianceMusic-Setup.exe", Some("k1")).map(|h| h.0), Some("X-Radiance-Tester"));
        assert_eq!(tester_header("https://api.github.com/repos/RadianceADM/radiance-music/releases/latest", Some("k1")), None);
        assert_eq!(tester_header("https://downloads.radianceph.com.evil.example/x", Some("k1")), None);
        assert_eq!(tester_header("https://downloads.radianceph.com/radiance-music/latest.json", None), None);
        assert_eq!(tester_header("https://downloads.radianceph.com/radiance-music/latest.json", Some("  ")), None);
    }

    /// Our copy refused or failed (a limit, a 5xx, a dropped line): the download carries on from GitHub's copy of the
    /// same file (same version, the same sha256 check); never GitHub first (the Android session's catch, 2026-11-08).
    #[test]
    fn a_refused_download_carries_on_from_githubs_copy() {
        assert_eq!(
            github_copy("https://downloads.radianceph.com/radiance-music/v0.7.21/RadianceMusic-Setup.exe", "0.7.21").as_deref(),
            Some("https://github.com/RadianceADM/radiance-music/releases/download/v0.7.21/RadianceMusic-Setup.exe")
        );
        assert_eq!(github_copy("https://downloads.radianceph.com/radiance-music/latest/RadianceMusic-linux-x86_64.tar.gz", "0.7.21").as_deref(), Some("https://github.com/RadianceADM/radiance-music/releases/download/v0.7.21/RadianceMusic-linux-x86_64.tar.gz"));
        // GitHub's own address, or anyone else's: no second copy
        assert_eq!(github_copy("https://github.com/RadianceADM/radiance-music/releases/download/v0.7.21/RadianceMusic-Setup.exe", "0.7.21"), None);
        assert_eq!(github_copy("https://downloads.radianceph.com/radiance-music-android/v1.2.2/RadianceMusic.apk", "1.2.2"), None);
    }

    #[test]
    fn a_release_reads_with_this_installs_file_and_its_sha256() {
        let j = json!({
            "tag_name": "v0.7.13", "body": "## What's new\n- **Updates** pop up\n- Faster", "html_url": "https://github.com/x/y/releases/tag/v0.7.13",
            "assets": [
                {"name": "RadianceMusic-Setup.exe", "size": 9, "browser_download_url": "https://e/Setup.exe", "digest": "sha256:aa"},
                {"name": "RadianceMusic-linux-x86_64.tar.gz", "size": 5, "browser_download_url": "https://e/l.tar.gz", "digest": "sha256:bb"},
            ]
        });
        let r = release_from(&j, &Install::Tarball).unwrap();
        assert_eq!((r.version.as_str(), r.url.as_str(), r.size, r.sha256.as_str()), ("0.7.13", "https://e/l.tar.gz", 5, "bb"));
        assert_eq!(r.bullets(4), vec!["Updates pop up", "Faster"]);
        let w = release_from(&j, &Install::Windows(PathBuf::from("C:/x"))).unwrap();
        assert_eq!(w.url, "https://e/Setup.exe");
        let o = release_from(&j, &Install::Other).unwrap();
        assert!(o.url.is_empty());
        assert_eq!(Release::from_json(&r.to_json()), Some(r));
    }

    #[test]
    fn windows_runs_the_installer_quietly_into_the_same_folder_after_the_app_closes() {
        let root = PathBuf::from(r"C:\Users\Ana's PC\AppData\Local\Programs\Radiance Music");
        let s = windows_script(4242, &root, Path::new(r"C:\cache\0.7.13-RadianceMusic-Setup.exe"), "0.7.13", false);
        assert!(s.contains("Wait-Process -Id 4242 -Timeout 60"));
        assert!(s.contains(r"$log='C:\cache\0.7.13-RadianceMusic-Setup.log'"));
        // NSIS: /D= last and unquoted (spaces and all); a ' doubled inside PowerShell's '...'
        // started straight (no shell: no "unknown publisher" prompt), arguments as they are
        assert!(s.contains(r"$si.Arguments = '/S /D=C:\Users\Ana''s PC\AppData\Local\Programs\Radiance Music'; $si.UseShellExecute = $false"));
        assert!(!s.contains("Start-Process -FilePath 'C:\\cache"));
        // done when its uninstall entry says the new version
        assert!(s.contains("DisplayVersion -ne '0.7.13'"));
        assert_eq!(version_of(Path::new(r"C:\cache\0.7.13-RadianceMusic-Setup.exe"), "RadianceMusic-Setup.exe"), "0.7.13");
        // then opens the app the installer left there (the WinUI app after the switch, else itself)
        assert!(s.contains(r"$app = 'C:\Users\Ana''s PC\AppData\Local\Programs\Radiance Music\RadianceMusic.exe'; if (-not (Test-Path $app)) { $app = 'C:\Users\Ana''s PC\AppData\Local\Programs\Radiance Music\bin\radiance-music.exe' }; Start-Process -FilePath $app"), "{s}");
        assert!(!s.contains('"'));
    }

    #[test]
    fn the_flatpak_portal_gets_its_own_shapes() {
        assert_eq!(monitor_args().type_().as_str(), "(a{sv})");
        assert_eq!(update_args().type_().as_str(), "(sa{sv})");
        let spawn = spawn_args().unwrap();
        assert_eq!(spawn.type_().as_str(), "(ayaaya{uh}a{ss}ua{sv})");
        // argv[0] is the app's command, NUL-terminated as the portal wants its bytestrings
        assert_eq!(spawn.child_value(1).child_value(0).fixed_array::<u8>().unwrap(), b"radiance-music\0");
        let env = spawn.child_value(3).get::<std::collections::HashMap<String, String>>().unwrap();
        assert_eq!(env.get(AFTER_UPDATE).map(String::as_str), Some("1"));
        assert_eq!(spawn.child_value(4).get::<u32>(), Some(2));
    }

    #[test]
    fn sha256_of_a_file() {
        let p = std::env::temp_dir().join(format!("rm-update-test-{}", std::process::id()));
        fs::write(&p, b"abc").unwrap();
        assert_eq!(sha256(&p).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let _ = fs::remove_file(p);
    }
}
