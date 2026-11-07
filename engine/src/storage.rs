//! Where the library is kept, how much room it all takes, and moving it to
//! another folder (worker threads).

use std::fs;
use std::path::{Path, PathBuf};


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
#[cfg(unix)]
pub fn free_space(p: &Path) -> u64 {
    let Ok(c) = std::ffi::CString::new(p.as_os_str().as_encoded_bytes()) else { return 0 };
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut s) } == 0 {
        s.f_bavail as u64 * s.f_frsize as u64
    } else {
        0
    }
}

/// Free space on the drive holding `p`, in bytes (Windows: as GIO reads it).
#[cfg(not(unix))]
pub fn free_space(p: &Path) -> u64 {
    use gio::prelude::*;
    gio::File::for_path(p)
        .query_filesystem_info("filesystem::free", gio::Cancellable::NONE)
        .map(|i| i.attribute_uint64("filesystem::free"))
        .unwrap_or(0)
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
    u.splitter = splitter_size(&data);
    u.app = size_of(&data).saturating_sub(u.splitter) + size_of(&glib::user_cache_dir().join("radiance-music"));
    u.free = free_space(dir);
    u
}

/// This computer's splitter: Linux's Python with Demucs, Windows' pack and the fast split's files
/// (the Windows two counted as App data before).
pub fn splitter_size(data: &Path) -> u64 {
    ["demucs", "windows-splitter", "two-part"].iter().map(|d| size_of(&data.join(d))).sum()
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
    move_library_with(from, to, report, move_file, true)
}

/// move_library with its way of moving one file; `undo`: a failure puts the files already moved
/// back (the database still names the old folder: they'd be missing). The sweep after the switch
/// doesn't undo (the database names the new folder by then).
fn move_library_with(from: &Path, to: &Path, report: impl Fn(usize, usize), mover: impl Fn(&Path, &Path) -> Result<(), String>, undo: bool) -> Result<(usize, usize), String> {
    if to.starts_with(from) || from == to {
        return Err("Pick a folder outside the current one.".into());
    }
    fs::create_dir_all(to).map_err(|e| e.to_string())?;
    let files: Vec<PathBuf> = fs::read_dir(from).map_err(|e| e.to_string())?.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_file()).collect();
    let (mut moved, mut kept) = (0, 0);
    let mut done: Vec<(&Path, PathBuf)> = vec![];
    for (i, f) in files.iter().enumerate() {
        let Some(name) = f.file_name() else { continue };
        let target = to.join(name);
        if target.exists() {
            kept += 1;
        } else {
            if let Err(why) = mover(f, &target) {
                if !undo {
                    return Err(why);
                }
                let stuck = done.iter().rev().filter(|(back, went)| move_file(went, back).is_err()).count();
                return Err(if stuck == 0 { why } else { format!("{why} ({stuck} files couldn't go back to {})", from.display()) });
            }
            done.push((f, target));
            moved += 1;
        }
        report(i + 1, files.len());
    }
    Ok((moved, kept))
}

/// Where the songs go when `chosen` is picked: "Radiance Music" inside it (unless it's that already).
pub fn target(chosen: &Path) -> PathBuf {
    if chosen.file_name().is_some_and(|n| n == "Radiance Music") {
        chosen.to_path_buf()
    } else {
        chosen.join("Radiance Music")
    }
}

/// Moves the songs' folder, as the GTK app's confirm_move (moved here, issue #45): every file,
/// then the folder switched (`set_dir`) and the database's paths (`relocate`), then whatever was
/// written meanwhile (a split finishing). On a failure the old folder stays the one used, the
/// database isn't touched and the files already moved go back to it.
pub fn move_and_switch(from: &Path, to: &Path, set_dir: impl Fn(Option<PathBuf>), relocate: impl Fn(&Path, &Path)) -> Result<(usize, usize), String> {
    match move_library(from, to, |_, _| {}) {
        Ok(done) => {
            set_dir(Some(to.to_path_buf()));
            relocate(from, to);
            let _ = move_library_with(from, to, |_, _| {}, move_file, false);
            Ok(done)
        }
        Err(e) => {
            set_dir(Some(from.to_path_buf()));
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn base(name: &str) -> PathBuf {
        let b = std::env::temp_dir().join(format!("radiance-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&b);
        b
    }

    /// The folder is "Radiance Music" inside the chosen one (unless that's it already).
    #[test]
    fn the_chosen_folder_gets_radiance_music() {
        assert_eq!(target(Path::new("/d/Songs")), PathBuf::from("/d/Songs/Radiance Music"));
        assert_eq!(target(Path::new("/d/Radiance Music")), PathBuf::from("/d/Radiance Music"));
    }

    /// The GTK app's sequence: the files, then the folder switched, then the database's paths.
    #[test]
    fn a_move_switches_the_folder_and_the_database() {
        let b = base("switch");
        let (from, to) = (b.join("old/Radiance Music"), b.join("new/Radiance Music"));
        fs::create_dir_all(&from).unwrap();
        fs::write(from.join("A.opus"), "a").unwrap();
        let (dir, moved_db) = (RefCell::new(None), RefCell::new(None));
        let got = move_and_switch(&from, &to, |d| *dir.borrow_mut() = d, |a, z| *moved_db.borrow_mut() = Some((a.to_path_buf(), z.to_path_buf())));
        assert_eq!(got, Ok((1, 0)));
        assert_eq!(*dir.borrow(), Some(to.clone()));
        assert_eq!(*moved_db.borrow(), Some((from.clone(), to.clone())));
        assert!(to.join("A.opus").exists());
        let _ = fs::remove_dir_all(&b);
    }

    /// Final plan Review Focus 3: a move that fails keeps the old folder, the database untouched.
    #[test]
    fn a_failed_move_keeps_the_old_folder() {
        let b = base("fail");
        let from = b.join("old/Radiance Music");
        fs::create_dir_all(&from).unwrap();
        fs::write(from.join("A.opus"), "a").unwrap();
        fs::write(b.join("blocked"), "a file where the folder would go").unwrap();
        let to = b.join("blocked/Radiance Music");
        let (dir, relocated) = (RefCell::new(None), RefCell::new(false));
        let got = move_and_switch(&from, &to, |d| *dir.borrow_mut() = d, |_, _| *relocated.borrow_mut() = true);
        assert!(got.is_err());
        assert_eq!(*dir.borrow(), Some(from.clone()), "the old folder is the one used");
        assert!(!*relocated.borrow(), "the database wasn't changed");
        assert!(from.join("A.opus").exists());
        let _ = fs::remove_dir_all(&b);
    }

    /// Final review: a move that failed half-way (on Windows, the song playing can't be moved) left
    /// the songs moved before it in the new folder while the database still named the old one, so
    /// they didn't play. They go back; the old folder is as it was.
    #[test]
    fn a_move_that_fails_half_way_puts_the_moved_songs_back() {
        let b = base("halfway");
        let (from, to) = (b.join("old/Radiance Music"), b.join("new/Radiance Music"));
        fs::create_dir_all(&from).unwrap();
        for n in ["A.opus", "B.opus", "C.opus"] {
            fs::write(from.join(n), n).unwrap();
        }
        let tries = RefCell::new(0);
        let in_use = |a: &Path, t: &Path| {
            *tries.borrow_mut() += 1;
            if *tries.borrow() == 3 { Err("in use".to_string()) } else { move_file(a, t) }
        };
        let got = move_library_with(&from, &to, |_, _| {}, in_use, true);
        assert!(got.is_err());
        for n in ["A.opus", "B.opus", "C.opus"] {
            assert!(from.join(n).exists(), "{n} isn't back in the old folder");
            assert!(!to.join(n).exists(), "{n} stayed in the new folder");
        }
        let _ = fs::remove_dir_all(&b);
    }

    #[test]
    fn files_written_during_a_move_follow_it() {
        let b = base("follow");
        let (from, to) = (b.join("old/Radiance Music"), b.join("new/Radiance Music"));
        fs::create_dir_all(&from).unwrap();
        fs::write(from.join("A.opus"), "a").unwrap();
        // a split finishing while the database is moved writes into the old folder
        let got = move_and_switch(&from, &to, |_| {}, |a, _| fs::write(a.join("A.vocals.ogg"), "v").unwrap());
        assert_eq!(got, Ok((1, 0)));
        assert!(to.join("A.vocals.ogg").exists(), "it followed");
        let _ = fs::remove_dir_all(&b);
    }

    /// Windows' splitter pack and the fast split's files are this computer's splitter, not App data.
    #[test]
    fn the_windows_splitter_counts_as_the_splitter() {
        let b = base("splitter");
        for (dir, bytes) in [("demucs", 10usize), ("windows-splitter", 20), ("two-part", 30)] {
            fs::create_dir_all(b.join(dir)).unwrap();
            fs::write(b.join(dir).join("f"), vec![0u8; bytes]).unwrap();
        }
        assert_eq!(splitter_size(&b), 60);
        let _ = fs::remove_dir_all(&b);
    }

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
