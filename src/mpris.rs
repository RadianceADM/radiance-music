//! The desktop's media controls (MPRIS): GNOME's top bar, the lock screen and
//! the keyboard's media keys show the song playing and play, pause and skip it,
//! with the window closed too. Owns org.mpris.MediaPlayer2.RadianceMusic on
//! the session bus; everything runs on the window's thread.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::gio;
use gtk::glib;
use gtk::glib::prelude::*;

const PATH: &str = "/org/mpris/MediaPlayer2";
const ROOT: &str = "org.mpris.MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";

const XML: &str = r#"<node>
  <interface name="org.mpris.MediaPlayer2">
    <method name="Raise"/>
    <method name="Quit"/>
    <property name="CanQuit" type="b" access="read"/>
    <property name="CanRaise" type="b" access="read"/>
    <property name="HasTrackList" type="b" access="read"/>
    <property name="Identity" type="s" access="read"/>
    <property name="DesktopEntry" type="s" access="read"/>
    <property name="SupportedUriSchemes" type="as" access="read"/>
    <property name="SupportedMimeTypes" type="as" access="read"/>
  </interface>
  <interface name="org.mpris.MediaPlayer2.Player">
    <method name="Next"/>
    <method name="Previous"/>
    <method name="Pause"/>
    <method name="PlayPause"/>
    <method name="Stop"/>
    <method name="Play"/>
    <method name="Seek"><arg direction="in" name="Offset" type="x"/></method>
    <method name="SetPosition"><arg direction="in" name="TrackId" type="o"/><arg direction="in" name="Position" type="x"/></method>
    <method name="OpenUri"><arg direction="in" name="Uri" type="s"/></method>
    <signal name="Seeked"><arg name="Position" type="x"/></signal>
    <property name="PlaybackStatus" type="s" access="read"/>
    <property name="Rate" type="d" access="read"/>
    <property name="Metadata" type="a{sv}" access="read"/>
    <property name="Volume" type="d" access="read"/>
    <property name="Position" type="x" access="read"/>
    <property name="MinimumRate" type="d" access="read"/>
    <property name="MaximumRate" type="d" access="read"/>
    <property name="CanGoNext" type="b" access="read"/>
    <property name="CanGoPrevious" type="b" access="read"/>
    <property name="CanPlay" type="b" access="read"/>
    <property name="CanPause" type="b" access="read"/>
    <property name="CanSeek" type="b" access="read"/>
    <property name="CanControl" type="b" access="read"/>
  </interface>
</node>"#;

/// What the media controls ask for.
pub enum Command {
    PlayPause,
    Play,
    Pause,
    Next,
    Previous,
    /// Show the window again.
    Raise,
    Quit,
}

/// What the media controls show.
#[derive(Clone, Default, PartialEq)]
pub struct State {
    pub playing: bool,
    /// A song to play (else play does nothing).
    pub song: Option<Song>,
    /// Where the song is (µs).
    pub position: i64,
}

#[derive(Clone, Default, PartialEq)]
pub struct Song {
    /// Unique to the song (its key).
    pub key: String,
    pub title: String,
    pub artists: Vec<String>,
    pub album: String,
    /// Its cover: a web address or a file on this computer.
    pub art: String,
    /// µs (0: not known).
    pub length: i64,
}

pub struct Mpris {
    connection: RefCell<Option<gio::DBusConnection>>,
    shown: RefCell<State>,
    state: Box<dyn Fn() -> State>,
}

/// The song's id as MPRIS wants it: a D-Bus object path.
fn track_id(key: &str) -> glib::Variant {
    let hash = key.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3));
    glib::variant::ObjectPath::try_from(format!("/io/github/radianceadm/Music/track/t{hash:016x}"))
        .map(|p| p.to_variant())
        .unwrap_or_else(|_| glib::variant::ObjectPath::try_from("/org/mpris/MediaPlayer2/TrackList/NoTrack".to_string()).unwrap().to_variant())
}

fn metadata(s: &State) -> glib::Variant {
    let d = glib::VariantDict::new(None);
    match &s.song {
        Some(song) => {
            d.insert_value("mpris:trackid", &track_id(&song.key));
            d.insert_value("xesam:title", &song.title.to_variant());
            d.insert_value("xesam:artist", &song.artists.to_variant());
            if !song.album.is_empty() {
                d.insert_value("xesam:album", &song.album.to_variant());
            }
            if song.length > 0 {
                d.insert_value("mpris:length", &song.length.to_variant());
            }
            if !song.art.is_empty() {
                let url = if song.art.starts_with("http") { song.art.clone() } else { glib::filename_to_uri(&song.art, None).map(|u| u.to_string()).unwrap_or_default() };
                d.insert_value("mpris:artUrl", &url.to_variant());
            }
        }
        None => d.insert_value("mpris:trackid", &track_id("")),
    }
    d.end()
}

fn player_property(s: &State, name: &str) -> glib::Variant {
    match name {
        "PlaybackStatus" => (if s.playing { "Playing" } else if s.song.is_some() { "Paused" } else { "Stopped" }).to_variant(),
        "Rate" | "MinimumRate" | "MaximumRate" | "Volume" => 1.0f64.to_variant(),
        "Metadata" => metadata(s),
        "Position" => s.position.to_variant(),
        "CanGoNext" | "CanGoPrevious" | "CanPlay" | "CanPause" => s.song.is_some().to_variant(),
        "CanSeek" => false.to_variant(),
        "CanControl" => true.to_variant(),
        _ => false.to_variant(),
    }
}

fn root_property(name: &str) -> glib::Variant {
    match name {
        "CanQuit" | "CanRaise" => true.to_variant(),
        "HasTrackList" => false.to_variant(),
        "Identity" => "Radiance Music".to_variant(),
        "DesktopEntry" => crate::APP_ID.to_variant(),
        "SupportedUriSchemes" | "SupportedMimeTypes" => Vec::<String>::new().to_variant(),
        _ => false.to_variant(),
    }
}

impl Mpris {
    /// On the session bus; `command` is called for what the controls ask,
    /// `state` for what they show.
    pub fn start(command: impl Fn(Command) + 'static, state: impl Fn() -> State + 'static) -> Rc<Mpris> {
        let me = Rc::new(Mpris { connection: RefCell::new(None), shown: RefCell::new(State::default()), state: Box::new(state) });
        let command = Rc::new(command);
        let weak = Rc::downgrade(&me);
        gio::bus_own_name(
            gio::BusType::Session,
            "org.mpris.MediaPlayer2.RadianceMusic",
            gio::BusNameOwnerFlags::NONE,
            move |connection, _| {
                let Some(me) = weak.upgrade() else { return };
                let Ok(node) = gio::DBusNodeInfo::for_xml(XML) else { return };
                for name in [ROOT, PLAYER] {
                    let Some(info) = node.lookup_interface(name) else { continue };
                    let (cmd, m) = (command.clone(), Rc::downgrade(&me));
                    let m2 = m.clone();
                    let registered = connection
                        .register_object(PATH, &info)
                        .method_call(move |_, _, _, iface, method, _, invocation| {
                            let c = match (iface == Some(ROOT), method) {
                                (true, "Raise") => Some(Command::Raise),
                                (true, "Quit") => Some(Command::Quit),
                                (false, "PlayPause") => Some(Command::PlayPause),
                                (false, "Play") => Some(Command::Play),
                                (false, "Pause") | (false, "Stop") => Some(Command::Pause),
                                (false, "Next") => Some(Command::Next),
                                (false, "Previous") => Some(Command::Previous),
                                _ => None, // Seek, SetPosition, OpenUri: not offered
                            };
                            invocation.return_value(None);
                            if let Some(c) = c {
                                cmd(c);
                            }
                            if let Some(m) = m.upgrade() {
                                m.changed();
                            }
                        })
                        .property(move |_, _, _, iface, prop| match m2.upgrade() {
                            Some(m) if iface == PLAYER => player_property(&(m.state)(), prop),
                            _ => root_property(prop),
                        })
                        .build();
                    if registered.is_err() {
                        return;
                    }
                }
                *me.connection.borrow_mut() = Some(connection);
                me.changed();
            },
            |_, _| {},
            |_, _| {},
        );
        me
    }

    /// Tells the controls what changed (call it freely: only real changes go out).
    pub fn changed(&self) {
        let now = (self.state)();
        let was = self.shown.replace(now.clone());
        let Some(connection) = self.connection.borrow().clone() else { return };
        let d = glib::VariantDict::new(None);
        let mut any = false;
        if was.playing != now.playing || was.song.is_some() != now.song.is_some() {
            d.insert_value("PlaybackStatus", &player_property(&now, "PlaybackStatus"));
            any = true;
        }
        if was.song != now.song {
            d.insert_value("Metadata", &metadata(&now));
            for p in ["CanGoNext", "CanGoPrevious", "CanPlay", "CanPause"] {
                d.insert_value(p, &player_property(&now, p));
            }
            any = true;
        }
        if !any {
            return;
        }
        let args = glib::Variant::tuple_from_iter([PLAYER.to_variant(), d.end(), Vec::<String>::new().to_variant()]);
        let _ = connection.emit_signal(None, PATH, "org.freedesktop.DBus.Properties", "PropertiesChanged", Some(&args));
    }
}
