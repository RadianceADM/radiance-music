//! The splitter's first-start downloads into data/models (owner, 2026-10-31:
//! "download on first open"; never in the image): ONNX Runtime (the graphics
//! card's build on Windows with an NVIDIA card, with NVIDIA's CUDA 12 and cuDNN
//! files), Demucs and MMS as ONNX, and ffmpeg on Windows when there's none.
//! Every file is pinned (its size and SHA-256), a cut-off download carries on
//! where it stopped, and a wrong one is thrown away.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

/// ONNX Runtime's release, its CPU and CUDA 12 builds (CUDA 13 dropped the GTX 10xx cards, #72).
pub const ORT: &str = "1.31.1";

/// What to take out of a downloaded archive: (the entry's file name, or
/// "prefix*suffix"; the name to keep it under, "" for its own), flat in models/.
pub type Take = &'static [(&'static str, &'static str)];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Unpack {
    /// The file itself, under the item's name.
    Keep,
    Zip(Take),
    Tgz(Take),
}

/// One download: its pieces (joined in order), all of it pinned.
#[derive(Clone, Debug)]
pub struct Item {
    /// Its name in models/ (for an archive: just its record of being there).
    pub name: String,
    /// What it is, for the page ("the splitting model").
    pub what: String,
    pub urls: Vec<String>,
    pub size: u64,
    pub sha256: String,
    pub unpack: Unpack,
    /// Its folder in models/ ("" for models/ itself): ONNX Runtime's two builds each have their own
    /// (cpu/, gpu/), so neither ever overwrites the other's files (#72 review).
    pub dir: &'static str,
}

fn item(name: &str, what: &str, urls: Vec<String>, size: u64, sha256: &str, unpack: Unpack) -> Item {
    Item { name: name.into(), what: what.into(), urls, size, sha256: sha256.into(), unpack, dir: "" }
}

#[cfg(test)]
fn into(dir: &'static str, mut i: Item) -> Item {
    i.dir = dir;
    i
}

/// What this server needs, here: `windows`, whether it has an NVIDIA card,
/// and whether ffmpeg is there already.
pub fn wanted(windows: bool, nvidia: bool, has_ffmpeg: bool) -> Vec<Item> {
    let ort = |file: &str| format!("https://github.com/microsoft/onnxruntime/releases/download/v{ORT}/{file}");
    let nv = |path: &str| format!("https://developer.download.nvidia.com/compute/{path}");
    let mut v = Vec::new();
    let runtime = if windows && nvidia { "gpu" } else { "cpu" };
    if windows && nvidia {
        v.push(item(
            "onnxruntime-gpu",
            "ONNX Runtime for the graphics card",
            vec![ort(&format!("onnxruntime-win-x64-gpu_cuda12-{ORT}.zip"))],
            392_924_457,
            "5cbf65edc93dc7bb3296249b1b6e1b2228e575902af64d6163d0da2103cef9d5",
            Unpack::Zip(&[("onnxruntime.dll", ""), ("onnxruntime_providers_shared.dll", ""), ("onnxruntime_providers_cuda.dll", "")]),
        ));
        v.push(item(
            "cuda-runtime",
            "NVIDIA's CUDA runtime",
            vec![nv("cuda/redist/cuda_cudart/windows-x86_64/cuda_cudart-windows-x86_64-12.8.90-archive.zip")],
            3_037_735,
            "4a39058fd8519444a81cfc7ae055d136f48d1a31ffa41ae255b35b2edd61e13b",
            Unpack::Zip(&[("cudart64_12.dll", "")]),
        ));
        v.push(item(
            "cublas",
            "NVIDIA's cuBLAS",
            vec![nv("cuda/redist/libcublas/windows-x86_64/libcublas-windows-x86_64-12.8.4.1-archive.zip")],
            563_660_944,
            "57a470112cec7e112c95253dde8b3c7184d795dbd92b0bde77a4cb7f8c94c8aa",
            Unpack::Zip(&[("cublas64_12.dll", ""), ("cublasLt64_12.dll", "")]),
        ));
        v.push(item(
            "cufft",
            "NVIDIA's cuFFT",
            vec![nv("cuda/redist/libcufft/windows-x86_64/libcufft-windows-x86_64-11.3.3.83-archive.zip")],
            190_568_498,
            "cc6e0ba958cf23387b462017a24464c72bd901549046133f3d1ebcc3d7444c90",
            Unpack::Zip(&[("cufft64_11.dll", "")]),
        ));
        v.push(item(
            "cudnn",
            "NVIDIA's cuDNN",
            vec![nv("cudnn/redist/cudnn/windows-x86_64/cudnn-windows-x86_64-9.10.2.21_cuda12-archive.zip")],
            683_336_095,
            "c1a4567d822ebda7373fa1f19255dff4942302de741f830160b6c7d1fb31af23",
            Unpack::Zip(&[("cudnn*64_9.dll", "")]),
        ));
    } else if windows {
        v.push(item(
            "onnxruntime",
            "ONNX Runtime",
            vec![ort(&format!("onnxruntime-win-x64-{ORT}.zip"))],
            82_929_944,
            "5869e26bb25314375f2d5ef1877e3ea06b1543d6c710e6e4a1ec2f7177ab4b2c",
            Unpack::Zip(&[("onnxruntime.dll", ""), ("onnxruntime_providers_shared.dll", "")]),
        ));
    } else {
        v.push(item(
            "onnxruntime",
            "ONNX Runtime",
            vec![ort(&format!("onnxruntime-linux-x64-{ORT}.tgz"))],
            11_413_655,
            "c849e36a1ef3e22d4ff217b9d1ce4a3cf5b9467b975610913ae959ad5b16bdc9",
            Unpack::Tgz(&[("libonnxruntime.so.1.31.1", "libonnxruntime.so")]),
        ));
    }
    if windows {
        // ONNX Runtime needs the Visual C++ runtime, which a PC may not have (the app's same pin)
        v.push(item(
            "vc-runtime",
            "the Visual C++ runtime",
            vec!["https://files.pythonhosted.org/packages/98/33/be9ef68901200813fc174a06758b4ae48ca4eb58fa783280dd879d5fd048/msvc_runtime-14.51.36231-cp312-cp312-win_amd64.whl".into()],
            2_052_033,
            "89f70d1a37bb64164cdb09d02dd82d6b1400e1310be2995b4ed5a9f23efe2222",
            Unpack::Zip(&[("vcruntime140.dll", ""), ("vcruntime140_1.dll", ""), ("msvcp140.dll", ""), ("msvcp140_1.dll", "")]),
        ));
    }
    if windows && !has_ffmpeg {
        v.push(item(
            "ffmpeg",
            "ffmpeg",
            vec!["https://github.com/GyanD/codexffmpeg/releases/download/9.0.2/ffmpeg-9.0.2-essentials_build.zip".into()],
            114_768_076,
            "60f467265b1e312373dbcd92200c2618a74850f98d3d078e94296bb3fa2047ba",
            Unpack::Zip(&[("ffmpeg.exe", ""), ("ffprobe.exe", "")]),
        ));
    }
    for i in v.iter_mut() {
        if matches!(i.name.as_str(), "onnxruntime-gpu" | "cuda-runtime" | "cublas" | "cufft" | "cudnn" | "onnxruntime") {
            i.dir = runtime;
        }
        if i.name == "vc-runtime" {
            // beside the runtime it's for, under a name of its own (each folder's record)
            i.dir = runtime;
            i.name = format!("vc-runtime-{runtime}");
        }
    }
    // Demucs (htdemucs, MIT), as the phone's and the app's
    v.push(item(
        "htdemucs_fp16weights.onnx",
        "the splitting model",
        vec!["https://huggingface.co/StemSplitio/htdemucs-onnx/resolve/d54ed9eb60e258ea82131c6ee14578628816456a/htdemucs_fp16weights.onnx".into()],
        165_612_636,
        "d05c269d0178d2a72ad484b10b11dd370193fc923201c3b27a99f848745db70a",
        Unpack::Keep,
    ));
    // MMS forced alignment (Meta, CC-BY-NC 4.0), half-size, in seven pieces (models-mms-1)
    v.push(item(
        "mms_fa_fp16w.onnx",
        "the word-timing model",
        (0..7).map(|i| format!("https://github.com/RadianceADM/radiance-music/releases/download/models-mms-1/mms_fa_fp16w.onnx.part{i:02}")).collect(),
        631_506_860,
        "de58e98724f2b4ac9f8428e360ee5f44070ae58dbace2b78d5434e0f9f5f3e89",
        Unpack::Keep,
    ));
    v
}

/// The bytes still to come for `items` (for the page's first words).
pub fn missing_bytes(dir: &Path, items: &[Item]) -> u64 {
    items.iter().filter(|i| !got(dir, i)).map(|i| i.size).sum()
}

/// Its record of being there, under its folder's name too (a record from the flat layout of
/// before doesn't count for gpu/ or cpu/).
fn stamp(dir: &Path, item: &Item) -> PathBuf {
    dir.join(".got").join(item.dir).join(&item.name)
}

/// Fetched already (its record names the same file).
pub fn got(dir: &Path, item: &Item) -> bool {
    std::fs::read_to_string(stamp(dir, item)).is_ok_and(|s| s.trim() == item.sha256)
}

/// Gets every item not there yet into `dir`; `progress`: how far all of them
/// are (0-1) and what's arriving.
pub fn fetch_all(dir: &Path, items: &[Item], progress: &dyn Fn(f64, &str)) -> Result<(), String> {
    let todo: Vec<&Item> = items.iter().filter(|i| !got(dir, i)).collect();
    let total = todo.iter().map(|i| i.size).sum::<u64>().max(1) as f64;
    let mut before = 0u64;
    for item in todo {
        let say = |got: u64| {
            let mb = |b: u64| (b as f64 / 1e6).round() as u64;
            progress(((before + got.min(item.size)) as f64 / total).min(1.0), &format!("Downloading {} ({} of {} MB)…", item.what, mb(got.min(item.size)), mb(item.size)));
        };
        fetch_one(dir, item, &say)?;
        before += item.size;
    }
    progress(1.0, "Ready.");
    Ok(())
}

fn fetch_one(models: &Path, item: &Item, say: &dyn Fn(u64)) -> Result<(), String> {
    let dir = &models.join(item.dir);
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let dl = models.join(".download");
    std::fs::create_dir_all(&dl).map_err(|e| e.to_string())?;
    let part = |i: usize| dl.join(format!("{}.part{i}", item.name));
    let done = |i: usize| dl.join(format!("{}.part{i}.done", item.name));
    let have = |upto: usize| (0..upto).map(|i| std::fs::metadata(part(i)).map(|m| m.len()).unwrap_or(0)).sum::<u64>();
    for (i, url) in item.urls.iter().enumerate() {
        if done(i).exists() {
            if part(i).exists() {
                continue;
            }
            // its record without its piece (stopped between the two): fetched again
            let _ = std::fs::remove_file(done(i));
        }
        let before = have(i);
        download(url, &part(i), &|n| say(before + n)).map_err(|e| format!("{}: {e}", item.what))?;
        std::fs::write(done(i), "").map_err(|e| e.to_string())?;
    }
    // the pieces joined, and checked
    let joined = dl.join(format!("{}.joined", item.name));
    let forget = || {
        for i in 0..item.urls.len() {
            let _ = std::fs::remove_file(part(i));
            let _ = std::fs::remove_file(done(i));
        }
        let _ = std::fs::remove_file(&joined);
    };
    let sha = {
        let mut out = std::fs::File::create(&joined).map_err(|e| e.to_string())?;
        let mut hash = Sha256::new();
        let mut buf = vec![0u8; 1 << 20];
        for i in 0..item.urls.len() {
            let mut f = std::fs::File::open(part(i)).map_err(|e| e.to_string())?;
            loop {
                let n = f.read(&mut buf).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                hash.update(&buf[..n]);
                out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
            }
        }
        hash.finalize().iter().map(|b| format!("{b:02x}")).collect::<String>()
    };
    if sha != item.sha256 {
        forget();
        return Err(format!("{}: the download came out wrong (its checksum); it starts over", item.what));
    }
    for i in 0..item.urls.len() {
        let _ = std::fs::remove_file(part(i));
        let _ = std::fs::remove_file(done(i));
    }
    let unpacked = match item.unpack {
        Unpack::Keep => std::fs::rename(&joined, dir.join(&item.name)).map_err(|e| e.to_string()),
        Unpack::Zip(take) => unzip(&joined, dir, take),
        Unpack::Tgz(take) => untgz(&joined, dir, take),
    };
    let _ = std::fs::remove_file(&joined);
    unpacked.map_err(|e| format!("{}: {e}", item.what))?;
    std::fs::create_dir_all(models.join(".got").join(item.dir)).map_err(|e| e.to_string())?;
    std::fs::write(stamp(models, item), &item.sha256).map_err(|e| e.to_string())
}

/// `url` into `file`, carrying on from what's there; `moved`: its bytes so far.
fn download(url: &str, file: &Path, moved: &dyn Fn(u64)) -> Result<(), String> {
    let from = std::fs::metadata(file).map(|m| m.len()).unwrap_or(0);
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .ip_family(ureq::config::IpFamily::Ipv4Only)
        .user_agent(concat!("RadianceMusicServer/", env!("CARGO_PKG_VERSION")))
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .timeout_per_read(Some(Duration::from_secs(60)))
        .http_status_as_error(false)
        .build()
        .into();
    let mut req = agent.get(url);
    if from > 0 {
        req = req.header("Range", &format!("bytes={from}-"));
    }
    let mut resp = req.call().map_err(|e| e.to_string())?;
    let code = resp.status().as_u16();
    let (mut out, mut got) = match code {
        // all of it there already
        416 if from > 0 => return Ok(()),
        206 if from > 0 => (std::fs::OpenOptions::new().append(true).open(file).map_err(|e| e.to_string())?, from),
        200 => (std::fs::File::create(file).map_err(|e| e.to_string())?, 0),
        _ => return Err(format!("the download's server answered {code}")),
    };
    let mut body = resp.body_mut().with_config().limit(u64::MAX).reader();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = body.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        got += n as u64;
        moved(got);
    }
    out.flush().map_err(|e| e.to_string())
}

/// The entry's file name matches `pattern` ("name" or "prefix*suffix").
fn matches(name: &str, pattern: &str) -> bool {
    match pattern.split_once('*') {
        Some((a, b)) => name.len() >= a.len() + b.len() && name.starts_with(a) && name.ends_with(b),
        None => name == pattern,
    }
}

fn keep_as(name: &str, take: Take) -> Option<String> {
    take.iter().find(|(p, _)| matches(name, p)).map(|(_, to)| if to.is_empty() { name.to_string() } else { to.to_string() })
}

/// Writes `from` into dir/name through a temporary file (a half-written file never has the name).
fn keep(mut from: impl Read, dir: &Path, name: &str) -> Result<(), String> {
    let tmp = dir.join(format!(".{name}.tmp"));
    let mut f = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
    std::io::copy(&mut from, &mut f).map_err(|e| e.to_string())?;
    drop(f);
    std::fs::rename(&tmp, dir.join(name)).map_err(|e| e.to_string())
}

fn unzip(archive: &Path, dir: &Path, take: Take) -> Result<(), String> {
    let f = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut z = zip::ZipArchive::new(f).map_err(|e| e.to_string())?;
    let mut found = 0;
    for i in 0..z.len() {
        let entry = z.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().map(|n| n.rsplit('/').next().unwrap_or("").to_string()).unwrap_or_default();
        if let Some(to) = keep_as(&name, take) {
            keep(entry, dir, &to)?;
            found += 1;
        }
    }
    if found < take.len() {
        return Err("the archive lacks a file it should have".into());
    }
    Ok(())
}

fn untgz(archive: &Path, dir: &Path, take: Take) -> Result<(), String> {
    let f = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut t = tar::Archive::new(flate2::read::GzDecoder::new(f));
    let mut found = 0;
    for entry in t.entries().map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let name = entry.path().ok().and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())).unwrap_or_default();
        if let Some(to) = keep_as(&name, take) {
            keep(entry, dir, &to)?;
            found += 1;
        }
    }
    if found < take.len() {
        return Err("the archive lacks a file it should have".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use axum::extract::{Path as UrlPath, State};
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::IntoResponse;

    /// A stand-in for the downloads' servers: its files, and every request's Range start.
    #[derive(Clone, Default)]
    struct Files {
        files: Arc<Mutex<HashMap<String, Vec<u8>>>>,
        asked: Arc<Mutex<Vec<(String, u64)>>>,
        /// answers 200 with the whole file whatever Range asks (some servers do)
        no_ranges: Arc<std::sync::atomic::AtomicBool>,
    }

    async fn serve(State(f): State<Files>, UrlPath(name): UrlPath<String>, headers: HeaderMap) -> axum::response::Response {
        let Some(bytes) = f.files.lock().unwrap().get(&name).cloned() else { return StatusCode::NOT_FOUND.into_response() };
        let ranges = !f.no_ranges.load(std::sync::atomic::Ordering::Relaxed);
        let from = headers.get("range").filter(|_| ranges).and_then(|v| v.to_str().ok()).and_then(|r| r.strip_prefix("bytes=")).and_then(|r| r.trim_end_matches('-').parse::<u64>().ok());
        f.asked.lock().unwrap().push((name, from.unwrap_or(0)));
        match from {
            Some(a) if a as usize >= bytes.len() => StatusCode::RANGE_NOT_SATISFIABLE.into_response(),
            Some(a) => (StatusCode::PARTIAL_CONTENT, bytes[a as usize..].to_vec()).into_response(),
            None => (StatusCode::OK, bytes).into_response(),
        }
    }

    async fn stand_in(files: &[(&str, Vec<u8>)]) -> (String, Files) {
        let f = Files::default();
        for (n, b) in files {
            f.files.lock().unwrap().insert(n.to_string(), b.clone());
        }
        let app = axum::Router::new().route("/f/{name}", axum::routing::get(serve)).with_state(f.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/f", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (base, f)
    }

    fn sha(b: &[u8]) -> String {
        Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
    }

    fn bytes(n: usize, seed: u8) -> Vec<u8> {
        (0..n).map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed)).collect()
    }

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rms-fetch-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    async fn run(d: &Path, items: Vec<Item>) -> (Result<(), String>, Vec<f64>) {
        let d = d.to_path_buf();
        tokio::task::spawn_blocking(move || {
            let seen = Mutex::new(Vec::new());
            let r = fetch_all(&d, &items, &|p, _| seen.lock().unwrap().push(p));
            (r, seen.into_inner().unwrap())
        })
        .await
        .unwrap()
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn pieces_are_joined_in_order_and_checked() {
        let (a, b) = (bytes(300_000, 1), bytes(123_457, 2));
        let whole = [a.clone(), b.clone()].concat();
        let (base, _) = stand_in(&[("m.part00", a), ("m.part01", b)]).await;
        let d = dir("join");
        let it = item("m.onnx", "a model", vec![format!("{base}/m.part00"), format!("{base}/m.part01")], whole.len() as u64, &sha(&whole), Unpack::Keep);
        let (r, seen) = run(&d, vec![it.clone()]).await;
        r.unwrap();
        assert_eq!(std::fs::read(d.join("m.onnx")).unwrap(), whole);
        assert!(got(&d, &it));
        assert_eq!(std::fs::read_dir(d.join(".download")).unwrap().count(), 0, "no pieces left");
        // progress only rises, and ends at 1
        assert!(seen.windows(2).all(|w| w[1] >= w[0]), "{seen:?}");
        assert_eq!(*seen.last().unwrap(), 1.0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_wrong_checksum_keeps_nothing() {
        let (base, _) = stand_in(&[("x", bytes(5000, 3))]).await;
        let d = dir("wrong");
        let it = item("x.onnx", "a model", vec![format!("{base}/x")], 5000, &sha(b"something else"), Unpack::Keep);
        let (r, _) = run(&d, vec![it.clone()]).await;
        let e = r.unwrap_err();
        assert!(e.contains("checksum") && e.contains("a model"), "{e}");
        assert!(!d.join("x.onnx").exists());
        assert!(!got(&d, &it));
        assert_eq!(std::fs::read_dir(d.join(".download")).unwrap().count(), 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_cut_off_download_carries_on() {
        let whole = bytes(200_000, 4);
        let (base, f) = stand_in(&[("y", whole.clone())]).await;
        let d = dir("resume");
        // the first 70 000 bytes arrived before it was cut off
        std::fs::create_dir_all(d.join(".download")).unwrap();
        std::fs::write(d.join(".download/y.onnx.part0"), &whole[..70_000]).unwrap();
        let it = item("y.onnx", "a model", vec![format!("{base}/y")], whole.len() as u64, &sha(&whole), Unpack::Keep);
        let (r, _) = run(&d, vec![it]).await;
        r.unwrap();
        assert_eq!(std::fs::read(d.join("y.onnx")).unwrap(), whole);
        assert_eq!(*f.asked.lock().unwrap(), vec![("y".to_string(), 70_000)]);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_server_that_ignores_ranges_starts_it_over() {
        let whole = bytes(150_000, 6);
        let (base, f) = stand_in(&[("w", whole.clone())]).await;
        f.no_ranges.store(true, std::sync::atomic::Ordering::Relaxed);
        let d = dir("noranges");
        std::fs::create_dir_all(d.join(".download")).unwrap();
        std::fs::write(d.join(".download/w.onnx.part0"), &whole[..40_000]).unwrap();
        let it = item("w.onnx", "a model", vec![format!("{base}/w")], whole.len() as u64, &sha(&whole), Unpack::Keep);
        run(&d, vec![it]).await.0.unwrap();
        assert_eq!(std::fs::read(d.join("w.onnx")).unwrap(), whole);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_pieces_record_without_the_piece_is_fetched_again() {
        let (a, b) = (bytes(10_000, 7), bytes(5_000, 8));
        let whole = [a.clone(), b.clone()].concat();
        let (base, _) = stand_in(&[("v0", a), ("v1", b)]).await;
        let d = dir("stray");
        // stopped between deleting the first piece and its record, last time
        std::fs::create_dir_all(d.join(".download")).unwrap();
        std::fs::write(d.join(".download/v.onnx.part0.done"), "").unwrap();
        let it = item("v.onnx", "a model", vec![format!("{base}/v0"), format!("{base}/v1")], whole.len() as u64, &sha(&whole), Unpack::Keep);
        run(&d, vec![it]).await.0.unwrap();
        assert_eq!(std::fs::read(d.join("v.onnx")).unwrap(), whole);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_items_files_go_to_its_folder() {
        let whole = bytes(2_000, 9);
        let (base, _) = stand_in(&[("u", whole.clone())]).await;
        let d = dir("folder");
        let it = into("gpu", item("u.dll", "a runtime", vec![format!("{base}/u")], 2_000, &sha(&whole), Unpack::Keep));
        run(&d, vec![it.clone()]).await.0.unwrap();
        assert_eq!(std::fs::read(d.join("gpu/u.dll")).unwrap(), whole);
        assert!(!d.join("u.dll").exists());
        assert!(got(&d, &it));
        // a record of the same name from the flat layout of before doesn't count
        let flat = item("u.dll", "a runtime", vec![format!("{base}/u")], 2_000, &sha(&whole), Unpack::Keep);
        assert!(!got(&d, &flat));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_zips_chosen_files_land_flat() {
        let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default();
        for (n, b) in [("pkg/lib/one.dll", &b"one"[..]), ("pkg/bin/sub/cudnn_ops64_9.dll", b"two"), ("pkg/README.txt", b"no"), ("pkg/lib/one.lib", b"no")] {
            z.start_file(n, opts).unwrap();
            z.write_all(b).unwrap();
        }
        let zipped = z.finish().unwrap().into_inner();
        let (base, _) = stand_in(&[("p.zip", zipped.clone())]).await;
        let d = dir("zip");
        let it = item("pkg", "a runtime", vec![format!("{base}/p.zip")], zipped.len() as u64, &sha(&zipped), Unpack::Zip(&[("one.dll", ""), ("cudnn*64_9.dll", "")]));
        let (r, _) = run(&d, vec![it]).await;
        r.unwrap();
        assert_eq!(std::fs::read(d.join("one.dll")).unwrap(), b"one");
        assert_eq!(std::fs::read(d.join("cudnn_ops64_9.dll")).unwrap(), b"two");
        assert!(!d.join("README.txt").exists() && !d.join("one.lib").exists());
        assert!(!d.join("pkg").exists(), "the archive isn't kept");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_tgzs_file_is_kept_under_its_new_name() {
        let mut t = tar::Builder::new(flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast()));
        let mut h = tar::Header::new_gnu();
        h.set_size(4);
        h.set_mode(0o644);
        h.set_cksum();
        t.append_data(&mut h, "ort/lib/libonnxruntime.so.1.31.1", &b"real"[..]).unwrap();
        let tgz = t.into_inner().unwrap().finish().unwrap();
        let (base, _) = stand_in(&[("o.tgz", tgz.clone())]).await;
        let d = dir("tgz");
        let it = item("onnxruntime", "ONNX Runtime", vec![format!("{base}/o.tgz")], tgz.len() as u64, &sha(&tgz), Unpack::Tgz(&[("libonnxruntime.so.1.31.1", "libonnxruntime.so")]));
        let (r, _) = run(&d, vec![it]).await;
        r.unwrap();
        assert_eq!(std::fs::read(d.join("libonnxruntime.so")).unwrap(), b"real");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn what_is_there_already_isnt_fetched_again() {
        let whole = bytes(1000, 5);
        let (base, f) = stand_in(&[("z", whole.clone())]).await;
        let d = dir("again");
        let it = item("z.onnx", "a model", vec![format!("{base}/z")], 1000, &sha(&whole), Unpack::Keep);
        run(&d, vec![it.clone()]).await.0.unwrap();
        run(&d, vec![it.clone()]).await.0.unwrap();
        assert_eq!(f.asked.lock().unwrap().len(), 1);
        assert_eq!(missing_bytes(&d, &[it]), 0);
    }

    #[test]
    fn what_each_server_needs() {
        let names = |v: Vec<Item>| v.into_iter().map(|i| i.name).collect::<Vec<_>>();
        assert_eq!(names(wanted(false, false, true)), ["onnxruntime", "htdemucs_fp16weights.onnx", "mms_fa_fp16w.onnx"]);
        assert_eq!(names(wanted(false, true, true)), ["onnxruntime", "htdemucs_fp16weights.onnx", "mms_fa_fp16w.onnx"], "Linux: the processor");
        assert_eq!(names(wanted(true, false, false)), ["onnxruntime", "vc-runtime-cpu", "ffmpeg", "htdemucs_fp16weights.onnx", "mms_fa_fp16w.onnx"]);
        assert_eq!(names(wanted(true, true, true)), ["onnxruntime-gpu", "cuda-runtime", "cublas", "cufft", "cudnn", "vc-runtime-gpu", "htdemucs_fp16weights.onnx", "mms_fa_fp16w.onnx"]);
        // the two builds of ONNX Runtime never share a folder (nor a record)
        let dirs = |v: Vec<Item>| v.into_iter().map(|i| (i.name, i.dir)).collect::<Vec<_>>();
        assert!(dirs(wanted(true, true, true)).iter().filter(|(n, _)| !n.ends_with(".onnx")).all(|(_, d)| *d == "gpu"));
        assert!(dirs(wanted(true, false, true)).iter().filter(|(n, _)| !n.ends_with(".onnx")).all(|(_, d)| *d == "cpu"));
        assert_eq!(dirs(wanted(false, false, true))[0], ("onnxruntime".to_string(), "cpu"));
        assert!(dirs(wanted(true, false, false)).iter().any(|(n, d)| n == "ffmpeg" && d.is_empty()), "ffmpeg beside the models");
        for i in wanted(true, true, false).into_iter().chain(wanted(false, false, false)) {
            assert!(i.sha256.len() == 64 && i.sha256.chars().all(|c| c.is_ascii_hexdigit()), "{}", i.name);
            assert!(i.size > 0 && !i.urls.is_empty() && i.urls.iter().all(|u| u.starts_with("https://")), "{}", i.name);
        }
        assert_eq!(wanted(false, false, true).last().unwrap().urls.len(), 7);
    }
}
