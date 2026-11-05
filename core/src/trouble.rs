//! A song that won't play: why, in plain words, and whether the music goes on to the next one.
//! Moved out of the GTK app (ui.rs) so the Windows app says and does the same (issue #45).

/// Windows' sound output lost its device (WASAPI: AUDCLNT_E_DEVICE_INVALIDATED), not a bad song.
pub fn sound_device_gone(error: &str) -> bool {
    let e = error.to_lowercase();
    e.contains("audclnt_e_device_invalidated") || e.contains("88890004")
}

/// Why a song won't play, in plain words, from its error (`online`: the app thinks it is), and
/// whether every song would hit it (then the music doesn't go on to the next).
pub fn why_not(error: &str, online: bool) -> (&'static str, bool) {
    let e = error.to_lowercase();
    let any = |words: &[&str]| words.iter().any(|w| e.contains(w));
    if !online || any(&["resolve", "name resolution", "unreachable", "no route", "getaddrinfo", "connection refused", "could not connect", "network is down", "failed to establish a new connection"]) {
        ("you're offline", true)
    } else if e.contains("yt-dlp isn't installed") {
        ("yt-dlp, which plays YouTube songs, isn't installed", true)
    } else if any(&["sign in to confirm", "not a bot"]) {
        ("YouTube wants to check you're not a bot; try again in a while", true)
    } else if any(&["timed out", "timeout"]) {
        ("the internet is too slow right now", true)
    } else if any(&["missing plugin", "no decoder", "not-negotiated", "not negotiated", "no element"]) {
        ("this computer can't play this kind of file", false)
    } else if sound_device_gone(&e) {
        ("the sound device went away: check your speakers or headphones", true)
    } else if any(&["unavailable", "private video", "removed", "copyright", "not available", "terminated"]) {
        ("it isn't on YouTube anymore", false)
    } else {
        ("its source didn't answer", false)
    }
}

/// After a song that won't play.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum After {
    /// On to the next song.
    Next,
    /// It stops, and says why (with Try Again).
    Stop,
}

/// `fails` songs in a row wouldn't play (this one counted): on to the next while there's one,
/// it isn't something every song would hit, and fewer than 3 failed.
pub fn after_failure(fails: u32, more: bool, every_song: bool) -> After {
    if more && !every_song && fails < 3 {
        After::Next
    } else {
        After::Stop
    }
}

/// What it says when it stops.
pub fn cant_play_text(title: &str, reason: &str, fails: u32, every_song: bool) -> String {
    if fails >= 3 && !every_song {
        format!("{fails} songs in a row wouldn't play: {reason}")
    } else {
        format!("“{title}” can't play: {reason}")
    }
}

/// What it says when it goes on to the next song.
pub fn next_song_text(title: &str, reason: &str) -> String {
    format!("“{title}” can't play: {reason}. On to the next song.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lost_sound_device_isnt_a_bad_song() {
        // ribs03's app.log, 2026-11-03 (#38)
        assert!(sound_device_gone("IAudioClient::GetCurrentPadding failed (88890004): AUDCLNT_E_DEVICE_INVALIDATED"));
        assert!(!sound_device_gone("Internal data stream error."));
        assert!(!sound_device_gone("Could not resolve host: www.youtube.com"));
    }

    #[test]
    fn reasons_in_plain_words() {
        assert_eq!(why_not("Could not resolve host: www.youtube.com", true), ("you're offline", true));
        assert_eq!(why_not("anything", false), ("you're offline", true));
        assert_eq!(why_not("yt-dlp isn't installed", true), ("yt-dlp, which plays YouTube songs, isn't installed", true));
        assert_eq!(why_not("Sign in to confirm you're not a bot", true), ("YouTube wants to check you're not a bot; try again in a while", true));
        assert_eq!(why_not("Operation timed out", true), ("the internet is too slow right now", true));
        assert_eq!(why_not("missing plugin: opus", true), ("this computer can't play this kind of file", false));
        assert_eq!(why_not("AUDCLNT_E_DEVICE_INVALIDATED", true), ("the sound device went away: check your speakers or headphones", true));
        assert_eq!(why_not("Video unavailable", true), ("it isn't on YouTube anymore", false));
        assert_eq!(why_not("something else", true), ("its source didn't answer", false));
    }

    #[test]
    fn three_failures_in_a_row_stop() {
        assert_eq!(after_failure(2, true, false), After::Next);
        assert_eq!(after_failure(3, true, false), After::Stop);
        assert_eq!(after_failure(1, false, false), After::Stop);
        assert_eq!(after_failure(1, true, true), After::Stop);
        assert_eq!(cant_play_text("A", "you're offline", 3, false), "3 songs in a row wouldn't play: you're offline");
        assert_eq!(cant_play_text("A", "you're offline", 3, true), "“A” can't play: you're offline");
        assert_eq!(cant_play_text("A", "it isn't on YouTube anymore", 1, false), "“A” can't play: it isn't on YouTube anymore");
        assert_eq!(next_song_text("A", "it isn't on YouTube anymore"), "“A” can't play: it isn't on YouTube anymore. On to the next song.");
    }
}
