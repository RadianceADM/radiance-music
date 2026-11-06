//! The words of the app's own parts (stage 4a, #45): Report a Problem, the rating pop-up, Storage,
//! the Offline sign and About, as the GTK app has them (src/ui/report.rs, src/ui/feedback.rs,
//! ui.rs's storage_dialog, offline_pill and about), for the Windows beta to show the same.

// ---- Report a Problem (src/ui/report.rs)
pub const REPORT_TITLE: &str = "Report a Problem";
pub const REPORT_INTRO: &str = "Tell us what went wrong. Nothing is sent until you press Send, and you can see below exactly what goes.";
pub const REPORT_HEADING: &str = "What happened? (you can leave it empty)";
pub const REPORT_READING: &str = "Reading the app's log…";
pub const REPORT_WHAT: &str = "What will be sent";
pub const CANCEL: &str = "Cancel";
pub const SEND: &str = "Send";
pub const SENT_TITLE: &str = "Sent. Thank you!";
pub const COPY_CODE: &str = "Copy the Code";
pub const CLOSE: &str = "Close";

/// What will be sent, exactly as the expander shows it.
pub fn report_what(version: &str, platform: &str, system: &str, install: &str, log: &str) -> String {
    let log = if log.trim().is_empty() { "(empty)" } else { log };
    format!("Version: {version}\nInstall: {platform}\nSystem: {system}\nInstall ID: {install} (made once, at random)\n\nThe app's recent log (your name, home folder and email addresses taken out):\n\n{log}")
}

pub fn report_sent(code: &str) -> String {
    format!("Your report is {code}. If you write to us about it (hello@radianceph.com), quote that.")
}

// ---- the rating pop-up (src/ui/feedback.rs)
pub const RATE_TITLE: &str = "Rate Radiance Music";
pub const RATE_HEAD: &str = "Are you enjoying Radiance Music?";
pub const RATE_SUB: &str = "A minute of your time helps make the next version better.";
pub const YES: &str = "Yes!";
pub const NOT_REALLY: &str = "Not really";
pub const RATE_COMMENT: &str = "Anything to tell us? (optional)";
pub const CANCEL_SIGN_IN: &str = "Cancel Sign-In";
pub const SIGN_OUT: &str = "Sign Out";
pub const NOT_NOW: &str = "Not Now";
pub const DONT_ASK: &str = "Don't Ask Again";
pub const RATE_CHOOSE: &str = "Choose Yes! or Not really, and give each one its stars.";
pub const PRIVACY: &str = "https://music.radianceph.com/privacy";
pub const THANKS: &str = "Thanks!";
pub const THANKS_SHARE: &str = "Glad you're enjoying it. Know someone who'd like it too?";
pub const THANKS_PLAIN: &str = "Thanks for telling us. It helps make the next version better.";
pub const SHARE_APP: &str = "Share Radiance Music";
pub const SHARE_LINK: &str = "https://music.radianceph.com";
pub const SHARE_TOAST: &str = "Link copied: music.radianceph.com. Paste it in a chat or a post.";
pub const DONE: &str = "Done";

/// The rating's five rows (feedback::AREAS' labels, in order).
pub const RATE_AREAS: [&str; 5] = ["Overall", "Sound", "Splitting & Mixer", "Lyrics", "Sharing"];

pub fn star_tip(label: &str, n: u8) -> String {
    format!("{label}: {n} of 5")
}

/// Who the rating is sent as (the Privacy link after it, in the app's own way).
pub fn rate_who(signed_in: Option<(&str, &str)>) -> String {
    match signed_in {
        Some((name, email)) => format!("Sending as {name} ({email}). Your name and email are kept with your rating"),
        None => "Send opens your browser to sign in with Google. Your name and email are kept with your rating".to_string(),
    }
}
pub const PRIVACY_WORD: &str = "Privacy";

// ---- Storage (ui.rs storage_dialog)
pub const STORAGE_TITLE: &str = "Storage";
pub const KEPT_GROUP: &str = "Where your songs are kept";
pub const FOLDER: &str = "Folder";
pub const CHANGE: &str = "Change…";
pub const OPEN_FOLDER: &str = "Open the Music Folder";
pub const SPACE_GROUP: &str = "Space used";
pub const COUNTING: &str = "Counting…";
pub const PICK_FOLDER: &str = "Keep your songs in…";

pub fn space_total(total: &str) -> String {
    format!("{total} in your songs' folder")
}

// ---- the Offline sign (ui.rs offline_pill)
pub const OFFLINE: &str = "Offline";
pub const OFFLINE_TIP: &str = "No internet: your saved songs still play. Searching, streaming and lyrics come back with it.";

// ---- About (ui.rs about)
pub const ABOUT: &str = "About Radiance Music";
pub const APP_NAME: &str = "Radiance Music";
pub const DEVELOPER: &str = "Radiance Innovation Centers";
pub const WEBSITE: &str = "https://github.com/RadianceADM/radiance-music";
pub const ISSUES: &str = "https://github.com/RadianceADM/radiance-music/issues";
pub const COPYRIGHT: &str = "© 2026 Radiance Innovation Centers";
pub const ABOUT_TEXT: &str = "Search any song and play it. Lossless recordings from the free libraries come first; YouTube Music fills in the rest. Like a song to keep it, with its synced lyrics and album art, and shape the sound with your EQ.";
pub const CREDITS_MUSIC: &str = "Music and lyrics from";
pub const CREDITS_BUILT: &str = "Built with";
pub const SOURCES: [(&str, &str); 3] = [("Internet Archive", "https://archive.org"), ("YouTube Music", "https://music.youtube.com"), ("LRCLIB", "https://lrclib.net")];
pub const BUILT_WITH: [(&str, &str); 3] = [("GStreamer", "https://gstreamer.freedesktop.org"), ("yt-dlp", "https://github.com/yt-dlp/yt-dlp"), ("Windows App SDK", "https://learn.microsoft.com/windows/apps/windows-app-sdk/")];
pub const WHATS_NEW: &str = "What's New";
pub const REPORT_ISSUE: &str = "Report an Issue";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_report_says_what_goes_exactly() {
        assert_eq!(
            report_what("0.8.0-beta.4", "windows", "Windows 11", "abc", ""),
            "Version: 0.8.0-beta.4\nInstall: windows\nSystem: Windows 11\nInstall ID: abc (made once, at random)\n\nThe app's recent log (your name, home folder and email addresses taken out):\n\n(empty)"
        );
        assert_eq!(report_sent("RM-0042"), "Your report is RM-0042. If you write to us about it (hello@radianceph.com), quote that.");
    }

    #[test]
    fn the_rating_and_storage_say_it_as_the_linux_app() {
        assert_eq!(star_tip("Sound", 4), "Sound: 4 of 5");
        assert!(rate_who(Some(("Bassi", "b@x.com"))).starts_with("Sending as Bassi (b@x.com)."));
        assert_eq!(space_total("1.2 GB"), "1.2 GB in your songs' folder");
        assert_eq!(RATE_AREAS[2], "Splitting & Mixer");
    }
}
