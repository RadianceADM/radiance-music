//! Share Songs with a Phone and Copy Link (stage 4b): the Linux app's words (src/ui/sharing.rs, src/ui.rs), now
//! core's for both computer apps.

pub const TITLE: &str = "Share Songs with a Phone";
pub const MENU: &str = "Share Songs with a Phone…";
pub const TO_PHONE: &str = "To the Phone";
pub const FROM_PHONE: &str = "From the Phone";
pub const FROM_PHONE_CAPTION: &str = "Scan with Radiance Music on the phone (Settings → Get songs; before 0.5.9, Library → the QR button). Its songs come here with their parts, lyrics and covers, but only what this computer hasn't got.";
pub const NO_NETWORK: &str = "This computer isn't on a network. Connect it to the same Wi-Fi or router as the phone.";
pub const GENTLE: &str = "Going slowly while music plays to Bluetooth: on this computer Wi-Fi and Bluetooth share a radio. Plug in the cable for full speed.";
pub const FIREWALL_WINDOWS: &str = "Windows' firewall may keep the phone out";
pub const FIREWALL_LINUX: &str = "This computer's firewall is on";
pub const LET_IT_IN: &str = "Let It In";
pub const LET_IN_TOAST: &str = "The phone can reach this computer now";
pub const COPY_LINK: &str = "Copy Link";
pub const LINK_COPIED: &str = "Link copied";

/// To the phone: the liked songs here that go (only what it hasn't got).
pub fn to_phone_caption(liked: usize) -> String {
    format!("Scan with Radiance Music on the phone (Settings → Get songs; before 0.5.9, Library → the QR button). It takes your {liked} liked songs with their parts, lyrics and covers, but only what it hasn't got.")
}

pub fn couldnt_get_ready(why: &str) -> String {
    format!("Couldn't get ready: {why}")
}

/// What the phone sees this computer as.
pub fn device_name(host: &str) -> String {
    format!("{host} (Radiance Music)")
}

/// The firewall row's words (ports first..=last): Windows asks to allow it, Linux for the password.
pub fn firewall_sub(windows: bool, first: u16, last: u16) -> String {
    if windows {
        format!("If Windows asked about Radiance Music and it wasn't allowed, the phone can't reach this computer. Let the phone in from your home network (ports {first} to {last}); Windows asks you to allow it.")
    } else {
        format!("It may keep the phone out. Let the phone in on your home network (ports {first} to {last}); it asks for your password.")
    }
}

/// A size as the dialog says it: MB, or GB with one decimal from 1 GB.
pub fn mb(bytes: u64) -> String {
    if bytes >= 1 << 30 {
        format!("{:.1} GB", bytes as f64 / 1073741824.0)
    } else {
        format!("{} MB", bytes >> 20)
    }
}

/// How far along: songs and sizes when the total's known (from the phone), else what's been sent; the song now.
pub fn progress(songs: usize, of: usize, bytes: u64, total: u64, now: Option<&str>) -> String {
    let mut words = if of > 0 { format!("{songs} of {of} songs · {} of {}", mb(bytes), mb(total)) } else { format!("{} sent", mb(bytes)) };
    if let Some(t) = now {
        words.push_str(&format!(" · “{t}”"));
    }
    words
}

pub fn done(of: usize) -> String {
    format!("Done: {of} song{} here", if of == 1 { "" } else { "s" })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stage 4b: the Linux app's sharing words, said the same by the beta.
    #[test]
    fn sharing_says_it_as_the_linux_app() {
        assert_eq!(mb(5 << 20), "5 MB");
        assert_eq!(mb(3 << 29), "1.5 GB");
        assert_eq!(progress(2, 5, 30 << 20, 90 << 20, Some("Yellow")), "2 of 5 songs · 30 MB of 90 MB · “Yellow”");
        assert_eq!(progress(0, 0, 12 << 20, 0, None), "12 MB sent");
        assert_eq!((done(1), done(3)), ("Done: 1 song here".to_string(), "Done: 3 songs here".to_string()));
        assert!(to_phone_caption(12).contains("It takes your 12 liked songs"));
        assert!(firewall_sub(true, 47800, 47809).ends_with("(ports 47800 to 47809); Windows asks you to allow it."));
        assert_eq!(device_name("ribs03"), "ribs03 (Radiance Music)");
        assert_eq!((COPY_LINK, LINK_COPIED, LET_IT_IN), ("Copy Link", "Link copied", "Let It In"));
    }
}
