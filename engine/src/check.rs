//! The Windows app's self-check (issue #45; run over SSH, no window): what it sees of its data,
//! GStreamer, the plugins today's Windows app ships, and yt-dlp.

/// The plugins today's Windows app ships (packaging/windows/ribs03/gst-plugins.txt).
const PLUGINS: &str = include_str!("../../packaging/windows/ribs03/gst-plugins.txt");

/// A plugin's name in GStreamer's registry from its file: "libgstcoreelements.dll" (MSYS2) or
/// "gstcoreelements.dll" (GStreamer's own MSVC build) → "coreelements".
pub fn plugin_name(file: &str) -> String {
    let f = file.trim();
    let f = f.strip_suffix(".dll").unwrap_or(f);
    let f = f.strip_prefix("lib").unwrap_or(f);
    f.strip_prefix("gst").unwrap_or(f).to_string()
}

/// Plugins on ribs02's list that MSYS2 doesn't build: today's Windows app ships without them (#39).
const NOT_IN_MSYS2: &[&str] = &["ipcpipeline"];

/// The plugins the app should find: today's Windows app's.
pub fn expected() -> Vec<String> {
    PLUGINS.lines().filter(|l| !l.trim().is_empty()).map(plugin_name).filter(|p| !NOT_IN_MSYS2.contains(&p.as_str())).collect()
}

/// One line each: the engine's version, the data folder, GStreamer's version, the plugins it
/// can't find, yt-dlp, and how many songs the history has.
pub fn lines() -> Vec<String> {
    let mut out = vec![format!("engine {}", crate::VERSION), format!("data {}", glib::user_data_dir().join("radiance-music").display())];
    match gst::init() {
        Ok(()) => {
            out.push(format!("gstreamer {}", gst::version_string()));
            let registry = gst::Registry::get();
            let missing: Vec<String> = expected().into_iter().filter(|p| registry.find_plugin(p).is_none()).collect();
            out.push(format!("missing plugins: {}", if missing.is_empty() { "none".to_string() } else { missing.join(", ") }));
        }
        Err(e) => out.push(format!("gstreamer didn't start: {e}")),
    }
    out.push(format!("yt-dlp {}", if crate::youtube::installed() { "found" } else { "missing" }));
    out.push(format!("recent {}", crate::session::recent(&crate::session::load().history).len()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plugins_expected_are_the_ones_todays_windows_app_ships() {
        let e = expected();
        assert!(e.contains(&"coreelements".to_string()));
        assert!(!e.contains(&"ipcpipeline".to_string()), "MSYS2 doesn't build it; today's Windows app ships without it");
        assert!(e.len() >= 150);
    }

    #[test]
    fn plugin_names_from_their_files() {
        assert_eq!(plugin_name("libgstcoreelements.dll"), "coreelements");
        assert_eq!(plugin_name("gstcoreelements.dll"), "coreelements");
        assert_eq!(plugin_name("libgstopus.dll\r"), "opus");
        assert!(PLUGINS.lines().count() >= 150);
    }
}
