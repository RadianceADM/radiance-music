//! What the two-part split (spleeter.rs, #43) needs, fetched once into the app's own folder:
//! Spleeter's two models (Hugging Face, a fixed commit) and ONNX Runtime's library for this
//! computer (Microsoft's own release: on Linux the library out of its .tgz; on Windows just the
//! DLL's entry out of its 83 MB .zip, by byte ranges, about 6 MB). Every file is checked against
//! its SHA-256 before it's kept. Fetched at the first start (owner: what the app needs comes at
//! the first open) and again before a two-part split if something is missing.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use crate::spleeter::{MUSIC, VOCALS};

/// Where a piece comes from.
enum From {
    /// The whole file at the URL.
    Whole,
    /// A member of the .tgz at the URL (its size and SHA-256 checked first).
    #[cfg_attr(windows, allow(dead_code))]
    Tgz { member: &'static str, size: u64, sha256: &'static str },
    /// A .zip's entry, `offset` its local header, `compressed` its deflated size.
    #[cfg_attr(not(windows), allow(dead_code))]
    ZipEntry { offset: u64, compressed: u64 },
}

struct Piece {
    file: &'static str,
    url: &'static str,
    size: u64,
    sha256: &'static str,
    from: From,
}

/// Spleeter's two models at a fixed commit (as the phone's).
const VOCALS_URL: &str = "https://huggingface.co/csukuangfj/sherpa-onnx-spleeter-2stems-fp16/resolve/93ba771920ade509f8cbd6825b1a90856c797e08/vocals.fp16.onnx";
const MUSIC_URL: &str = "https://huggingface.co/csukuangfj/sherpa-onnx-spleeter-2stems-fp16/resolve/93ba771920ade509f8cbd6825b1a90856c797e08/accompaniment.fp16.onnx";

/// ONNX Runtime's library as it's kept here.
#[cfg(windows)]
pub const RUNTIME: &str = "onnxruntime.dll";
#[cfg(not(windows))]
pub const RUNTIME: &str = "libonnxruntime.so";

#[cfg(not(windows))]
const RUNTIME_PIECE: Piece = Piece {
    file: RUNTIME,
    url: "https://github.com/microsoft/onnxruntime/releases/download/v1.31.1/onnxruntime-linux-x64-1.31.1.tgz",
    size: 29_345_384,
    sha256: "a96f132e5eb5ca04e23a7d2fc1078fb95aa591e8d71d0f237702ece3a1e0f4ea",
    from: From::Tgz {
        member: "onnxruntime-linux-x64-1.31.1/lib/libonnxruntime.so.1.31.1",
        size: 11_413_655,
        sha256: "c849e36a1ef3e22d4ff217b9d1ce4a3cf5b9467b975610913ae959ad5b16bdc9",
    },
};
#[cfg(windows)]
const RUNTIME_PIECE: Piece = Piece {
    file: RUNTIME,
    url: "https://github.com/microsoft/onnxruntime/releases/download/v1.31.1/onnxruntime-win-x64-1.31.1.zip",
    size: 17_253_728,
    sha256: "cb2fa518b18356617f6f9f460c012501da2819ec7e5c1d80bd8f5de20492db0c",
    from: From::ZipEntry { offset: 221_047, compressed: 6_178_062 },
};

/// Windows: Microsoft's ONNX Runtime needs the Visual C++ runtime, which a PC may not have: its
/// four DLLs (as the splitter pack has them), out of the msvc-runtime wheel's entries by ranges
/// (about 330 KB), loaded from this folder before ONNX Runtime (spleeter.rs). In this order.
#[cfg(windows)]
pub const C_RUNTIME: [&str; 4] = ["vcruntime140.dll", "vcruntime140_1.dll", "msvcp140.dll", "msvcp140_1.dll"];
#[cfg(windows)]
const WHEEL: &str = "https://files.pythonhosted.org/packages/98/33/be9ef68901200813fc174a06758b4ae48ca4eb58fa783280dd879d5fd048/msvc_runtime-14.51.36231-cp312-cp312-win_amd64.whl";

fn pieces() -> Vec<Piece> {
    #[allow(unused_mut)]
    let mut all = vec![
        Piece { file: VOCALS, url: VOCALS_URL, size: 19_681_017, sha256: "24cef84aedcd1fe87c0b743ef3370ad34dc1fabf6c9014d6128a75a538c7b668", from: From::Whole },
        Piece { file: MUSIC, url: MUSIC_URL, size: 19_681_024, sha256: "d14cea55793cc531a5875f5f4da08207d1c5ab9292e8e0099a104eecb014fcc0", from: From::Whole },
        RUNTIME_PIECE,
    ];
    #[cfg(windows)]
    all.extend([
        Piece { file: C_RUNTIME[0], url: WHEEL, size: 178_616, sha256: "d1f4225df2cd877dbf130d5668a021dce3f94118455ff5ec952061c30afc9ce7", from: From::ZipEntry { offset: 1_914_358, compressed: 85_393 } },
        Piece { file: C_RUNTIME[1], url: WHEEL, size: 50_112, sha256: "a7146c08f89fe5b04541ab507cdb59ff7b44534d4ba3c668a426c6450a03434e", from: From::ZipEntry { offset: 1_999_840, compressed: 26_615 } },
        Piece { file: C_RUNTIME[2], url: WHEEL, size: 643_512, sha256: "7c26614e1d733892c2deac7e245ce115504b1d80592dd0a01b08e3e5a55f89ca", from: From::ZipEntry { offset: 1_135_354, compressed: 202_959 } },
        Piece { file: C_RUNTIME[3], url: WHEEL, size: 35_768, sha256: "206c931bf90fdad8816de3b5e2ef80b2bcaa9406c89ecc05fe6fddffe251e982", from: From::ZipEntry { offset: 1_338_398, compressed: 17_788 } },
    ]);
    all
}

/// The app's folder for them.
pub fn dir() -> PathBuf {
    glib::user_data_dir().join("radiance-music").join("two-part")
}

/// ONNX Runtime's library and the models' folder, when everything is there.
pub fn ready() -> Option<(PathBuf, PathBuf)> {
    ready_in(&dir())
}

fn ready_in(d: &Path) -> Option<(PathBuf, PathBuf)> {
    pieces().iter().all(|p| fs::metadata(d.join(p.file)).is_ok_and(|m| m.len() == p.size)).then(|| (d.join(RUNTIME), d.to_path_buf()))
}

/// How much is still to fetch (bytes, roughly what goes over the network).
pub fn missing_bytes() -> u64 {
    let d = dir();
    pieces()
        .iter()
        .filter(|p| !fs::metadata(d.join(p.file)).is_ok_and(|m| m.len() == p.size))
        .map(|p| match p.from {
            From::Whole => p.size,
            From::Tgz { size, .. } => size,
            From::ZipEntry { compressed, .. } => compressed,
        })
        .sum()
}

/// One fetch at a time (the start's and a split's could meet); how far along it is (f64 bits).
static FETCHING: Mutex<()> = Mutex::new(());
static ALONG: AtomicU64 = AtomicU64::new(0);

/// Fetches what's missing (`say`: how far along, 0..1). Each file is checked, then put in place.
/// Another fetch already on its way: this one waits for it, saying how far along that one is.
pub fn fetch(say: &dyn Fn(f64)) -> Result<(PathBuf, PathBuf), String> {
    let _one = loop {
        match FETCHING.try_lock() {
            Ok(g) => break g,
            Err(std::sync::TryLockError::Poisoned(e)) => break e.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => {
                say(f64::from_bits(ALONG.load(Ordering::Relaxed)));
                std::thread::sleep(Duration::from_millis(250));
            }
        }
    };
    let say = |p: f64| {
        ALONG.store(p.to_bits(), Ordering::Relaxed);
        say(p);
    };
    say(0.0);
    let d = dir();
    fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    let total = missing_bytes().max(1);
    let mut done = 0u64;
    for p in pieces() {
        let to = d.join(p.file);
        if fs::metadata(&to).is_ok_and(|m| m.len() == p.size) {
            continue;
        }
        let bytes = match p.from {
            From::Whole => get(p.url, None, &mut |n| say((done + n) as f64 / total as f64))?,
            From::Tgz { member, size, sha256 } => {
                let tgz = get(p.url, None, &mut |n| say((done + n) as f64 / total as f64))?;
                check(&tgz, size, sha256).map_err(|e| format!("ONNX Runtime's download: {e}"))?;
                tar_member(flate2::read::GzDecoder::new(&tgz[..]), member)?
            }
            From::ZipEntry { offset, compressed } => {
                // the entry's header (its name and extra field are short) and its data, in one go
                let local = get(p.url, Some((offset, offset + 30 + 1024 + compressed)), &mut |n| say((done + n) as f64 / total as f64))?;
                zip_entry(&local, compressed)?
            }
        };
        check(&bytes, p.size, p.sha256).map_err(|e| format!("{}: {e}", p.file))?;
        let part = d.join(format!("{}.part", p.file));
        {
            // on the disk before it's put in place (a power cut can't leave a damaged file there)
            let mut f = fs::File::create(&part).map_err(|e| e.to_string())?;
            f.write_all(&bytes).and_then(|_| f.sync_all()).map_err(|e| e.to_string())?;
        }
        fs::rename(&part, &to).map_err(|e| e.to_string())?;
        done += match p.from {
            From::Whole => p.size,
            From::Tgz { size, .. } => size,
            From::ZipEntry { compressed, .. } => compressed,
        };
        say(done as f64 / total as f64);
    }
    let ready = ready_in(&d).ok_or_else(|| "the two-part split's files aren't all there".to_string())?;
    say(1.0); // (also when nothing was missing: what waits on it sees it done)
    Ok(ready)
}

/// ONNX Runtime or a model wouldn't load: its files are checked again (each one's SHA-256);
/// a damaged one goes, and comes again with the next fetch.
pub fn check_again() {
    let gone = check_again_in(&dir());
    if gone > 0 {
        crate::log::note(&format!("the fast split: {gone} damaged file(s) removed, fetched again before the next split"));
    }
}

fn check_again_in(d: &Path) -> usize {
    let _one = FETCHING.lock().unwrap_or_else(|e| e.into_inner());
    pieces()
        .iter()
        .filter(|p| match fs::read(d.join(p.file)) {
            Ok(bytes) => check(&bytes, p.size, p.sha256).is_err() && fs::remove_file(d.join(p.file)).is_ok(),
            Err(_) => false,
        })
        .count()
}

/// Bluetooth music playing over Wi-Fi now (they share a radio: a fast download makes it
/// stutter). Windows can't tell yet: always gently there.
fn bluetooth_busy() -> bool {
    cfg!(windows) || crate::share::bluetooth_busy()
}

/// Keeps a download to 256 KB/s while Bluetooth music plays, a second at a time (as sharing
/// does): a slow start is no allowance for a burst later.
struct Gentle {
    since: Instant,
    moved: u64,
}

impl Gentle {
    fn new(now: Instant) -> Gentle {
        Gentle { since: now, moved: 0 }
    }

    /// `n` more bytes came at `now`: how long to wait before reading on.
    fn wait(&mut self, n: u64, busy: bool, now: Instant) -> Duration {
        if !busy {
            *self = Gentle::new(now);
            return Duration::ZERO;
        }
        if now.duration_since(self.since) > Duration::from_secs(1) {
            *self = Gentle::new(now);
        }
        self.moved += n;
        pause_for(self.moved, now.duration_since(self.since), true)
    }
}

/// The file at `url` (or its bytes `range` start..end, end past the file is fine), in memory.
/// A connection that goes quiet for 30 s fails (a split waiting on it would wait for ever).
fn get(url: &str, range: Option<(u64, u64)>, moved: &mut dyn FnMut(u64)) -> Result<Vec<u8>, String> {
    let mut req = crate::net::agent().get(url).config().timeout_per_read(Some(Duration::from_secs(30))).build();
    if let Some((a, b)) = range {
        req = req.header("Range", &format!("bytes={a}-{}", b - 1));
    }
    let mut resp = req.call().map_err(|e| e.to_string())?;
    if range.is_some() && resp.status().as_u16() != 206 {
        return Err("the server sent the whole file, not the part asked for".into());
    }
    let mut body = resp.body_mut().as_reader();
    let mut out = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    // Bluetooth music over Wi-Fi: looked at every few seconds; meanwhile it goes gently
    let (mut checked, mut busy) = (Instant::now(), bluetooth_busy());
    let mut gentle = Gentle::new(Instant::now());
    loop {
        let n = body.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n]);
        moved(out.len() as u64);
        if checked.elapsed() > Duration::from_secs(3) {
            (checked, busy) = (Instant::now(), bluetooth_busy());
        }
        std::thread::sleep(gentle.wait(n as u64, busy, Instant::now()));
    }
    Ok(out)
}

/// How long to wait so `bytes` sent over `took` keep to 256 KB/s while `busy` (Bluetooth music
/// over Wi-Fi: they share a radio, and a fast download makes the music stutter; as sharing does).
fn pause_for(bytes: u64, took: Duration, busy: bool) -> Duration {
    if !busy {
        return Duration::ZERO;
    }
    Duration::from_secs_f64(bytes as f64 / crate::share::GENTLE as f64).saturating_sub(took)
}

/// The download is the file pinned: its size and SHA-256.
fn check(bytes: &[u8], size: u64, sha256: &str) -> Result<(), String> {
    if bytes.len() as u64 != size {
        return Err(format!("came as {} bytes, not {size}", bytes.len()));
    }
    let got: String = Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect();
    if got != sha256 {
        return Err("came damaged (its checksum differs)".into());
    }
    Ok(())
}

/// A regular file's bytes out of a tar stream (plain ustar headers, as Microsoft's .tgz has).
fn tar_member(mut tar: impl Read, member: &str) -> Result<Vec<u8>, String> {
    let mut header = [0u8; 512];
    loop {
        tar.read_exact(&mut header).map_err(|_| format!("{member} isn't in the download"))?;
        if header.iter().all(|b| *b == 0) {
            return Err(format!("{member} isn't in the download"));
        }
        let text = |r: std::ops::Range<usize>| String::from_utf8_lossy(&header[r]).trim_end_matches('\0').to_string();
        let (name, prefix) = (text(0..100), text(345..500));
        let name = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
        let size = u64::from_str_radix(text(124..136).trim(), 8).map_err(|_| "a damaged download (tar)".to_string())?;
        let regular = matches!(header[156], b'0' | 0);
        let padded = size.div_ceil(512) * 512;
        if regular && name == member {
            let mut out = vec![0u8; size as usize];
            tar.read_exact(&mut out).map_err(|e| e.to_string())?;
            return Ok(out);
        }
        std::io::copy(&mut (&mut tar).take(padded), &mut std::io::sink()).map_err(|e| e.to_string())?;
    }
}

/// A zip entry's file, from the bytes starting at its local header: its `compressed` bytes of
/// deflate (method 8) or stored (method 0) data, after its name and extra field.
fn zip_entry(local: &[u8], compressed: u64) -> Result<Vec<u8>, String> {
    if local.len() < 30 || local[..4] != [0x50, 0x4b, 0x03, 0x04] {
        return Err("a damaged download (no zip entry there)".into());
    }
    let u16_at = |i: usize| u16::from_le_bytes([local[i], local[i + 1]]) as usize;
    let method = u16_at(8);
    let start = 30 + u16_at(26) + u16_at(28);
    let end = start + compressed as usize;
    let data = local.get(start..end).ok_or("the download stopped short")?;
    match method {
        0 => Ok(data.to_vec()),
        8 => {
            let mut out = Vec::new();
            flate2::read::DeflateDecoder::new(data).read_to_end(&mut out).map_err(|e| format!("a damaged download: {e}"))?;
            Ok(out)
        }
        m => Err(format!("a zip entry packed in a way this app doesn't read ({m})")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn sha(b: &[u8]) -> String {
        Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
    }

    /// While Bluetooth music plays over Wi-Fi (one radio), the download keeps to 256 KB/s.
    #[test]
    fn it_goes_gently_while_bluetooth_music_plays() {
        use std::time::Duration;
        assert_eq!(pause_for(10 << 20, Duration::from_millis(100), false), Duration::ZERO);
        assert_eq!(pause_for(512 * 1024, Duration::from_millis(500), true), Duration::from_millis(1500), "512 KB takes 2 s");
        assert_eq!(pause_for(128 * 1024, Duration::from_secs(1), true), Duration::ZERO, "already slower than that");
    }

    /// The pace is kept a second at a time: a slow start is no allowance for a burst later.
    #[test]
    fn a_slow_start_is_no_allowance_for_a_burst() {
        use std::time::Duration;
        let t0 = Instant::now();
        let mut g = Gentle::new(t0);
        // 5 s of nothing, then 1 MB at once while Bluetooth music plays: about 4 s, not 0
        let w = g.wait(1 << 20, true, t0 + Duration::from_secs(5));
        assert!(w >= Duration::from_millis(3900), "waited {w:?}");
        assert_eq!(g.wait(10 << 20, false, t0 + Duration::from_secs(6)), Duration::ZERO, "no Bluetooth music: full speed");
    }

    /// ONNX Runtime or a model that won't load: its files are checked again, a damaged one goes.
    #[test]
    fn a_damaged_piece_goes_when_checked_again() {
        let d = std::env::temp_dir().join(format!("radiance-two-part-again-{}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        let p = &pieces()[0];
        fs::File::create(d.join(p.file)).unwrap().set_len(p.size).unwrap(); // the right size, not the right file
        assert_eq!(check_again_in(&d), 1);
        assert!(!d.join(p.file).exists());
        assert_eq!(check_again_in(&d), 0, "nothing left to check");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn a_download_that_isnt_the_pinned_file_is_refused() {
        let good = b"the model".to_vec();
        assert!(check(&good, 9, &sha(&good)).is_ok());
        assert!(check(&good, 10, &sha(&good)).unwrap_err().contains("9 bytes"));
        let mut bad = good.clone();
        bad[0] = b'T';
        assert!(check(&bad, 9, &sha(&good)).unwrap_err().contains("damaged"));
    }

    /// A tar header for `name` (`kind`: b'0' a file, b'2' a link, b'5' a folder) of `size` bytes.
    fn header(name: &str, size: usize, kind: u8) -> [u8; 512] {
        let mut h = [0u8; 512];
        h[..name.len()].copy_from_slice(name.as_bytes());
        h[100..107].copy_from_slice(b"0000644");
        h[124..135].copy_from_slice(format!("{size:011o}").as_bytes());
        h[156] = kind;
        h[257..263].copy_from_slice(b"ustar\0");
        h[148..156].copy_from_slice(b"        ");
        let sum: u32 = h.iter().map(|b| *b as u32).sum();
        h[148..155].copy_from_slice(format!("{sum:06o}\0").as_bytes());
        h
    }

    #[test]
    fn the_runtime_comes_out_of_its_tgz() {
        let lib: Vec<u8> = (0..3000u32).map(|i| (i % 251) as u8).collect();
        let mut tar = Vec::new();
        tar.extend_from_slice(&header("ort/lib/", 0, b'5'));
        tar.extend_from_slice(&header("ort/lib/libonnxruntime.so", 0, b'2'));
        tar.extend_from_slice(&header("ort/lib/other.so", 700, b'0'));
        tar.extend_from_slice(&[7u8; 1024]); // 700 bytes, padded to 1024
        tar.extend_from_slice(&header("ort/lib/libonnxruntime.so.1.31.1", lib.len(), b'0'));
        tar.extend_from_slice(&lib);
        tar.resize(tar.len().div_ceil(512) * 512 + 1024, 0);
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(&tar).unwrap();
        let tgz = gz.finish().unwrap();
        let got = tar_member(flate2::read::GzDecoder::new(&tgz[..]), "ort/lib/libonnxruntime.so.1.31.1").unwrap();
        assert_eq!(got, lib);
        assert!(tar_member(flate2::read::GzDecoder::new(&tgz[..]), "ort/lib/missing.so").unwrap_err().contains("isn't in"));
    }

    #[test]
    fn the_dll_comes_out_of_its_zip_entry() {
        let dll: Vec<u8> = (0..5000u32).map(|i| (i * 31 % 256) as u8).collect();
        let mut deflated = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        deflated.write_all(&dll).unwrap();
        let data = deflated.finish().unwrap();
        let name = b"ort/lib/onnxruntime.dll";
        let extra = [1u8, 0, 4, 0, 9, 9, 9, 9];
        let mut local = vec![0x50, 0x4b, 0x03, 0x04, 20, 0, 8, 0, 8, 0, 0, 0, 0, 0];
        local.extend_from_slice(&[0u8; 12]); // crc, sizes (in the data descriptor)
        local.extend_from_slice(&(name.len() as u16).to_le_bytes());
        local.extend_from_slice(&(extra.len() as u16).to_le_bytes());
        local.extend_from_slice(name);
        local.extend_from_slice(&extra);
        local.extend_from_slice(&data);
        local.extend_from_slice(&[0u8; 900]); // what the range fetched past the entry
        assert_eq!(zip_entry(&local, data.len() as u64).unwrap(), dll);
        assert!(zip_entry(&local[..60], data.len() as u64).unwrap_err().contains("short"));
        assert!(zip_entry(&[0u8; 40], 1).is_err(), "no zip entry there");
    }

    /// The real downloads (about 50 MB): run with --ignored, RADIANCE_TWO_PART_FETCH=1 and
    /// XDG_DATA_HOME somewhere empty.
    #[test]
    #[ignore]
    fn the_real_pieces_come_and_check_out() {
        if std::env::var_os("RADIANCE_TWO_PART_FETCH").is_none() {
            return;
        }
        let last = std::cell::Cell::new(0.0);
        let (runtime, models) = fetch(&|p| last.set(p)).unwrap();
        assert!(runtime.exists() && models.join(VOCALS).exists() && models.join(MUSIC).exists());
        assert_eq!(last.get(), 1.0);
        assert_eq!(missing_bytes(), 0);
    }

    /// Windows' DLL out of Microsoft's .zip by byte ranges, on any computer (about 6 MB).
    #[test]
    #[ignore]
    fn the_windows_dll_comes_by_ranges() {
        if std::env::var_os("RADIANCE_TWO_PART_FETCH").is_none() {
            return;
        }
        let (offset, compressed) = (221_047u64, 6_178_062u64);
        let url = "https://github.com/microsoft/onnxruntime/releases/download/v1.31.1/onnxruntime-win-x64-1.31.1.zip";
        let local = get(url, Some((offset, offset + 30 + 1024 + compressed)), &mut |_| {}).unwrap();
        let dll = zip_entry(&local, compressed).unwrap();
        check(&dll, 17_253_728, "cb2fa518b18356617f6f9f460c012501da2819ec7e5c1d80bd8f5de20492db0c").unwrap();
    }

    #[test]
    fn ready_only_when_every_piece_is_there_whole() {
        let d = std::env::temp_dir().join(format!("radiance-two-part-{}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        assert!(ready_in(&d).is_none());
        for p in pieces() {
            fs::File::create(d.join(p.file)).unwrap().set_len(p.size).unwrap();
        }
        let (runtime, models) = ready_in(&d).expect("all there");
        assert_eq!(runtime, d.join(RUNTIME));
        assert_eq!(models, d);
        fs::File::create(d.join(VOCALS)).unwrap().set_len(10).unwrap(); // half a download
        assert!(ready_in(&d).is_none());
        let _ = fs::remove_dir_all(&d);
    }
}
