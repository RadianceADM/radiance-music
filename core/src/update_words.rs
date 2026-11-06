//! The update pop-up's words, as the GTK app's src/ui/update.rs (moved out, issue #45).

/// "1.5 GB" / "84 MB" (empty for 0), as the pop-up says sizes.
pub fn mb(bytes: u64) -> String {
    if bytes == 0 {
        String::new()
    } else if bytes >= 1 << 30 {
        format!("{:.1} GB", bytes as f64 / 1e9)
    } else {
        format!("{} MB", (bytes + 500_000) / 1_000_000)
    }
}

/// Under the download's bar: "{have} of {total}", or "{n}%" when the size isn't known (or Flatpak).
pub fn downloading_words(size: u64, done: f64, percent_only: bool) -> String {
    if percent_only || size == 0 {
        format!("{}%", (done * 100.0).round() as u32)
    } else {
        format!("{} of {}", mb((size as f64 * done) as u64).trim(), mb(size))
    }
}

pub fn out_title(version: &str) -> String {
    format!("Radiance Music {version} is out")
}
pub const OUT_NOTE: &str = "The newest open beta.";
/// The main button: the download page (an install it can't update), or Update Now with its size.
pub fn update_now_label(size: u64, page_only: bool, flatpak: bool) -> String {
    if page_only {
        "Open the Download Page".into()
    } else if flatpak || size == 0 {
        "Update Now".into()
    } else {
        format!("Update Now ({})", mb(size))
    }
}
pub const KEEPS_PLAYING: &str = "Music goes on playing while it downloads.";
pub const REMIND_LATER: &str = "Remind Me Later";
pub const SKIP_VERSION: &str = "Skip This Version";
pub fn downloading_title(version: &str) -> String {
    format!("Downloading {version}…")
}
pub const DOWNLOADING_NOTE: &str = "Music goes on playing. You can hide this: it goes on downloading.";
pub const HIDE: &str = "Hide";
pub const CANCEL_DOWNLOAD: &str = "Cancel the Download";
pub fn ready_title(version: &str) -> String {
    format!("Radiance Music {version} is ready")
}
/// Ready's words: Windows' installer opens it again by itself; elsewhere it opens on the new version.
pub fn ready_text(playing: bool, installer: bool) -> String {
    let stops = if playing { " and stops the music" } else { "" };
    let how = if installer { "the installer puts it on and opens Radiance Music again by itself" } else { "it opens again on the new version" };
    format!("Downloaded and checked. Restarting closes the app for a moment{stops}; {how}.")
}
pub const RESTART: &str = "Restart to Update";
pub fn restarting_title(version: &str) -> String {
    format!("Updating to {version}…")
}
pub const RESTARTING_NOTE: &str = "Radiance Music closes now and opens again on the new version.";
pub fn failed_text(current: &str) -> String {
    format!("Nothing changed: you're still on {current}.")
}
pub const TRY_AGAIN: &str = "Try Again";
pub const FROM_GITHUB: &str = "Get It from GitHub";
pub const TITLE: &str = "Update";
pub const LOOK: &str = "Look for Updates";
pub fn newest(version: &str) -> String {
    format!("You have the newest version ({version})")
}
pub fn couldnt_look(e: &str) -> String {
    format!("Couldn't look for updates: {e}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_words_say_sizes_as_the_linux_app() {
        assert_eq!(mb(0), "");
        assert_eq!(mb(84_400_000), "84 MB");
        assert_eq!(mb(1_500_000_000), "1.5 GB");
        assert_eq!(downloading_words(150_000_000, 0.5, false), "75 MB of 150 MB");
        assert_eq!(downloading_words(0, 0.42, false), "42%");
        assert_eq!(downloading_words(150_000_000, 0.42, true), "42%");
        assert_eq!(update_now_label(149_000_000, false, false), "Update Now (149 MB)");
        assert_eq!(update_now_label(149_000_000, true, false), "Open the Download Page");
        assert_eq!(ready_text(true, true), "Downloaded and checked. Restarting closes the app for a moment and stops the music; the installer puts it on and opens Radiance Music again by itself.");
    }
}
