//! Splitting on this computer: Demucs (a song's four parts) and MMS (each
//! word's time), both on PyTorch for the CPU, installed once into the app's
//! own folder (~/.local/share/radiance-music/demucs) and run at idle priority
//! on as many cores as you allow. Nothing leaves your computer.

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use gtk::glib;

use crate::settings::LocalSplitter;

/// The two scripts, kept up to date with the app.
const SPLIT_PY: &str = include_str!("../server/split.py");
const ALIGN_PY: &str = include_str!("../server/align.py");

/// What splitting needs (see TIMINGS): up to 2.7 GB of memory while it works;
/// the install is ~1.2 GB and the models (four parts, six parts, word timing) ~1.4 GB more.
pub const MIN_CORES: usize = 2;
pub const MIN_RAM_GB: f64 = 4.0;
pub const MIN_FREE_GB: f64 = 4.0;
pub const GOOD_CORES: usize = 4;
pub const GOOD_RAM_GB: f64 = 8.0;

/// Measured with the app's own scripts on a 4-minute song (2026-10-28, two
/// runs each; ribs02 runs other services too): (cores, minutes to split it,
/// minutes to time its words, most memory in GB). On this 35 W chip more
/// cores help little past 2: its power limit holds it back.
pub const MEASURED_ON: &str = "Measured on a 6-core Intel Core i5-9500T (35 W, 2019) with 16 GB, a 4-minute song, other programs running. On low-power processors more cores help little past 2; desktop processors gain more.";
pub const TIMINGS: &[(usize, f64, f64, f64)] = &[(6, 3.5, 2.4, 2.7), (4, 4.0, 2.3, 2.7), (2, 3.5, 2.1, 2.7), (1, 6.2, 2.2, 2.7)];

fn home() -> PathBuf {
    glib::user_data_dir().join("radiance-music")
}

fn venv() -> PathBuf {
    home().join("demucs")
}

/// Cores to use when you haven't said: half of them (at least one).
pub fn half_the_cores() -> u32 {
    (cores() / 2).max(1) as u32
}

pub fn cores() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2)
}

/// This computer, against what splitting needs.
pub struct Check {
    /// "Python 3.12", or None: no Python 3 with venv.
    pub python: Option<String>,
    pub cores: usize,
    pub ram_gb: f64,
    pub free_gb: f64,
}

impl Check {
    /// What stops it, in plain words (empty: it can be set up).
    pub fn problems(&self) -> Vec<String> {
        let mut p = vec![];
        if self.python.is_none() {
            p.push("Python 3 (with its venv module) isn't installed: install python3 and python3-venv from your system's software first.".into());
        }
        if self.cores < MIN_CORES {
            p.push(format!("It needs at least {MIN_CORES} cores; this computer has {}.", self.cores));
        }
        if self.ram_gb + 0.5 < MIN_RAM_GB {
            p.push(format!("It needs at least {MIN_RAM_GB:.0} GB of memory; this computer has {:.1} GB.", self.ram_gb));
        }
        if self.free_gb < MIN_FREE_GB {
            p.push(format!("It needs {MIN_FREE_GB:.0} GB free on the drive with your home folder; {:.1} GB is free.", self.free_gb));
        }
        p
    }
}

/// Windows: the Python the installer brings, with Demucs, MMS and their models beside it
/// (python\ and models\ in the app's folder): nothing to set up there.
pub fn bundled() -> Option<PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    // the pack downloaded at the first start (see install_pack), or one inside the install (test builds)
    let downloaded = pack_dir().join("python").join("python.exe");
    if downloaded.exists() {
        return Some(downloaded);
    }
    let root = std::env::current_exe().ok()?.parent()?.parent()?.to_path_buf();
    let python = root.join("python").join("python.exe");
    python.exists().then_some(python)
}

/// Windows: the splitter pack (the same Python, PyTorch, Demucs and the two models the installer
/// held before) is downloaded the first time the app opens (owner, 2026-10-31: "download on first
/// open ng app", "setup": the installer stays small, as Linux's download does). In the app's data
/// folder, so updating the app keeps it.
fn pack_dir() -> PathBuf {
    home().join("windows-splitter")
}

/// Where it comes from (this release's asset, kept for later versions), its size and SHA-256.
#[cfg(windows)]
const PACK_URL: &str = "https://github.com/RadianceADM/radiance-music/releases/download/v0.7.12/RadianceMusic-Splitter-win64.zip";
#[cfg(windows)]
const PACK_BYTES: u64 = 841_859_602;
#[cfg(windows)]
const PACK_SHA256: &str = "be9bf6a714fba256f05ecce45b9f90e4eaac6c52f6a73ae522f84473b30ca364";

/// The models the Windows installer brings (Demucs from Hugging Face's cache, MMS from torch's).
pub fn bundled_models() -> Option<PathBuf> {
    let root = bundled()?.parent()?.parent()?.join("models");
    root.exists().then_some(root)
}

pub fn check() -> Check {
    if let Some(p) = bundled() {
        let _ = fs::create_dir_all(home());
        return Check { python: Some(format!("Python (comes with the app: {})", p.display())), cores: cores(), ram_gb: ram_gb(), free_gb: crate::storage::free_space(&home()) as f64 / 1e9 };
    }
    if cfg!(windows) {
        // it comes with the splitter pack (install_pack)
        let _ = fs::create_dir_all(home());
        return Check { python: Some("Python (comes with the splitter pack)".into()), cores: cores(), ram_gb: ram_gb(), free_gb: crate::storage::free_space(&home()) as f64 / 1e9 };
    }
    let python = Command::new("python3")
        .args(["-c", "import sys, venv, ensurepip; print('Python %d.%d' % sys.version_info[:2])"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    let ram_kb = fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|m| m.lines().find(|l| l.starts_with("MemTotal:")).and_then(|l| l.split_whitespace().nth(1)?.parse::<f64>().ok()))
        .unwrap_or(0.0);
    let _ = fs::create_dir_all(home());
    Check { python, cores: cores(), ram_gb: ram_kb / 1024.0 / 1024.0, free_gb: crate::storage::free_space(&home()) as f64 / 1e9 }
}

/// This computer's memory, GB (Windows: as GLib's sysinfo can't say, ask Windows).
fn ram_gb() -> f64 {
    #[cfg(windows)]
    {
        #[repr(C)]
        struct MemoryStatusEx { length: u32, load: u32, total_phys: u64, avail_phys: u64, total_page: u64, avail_page: u64, total_virt: u64, avail_virt: u64, avail_ext: u64 }
        extern "system" {
            fn GlobalMemoryStatusEx(status: *mut MemoryStatusEx) -> i32;
        }
        let mut m = MemoryStatusEx { length: std::mem::size_of::<MemoryStatusEx>() as u32, load: 0, total_phys: 0, avail_phys: 0, total_page: 0, avail_page: 0, total_virt: 0, avail_virt: 0, avail_ext: 0 };
        if unsafe { GlobalMemoryStatusEx(&mut m) } != 0 {
            return m.total_phys as f64 / 1024.0 / 1024.0 / 1024.0;
        }
    }
    0.0
}

/// Set up already: the splitter in the app's folder (None: not yet).
pub fn installed(threads: u32) -> Option<LocalSplitter> {
    if let Some(python) = bundled() {
        // the scripts are written into the app's data folder at start (refresh_scripts)
        refresh_scripts();
        return Some(LocalSplitter { python: python.display().to_string(), script: home().join("split.py").display().to_string(), threads });
    }
    let python = venv().join("bin/python");
    let script = home().join("split.py");
    (python.exists() && script.exists() && venv().join(".radiance-ready").exists())
        .then(|| LocalSplitter { python: python.display().to_string(), script: script.display().to_string(), threads })
}

/// Windows: the splitter pack downloaded (picking up where a dropped download stopped), checked
/// and unpacked into the data folder (worker thread). A damaged download starts over.
#[cfg(windows)]
fn install_pack(threads: u32, say: &(dyn Fn(&str, f64) + Send + Sync)) -> Result<LocalSplitter, String> {
    use sha2::{Digest, Sha256};
    use std::io::{Read, Write};
    if !pack_dir().join("python").join("python.exe").exists() {
        fs::create_dir_all(home()).map_err(|e| e.to_string())?;
        let part = home().join("windows-splitter.zip.part");
        let mb = |b: u64| (b as f64 / 1e6).round() as u64;
        // the download, from where it stopped (a Range request; 200: the server starts over)
        let mut have = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
        if have > PACK_BYTES {
            let _ = fs::remove_file(&part);
            have = 0;
        }
        if have < PACK_BYTES {
            // RADIANCE_PACK_URL: another place to get the same pack from (a test machine before it's published)
            let url = std::env::var("RADIANCE_PACK_URL").unwrap_or_else(|_| PACK_URL.to_string());
            let mut req = crate::net::agent().get(&url);
            if have > 0 {
                req = req.header("Range", &format!("bytes={have}-"));
            }
            let mut resp = req.call().map_err(|e| e.to_string())?;
            let resumed = resp.status().as_u16() == 206;
            if !resumed {
                have = 0;
            }
            let mut file = fs::OpenOptions::new().create(true).write(true).append(resumed).truncate(!resumed).open(&part).map_err(|e| e.to_string())?;
            let mut body = resp.body_mut().as_reader();
            let mut buf = vec![0u8; 256 * 1024];
            let mut said = 0u64;
            loop {
                let n = body.read(&mut buf).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
                have += n as u64;
                if have - said >= 4_000_000 || have >= PACK_BYTES {
                    said = have;
                    say(&format!("Downloading the splitter: {} of {} MB…", mb(have), mb(PACK_BYTES)), 0.8 * have as f64 / PACK_BYTES as f64);
                }
            }
            file.flush().map_err(|e| e.to_string())?;
            if have < PACK_BYTES {
                return Err(format!("the download stopped at {} of {} MB (connection reset)", mb(have), mb(PACK_BYTES)));
            }
        }
        say("Checking the download…", 0.8);
        let mut hash = Sha256::new();
        let mut f = fs::File::open(&part).map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; 1 << 20];
        loop {
            let n = f.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            hash.update(&buf[..n]);
        }
        let got: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
        if got != PACK_SHA256 {
            let _ = fs::remove_file(&part);
            return Err("the download came damaged; it starts over".into());
        }
        // unpacked beside, then moved into place: half an unpack never looks ready
        let new = home().join("windows-splitter.new");
        let _ = fs::remove_dir_all(&new);
        let mut zip = zip::ZipArchive::new(fs::File::open(&part).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        let count = zip.len();
        for i in 0..count {
            let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
            let Some(rel) = entry.enclosed_name() else { continue };
            let to = new.join(rel);
            if entry.is_dir() {
                fs::create_dir_all(&to).map_err(|e| e.to_string())?;
                continue;
            }
            if let Some(d) = to.parent() {
                fs::create_dir_all(d).map_err(|e| e.to_string())?;
            }
            let mut out = fs::File::create(&to).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
            if i % 200 == 0 {
                say("Unpacking the splitter…", 0.8 + 0.19 * i as f64 / count as f64);
            }
        }
        let _ = fs::remove_dir_all(pack_dir());
        fs::rename(&new, pack_dir()).map_err(|e| e.to_string())?;
        let _ = fs::remove_file(&part);
    }
    say("Ready", 1.0);
    installed(threads).ok_or_else(|| "the splitter pack is there, but its Python isn't".into())
}

/// The scripts written again when this app's are newer (worker thread).
pub fn refresh_scripts() {
    // a fresh install has no data folder yet (the first split can come before anything else)
    let _ = fs::create_dir_all(home());
    for (name, text) in [("split.py", SPLIT_PY), ("align.py", ALIGN_PY)] {
        let p = home().join(name);
        if fs::read_to_string(&p).ok().as_deref() != Some(text) {
            let _ = fs::write(&p, text);
        }
    }
}

/// Windows: a program run from the app opens no console window of its own (and at idle
/// priority when `idle`). Elsewhere it does nothing.
#[allow(unused_variables)]
pub fn quiet_window(cmd: &mut Command, idle: bool) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const IDLE_PRIORITY_CLASS: u32 = 0x0000_0040;
        cmd.creation_flags(CREATE_NO_WINDOW | if idle { IDLE_PRIORITY_CLASS } else { 0 });
    }
}

/// A step's command, at the lowest priority (setting up never slows you down).
fn quiet(program: &Path, args: &[&str]) -> Command {
    if !cfg!(unix) {
        // Windows has no nice or ionice: the step runs as it is
        let mut cmd = Command::new(program);
        cmd.args(args);
        return cmd;
    }
    let mut cmd = Command::new("nice");
    cmd.arg("-n").arg("19");
    if Path::new("/usr/bin/ionice").exists() {
        cmd = Command::new("/usr/bin/ionice");
        cmd.args(["-c", "3", "nice", "-n", "19"]);
    }
    cmd.arg(program).args(args);
    cmd
}

/// Runs one step, its output into the log; `line` gets each line as it comes.
fn step(cmd: &mut Command, log: &mut fs::File, line: &dyn Fn(&str)) -> Result<(), String> {
    use std::io::Write;
    let _ = writeln!(log, "+ {cmd:?}");
    let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| e.to_string())?;
    let err = child.stderr.take().ok_or("no stderr")?;
    let errs = std::thread::spawn(move || BufReader::new(err).lines().map_while(Result::ok).collect::<Vec<_>>());
    for l in BufReader::new(child.stdout.take().ok_or("no stdout")?).lines().map_while(Result::ok) {
        let _ = writeln!(log, "{l}");
        line(&l);
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let errs = errs.join().unwrap_or_default();
    for l in &errs {
        let _ = writeln!(log, "{l}");
    }
    if status.success() {
        Ok(())
    } else {
        Err(errs.iter().rev().find(|l| !l.trim().is_empty()).cloned().unwrap_or_else(|| "a step failed".into()))
    }
}

/// Sets splitting up on this computer (worker thread; 10 to 30 minutes, about
/// 2.5 GB downloaded). `say(what's happening, part done 0-1)`.
#[cfg_attr(windows, allow(unreachable_code))] // (Windows: the pack, not a venv)
pub fn install(threads: u32, say: &(dyn Fn(&str, f64) + Send + Sync)) -> Result<LocalSplitter, String> {
    let problems = check().problems();
    if !problems.is_empty() {
        return Err(problems.join(" "));
    }
    #[cfg(windows)]
    return install_pack(threads, say);
    fs::create_dir_all(home()).map_err(|e| e.to_string())?;
    let mut log = fs::File::create(home().join("demucs.install.log")).map_err(|e| e.to_string())?;
    let (py, pip) = (venv().join("bin/python"), venv().join("bin/pip"));
    let none = |_: &str| {};
    say("Making a place for it…", 0.02);
    if !py.exists() {
        step(&mut quiet(Path::new("python3"), &["-m", "venv", &venv().display().to_string()]), &mut log, &none)?;
    }
    step(&mut quiet(&pip, &["install", "-q", "--upgrade", "pip"]), &mut log, &none)?;
    say("Downloading PyTorch for the CPU (about 1 GB)…", 0.08);
    step(
        &mut quiet(
            &pip,
            &["install", "--progress-bar", "off", "--only-binary=:all:", "--index-url", "https://download.pytorch.org/whl/cpu", "--extra-index-url", "https://pypi.org/simple", "torch", "torchaudio", "soundfile", "av", "numpy", "pyyaml"],
        ),
        &mut log,
        &|l| {
            if let Some(name) = l.strip_prefix("Collecting ") {
                say(&format!("Downloading {}…", name.split_whitespace().next().unwrap_or(name)), 0.3);
            }
        },
    )?;
    say("Downloading Demucs…", 0.55);
    step(&mut quiet(&pip, &["install", "-q", "--only-binary=:all:", "demucs"]), &mut log, &none)?;
    refresh_scripts();
    say("Downloading the splitting model (about 80 MB)…", 0.65);
    step(&mut quiet(&py, &["-c", "from demucs.pretrained import get_model; get_model('htdemucs'); print('ok')"]), &mut log, &none)?;
    // the six-part one too, once six parts are back (a song ticked for Piano and Guitar never waits for a download)
    if crate::split::SIX_PARTS_ON {
        say("Downloading the six-part model (about 80 MB)…", 0.7);
        step(&mut quiet(&py, &["-c", "from demucs.pretrained import get_model; get_model('htdemucs_6s'); print('ok')"]), &mut log, &none)?;
    }
    say("Downloading the word-timing model (about 1.2 GB)…", 0.75);
    step(&mut quiet(&py, &["-c", "import torchaudio; torchaudio.pipelines.MMS_FA.get_model(); print('ok')"]), &mut log, &none)?;
    say("Checking it works…", 0.97);
    step(&mut quiet(&py, &["-c", "import torch, demucs, soundfile, av, torchaudio; print('ok', torch.__version__)"]), &mut log, &none)?;
    fs::write(venv().join(".radiance-ready"), "ready\n").map_err(|e| e.to_string())?;
    say("Ready", 1.0);
    installed(threads).ok_or_else(|| "the install finished, but the splitter isn't there".into())
}
