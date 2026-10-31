//! Windows' media controls: the keyboard's media keys, the player in the volume
//! flyout and on the lock screen. What mpris.rs is on Linux, with the same face (the
//! window starts them and tells them what changed), through souvlaki (Windows' System
//! Media Transport Controls). They belong to a window, so they start once it is shown.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;
use souvlaki::{MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, PlatformConfig};

/// What the media controls ask for.
#[derive(Clone, Copy)]
pub enum Command {
    PlayPause,
    Play,
    Pause,
    Next,
    Previous,
    /// Show the window again.
    #[allow(dead_code)] // Windows' controls don't ask for it (Linux's do)
    Raise,
    #[allow(dead_code)]
    Quit,
}

/// What the media controls show.
#[derive(Clone, Default, PartialEq)]
pub struct State {
    pub playing: bool,
    /// A song to play (else play does nothing).
    pub song: Option<Song>,
    /// Where the song is (µs).
    #[allow(dead_code)]
    pub position: i64,
}

#[derive(Clone, Default, PartialEq)]
pub struct Song {
    /// Unique to the song (its key).
    #[allow(dead_code)]
    pub key: String,
    pub title: String,
    pub artists: Vec<String>,
    pub album: String,
    /// Its cover: a web address or a file on this computer.
    pub art: String,
    /// µs (0: not known).
    pub length: i64,
}

thread_local! {
    /// What the controls ask is done here, on the window's thread (Windows asks on one of its own).
    static COMMAND: RefCell<Option<Rc<dyn Fn(Command)>>> = RefCell::new(None);
}

pub struct Mpris {
    controls: RefCell<Option<MediaControls>>,
    shown: RefCell<Option<State>>,
    state: Box<dyn Fn() -> State>,
}

impl Mpris {
    /// `command` is called for what the controls ask, `state` for what they show.
    pub fn start(command: impl Fn(Command) + 'static, state: impl Fn() -> State + 'static) -> Rc<Mpris> {
        COMMAND.with(|c| *c.borrow_mut() = Some(Rc::new(command)));
        let me = Rc::new(Mpris { controls: RefCell::new(None), shown: RefCell::new(None), state: Box::new(state) });
        // the window has a handle once it's shown: until then, look again now and then
        let weak = Rc::downgrade(&me);
        glib::timeout_add_local(Duration::from_millis(500), move || {
            let Some(me) = weak.upgrade() else { return glib::ControlFlow::Break };
            me.changed();
            if me.controls.borrow().is_some() {
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
        me
    }

    /// The app's window's handle, once it's shown.
    fn hwnd() -> Option<*mut std::ffi::c_void> {
        gtk::Window::list_toplevels().into_iter().filter_map(|w| w.downcast::<gtk::ApplicationWindow>().ok()).find_map(|w| {
            let surface = w.surface()?.downcast::<gdk4_win32::Win32Surface>().ok()?;
            let hwnd = surface.handle().0;
            (!hwnd.is_null()).then_some(hwnd)
        })
    }

    /// The controls, made the first time the window has a handle.
    fn connect(&self) -> bool {
        if self.controls.borrow().is_some() {
            return true;
        }
        let Some(hwnd) = Self::hwnd() else { return false };
        let config = PlatformConfig { display_name: "Radiance Music", dbus_name: "radiance_music", hwnd: Some(hwnd) };
        let Ok(mut controls) = MediaControls::new(config) else {
            crate::log::note("media controls: Windows didn't give them");
            return false;
        };
        let attached = controls.attach(|event| {
            let command = match event {
                MediaControlEvent::Toggle => Command::PlayPause,
                MediaControlEvent::Play => Command::Play,
                MediaControlEvent::Pause | MediaControlEvent::Stop => Command::Pause,
                MediaControlEvent::Next => Command::Next,
                MediaControlEvent::Previous => Command::Previous,
                _ => return, // seeking from the flyout: not offered (as on Linux)
            };
            glib::MainContext::default().invoke(move || {
                let f = COMMAND.with(|c| c.borrow().clone());
                if let Some(f) = f {
                    f(command);
                }
            });
        });
        if attached.is_err() {
            crate::log::note("media controls: couldn't listen to them");
            return false;
        }
        *self.controls.borrow_mut() = Some(controls);
        self.shown.replace(None); // all of it goes out the first time
        true
    }

    /// Tells the controls what changed (call it freely: only real changes go out).
    pub fn changed(&self) {
        if !self.connect() {
            return;
        }
        let now = (self.state)();
        let was = self.shown.replace(Some(now.clone()));
        let mut controls = self.controls.borrow_mut();
        let Some(controls) = controls.as_mut() else { return };
        if was.as_ref().map(|w| &w.song) != Some(&now.song) {
            if let Some(s) = &now.song {
                let artist = s.artists.join(", ");
                let cover = cover(&s.art);
                let _ = controls.set_metadata(MediaMetadata {
                    title: Some(&s.title),
                    album: (!s.album.is_empty()).then_some(s.album.as_str()),
                    artist: Some(&artist),
                    cover_url: cover.as_deref(),
                    duration: (s.length > 0).then(|| Duration::from_micros(s.length as u64)),
                });
            }
        }
        if was.as_ref().map(|w| (w.playing, w.song.is_some())) != Some((now.playing, now.song.is_some())) {
            let playback = match (&now.song, now.playing) {
                (None, _) => MediaPlayback::Stopped,
                (Some(_), true) => MediaPlayback::Playing { progress: None },
                (Some(_), false) => MediaPlayback::Paused { progress: None },
            };
            let _ = controls.set_playback(playback);
        }
    }
}

/// The cover as souvlaki takes it: a web address, or "file://" and a Windows path
/// (it cuts "file://" off and opens the rest; "file:///C:/..." would leave "/C:/...").
fn cover(art: &str) -> Option<String> {
    if art.is_empty() {
        return None;
    }
    if art.starts_with("http://") || art.starts_with("https://") {
        return Some(art.to_string());
    }
    let path = if art.starts_with("file:") { glib::filename_from_uri(art).ok()?.0 } else { art.into() };
    path.exists().then(|| format!("file://{}", path.display()))
}
