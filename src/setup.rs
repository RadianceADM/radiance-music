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

pub fn check() -> Check {
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

/// Set up already: the splitter in the app's folder (None: not yet).
pub fn installed(threads: u32) -> Option<LocalSplitter> {
    let python = venv().join("bin/python");
    let script = home().join("split.py");
    (python.exists() && script.exists() && venv().join(".radiance-ready").exists())
        .then(|| LocalSplitter { python: python.display().to_string(), script: script.display().to_string(), threads })
}

/// The scripts written again when this app's are newer (worker thread).
pub fn refresh_scripts() {
    for (name, text) in [("split.py", SPLIT_PY), ("align.py", ALIGN_PY)] {
        let p = home().join(name);
        if fs::read_to_string(&p).ok().as_deref() != Some(text) {
            let _ = fs::write(&p, text);
        }
    }
}

/// A step's command, at the lowest priority (setting up never slows you down).
fn quiet(program: &Path, args: &[&str]) -> Command {
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
pub fn install(threads: u32, say: &(dyn Fn(&str, f64) + Send + Sync)) -> Result<LocalSplitter, String> {
    let problems = check().problems();
    if !problems.is_empty() {
        return Err(problems.join(" "));
    }
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
