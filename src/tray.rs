//! Windows' notification area (the "system tray"): Radiance Music's icon is there for as
//! long as the app is open (owner, 2026-11-04, #37). A click shows the window; its menu
//! plays or pauses, skips, shows the window or quits. Closing the window while music plays
//! leaves the music going with this icon to come back by; with nothing playing, closing
//! still quits. What it asks goes the media controls' way (smtc.rs), so it does what the
//! media keys do. Linux has no tray here: GNOME's top bar shows the song (mpris.rs).

use std::cell::RefCell;
use std::path::PathBuf;

use gtk::glib;
use gtk::prelude::*;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::mpris::{Command, State};

/// The icon itself, for a copy that isn't the installer's (the installer puts it beside `bin`).
const ICO: &[u8] = include_bytes!("../packaging/windows/radiance-music.ico");
/// Windows cuts a tooltip at 127 characters (UTF-16).
const MOST_TIP: usize = 127;

struct Tray {
    icon: TrayIcon,
    play: MenuItem,
    next: MenuItem,
    previous: MenuItem,
    /// What it shows now: playing, a song, the tooltip.
    shown: (bool, bool, String),
}

thread_local! {
    static TRAY: RefCell<Option<Tray>> = const { RefCell::new(None) };
}

/// Puts the icon in the notification area (once; on the window's thread).
pub fn start() {
    if TRAY.with(|t| t.borrow().is_some()) {
        return;
    }
    let Some(icon) = icon() else {
        crate::log::note("tray: no icon to show");
        return;
    };
    let play = MenuItem::with_id("play", "Play", false, None);
    let next = MenuItem::with_id("next", "Next", false, None);
    let previous = MenuItem::with_id("previous", "Previous", false, None);
    let show = MenuItem::with_id("show", "Show Radiance Music", true, None);
    let quit = MenuItem::with_id("quit", "Quit", true, None);
    let menu = Menu::new();
    let (line, line2) = (PredefinedMenuItem::separator(), PredefinedMenuItem::separator());
    if let Err(e) = menu.append_items(&[&play, &next, &previous, &line, &show, &line2, &quit]) {
        crate::log::note(&format!("tray: no menu: {e}"));
        return;
    }
    // Windows tells these inside its message handling: what they ask is done a moment later
    MenuEvent::set_event_handler(Some(|e: MenuEvent| {
        let command = match e.id.0.as_str() {
            "play" => Command::PlayPause,
            "next" => Command::Next,
            "previous" => Command::Previous,
            "show" => Command::Raise,
            "quit" => Command::Quit,
            _ => return,
        };
        glib::idle_add_once(move || crate::mpris::run(command));
    }));
    TrayIconEvent::set_event_handler(Some(|e: TrayIconEvent| {
        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
            glib::idle_add_once(|| crate::mpris::run(Command::Raise));
        }
    }));
    let built = TrayIconBuilder::new()
        .with_icon(icon)
        .with_tooltip("Radiance Music")
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false) // a click shows the window; the menu is the right click's
        .build();
    let icon = match built {
        Ok(i) => i,
        Err(e) => {
            crate::log::note(&format!("tray: Windows didn't take the icon: {e}"));
            return;
        }
    };
    TRAY.with(|t| *t.borrow_mut() = Some(Tray { icon, play, next, previous, shown: (false, false, "Radiance Music".into()) }));
    crate::log::note("tray: the icon is in the notification area");
    // gone when the app quits (else Windows shows a dead icon until the mouse passes over it)
    if let Some(app) = gtk::gio::Application::default() {
        app.connect_shutdown(|_| {
            TRAY.with(|t| t.borrow_mut().take());
        });
    }
}

/// Shows what plays (call it freely: only changes reach Windows).
pub fn show(state: &State) {
    TRAY.with(|t| {
        let mut t = t.borrow_mut();
        let Some(t) = t.as_mut() else { return };
        let tip = tooltip(state);
        let now = (state.playing, state.song.is_some(), tip);
        if t.shown == now {
            return;
        }
        t.play.set_text(if state.playing { "Pause" } else { "Play" });
        for item in [&t.play, &t.next, &t.previous] {
            item.set_enabled(state.song.is_some());
        }
        if let Err(e) = t.icon.set_tooltip(Some(&now.2)) {
            crate::log::note(&format!("tray: tooltip: {e}"));
        }
        t.shown = now;
    });
}

/// "Radiance Music", and the song under it (cut to what Windows shows).
fn tooltip(state: &State) -> String {
    let Some(s) = &state.song else { return "Radiance Music".into() };
    let mut tip = format!("Radiance Music\n{}", s.title);
    if !s.artists.is_empty() {
        tip.push_str(" · ");
        tip.push_str(&s.artists.join(", "));
    }
    cut(&tip, MOST_TIP)
}

/// `text` in at most `most` UTF-16 units, "…" where it's cut.
fn cut(text: &str, most: usize) -> String {
    if text.encode_utf16().count() <= most {
        return text.to_string();
    }
    let mut out = String::new();
    let mut n = 1; // the "…"
    for c in text.chars() {
        n += c.len_utf16();
        if n > most {
            break;
        }
        out.push(c);
    }
    out.push('…');
    out
}

/// The app's icon at the notification area's size (sharp at every scaling).
fn icon() -> Option<Icon> {
    let size = unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(windows_sys::Win32::UI::WindowsAndMessaging::SM_CXSMICON)
    };
    let size = if size > 0 { size as u32 } else { 16 };
    Icon::from_path(ico_file()?, Some((size, size))).ok()
}

/// The installer's radiance-music.ico (beside `bin`), else this copy's own, written out once.
fn ico_file() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    if let Some(p) = exe.parent().and_then(|b| b.parent()).map(|d| d.join("radiance-music.ico")) {
        if p.exists() {
            return Some(p);
        }
    }
    let p = glib::user_cache_dir().join("radiance-music").join("tray.ico");
    if std::fs::metadata(&p).map(|m| m.len() as usize).ok() != Some(ICO.len()) {
        std::fs::create_dir_all(p.parent()?).ok()?;
        std::fs::write(&p, ICO).ok()?;
    }
    Some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cut_keeps_short_and_cuts_long() {
        assert_eq!(cut("Radiance Music", 127), "Radiance Music");
        let long = "a".repeat(200);
        let c = cut(&long, 127);
        assert_eq!(c.encode_utf16().count(), 127);
        assert!(c.ends_with('…'));
        // a character outside the BMP (two UTF-16 units) is never split
        let emoji = "🎵".repeat(100);
        assert!(cut(&emoji, 127).encode_utf16().count() <= 127);
    }
}
