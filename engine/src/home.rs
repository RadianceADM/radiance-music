//! Home's mix, "Made for you": songs like the likes you play most, none you keep skipping.
//! The GTK app and the Windows app make it the same way (core's queue::mix_seeds, issue #45).

use std::collections::HashMap;

use crate::track::Track;

/// The mix for `liked` (newest first), given your `history` and each song's (plays, skips) in
/// `counts` (db::counts). Looks songs up on YouTube Music: call it off the main thread.
pub fn made_for_you(liked: &[Track], history: &[(Track, i64)], counts: &HashMap<String, (usize, usize)>) -> Vec<Track> {
    if liked.is_empty() {
        return Vec::new();
    }
    let (seeds, avoid) = radiance_core::queue::mix_seeds(liked, history, |t| {
        let (plays, skips) = counts.get(&t.key()).copied().unwrap_or_default();
        (plays as u32, skips as u32)
    });
    crate::mix::from_likes(liked, &seeds, &avoid)
}
