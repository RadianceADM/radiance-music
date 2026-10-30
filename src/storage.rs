//! Where the library is kept, how much room it all takes, and moving it to
//! another folder (worker threads).

use std::fs;
use std::path::{Path, PathBuf};

use gtk::glib;

/// How many files, how many bytes.
#[derive(Clone, Copy, Default)]
pub struct Count {
    pub files: usize,
    pub bytes: u64,
}

impl Count {
    fn add(&mut self, bytes: u64) {
        self.files += 1;
        self.bytes += bytes;
    }
}

#[derive(Clone, Copy, Default)]
pub struct Usage {
    pub songs: Count,
    pub videos: Count,
    /// The split parts (drums, bass, other, vocals; the no-vocals mix).
    pub parts: Count,
    /// Lyrics files and covers.
    pub extras: Count,
    /// The app's own data: its database and cache.
    pub app: u64,
    /// This computer's splitter (Python + Demucs), if installed.
    pub splitter: u64,
    /// Free on the drive the library is on.
    pub free: u64,
}

fn size_of(p: &Path) -> u64 {
    match fs::symlink_metadata(p) {
        Ok(m) if m.is_dir() => fs::read_dir(p).map(|d| d.filter_map(|e| e.ok()).map(|e| size_of(&e.path())).sum()).unwrap_or(0),
        Ok(m) => m.len(),
        Err(_) => 0,
    }
}

/// Free space on the drive holding `p`, in bytes.
pub fn free_space(p: &Path) -> u64 {
    let Ok(c) = std::ffi::CString::new(p.as_os_str().as_encoded_bytes()) else { return 0 };
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut s) } == 0 {
        s.f_bavail as u64 * s.f_frsize as u64
    } else {
        0
    }
}

/// What the library and the app take (worker thread).
pub fn usage(dir: &Path) -> Usage {
    let mut u = Usage::default();
    for e in fs::read_dir(dir).into_iter().flatten().filter_map(|e| e.ok()) {
        let name = e.file_name().to_string_lossy().to_lowercase();
        let bytes = e.metadata().map(|m| m.len()).unwrap_or(0);
        if name.contains(".video.") {
            u.videos.add(bytes);
        } else if [".vocals.", ".music.", ".drums.", ".bass.", ".other."].iter().any(|p| name.contains(p)) {
            u.parts.add(bytes);
        } else if name.ends_with(".lyrics.yaml") || name.ends_with(".lrc") || name.ends_with(".jpg") {
            u.extras.add(bytes);
        } else if [".opus", ".flac", ".ogg", ".m4a", ".webm", ".mp3", ".wav"].iter().any(|x| name.ends_with(x)) {
            u.songs.add(bytes);
        }
    }
    let data = glib::user_data_dir().join("radiance-music");
    u.splitter = size_of(&data.join("demucs"));
    u.app = size_of(&data).saturating_sub(u.splitter) + size_of(&glib::user_cache_dir().join("radiance-music"));
    u.free = free_space(dir);
    u
}

/// "1.4 GB", "820 MB", "12 KB".
pub fn human(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= 1e9 {
        format!("{:.1} GB", b / 1e9)
    } else if b >= 1e6 {
        format!("{:.0} MB", b / 1e6)
    } else {
        format!("{:.0} KB", (b / 1e3).max(1.0))
    }
}

/// One file into `to`: renamed when it's the same drive; else copied, checked
/// (same size) and only then taken away from the old place.
fn move_file(from: &Path, to: &Path) -> Result<(), String> {
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    let part = to.with_extension("moving");
    fs::copy(from, &part).map_err(|e| e.to_string())?;
    let (a, b) = (fs::metadata(from).map(|m| m.len()).unwrap_or(0), fs::metadata(&part).map(|m| m.len()).unwrap_or(1));
    if a != b {
        let _ = fs::remove_file(&part);
        return Err(format!("{} didn't copy whole", from.display()));
    }
    fs::rename(&part, to).map_err(|e| e.to_string())?;
    fs::remove_file(from).map_err(|e| e.to_string())
}

/// Moves every file of the library from `from` into `to` (made if needed); a
/// file of the same name already there is left as it is, the old one kept
/// too. `report(done, total)` as it goes. Returns (moved, kept back).
pub fn move_library(from: &Path, to: &Path, report: impl Fn(usize, usize)) -> Result<(usize, usize), String> {
    if to.starts_with(from) || from == to {
        return Err("Pick a folder outside the current one.".into());
    }
    fs::create_dir_all(to).map_err(|e| e.to_string())?;
    let files: Vec<PathBuf> = fs::read_dir(from).map_err(|e| e.to_string())?.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_file()).collect();
    let (mut moved, mut kept) = (0, 0);
    for (i, f) in files.iter().enumerate() {
        let Some(name) = f.file_name() else { continue };
        let target = to.join(name);
        if target.exists() {
            kept += 1;
        } else {
            move_file(f, &target)?;
            moved += 1;
        }
        report(i + 1, files.len());
    }
    Ok((moved, kept))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_every_file_and_keeps_clashes() {
        let base = std::env::temp_dir().join(format!("radiance-move-{}", std::process::id()));
        let (from, to) = (base.join("old/Radiance Music"), base.join("new/Radiance Music"));
        fs::create_dir_all(&from).unwrap();
        fs::create_dir_all(&to).unwrap();
        for (name, body) in [("A - Song.opus", "aaaa"), ("A - Song.lyrics.yaml", "lines"), ("A - Song.video.mkv", "vv"), ("B.opus", "old b")] {
            fs::write(from.join(name), body).unwrap();
        }
        fs::write(to.join("B.opus"), "new b").unwrap(); // already there: left as it is
        let (moved, kept) = move_library(&from, &to, |_, _| {}).unwrap();
        assert_eq!((moved, kept), (3, 1));
        assert_eq!(fs::read_to_string(to.join("A - Song.opus")).unwrap(), "aaaa");
        assert!(!from.join("A - Song.opus").exists());
        assert_eq!(fs::read_to_string(to.join("B.opus")).unwrap(), "new b");
        assert!(from.join("B.opus").exists(), "the clashing one stays where it was");
        assert!(move_library(&from, &from.join("inside"), |_, _| {}).is_err(), "not into itself");
        let u = usage(&to);
        assert_eq!((u.songs.files, u.videos.files, u.extras.files), (2, 1, 1));
        let _ = fs::remove_dir_all(&base);
    }
}
