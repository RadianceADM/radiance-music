//! Radiance Music Servers in the window (servers.rs talks to them): Servers…
//! in the menu (join with an invite link, leave, what you can do on each),
//! each server's library as its own page under Library (play, search, save a
//! song, add one of yours, split on the server), and its listening party,
//! played here in step with everyone by the server's clock.

use super::*;
use crate::servers::{self, Asked, Failure, Joined, Live, Me, Party, Song};

pub(super) const CSS: &str = "
.rms-card { padding: 16px 18px; border-radius: 16px; background: alpha(#ffffff, 0.05); }
.rms-card.on { background: alpha(#F38A35, 0.09); box-shadow: inset 0 0 0 1px alpha(#F38A35, 0.28); }
.rms-problem { padding: 12px 16px; border-radius: 14px; background: alpha(#F38A35, 0.12); }
.rms-chip { padding: 3px 12px 3px 3px; border-radius: 999px; background: alpha(#ffffff, 0.08); font-weight: 600; }
.rms-react { padding: 6px; border-radius: 999px; }
.rms-sync { font-size: 0.85em; }
.rms-now-art { border-radius: 12px; }
";

/// A song whose jumps land early (an Ogg Opus file lands on its page's start,
/// up to a second before the spot) is jumped this far past the spot, then waits.
const PAST_THE_SPOT: f64 = 1.0;
/// The party's place is checked this often (its clock offset once a minute).
const PARTY_CHECK: Duration = Duration::from_secs(1);
/// A server's split queue is asked this often while one of your songs is in it.
const QUEUE_CHECK: Duration = Duration::from_secs(3);
/// The page's server is asked about its party this often (not joined: no WebSocket).
const PARTY_LOOK: Duration = Duration::from_secs(10);

/// A joined server, as the window last heard from it.
#[derive(Default)]
pub(super) struct Known {
    me: Option<Me>,
    /// Its songs (None: not fetched yet).
    songs: Option<Vec<Song>>,
    /// The last call that didn't work (cleared when one does).
    problem: Option<Failure>,
    /// Its party, as last asked (not joined).
    party: Option<Party>,
    fetching: bool,
}

/// One of your songs being split on a server: in the split card until it's done.
pub(super) struct Away {
    server: Joined,
    job: servers::Job,
    track: Track,
    ahead: Option<usize>,
    next_turn: Option<String>,
}

/// Your own music as it was when you joined a party: it comes back when you leave.
struct Before {
    queue: Vec<Track>,
    index: usize,
    current: Option<Track>,
    at: f64,
    playing: bool,
}

/// The party this computer is in.
pub(super) struct InParty {
    server: Joined,
    me: Me,
    party: Party,
    /// The server's clock less this one's (ms), and the quickest ping's round trip.
    offset: f64,
    rtt: f64,
    _socket: servers::Socket,
    /// Heard here (off: paused here while the party plays on).
    listening: bool,
    /// The party's song in the player (its entry).
    entry: Option<u64>,
    /// How far ahead a jump aims (s): the time a jump takes to be heard, learnt from the last one.
    lead: f64,
    /// When the last jump (or wait) was; whether it was a jump.
    jumped: Option<Instant>,
    seeked: bool,
    /// This song's jumps land early: they aim PAST_THE_SPOT further, then wait.
    slack: f64,
    /// Waiting (paused) to let the party catch up: its end.
    hold: Option<glib::SourceId>,
    drift_ms: Option<f64>,
    start: Option<glib::SourceId>,
    lost: bool,
    before: Option<Before>,
    checks: u32,
}

/// The servers' part of the window.
pub(super) struct Rms {
    page: gtk::Box,
    /// The server on the page (its address).
    shown: RefCell<Option<String>>,
    known: RefCell<HashMap<String, Known>>,
    search: gtk::SearchEntry,
    group: adw::PreferencesGroup,
    spinner: gtk::Spinner,
    rows: RefCell<Vec<(Track, adw::ActionRow)>>,
    party_box: gtk::Box,
    sync_label: gtk::Label,
    away: RefCell<Vec<Away>>,
    pub(super) away_line: gtk::Label,
    pub(super) away_bar: gtk::ProgressBar,
    asking: Cell<bool>,
    /// Server songs whose parts are being fetched for the Mixer (their keys).
    fetching_parts: RefCell<HashSet<String>>,
    party: RefCell<Option<InParty>>,
    party_gen: Cell<u64>,
    joining: Cell<bool>,
    watching: Cell<bool>,
}

impl Default for Rms {
    fn default() -> Self {
        let search = gtk::SearchEntry::builder().hexpand(true).build();
        Rms {
            page: gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(20).margin_top(18).margin_bottom(28).margin_start(18).margin_end(18).build(),
            shown: RefCell::default(),
            known: RefCell::default(),
            search,
            group: adw::PreferencesGroup::new(),
            spinner: gtk::Spinner::builder().spinning(true).visible(false).build(),
            rows: RefCell::default(),
            party_box: gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(12).css_classes(["rms-card"]).build(),
            sync_label: gtk::Label::builder().xalign(0.0).wrap(true).css_classes(["dim-label", "rms-sync", "numeric"]).build(),
            away: RefCell::default(),
            away_line: gtk::Label::builder().xalign(0.0).wrap(true).wrap_mode(gtk::pango::WrapMode::WordChar).css_classes(["dim-label", "caption"]).visible(false).build(),
            away_bar: gtk::ProgressBar::builder().show_text(true).visible(false).margin_top(4).css_classes(["split-bar"]).build(),
            asking: Cell::new(false),
            fetching_parts: RefCell::default(),
            party: RefCell::default(),
            party_gen: Cell::new(0),
            joining: Cell::new(false),
            watching: Cell::new(false),
        }
    }
}

/// A button with its icon before its words (every button carries one).
fn icon_button(icon: &str, label: &str) -> gtk::Button {
    gtk::Button::builder().child(&adw::ButtonContent::builder().icon_name(icon).label(label).build()).build()
}

fn clear(b: &gtk::Box) {
    while let Some(c) = b.first_child() {
        b.remove(&c);
    }
}

/// "a", "a and b", "a, b and c".
fn and_list(items: &[&str]) -> String {
    match items.len() {
        0 => String::new(),
        1 => items[0].to_string(),
        n => format!("{} and {}", items[..n - 1].join(", "), items[n - 1]),
    }
}

/// What you can do on a server, in plain words.
fn can_words(me: &Me) -> String {
    let mut can = vec!["play its songs"];
    if me.can_add {
        can.push("add songs");
    }
    if me.can_split {
        can.push("split songs");
    }
    can.push(if me.can_host { "join and host listening parties" } else { "join listening parties" });
    format!("You can {}.", and_list(&can))
}

/// What the owner turned off for you there ("" when nothing is).
fn off_words(me: &Me) -> String {
    let mut off = vec![];
    if !me.can_add {
        off.push("adding songs");
    }
    if !me.can_split {
        off.push("splitting");
    }
    if !me.can_host {
        off.push("hosting parties");
    }
    if off.is_empty() || me.owner {
        String::new()
    } else {
        format!("Its owner turned off {} for you.", and_list(&off))
    }
}

impl Ui {
    // ------------------------------------------------------------------ joined servers

    fn joined(&self, address: &str) -> Option<Joined> {
        self.settings.borrow().servers.iter().find(|s| s.address == address).cloned()
    }

    /// What the window last heard from a server, read.
    fn known<T>(&self, address: &str, f: impl FnOnce(&Known) -> T) -> T {
        let mut k = self.rms.known.borrow_mut();
        f(k.entry(address.to_string()).or_default())
    }

    fn know(&self, address: &str, f: impl FnOnce(&mut Known)) {
        f(self.rms.known.borrow_mut().entry(address.to_string()).or_default());
    }

    /// At the start: the joined servers to the worker threads, their pages in
    /// the sidebar, and what each lets you do (asked quietly).
    pub(super) fn start_servers(self: &Rc<Self>) {
        let list = self.settings.borrow().servers.clone();
        servers::set_joined(&list);
        self.render_server_nav();
        for s in list {
            self.fetch_server(&s);
        }
    }

    /// The sidebar: each joined server under Library (after Genres), named as it's called.
    pub(super) fn render_server_nav(self: &Rc<Self>) {
        let mut child = self.nav.first_child();
        let mut after = None;
        while let Some(c) = child {
            child = c.next_sibling();
            let name = c.widget_name();
            if name.starts_with("server:") {
                self.nav.remove(&c);
            } else if name == "genres" {
                after = c.downcast_ref::<gtk::ListBoxRow>().map(|r| r.index());
            }
        }
        let Some(at) = after else { return };
        for (i, s) in self.settings.borrow().servers.iter().enumerate() {
            let line = gtk::Box::builder().spacing(12).build();
            line.append(&gtk::Image::from_icon_name("network-server-symbolic"));
            line.append(&gtk::Label::builder().label(&s.name).xalign(0.0).ellipsize(gtk::pango::EllipsizeMode::End).build());
            let row = gtk::ListBoxRow::builder().child(&line).name(format!("server:{}", s.address)).tooltip_text(format!("{}: its songs, its splitting and its listening party ({})", s.name, s.address)).build();
            self.nav.insert(&row, at + 1 + i as i32);
        }
        self.sync_nav();
    }

    /// Asks a server who this computer is there, its songs and its party (worker thread).
    fn fetch_server(self: &Rc<Self>, server: &Joined) {
        if self.known(&server.address, |k| k.fetching) {
            return;
        }
        self.know(&server.address, |k| k.fetching = true);
        if self.rms.shown.borrow().as_deref() == Some(server.address.as_str()) {
            self.rms.spinner.set_visible(true);
        }
        let (me, s) = (Rc::downgrade(self), server.clone());
        bg(
            move || {
                let who = servers::me(&s);
                let songs = who.as_ref().ok().map(|_| servers::library(&s));
                let party = who.as_ref().ok().map(|_| servers::party(&s));
                (who, songs, party)
            },
            {
                let address = server.address.clone();
                move |(who, songs, party)| {
                    let Some(me) = me.upgrade() else { return };
                    me.know(&address, |k| {
                        k.fetching = false;
                        k.problem = None;
                        match who {
                            Ok(m) => k.me = Some(m),
                            Err(f) => k.problem = Some(f),
                        }
                        match songs {
                            Some(Ok(list)) => k.songs = Some(list),
                            Some(Err(f)) => k.problem = Some(f),
                            None => {}
                        }
                        if let Some(Ok(p)) = party {
                            k.party = Some(p);
                        }
                    });
                    if me.rms.shown.borrow().as_deref() == Some(address.as_str()) {
                        me.render_server();
                    }
                    me.update_voice_status(); // a server's song playing: its parts, its split button
                }
            },
        );
    }

    // ------------------------------------------------------------------ Servers… (the menu)

    pub(super) fn servers_dialog(self: &Rc<Self>) {
        let dialog = adw::Dialog::builder().title("Servers").content_width(480).build();
        let view = adw::ToolbarView::new();
        view.add_top_bar(&adw::HeaderBar::new());
        let body = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(18).margin_top(6).margin_bottom(24).margin_start(18).margin_end(18).build();
        let scroll = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).propagate_natural_height(true).child(&body).build();
        view.set_content(Some(&scroll));
        dialog.set_child(Some(&view));
        body.append(
            &gtk::Label::builder()
                .label("A Radiance Music Server is your own, or a friend's: its songs play here, it splits songs for everyone on it, and it holds listening parties. Its owner sends you an invite link.")
                .wrap(true)
                .xalign(0.0)
                .css_classes(["dim-label"])
                .build(),
        );

        // join one
        let join_group = adw::PreferencesGroup::builder().title("Join a Server").description("Paste the invite link its owner sent you. Each link works once, on one device.").build();
        let entry = adw::EntryRow::builder().title("Invite link (https://…/join/…)").build();
        let join = icon_button("list-add-symbolic", "Join");
        join.set_valign(gtk::Align::Center);
        join.add_css_class("suggested-action");
        entry.add_suffix(&join);
        join_group.add(&entry);
        let note = gtk::Label::builder().xalign(0.0).wrap(true).wrap_mode(gtk::pango::WrapMode::WordChar).margin_top(10).visible(false).build();
        join_group.add(&note);
        body.append(&join_group);

        // yours
        let list_group = adw::PreferencesGroup::builder().title("Your Servers").build();
        body.append(&list_group);
        let rows: Rc<RefCell<Vec<gtk::Widget>>> = Rc::default();
        // the list, made again after a join or a leave (kept while the dialog is open)
        let refill: Rc<RefCell<Option<Rc<dyn Fn()>>>> = Rc::default();
        let fill: Rc<dyn Fn()> = {
            let (me, group, rows, dialog_w, again) = (Rc::downgrade(self), list_group.downgrade(), rows.clone(), dialog.downgrade(), Rc::downgrade(&refill));
            Rc::new(move || {
                let (Some(me), Some(group)) = (me.upgrade(), group.upgrade()) else { return };
                for r in rows.borrow_mut().drain(..) {
                    group.remove(&r);
                }
                let list = me.settings.borrow().servers.clone();
                if list.is_empty() {
                    let r = adw::ActionRow::builder().title("No servers yet").subtitle("Paste an invite link above to join one.").build();
                    r.add_prefix(&gtk::Image::from_icon_name("network-server-symbolic"));
                    group.add(&r);
                    rows.borrow_mut().push(r.upcast());
                }
                for s in list {
                    let row = me.server_row(&s, dialog_w.clone(), again.clone());
                    group.add(&row);
                    rows.borrow_mut().push(row.upcast());
                }
            })
        };
        *refill.borrow_mut() = Some(fill.clone());
        dialog.connect_closed(move |_| {
            refill.borrow_mut().take();
        });
        fill();
        // each server asked again: what you can do there, whether it answers
        for s in self.settings.borrow().servers.clone() {
            let (me, f, address) = (Rc::downgrade(self), fill.clone(), s.address.clone());
            bg(move || servers::me(&s), move |who| {
                let Some(me) = me.upgrade() else { return };
                me.know(&address, |k| match who {
                    Ok(m) => {
                        k.me = Some(m);
                        k.problem = None;
                    }
                    Err(e) => k.problem = Some(e),
                });
                f();
            });
        }

        let go = {
            let (me, entry, note, join, f) = (Rc::downgrade(self), entry.downgrade(), note.downgrade(), join.downgrade(), fill.clone());
            move || {
                let (Some(me), Some(entry), Some(note), Some(join)) = (me.upgrade(), entry.upgrade(), note.upgrade(), join.upgrade()) else { return };
                note.set_visible(true);
                note.remove_css_class("error");
                let (address, code) = match servers::parse_invite(&entry.text()) {
                    Ok(x) => x,
                    Err(e) => {
                        note.add_css_class("error");
                        note.set_text(&e);
                        return;
                    }
                };
                if let Some(s) = me.joined(&address) {
                    note.set_text(&format!("This computer is on {} already. Its songs are under Library in the sidebar.", s.name));
                    return;
                }
                join.set_sensitive(false);
                note.set_text("Joining…");
                let device = glib::host_name().to_string();
                let (mw, ew, nw, jw, f) = (Rc::downgrade(&me), entry.downgrade(), note.downgrade(), join.downgrade(), f.clone());
                bg(move || servers::join(&address, &code, &device), move |done| {
                    let Some(me) = mw.upgrade() else { return };
                    if let Some(j) = jw.upgrade() {
                        j.set_sensitive(true);
                    }
                    let note = nw.upgrade();
                    match done {
                        Ok(joined) => {
                            crate::log::note(&format!("joined {} ({})", joined.name, joined.address));
                            if let Some(e) = ew.upgrade() {
                                e.set_text("");
                            }
                            if let Some(n) = &note {
                                n.set_text(&format!("Joined {}. Its songs are under Library in the sidebar.", joined.name));
                            }
                            me.settings.borrow_mut().servers.push(joined.clone());
                            settings::save(&me.settings.borrow());
                            servers::set_joined(&me.settings.borrow().servers);
                            me.render_server_nav();
                            me.fetch_server(&joined);
                            f();
                            me.toast(&format!("Joined {}", joined.name));
                        }
                        Err(e) => {
                            if let Some(n) = &note {
                                n.add_css_class("error");
                                n.set_text(&e);
                            }
                        }
                    }
                });
            }
        };
        let go = Rc::new(go);
        let g = go.clone();
        join.connect_clicked(move |_| g());
        entry.connect_entry_activated(move |_| go());
        dialog.present(Some(&self.window));
    }

    /// A joined server in Servers…: its name and state; opened, what you can do there; Open and Leave.
    fn server_row(self: &Rc<Self>, s: &Joined, dialog: glib::WeakRef<adw::Dialog>, refill: std::rc::Weak<RefCell<Option<Rc<dyn Fn()>>>>) -> adw::ExpanderRow {
        let (who, problem) = self.known(&s.address, |k| (k.me.clone(), k.problem.clone()));
        let state = match (&who, &problem) {
            (_, Some(f)) => f.words(&s.name),
            (Some(m), None) => format!("{} · you're {}", s.address, if m.owner { "its owner".to_string() } else { m.name.clone() }),
            (None, None) => format!("{} · checking…", s.address),
        };
        // (one server: open, what you can do there in sight)
        let one = self.settings.borrow().servers.len() == 1;
        let row = adw::ExpanderRow::builder().title(&s.name).subtitle(&state).subtitle_lines(3).use_markup(false).expanded(one).build();
        row.add_prefix(&gtk::Image::from_icon_name(if problem.as_ref().is_some_and(|f| f.unreachable()) { "network-offline-symbolic" } else { "network-server-symbolic" }));
        let open = icon_button("go-next-symbolic", "Open Its Page");
        open.set_tooltip_text(Some("Its songs and its listening party"));
        let leave = icon_button("system-log-out-symbolic", "Leave");
        leave.set_tooltip_text(Some("This computer forgets its key to this server"));
        // what you can do there
        if let Some(m) = &who {
            for (yes, what) in [(true, "Play its songs and join its parties"), (m.can_add, "Add songs to it"), (m.can_split, "Split songs on it"), (m.can_host, "Host a listening party")] {
                let r = adw::ActionRow::builder().title(what).subtitle(if yes { "Yes" } else { "Off for you: its owner's choice" }).build();
                r.add_prefix(&gtk::Image::from_icon_name(if yes { "object-select-symbolic" } else { "action-unavailable-symbolic" }));
                row.add_row(&r);
            }
        } else if problem.is_none() {
            row.add_row(&adw::ActionRow::builder().title("Asking the server what you can do there…").build());
        }
        let buttons = adw::WrapBox::builder().child_spacing(8).line_spacing(8).margin_top(10).margin_bottom(10).margin_start(12).margin_end(12).build();
        buttons.append(&open);
        buttons.append(&leave);
        let line = gtk::ListBoxRow::builder().activatable(false).selectable(false).child(&buttons).build();
        row.add_row(&line);
        let (me, address, d) = (Rc::downgrade(self), s.address.clone(), dialog.clone());
        open.connect_clicked(move |_| {
            if let Some(me) = me.upgrade() {
                me.open_server(&address);
            }
            if let Some(d) = d.upgrade() {
                d.close();
            }
        });
        let (me, server) = (Rc::downgrade(self), s.clone());
        leave.connect_clicked(move |_| {
            let Some(me) = me.upgrade() else { return };
            let ask = adw::AlertDialog::new(
                Some(&format!("Leave {}?", server.name)),
                Some(&format!("This computer forgets its key to {0}: its songs, splitting and parties go from here. Songs you saved from it stay in your library. To come back, ask its owner for a new invite.", server.name)),
            );
            ask.add_response("cancel", "Cancel");
            ask.add_response("leave", "Leave");
            ask.set_response_appearance("leave", adw::ResponseAppearance::Destructive);
            ask.set_close_response("cancel");
            let (w, s, again) = (Rc::downgrade(&me), server.clone(), refill.clone());
            ask.connect_response(None, move |_, response| {
                if response != "leave" {
                    return;
                }
                if let Some(me) = w.upgrade() {
                    me.leave_server(&s);
                }
                if let Some(f) = again.upgrade().and_then(|r| r.borrow().clone()) {
                    f();
                }
            });
            ask.present(Some(&me.window));
        });
        row
    }

    /// Leave: the server stops taking this computer's key (as the phone's Leave does), then
    /// this computer forgets it (and the server's songs leave the window). Not answering, or
    /// the owner's last device: it's forgotten here anyway, and the toast says so.
    fn leave_server(self: &Rc<Self>, s: &Joined) {
        let (me, server) = (Rc::downgrade(self), s.clone());
        bg(
            {
                let server = server.clone();
                move || servers::leave(&server)
            },
            move |told| {
                if let Some(me) = me.upgrade() {
                    me.forget_server(&server, told.is_ok());
                }
            },
        );
    }

    fn forget_server(self: &Rc<Self>, s: &Joined, told: bool) {
        if self.rms.party.borrow().as_ref().is_some_and(|p| p.server.address == s.address) {
            self.leave_party(true);
        }
        self.settings.borrow_mut().servers.retain(|x| x.address != s.address);
        settings::save(&self.settings.borrow());
        servers::set_joined(&self.settings.borrow().servers);
        self.rms.known.borrow_mut().remove(&s.address);
        self.rms.away.borrow_mut().retain(|a| a.server.address != s.address);
        crate::log::note(&format!("left {} ({})", s.name, s.address));
        if self.rms.shown.borrow().as_deref() == Some(s.address.as_str()) {
            self.rms.shown.replace(None);
            if self.stack.visible_child_name().as_deref() == Some("server") {
                self.stack.set_visible_child_name("liked");
            }
        }
        self.render_server_nav();
        self.update_voice_status();
        self.toast(&if told { format!("Left {}", s.name) } else { format!("Left {} on this computer (the server didn't take it: its owner can remove this computer under People)", s.name) });
    }

    // ------------------------------------------------------------------ a server's page

    /// The page a server's songs and party are on (the server shown changes).
    pub(super) fn server_page(self: &Rc<Self>) -> gtk::ScrolledWindow {
        let me = Rc::downgrade(self);
        self.rms.search.connect_search_changed(move |_| {
            if let Some(me) = me.upgrade() {
                me.filter_server();
            }
        });
        self.rms.group.set_header_suffix(Some(&self.rms.spinner));
        let clamp = adw::Clamp::builder().maximum_size(1240).tightening_threshold(900).child(&self.rms.page).build();
        gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&clamp).build()
    }

    /// Shows a server's page (and asks it again for its songs and its party).
    pub(super) fn open_server(self: &Rc<Self>, address: &str) {
        let Some(server) = self.joined(address) else { return };
        let was = self.rms.shown.replace(Some(address.to_string()));
        if was.as_deref() != Some(address) {
            self.rms.search.set_text("");
        }
        self.render_server();
        if self.stack.visible_child_name().as_deref() != Some("server") {
            self.stack.set_visible_child_name("server");
        }
        self.sync_nav();
        self.fetch_server(&server);
        self.watch_party();
    }

    /// The shown server's page, made again: its name, what you can do, its
    /// party, its songs (and in plain words why not, when it isn't answering).
    pub(super) fn render_server(self: &Rc<Self>) {
        crate::watch::doing("showing a server's page");
        let Some(address) = self.rms.shown.borrow().clone() else { return };
        let Some(server) = self.joined(&address) else { return };
        let r = &self.rms;
        clear(&r.page);
        let (who, songs, problem, fetching) = self.known(&address, |k| (k.me.clone(), k.songs.clone(), k.problem.clone(), k.fetching));
        r.spinner.set_visible(fetching);
        let head = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(6).build();
        head.append(&gtk::Label::builder().label(&server.name).xalign(0.0).wrap(true).wrap_mode(gtk::pango::WrapMode::WordChar).css_classes(["page-title"]).build());
        let mut sub = vec![];
        if let Some(list) = &songs {
            sub.push(format!("{} song{}", list.len(), if list.len() == 1 { "" } else { "s" }));
        }
        if let Some(m) = &who {
            sub.push(if m.owner { "Your own server".to_string() } else { format!("You're on it as {}", m.name) });
        }
        head.append(&gtk::Label::builder().label(sub.join(" · ")).xalign(0.0).wrap(true).css_classes(["dim-label"]).visible(!sub.is_empty()).build());
        if let Some(m) = &who {
            let line = [can_words(m), off_words(m)].iter().filter(|x| !x.is_empty()).cloned().collect::<Vec<_>>().join(" ");
            head.append(&gtk::Label::builder().label(line).xalign(0.0).wrap(true).css_classes(["dim-label", "caption"]).build());
        }
        r.page.append(&head);

        // why it can't show its songs, and a way on
        if let Some(f) = &problem {
            let card = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(10).css_classes(["rms-problem"]).build();
            let line = gtk::Box::builder().spacing(12).build();
            line.append(&gtk::Image::builder().icon_name(if f.unreachable() { "network-offline-symbolic" } else { "dialog-information-symbolic" }).pixel_size(24).valign(gtk::Align::Start).build());
            line.append(&gtk::Label::builder().label(f.words(&server.name)).xalign(0.0).wrap(true).hexpand(true).build());
            card.append(&line);
            let buttons = adw::WrapBox::builder().child_spacing(8).line_spacing(8).build();
            let again = icon_button("view-refresh-symbolic", "Try Again");
            let (me, s) = (Rc::downgrade(self), server.clone());
            again.connect_clicked(move |_| {
                if let Some(me) = me.upgrade() {
                    me.fetch_server(&s);
                }
            });
            buttons.append(&again);
            if f.signed_out() {
                let open = icon_button("network-server-symbolic", "Servers…");
                let me = Rc::downgrade(self);
                open.connect_clicked(move |_| {
                    if let Some(me) = me.upgrade() {
                        me.servers_dialog();
                    }
                });
                buttons.append(&open);
            }
            card.append(&buttons);
            r.page.append(&card);
        }

        // play them all, or shuffled; ask again
        let tracks: Rc<Vec<Track>> = Rc::new(songs.as_ref().map(|l| l.iter().map(|s| s.track(&server)).collect()).unwrap_or_default());
        let buttons = adw::WrapBox::builder().child_spacing(10).line_spacing(10).build();
        let play = icon_button("media-playback-start-symbolic", "Play");
        play.add_css_class("suggested-action");
        play.add_css_class("pill-button");
        let shuffle = icon_button("media-playlist-shuffle-symbolic", "Shuffle");
        shuffle.add_css_class("pill-button");
        let refresh = icon_button("view-refresh-symbolic", "Refresh");
        refresh.add_css_class("pill-button");
        refresh.set_tooltip_text(Some("Ask the server again for its songs and its party"));
        play.set_sensitive(!tracks.is_empty());
        shuffle.set_sensitive(!tracks.is_empty());
        let (me, l) = (Rc::downgrade(self), tracks.clone());
        play.connect_clicked(move |_| {
            if let Some(me) = me.upgrade() {
                me.play_list((*l).clone(), 0);
            }
        });
        let (me, l) = (Rc::downgrade(self), tracks.clone());
        shuffle.connect_clicked(move |_| {
            if let Some(me) = me.upgrade() {
                me.play_list(shuffled((*l).clone()), 0);
            }
        });
        let (me, s) = (Rc::downgrade(self), server.clone());
        refresh.connect_clicked(move |_| {
            if let Some(me) = me.upgrade() {
                me.fetch_server(&s);
            }
        });
        buttons.append(&play);
        buttons.append(&shuffle);
        buttons.append(&refresh);
        r.page.append(&buttons);

        r.page.append(&r.party_box);
        self.render_party_card();

        r.search.set_placeholder_text(Some(&format!("Search {}", server.name)));
        r.page.append(&r.search);
        // the songs (in a party: a click adds one to it)
        for (_, row) in r.rows.borrow_mut().drain(..) {
            r.group.remove(&row);
        }
        let in_party = self.in_party_on(&address);
        let mut rows = vec![];
        for (i, t) in tracks.iter().enumerate() {
            let row = self.track_row(t, Place::Server(tracks.clone(), i));
            if let Some(by) = songs.as_ref().and_then(|l| l.get(i)).and_then(|s| s.added_by.clone()) {
                let line = t.byline();
                row.set_subtitle(&if line.is_empty() { format!("Added by {by}") } else { format!("{line} · added by {by}") });
            }
            if in_party {
                let add = gtk::Button::builder().icon_name("list-add-symbolic").tooltip_text("Add to the party's queue").valign(gtk::Align::Center).css_classes(["flat", "circular"]).build();
                let (me, t2) = (Rc::downgrade(self), t.clone());
                add.connect_clicked(move |_| {
                    if let Some(me) = me.upgrade() {
                        me.add_to_party(&t2);
                    }
                });
                row.add_suffix(&add);
            }
            r.group.add(&row);
            rows.push((t.clone(), row));
        }
        r.rows.replace(rows);
        r.page.append(&r.group);
        self.filter_server();
    }

    /// The shown server's songs that match its search; the list says what's what.
    fn filter_server(&self) {
        let Some(address) = self.rms.shown.borrow().clone() else { return };
        let name = self.joined(&address).map(|s| s.name).unwrap_or_default();
        let q = self.rms.search.text().trim().to_string();
        let mut shown = 0;
        for (t, row) in self.rms.rows.borrow().iter() {
            let on = q.is_empty() || matches(t, &q);
            row.set_visible(on);
            shown += on as usize;
        }
        let (songs, problem, fetching, who) = self.known(&address, |k| (k.songs.as_ref().map(|s| s.len()), k.problem.is_some(), k.fetching, k.me.clone()));
        let text = match songs {
            None if fetching => format!("Getting the songs on {name}…"),
            None => String::new(), // (the card above says why)
            Some(0) if who.as_ref().is_some_and(|m| m.can_add) => format!("No songs on {name} yet. Add one of yours: right-click a liked song, then “Add to {name}”."),
            Some(0) => format!("No songs on {name} yet."),
            Some(_) if shown == 0 => format!("No songs on {name} match “{q}”."),
            Some(n) if self.in_party_on(&address) => format!("{n} songs. Click one to add it to the party."),
            Some(n) if !q.is_empty() => format!("{shown} of {n} songs match."),
            Some(_) if problem => format!("As {name} last sent them."),
            Some(_) => "Click one to play it. Like it (the heart) to save it, with its parts, lyrics and cover, into your own library.".to_string(),
        };
        self.rms.group.set_description(if text.is_empty() { None } else { Some(&text) });
        self.rms.search.set_visible(songs.is_some_and(|n| n > 0));
    }

    /// Screenshots: the first joined server's party joined, once its page has come.
    pub fn shot_party(self: &Rc<Self>) {
        let me = Rc::downgrade(self);
        glib::timeout_add_local_once(Duration::from_millis(1500), move || {
            let Some(me) = me.upgrade() else { return };
            let first = me.settings.borrow().servers.first().cloned();
            if let Some(s) = first {
                // (RADIANCE_SHOT_PAGE=playing: Now Playing stays)
                if std::env::var("RADIANCE_SHOT_PAGE").is_err() {
                    me.open_server(&s.address);
                }
                me.join_party(&s);
            }
        });
    }

    /// Screenshots: the first server asked to split its song with `q` in its title, once its songs have come.
    pub fn shot_split_on(self: &Rc<Self>, q: &str) {
        let (me, q) = (Rc::downgrade(self), q.to_lowercase());
        glib::timeout_add_local_once(Duration::from_millis(2500), move || {
            let Some(me) = me.upgrade() else { return };
            let Some(s) = me.settings.borrow().servers.first().cloned() else { return };
            let found = me.known(&s.address, |k| k.songs.clone()).unwrap_or_default().into_iter().find(|x| x.title.to_lowercase().contains(&q));
            if let Some(song) = found {
                me.split_on_server(&song.track(&s));
            }
        });
    }

    /// Screenshots: the first joined server's page.
    pub(super) fn show_first_server(self: &Rc<Self>) {
        let first = self.settings.borrow().servers.first().map(|s| s.address.clone());
        if let Some(a) = first {
            self.open_server(&a);
        }
    }

    // ------------------------------------------------------------------ a song from a server

    /// A server's song (not saved) with its parts there: are they here for the
    /// Mixer yet? Not yet: they're fetched into the cache (servers::fetch_parts
    /// says why not straight from the server), and the Mixer's sliders apply
    /// once they've come; meanwhile the song plays on as it is.
    pub(super) fn server_parts_ready(self: &Rc<Self>, t: &Track) -> bool {
        if t.source != Source::Server || t.file.is_some() || !split::mix_parts(t).is_empty() || servers::mix_parts(t).is_empty() {
            return true;
        }
        if !self.rms.fetching_parts.borrow_mut().insert(t.key()) {
            return false; // on their way
        }
        let (me, song) = (Rc::downgrade(self), t.clone());
        bg(move || servers::fetch_parts(&song), {
            let (key, title) = (t.key(), t.title.clone());
            move |got| {
                let Some(me) = me.upgrade() else { return };
                me.rms.fetching_parts.borrow_mut().remove(&key);
                match got {
                    Ok(n) => crate::log::note(&format!("“{title}”: its {n} parts from its server, for the Mixer")),
                    Err(e) => {
                        me.toast(&format!("“{title}”'s parts didn't come from its server: {e}"));
                        return;
                    }
                }
                if me.current.borrow().as_ref().is_some_and(|c| c.key() == key) {
                    me.update_voice_status();
                    me.apply_mix(); // the sliders as they are, now with the parts here
                }
            }
        });
        false
    }

    /// A server's song can be split there: on a joined server that lets you split, not split yet.
    pub(super) fn splits_on_server(&self, t: &Track) -> Option<Joined> {
        if t.source != Source::Server || t.file.is_some() || t.video {
            return None;
        }
        let s = servers::of_track(t)?;
        let allowed = self.known(&s.address, |k| k.me.as_ref().is_none_or(|m| m.can_split));
        allowed.then_some(s)
    }

    /// Being split on its server now (or waiting there).
    pub(super) fn away_on_server(&self, t: &Track) -> bool {
        self.rms.away.borrow().iter().any(|a| a.track.key() == t.key())
    }

    /// Asks the song's server to split it (POST /api/split): its parts are then there for everyone.
    pub(super) fn split_on_server(self: &Rc<Self>, t: &Track) {
        let Some(server) = servers::of_track(t) else { return };
        if self.away_on_server(t) {
            self.side_tabs.set_active_name(Some("splits"));
            self.queue_button.set_active(true);
            return;
        }
        let (me, s, asked_for) = (Rc::downgrade(self), server.clone(), t.clone());
        let track = t.clone();
        bg(move || servers::split(&s, &asked_for), move |asked: Result<Asked, Failure>| {
            let Some(me) = me.upgrade() else { return };
            match asked {
                Ok(Asked::Split) => {
                    me.toast(&format!("“{}” is split on {} already: its parts are there for everyone", track.title, server.name));
                    me.fetch_server(&server);
                }
                Ok(Asked::Queued(job)) => {
                    crate::log::note(&format!("asked {} to split “{}”", server.name, track.title));
                    me.toast(&format!("{} splits “{}”: {}", server.name, track.title, job.note));
                    me.rms.away.borrow_mut().push(Away { server, job, track, ahead: None, next_turn: None });
                    me.follow_away();
                    me.update_voice_status();
                    me.refresh_split_marks();
                }
                Err(f) => me.toast(&f.words(&server.name)),
            }
        });
    }

    /// Follows your songs in the servers' split queues (every few seconds while there are any).
    fn follow_away(self: &Rc<Self>) {
        if self.rms.asking.replace(true) {
            return;
        }
        let me = Rc::downgrade(self);
        glib::timeout_add_local(QUEUE_CHECK, move || {
            let Some(me) = me.upgrade() else { return glib::ControlFlow::Break };
            let mut asked: Vec<Joined> = vec![];
            for a in me.rms.away.borrow().iter() {
                if !asked.iter().any(|s| s.address == a.server.address) {
                    asked.push(a.server.clone());
                }
            }
            if asked.is_empty() {
                me.rms.asking.set(false);
                return glib::ControlFlow::Break;
            }
            for s in asked {
                let w = Rc::downgrade(&me);
                let s2 = s.clone();
                bg(move || servers::queue(&s2), move |q| {
                    if let (Some(me), Ok(q)) = (w.upgrade(), q) {
                        me.queue_news(&s, &q);
                    }
                });
            }
            glib::ControlFlow::Continue
        });
    }

    /// A server's split queue, as it is now: where your songs are, and those that are done.
    fn queue_news(self: &Rc<Self>, server: &Joined, q: &servers::Queue) {
        let mut done = vec![];
        self.rms.away.borrow_mut().retain_mut(|a| {
            if a.server.address != server.address {
                return true;
            }
            match q.job(a.job.id) {
                Some(j) if !j.finished() => {
                    a.job = j.clone();
                    a.ahead = q.ahead(j.id);
                    a.next_turn = q.next_turn.clone();
                    true
                }
                found => {
                    done.push((a.track.clone(), found.cloned()));
                    false
                }
            }
        });
        for (t, job) in done {
            match job {
                Some(j) if j.state == "done" => {
                    // its words may be timed now: the server's lyrics again, not a copy kept here
                    let _ = fs::remove_file(lyrics_path(&t));
                    self.toast(&format!("“{}” is split on {}: its parts are there for everyone", t.title, server.name));
                    crate::log::note(&format!("{} split “{}”", server.name, t.title));
                    if self.current.borrow().as_ref().is_some_and(|c| c.key() == t.key()) {
                        self.refresh_lyrics();
                    }
                }
                Some(j) if j.state == "failed" => self.toast(&format!("{}: {}", server.name, j.note)),
                _ => self.toast(&format!("“{}” was taken out of {}'s split queue", t.title, server.name)),
            }
            self.fetch_server(server);
        }
        self.update_voice_status();
        self.refresh_split_marks();
    }

    /// The split card's line for your songs on servers (false: there are none).
    pub(super) fn show_away(&self) -> bool {
        let away = self.rms.away.borrow();
        let r = &self.rms;
        let Some(a) = away.first() else {
            r.away_line.set_visible(false);
            r.away_bar.set_visible(false);
            return false;
        };
        let more = if away.len() > 1 { format!(" ({} more on servers)", away.len() - 1) } else { String::new() };
        let turn = a.next_turn.as_ref().map(|n| format!(" Next turn: {n}.")).unwrap_or_default();
        let text = match a.job.state.as_str() {
            "splitting" => format!("Splitting “{}” on {}{more}", a.track.title, a.server.name),
            "timing" => format!("Timing “{}”'s words on {}{more}", a.track.title, a.server.name),
            _ => format!("“{}” on {}: {}{turn}{more}", a.track.title, a.server.name, a.job.note),
        };
        r.away_line.set_label(&text);
        r.away_line.set_visible(true);
        let working = matches!(a.job.state.as_str(), "splitting" | "timing");
        r.away_bar.set_visible(working);
        if working {
            r.away_bar.set_fraction(a.job.progress.clamp(0.0, 1.0));
            r.away_bar.set_text(Some(&format!("{} · {:.0}%", if a.job.state == "timing" { "Timing the words" } else { "Splitting" }, a.job.progress * 100.0)));
        }
        true
    }

    /// The Splits tab's rows for your songs on servers: (song, where it is).
    pub(super) fn away_rows(&self) -> Vec<(Track, String)> {
        self.rms
            .away
            .borrow()
            .iter()
            .map(|a| {
                let where_ = match (a.job.state.as_str(), a.ahead) {
                    ("splitting" | "timing", _) => format!("On {} · {}", a.server.name, a.job.note),
                    (_, Some(n)) => format!("On {} · number {} in line{}", a.server.name, n + 1, a.next_turn.as_ref().map(|t| format!(" · next turn: {t}")).unwrap_or_default()),
                    _ => format!("On {} · {}", a.server.name, a.job.note),
                };
                (a.track.clone(), where_)
            })
            .collect()
    }

    /// Adds one of your saved songs to a server (POST /api/songs).
    pub(super) fn add_to_server(self: &Rc<Self>, t: &Track, server: &Joined) {
        self.toast(&format!("Adding “{}” to {}…", t.title, server.name));
        let (me, s, song) = (Rc::downgrade(self), server.clone(), t.clone());
        bg(move || servers::upload(&s, &song), {
            let (server, title) = (server.clone(), t.title.clone());
            move |added: Result<servers::Added, String>| {
                let Some(me) = me.upgrade() else { return };
                match added {
                    Ok(a) if a.skipped && a.extras.is_empty() => me.toast(&format!("{} has “{title}” already", server.name)),
                    Ok(a) if a.skipped => me.toast(&format!("{} had “{title}” already; its {} went with it", server.name, and_list(&a.extras.iter().map(String::as_str).collect::<Vec<_>>()))),
                    Ok(_) => {
                        crate::log::note(&format!("added “{title}” to {}", server.name));
                        me.toast(&format!("“{title}” is on {} now, for everyone there", server.name));
                    }
                    Err(e) => me.toast(&e),
                }
                if me.rms.known.borrow().contains_key(&server.address) {
                    me.fetch_server(&server);
                }
            }
        });
    }

    /// The servers a saved song of yours can go to (from its menu): joined ones that let you add songs.
    pub(super) fn servers_for(&self, t: &Track) -> Vec<Joined> {
        if t.file.as_deref().is_none_or(|f| !Path::new(f).exists()) {
            return vec![];
        }
        let list = self.settings.borrow().servers.clone();
        list.into_iter().filter(|s| !t.id.starts_with(&format!("{}/", s.address)) && self.known(&s.address, |k| k.me.as_ref().is_none_or(|m| m.can_add))).collect()
    }

    /// Why a server's song won't play, in plain words (None: not a server's song).
    pub(super) fn server_reason(&self, why: &str) -> Option<(String, bool)> {
        let t = self.current.borrow().clone().filter(|t| t.source == Source::Server && t.file.is_none())?;
        let Some(s) = servers::of_track(&t) else { return Some(("you left its server".into(), false)) };
        let e = why.to_lowercase();
        let any = |words: &[&str]| words.iter().any(|w| e.contains(w));
        Some(if any(&["unauthorized", "401", "forbidden"]) {
            (format!("this computer isn't joined to {} anymore", s.name), true)
        } else if any(&["not found", "404"]) {
            (format!("it isn't on {} anymore", s.name), false)
        } else if any(&["resolve", "could not connect", "connection refused", "unreachable", "timed out", "timeout", "no route", "failed to connect"]) {
            (format!("{} isn't answering", s.name), true)
        } else {
            return None;
        })
    }

    // ------------------------------------------------------------------ the listening party

    /// The server whose page is shown (its address).
    pub(super) fn shown_server(&self) -> Option<String> {
        self.rms.shown.borrow().clone()
    }

    pub(super) fn in_party(&self) -> bool {
        self.rms.party.borrow().is_some()
    }

    pub(super) fn in_party_on(&self, address: &str) -> bool {
        self.rms.party.borrow().as_ref().is_some_and(|p| p.server.address == address)
    }

    /// The shown server's party, asked about now and then (no WebSocket until you join).
    fn watch_party(self: &Rc<Self>) {
        if self.rms.watching.replace(true) {
            return;
        }
        let me = Rc::downgrade(self);
        glib::timeout_add_local(PARTY_LOOK, move || {
            let Some(me) = me.upgrade() else { return glib::ControlFlow::Break };
            let on_page = me.stack.visible_child_name().as_deref() == Some("server");
            let Some(address) = me.rms.shown.borrow().clone().filter(|_| on_page) else {
                me.rms.watching.set(false);
                return glib::ControlFlow::Break;
            };
            if me.in_party_on(&address) {
                return glib::ControlFlow::Continue; // its WebSocket says
            }
            if let Some(s) = me.joined(&address) {
                let w = Rc::downgrade(&me);
                bg(move || servers::party(&s), move |p| {
                    let (Some(me), Ok(p)) = (w.upgrade(), p) else { return };
                    let changed = me.known(&address, |k| k.party.as_ref() != Some(&p));
                    if changed {
                        me.know(&address, |k| k.party = Some(p));
                        if me.rms.shown.borrow().as_deref() == Some(address.as_str()) {
                            me.render_party_card();
                        }
                    }
                });
            }
            glib::ControlFlow::Continue
        });
    }

    /// The party card on the shown server's page: start one, join it, or (in it) the party itself.
    fn render_party_card(self: &Rc<Self>) {
        let Some(address) = self.rms.shown.borrow().clone() else { return };
        let Some(server) = self.joined(&address) else { return };
        let b = &self.rms.party_box;
        clear(b);
        b.remove_css_class("on");
        let heading = |text: &str, icon: &str| {
            let line = gtk::Box::builder().spacing(10).build();
            line.append(&gtk::Image::builder().icon_name(icon).pixel_size(20).build());
            line.append(&gtk::Label::builder().label(text).xalign(0.0).wrap(true).css_classes(["heading"]).build());
            line
        };
        if self.in_party_on(&address) {
            self.render_in_party(b, &server);
            return;
        }
        let (party, who) = self.known(&address, |k| (k.party.clone(), k.me.clone()));
        let text = |t: &str| gtk::Label::builder().label(t).xalign(0.0).wrap(true).wrap_mode(gtk::pango::WrapMode::WordChar).css_classes(["dim-label"]).build();
        match party {
            None => {
                b.append(&heading("Listening party", "audio-headphones-symbolic"));
                b.append(&text(&format!("Asking {} about its party…", server.name)));
            }
            Some(p) if !p.on => {
                b.append(&heading("Listening party", "audio-headphones-symbolic"));
                b.append(&text("Nobody's having a party right now. In one, everyone hears the same song at the same moment and anyone adds songs."));
                if who.as_ref().is_none_or(|m| m.can_host) {
                    let everyone = gtk::CheckButton::builder().label("Let everyone play, pause and skip").build();
                    let start = icon_button("media-playback-start-symbolic", "Start a Party");
                    start.add_css_class("suggested-action");
                    start.add_css_class("pill-button");
                    start.set_halign(gtk::Align::Start);
                    let (me, s, e) = (Rc::downgrade(self), server.clone(), everyone.downgrade());
                    start.connect_clicked(move |btn| {
                        if let Some(me) = me.upgrade() {
                            btn.set_sensitive(false);
                            me.start_party(&s, e.upgrade().is_some_and(|c| c.is_active()));
                        }
                    });
                    b.append(&everyone);
                    b.append(&start);
                } else {
                    b.append(&text(&format!("Hosting a party is off for you on {}: its owner's choice. You can join one when someone starts it.", server.name)));
                }
            }
            Some(p) => {
                b.add_css_class("on");
                let yours = who.as_ref().is_some_and(|m| m.person == p.host.0);
                b.append(&heading(&if yours { "Your listening party is on".to_string() } else { format!("{} is hosting a listening party", p.host.1) }, "audio-headphones-symbolic"));
                let now = match &p.song {
                    Some(e) => format!("Playing “{}”{}.", e.title, if e.artist.is_empty() { String::new() } else { format!(" by {}", e.artist) }),
                    None => "Nothing's playing yet: add a song and it starts.".to_string(),
                };
                let listening = match p.members.len() {
                    0 => "Nobody's listening right now.".to_string(),
                    1 => "1 person listening.".to_string(),
                    n => format!("{n} people listening."),
                };
                b.append(&text(&format!("{now} {listening} Join, and you hear it here in step with everyone.")));
                let join = icon_button("audio-headphones-symbolic", if yours { "Join Your Party" } else { "Join the Party" });
                join.add_css_class("suggested-action");
                join.add_css_class("pill-button");
                join.set_halign(gtk::Align::Start);
                join.set_sensitive(!self.rms.joining.get());
                let (me, s) = (Rc::downgrade(self), server.clone());
                join.connect_clicked(move |_| {
                    if let Some(me) = me.upgrade() {
                        me.join_party(&s);
                    }
                });
                b.append(&join);
                if let Some(other) = self.rms.party.borrow().as_ref() {
                    b.append(&text(&format!("Joining leaves {}'s party.", other.party.host.1)));
                }
            }
        }
    }

    /// The party you're in: what plays, its controls, reactions, who's listening, what's next.
    fn render_in_party(self: &Rc<Self>, b: &gtk::Box, server: &Joined) {
        b.add_css_class("on");
        let (party, me_id, owner, listening, lost) = {
            let link = self.rms.party.borrow();
            let Some(l) = link.as_ref() else { return };
            (l.party.clone(), l.me.person, l.me.owner, l.listening, l.lost)
        };
        let host = party.host.0 == me_id;
        let control = party.can_control(me_id);
        let top = gtk::Box::builder().spacing(10).build();
        top.append(&gtk::Image::builder().icon_name("audio-headphones-symbolic").pixel_size(20).build());
        top.append(&gtk::Label::builder().label(if host { "You're hosting the party".to_string() } else { format!("You're in {}'s party", party.host.1) }).xalign(0.0).hexpand(true).wrap(true).css_classes(["heading"]).build());
        let pill = gtk::Label::builder().label(if party.playing { "Playing" } else if party.song.is_some() { "Paused" } else { "Quiet" }).css_classes(["pill", if party.playing { "split" } else { "compressed" }]).valign(gtk::Align::Center).build();
        top.append(&pill);
        b.append(&top);
        b.append(&self.rms.sync_label);
        if lost {
            self.rms.sync_label.set_label("Lost the party: joining again…");
        }

        // what plays
        let now = gtk::Box::builder().spacing(14).build();
        let art = gtk::Image::builder().icon_name("audio-x-generic-symbolic").pixel_size(64).overflow(gtk::Overflow::Hidden).css_classes(["rms-now-art"]).build();
        now.append(&art);
        let words = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(2).valign(gtk::Align::Center).hexpand(true).build();
        match &party.song {
            Some(e) => {
                words.append(&gtk::Label::builder().label(&e.title).xalign(0.0).wrap(true).wrap_mode(gtk::pango::WrapMode::WordChar).css_classes(["title-3"]).build());
                let by = if e.artist.is_empty() { format!("Added by {}", e.added_by.1) } else { format!("{} · added by {}", e.artist, e.added_by.1) };
                words.append(&gtk::Label::builder().label(by).xalign(0.0).wrap(true).css_classes(["dim-label"]).build());
                let t = e.track(server);
                let weak = art.downgrade();
                self.set_art(&t.art, THUMB, move |tex| {
                    if let Some(img) = weak.upgrade() {
                        img.set_paintable(Some(tex));
                    }
                });
            }
            None => {
                words.append(&gtk::Label::builder().label("Nothing playing").xalign(0.0).css_classes(["title-3"]).build());
                words.append(&gtk::Label::builder().label("Add a song from the list below: it starts at once.").xalign(0.0).wrap(true).css_classes(["dim-label"]).build());
            }
        }
        now.append(&words);
        b.append(&now);

        // its controls (the host's, or everyone's when the host allows), and hearing it here
        let controls = adw::WrapBox::builder().child_spacing(8).line_spacing(8).build();
        if control {
            let (icon, label, action) = if party.playing { ("media-playback-pause-symbolic", "Pause", "pause") } else { ("media-playback-start-symbolic", "Play", "play") };
            let pp = icon_button(icon, label);
            pp.set_sensitive(party.song.is_some() || !party.queue.is_empty());
            let me = Rc::downgrade(self);
            pp.connect_clicked(move |_| {
                if let Some(me) = me.upgrade() {
                    me.party_act(action, serde_json::json!({}));
                }
            });
            controls.append(&pp);
            let skip = icon_button("media-skip-forward-symbolic", "Skip");
            skip.set_sensitive(party.song.is_some());
            let me = Rc::downgrade(self);
            skip.connect_clicked(move |_| {
                if let Some(me) = me.upgrade() {
                    me.party_act("skip", serde_json::json!({}));
                }
            });
            controls.append(&skip);
        }
        let (icon, label) = if listening { ("audio-volume-muted-symbolic", "Stop Hearing It Here") } else { ("audio-volume-high-symbolic", "Hear It Here") };
        let hear = icon_button(icon, label);
        hear.set_tooltip_text(Some("The party plays on either way; back on, it's in step again at once"));
        let me = Rc::downgrade(self);
        hear.connect_clicked(move |_| {
            if let Some(me) = me.upgrade() {
                me.party_listen(!listening);
            }
        });
        controls.append(&hear);
        b.append(&controls);
        if !control {
            b.append(&gtk::Label::builder().label(format!("{} (the host) plays, pauses and skips.", party.host.1)).xalign(0.0).wrap(true).css_classes(["dim-label", "caption"]).build());
        }

        // reactions: the server's own pictures
        let reacts = adw::WrapBox::builder().child_spacing(6).line_spacing(6).build();
        for (emoji, name, icon) in servers::REACTIONS {
            let r = gtk::Button::builder().child(&gtk::Image::builder().icon_name(icon).pixel_size(26).build()).tooltip_text(name).css_classes(["flat", "rms-react"]).build();
            r.update_property(&[gtk::accessible::Property::Label(name)]);
            let me = Rc::downgrade(self);
            r.connect_clicked(move |_| {
                if let Some(me) = me.upgrade() {
                    me.party_react(emoji);
                }
            });
            reacts.append(&r);
        }
        b.append(&reacts);
        // the last minute's reactions, newest first
        let fresh: Vec<&servers::Reaction> = party.reactions.iter().rev().take(6).collect();
        if !fresh.is_empty() {
            let line = adw::WrapBox::builder().child_spacing(6).line_spacing(6).build();
            for r in fresh {
                let (icon, name) = servers::reaction_icon(&r.emoji);
                let chip = gtk::Box::builder().spacing(6).css_classes(["rms-chip"]).tooltip_text(format!("{}: {name}", r.by)).build();
                chip.append(&gtk::Image::builder().icon_name(icon).pixel_size(22).build());
                chip.append(&gtk::Label::new(Some(&r.by)));
                line.append(&chip);
            }
            b.append(&line);
        }

        // who's listening
        let people = adw::WrapBox::builder().child_spacing(6).line_spacing(6).build();
        for m in &party.members {
            let chip = gtk::Box::builder().spacing(8).css_classes(["rms-chip"]).build();
            chip.append(&adw::Avatar::builder().size(24).text(&m.name).show_initials(true).build());
            chip.append(&gtk::Label::new(Some(&if m.host { format!("{} · host", m.name) } else { m.name.clone() })));
            people.append(&chip);
        }
        let listeners = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(6).build();
        listeners.append(&gtk::Label::builder().label(format!("Listening ({})", party.members.len())).xalign(0.0).css_classes(["caption-heading"]).build());
        listeners.append(&people);
        b.append(&listeners);

        // up next
        b.append(&gtk::Label::builder().label("Up next").xalign(0.0).css_classes(["caption-heading"]).build());
        let next = gtk::ListBox::builder().selection_mode(gtk::SelectionMode::None).css_classes(["boxed-list"]).build();
        if party.queue.is_empty() {
            next.append(&adw::ActionRow::builder().title("Nothing up next").subtitle("Click a song in the list below to add it: anyone in the party can.").build());
        }
        let n = party.queue.len();
        for (i, e) in party.queue.iter().enumerate() {
            let row = adw::ActionRow::builder().use_markup(false).title(&e.title).subtitle(if e.artist.is_empty() { format!("Added by {}", e.added_by.1) } else { format!("{} · added by {}", e.artist, e.added_by.1) }).title_lines(1).subtitle_lines(1).build();
            let tool = |icon: &str, tip: &str, action: &'static str, extra: serde_json::Value, on: bool| {
                let btn = gtk::Button::builder().icon_name(icon).tooltip_text(tip).valign(gtk::Align::Center).css_classes(["flat", "circular"]).sensitive(on).build();
                let me = Rc::downgrade(self);
                btn.connect_clicked(move |_| {
                    if let Some(me) = me.upgrade() {
                        me.party_act(action, extra.clone());
                    }
                });
                btn
            };
            if control {
                row.add_suffix(&tool("go-up-symbolic", "Earlier", "reorder", serde_json::json!({ "entry": e.entry, "to": i.saturating_sub(1) }), i > 0));
                row.add_suffix(&tool("go-down-symbolic", "Later", "reorder", serde_json::json!({ "entry": e.entry, "to": i + 1 }), i + 1 < n));
            }
            if control || e.added_by.0 == me_id {
                row.add_suffix(&tool("list-remove-symbolic", "Take it out of the queue", "remove", serde_json::json!({ "entry": e.entry }), true));
            }
            next.append(&row);
        }
        b.append(&next);

        // the host's: who controls it, the end; everyone's: leave
        let end_row = adw::WrapBox::builder().child_spacing(8).line_spacing(8).margin_top(4).build();
        if host {
            let (icon, label) = if party.everyone_controls { ("system-users-symbolic", "Only I Play, Pause and Skip") } else { ("system-users-symbolic", "Let Everyone Play, Pause and Skip") };
            let allow = icon_button(icon, label);
            let (me, everyone) = (Rc::downgrade(self), !party.everyone_controls);
            allow.connect_clicked(move |_| {
                if let Some(me) = me.upgrade() {
                    me.party_act("allow", serde_json::json!({ "everyone": everyone }));
                }
            });
            end_row.append(&allow);
        }
        let leave = icon_button("system-log-out-symbolic", "Leave the Party");
        let me = Rc::downgrade(self);
        leave.connect_clicked(move |_| {
            if let Some(me) = me.upgrade() {
                me.leave_party(true);
                me.toast("You left the party: back to your own music");
            }
        });
        end_row.append(&leave);
        if host || owner {
            let end = icon_button("process-stop-symbolic", "End the Party");
            end.add_css_class("destructive-action");
            let (me, s) = (Rc::downgrade(self), server.clone());
            end.connect_clicked(move |_| {
                if let Some(me) = me.upgrade() {
                    me.end_party(&s);
                }
            });
            end_row.append(&end);
        }
        b.append(&end_row);
        self.show_sync();
    }

    /// The party's line: in time or catching up, and how well the clock is known.
    fn show_sync(&self) {
        let link = self.rms.party.borrow();
        let Some(l) = link.as_ref() else { return };
        if l.lost {
            self.rms.sync_label.set_label("Lost the party: joining again…");
            return;
        }
        let clock = format!(" · clock ±{} ms", ((l.rtt / 2.0).round() as i64).max(1));
        let text = if !l.listening {
            "Not heard here: the party plays on. Hear It Here to be back in step.".to_string()
        } else if l.party.song.is_none() {
            "Waiting for a song.".to_string()
        } else if !l.party.playing {
            format!("Paused by {}", if l.party.host.0 == l.me.person { "you".to_string() } else { l.party.host.1.clone() })
        } else {
            match (l.party.position_ms(servers::now_ms() + l.offset), l.drift_ms) {
                (Some(p), _) if p < 0.0 => "Starting…".to_string(),
                (_, Some(d)) if d.abs() <= servers::IN_STEP * 1000.0 => format!("In time with the party ({:+.0} ms)", d),
                (_, Some(d)) => format!("Catching up ({:+.0} ms)", d),
                _ => "Getting in step…".to_string(),
            }
        };
        self.rms.sync_label.set_label(&format!("{text}{clock}"));
    }

    fn start_party(self: &Rc<Self>, server: &Joined, everyone: bool) {
        let (me, s) = (Rc::downgrade(self), server.clone());
        bg(move || servers::party_start(&s, everyone), {
            let server = server.clone();
            move |started: Result<Party, Failure>| {
                let Some(me) = me.upgrade() else { return };
                match started {
                    Ok(p) => {
                        me.know(&server.address, |k| k.party = Some(p));
                        me.toast("The party is on. Add a song: it starts at once.");
                        me.join_party(&server); // the host is in it at once
                    }
                    Err(f) => {
                        me.toast(&f.words(&server.name));
                        me.render_party_card();
                    }
                }
            }
        });
    }

    fn end_party(self: &Rc<Self>, server: &Joined) {
        let ask = adw::AlertDialog::new(Some("End the party for everyone?"), Some("Everyone in it goes back to their own music."));
        ask.add_response("cancel", "Cancel");
        ask.add_response("end", "End the Party");
        ask.set_response_appearance("end", adw::ResponseAppearance::Destructive);
        ask.set_close_response("cancel");
        let (w, s) = (Rc::downgrade(self), server.clone());
        ask.connect_response(None, move |_, response| {
            if response != "end" {
                return;
            }
            let Some(me) = w.upgrade() else { return };
            let (w2, s2, name) = (Rc::downgrade(&me), s.clone(), s.name.clone());
            bg(move || servers::party_end(&s2), move |ended| {
                let Some(me) = w2.upgrade() else { return };
                if let Err(f) = ended {
                    me.toast(&f.words(&name));
                }
                // (the WebSocket says it's over: everyone, you too, goes back)
            });
        });
        ask.present(Some(&self.window));
    }

    /// Joins a server's party: its clock measured, its WebSocket opened; your
    /// own music waits (it comes back when you leave).
    fn join_party(self: &Rc<Self>, server: &Joined) {
        if self.rms.joining.replace(true) {
            return;
        }
        self.render_party_card();
        let (me, s) = (Rc::downgrade(self), server.clone());
        bg(
            move || {
                let who = servers::me(&s)?;
                let clock = servers::clock_offset(&s);
                Ok::<_, Failure>((who, clock))
            },
            {
                let server = server.clone();
                move |found: Result<(Me, Option<(f64, f64)>), Failure>| {
                    let Some(me) = me.upgrade() else { return };
                    me.rms.joining.set(false);
                    let (who, clock) = match found {
                        Ok(x) => x,
                        Err(f) => {
                            me.toast(&f.words(&server.name));
                            me.render_party_card();
                            return;
                        }
                    };
                    let Some((offset, rtt)) = clock else {
                        me.toast(&format!("{} isn't answering. Try joining again in a moment.", server.name));
                        me.render_party_card();
                        return;
                    };
                    if me.in_party() {
                        me.leave_party(false); // another server's party: this one now
                    }
                    // your own music, as it is (it comes back when you leave)
                    me.save_session();
                    let player = me.player.borrow();
                    let before = Before {
                        queue: me.queue.borrow().clone(),
                        index: me.index.get(),
                        current: me.current.borrow().clone(),
                        at: player.as_ref().and_then(|p| p.position()).or(me.restore_at.get()).unwrap_or(0.0),
                        playing: player.as_ref().is_some_and(|p| p.playing()),
                    };
                    if let Some(p) = player.as_ref() {
                        p.stop();
                    }
                    drop(player);
                    me.restored.set(false);
                    let gen = me.rms.party_gen.get() + 1;
                    me.rms.party_gen.set(gen);
                    let (tx, rx) = async_channel::unbounded::<Live>();
                    let socket = servers::listen(&server, tx);
                    crate::log::note(&format!("joined {}'s party (clock {offset:+.0} ms, ±{:.0} ms)", server.name, rtt / 2.0));
                    let party = me.known(&server.address, |k| k.party.clone()).unwrap_or_default();
                    me.rms.party.replace(Some(InParty {
                        server: server.clone(),
                        me: who,
                        party,
                        offset,
                        rtt,
                        _socket: socket,
                        listening: true,
                        entry: None,
                        lead: 0.15,
                        jumped: None,
                        seeked: false,
                        slack: 0.0,
                        hold: None,
                        drift_ms: None,
                        start: None,
                        lost: false,
                        before: Some(before),
                        checks: 0,
                    }));
                    let w = Rc::downgrade(&me);
                    glib::spawn_future_local(async move {
                        while let Ok(live) = rx.recv().await {
                            let Some(me) = w.upgrade() else { break };
                            if me.rms.party_gen.get() != gen {
                                break; // left (or another party since)
                            }
                            me.party_news(live);
                        }
                    });
                    // in step: checked every second; the clock measured again once a minute
                    let w = Rc::downgrade(&me);
                    glib::timeout_add_local(PARTY_CHECK, move || {
                        let Some(me) = w.upgrade() else { return glib::ControlFlow::Break };
                        if me.rms.party_gen.get() != gen || !me.in_party() {
                            return glib::ControlFlow::Break;
                        }
                        me.party_sync();
                        me.show_sync();
                        let again = me.rms.party.borrow_mut().as_mut().map(|l| {
                            l.checks += 1;
                            (l.checks % 60 == 0).then(|| l.server.clone())
                        });
                        if let Some(Some(s)) = again {
                            let w2 = Rc::downgrade(&me);
                            bg(move || servers::clock_offset(&s), move |c| {
                                if let (Some(me), Some((offset, rtt))) = (w2.upgrade(), c) {
                                    if let Some(l) = me.rms.party.borrow_mut().as_mut() {
                                        l.offset = offset;
                                        l.rtt = rtt;
                                    }
                                }
                            });
                        }
                        glib::ControlFlow::Continue
                    });
                    me.set_controls();
                    if me.rms.shown.borrow().as_deref() == Some(server.address.as_str()) {
                        me.render_server(); // its songs: a click adds one to the party now
                    }
                }
            },
        );
    }

    /// What the party's WebSocket said.
    fn party_news(self: &Rc<Self>, live: Live) {
        match live {
            Live::State(v) => {
                let p = servers::parse_party(&v);
                let Some(address) = self.rms.party.borrow().as_ref().map(|l| l.server.address.clone()) else { return };
                if !p.on {
                    // (the host who ended it is told no news of their own)
                    let host = self.rms.party.borrow().as_ref().filter(|l| l.party.host.0 != l.me.person).map(|l| l.party.host.1.clone()).unwrap_or_default();
                    self.leave_party(true);
                    self.toast(&if host.is_empty() { "The party is over: back to your own music".to_string() } else { format!("{host} ended the party: back to your own music") });
                    self.know(&address, |k| k.party = Some(p));
                    return;
                }
                if let Some(l) = self.rms.party.borrow_mut().as_mut() {
                    l.party = p.clone();
                    l.lost = false;
                }
                self.know(&address, |k| k.party = Some(p));
                self.party_sync();
                if self.rms.shown.borrow().as_deref() == Some(address.as_str()) {
                    self.render_party_card();
                }
                self.set_controls();
            }
            Live::Removed(why) => {
                self.leave_party(true);
                self.toast(&format!("{why} You're out of the party: back to your own music."));
            }
            Live::Lost | Live::Back => {
                if let Some(l) = self.rms.party.borrow_mut().as_mut() {
                    l.lost = matches!(live, Live::Lost);
                }
                self.show_sync();
            }
        }
    }

    /// Plays the party's song where everyone is (server time, through the
    /// clock offset): loads it when it's new, starts it on the dot when it's
    /// still to start, and, more than servers::IN_STEP off, jumps (or, a little
    /// ahead, waits just that long: a song whose jumps land early is jumped past
    /// the spot, then waits).
    fn party_sync(self: &Rc<Self>) {
        let (song, party, server, entry, listening, offset, lead, jumped, holding) = {
            let link = self.rms.party.borrow();
            let Some(l) = link.as_ref() else { return };
            (l.party.song.clone(), l.party.clone(), l.server.clone(), l.entry, l.listening, l.offset, l.lead, l.jumped, l.hold.is_some())
        };
        let player = self.player.borrow();
        let Some(p) = player.as_ref() else { return };
        let Some(song) = song else {
            // nothing playing in the party: quiet here too
            if entry.is_some() {
                p.stop();
                if let Some(l) = self.rms.party.borrow_mut().as_mut() {
                    l.entry = None;
                }
                drop(player);
                self.set_controls();
            }
            return;
        };
        let want = party.position_ms(servers::now_ms() + offset).unwrap_or(0.0) / 1000.0;
        let length = song.duration_ms.map(|d| d / 1000.0);
        if entry != Some(song.entry) {
            // a new song: loaded where the party is (or at its start, to begin on the dot)
            if let Some(l) = self.rms.party.borrow_mut().as_mut() {
                l.entry = Some(song.entry);
                l.jumped = None;
                l.seeked = false;
                l.slack = 0.0;
                l.drift_ms = None;
                if let Some(id) = l.start.take() {
                    id.remove();
                }
                if let Some(id) = l.hold.take() {
                    id.remove();
                }
            }
            drop(player);
            let t = song.track(&server);
            let from = if party.playing { (want + lead).max(0.0) } else { want.max(0.0) };
            self.party_load(&t, from, listening && party.playing && want >= 0.0);
            if party.playing && want < 0.0 {
                self.party_start_in(-want);
            }
            return;
        }
        if holding && listening && party.playing {
            return; // waiting for the party to catch up (see below)
        }
        let playing = p.playing();
        if !listening {
            if playing {
                p.set_playing(false);
            }
            return;
        }
        if !party.playing {
            if playing {
                p.set_playing(false);
            }
            // (a song whose jumps land early stays where it landed: near enough until it plays)
            let near = 0.3 + self.rms.party.borrow().as_ref().map(|l| l.slack).unwrap_or(0.0);
            if p.position().is_some_and(|at| (at - want).abs() > near) {
                p.seek(want.max(0.0));
            }
            return;
        }
        if want < 0.0 {
            // it starts in a moment: ready at its start, started right on time
            if playing {
                p.set_playing(false);
            }
            let no_start = self.rms.party.borrow().as_ref().is_some_and(|l| l.start.is_none());
            drop(player);
            if no_start {
                self.party_start_in(-want);
            }
            return;
        }
        if length.is_some_and(|l| l > 0.0 && want > l) {
            if playing {
                p.set_playing(false); // the next song in a moment (the server starts it)
            }
            return;
        }
        if !playing {
            let slack = self.rms.party.borrow().as_ref().map(|l| l.slack).unwrap_or(0.0);
            p.seek(want + lead + slack);
            p.set_playing(true);
            if let Some(l) = self.rms.party.borrow_mut().as_mut() {
                l.jumped = Some(Instant::now());
                l.seeked = true;
            }
            drop(player);
            self.set_controls();
            return;
        }
        let Some(have) = p.position() else { return };
        if jumped.is_some_and(|j| j.elapsed() < Duration::from_millis(400)) {
            return; // a jump still landing
        }
        let drift = have - want;
        let mut link = self.rms.party.borrow_mut();
        let Some(l) = link.as_mut() else { return };
        // the first look after a jump: how it landed
        if let (Some(j), true) = (l.jumped.take(), l.seeked) {
            if j.elapsed() < Duration::from_secs(3) {
                if drift < -servers::IN_STEP && l.slack == 0.0 {
                    l.slack = PAST_THE_SPOT; // this song's jumps land early: past the spot from now on, then wait
                } else if l.slack == 0.0 {
                    l.lead = (l.lead - drift).clamp(0.0, 1.5); // how long a jump takes here: aim that far ahead
                }
            }
        }
        l.drift_ms = Some(drift * 1000.0);
        // behind, or far ahead: a jump; a little ahead: a wait just that long
        let step = servers::step(drift);
        if std::env::var_os("RADIANCE_DEBUG").is_some() {
            let what = match step {
                servers::Step::Jump => format!(": a jump, {:.0} ms ahead", (l.lead + l.slack) * 1000.0),
                servers::Step::Wait(w) => format!(": a {:.0} ms wait", w * 1000.0),
                servers::Step::Keep => String::new(),
            };
            eprintln!("[party] here {have:.3} s, the party {want:.3} s ({:+.0} ms){what}", drift * 1000.0);
        }
        if step == servers::Step::Jump {
            p.seek(want + l.lead + l.slack);
            l.jumped = Some(Instant::now());
            l.seeked = true;
        } else if let servers::Step::Wait(drift) = step {
            p.set_playing(false);
            let w = Rc::downgrade(self);
            let id = glib::timeout_add_local_once(Duration::from_secs_f64(drift), move || {
                let Some(me) = w.upgrade() else { return };
                let go = {
                    let mut link = me.rms.party.borrow_mut();
                    let Some(l) = link.as_mut() else { return };
                    l.hold = None;
                    l.jumped = Some(Instant::now());
                    l.seeked = false;
                    l.listening && l.party.playing
                };
                if go {
                    if let Some(p) = me.player.borrow().as_ref() {
                        p.set_playing(true);
                    }
                }
            });
            l.hold = Some(id);
        }
    }

    /// A party song starts in `secs`: on the dot then (the check after puts it right).
    fn party_start_in(self: &Rc<Self>, secs: f64) {
        let w = Rc::downgrade(self);
        let id = glib::timeout_add_local_once(Duration::from_secs_f64(secs.max(0.0)), move || {
            let Some(me) = w.upgrade() else { return };
            let listening = {
                let mut link = me.rms.party.borrow_mut();
                let Some(l) = link.as_mut() else { return };
                l.start = None;
                l.jumped = Some(Instant::now());
                l.seeked = false;
                l.listening && l.party.playing
            };
            if listening {
                if let Some(p) = me.player.borrow().as_ref() {
                    p.set_playing(true);
                }
                me.set_controls();
            }
        });
        if let Some(l) = self.rms.party.borrow_mut().as_mut() {
            if let Some(old) = l.start.replace(id) {
                old.remove();
            }
        }
    }

    /// The party's song into the player (shown in the bar and Now Playing like any song).
    fn party_load(self: &Rc<Self>, t: &Track, from: f64, play: bool) {
        crate::watch::doing("loading the party's song");
        let gen = self.play_gen.get() + 1;
        self.play_gen.set(gen);
        self.kbps.set(0);
        self.quality_known.set(false);
        self.restored.set(false);
        self.restore_at.set(None);
        *self.queue.borrow_mut() = vec![t.clone()];
        *self.unshuffled.borrow_mut() = None;
        self.index.set(0);
        *self.current.borrow_mut() = Some(t.clone());
        self.on_half.set(None);
        self.on_stems.set(false);
        self.show_current(t);
        self.load_lyrics(t, gen);
        self.note_played(t);
        self.update_voice_status();
        self.render_queue();
        let Some(uri) = self.uri_plain(t) else { return };
        self.replan();
        self.current_uri.replace(uri.clone());
        if let Some(p) = self.player.borrow().as_ref() {
            p.load_at(&uri, from);
            if !play {
                p.set_playing(false); // ready, quiet until it's time
            }
        }
        if play {
            if let Some(l) = self.rms.party.borrow_mut().as_mut() {
                l.jumped = Some(Instant::now());
                l.seeked = true;
            }
        }
        self.set_controls();
    }

    /// Hearing the party here, or not (it plays on); back on, in step at once.
    fn party_listen(self: &Rc<Self>, on: bool) {
        if let Some(l) = self.rms.party.borrow_mut().as_mut() {
            l.listening = on;
            l.jumped = None;
            if let Some(id) = l.hold.take() {
                id.remove();
            }
        }
        self.party_sync();
        self.render_party_card();
        self.set_controls();
    }

    /// The bar's play button in a party: play / pause for everyone (when you
    /// control it), else hearing it here or not.
    pub(super) fn party_play_pause(self: &Rc<Self>) {
        let (control, playing, listening) = {
            let link = self.rms.party.borrow();
            let Some(l) = link.as_ref() else { return };
            (l.party.can_control(l.me.person), l.party.playing, l.listening)
        };
        if control && listening {
            self.party_act(if playing { "pause" } else { "play" }, serde_json::json!({}));
        } else {
            self.party_listen(!listening);
        }
    }

    /// Next in a party: skips for everyone (when you control it).
    pub(super) fn party_skip(self: &Rc<Self>, by: i32) {
        let (control, host) = {
            let link = self.rms.party.borrow();
            let Some(l) = link.as_ref() else { return };
            (l.party.can_control(l.me.person), l.party.host.1.clone())
        };
        if by > 0 && control {
            self.party_act("skip", serde_json::json!({}));
        } else if by > 0 {
            self.toast(&format!("{host} (the host) skips songs in this party"));
        }
    }

    /// The seek bar in a party: for everyone (when you control it); else it stays where the party is.
    pub(super) fn party_seek(self: &Rc<Self>, at: f64) {
        let control = self.rms.party.borrow().as_ref().is_some_and(|l| l.party.can_control(l.me.person));
        if control {
            self.party_act("seek", serde_json::json!({ "position_ms": (at.max(0.0) * 1000.0) as i64 }));
        }
    }

    /// The bar's buttons in a party: (you control it, a song plays, it's heard here now).
    pub(super) fn party_buttons(&self) -> Option<(bool, bool, bool)> {
        let link = self.rms.party.borrow();
        let l = link.as_ref()?;
        Some((l.party.can_control(l.me.person), l.party.song.is_some(), l.party.playing && l.listening))
    }

    /// Any of your songs in a server's split queue.
    pub(super) fn has_away(&self) -> bool {
        !self.rms.away.borrow().is_empty()
    }

    /// The player's news in a party (true: handled here). A song's end waits
    /// for the server's next one; a song that won't play says why.
    pub(super) fn party_event(self: &Rc<Self>, ev: &crate::player::Event) -> bool {
        if !self.in_party() {
            return false;
        }
        match ev {
            crate::player::Event::Ready => false,
            crate::player::Event::Ended => true,
            crate::player::Event::Error(e) => {
                let reason = self.server_reason(e).map(|r| r.0).unwrap_or_else(|| self.why_not(e).0.to_string());
                self.toast(&format!("The party's song can't play here: {reason}"));
                true
            }
        }
    }

    /// A control for everyone in the party (play, pause, skip, seek, reorder, remove, allow).
    fn party_act(self: &Rc<Self>, action: &'static str, extra: serde_json::Value) {
        let Some(server) = self.rms.party.borrow().as_ref().map(|l| l.server.clone()) else { return };
        let me = Rc::downgrade(self);
        bg(move || servers::party_control(&server, action, extra).map_err(|f| f.words(&server.name)), move |done| {
            let Some(me) = me.upgrade() else { return };
            match done {
                Ok(p) => {
                    if let Some(l) = me.rms.party.borrow_mut().as_mut() {
                        l.party = p;
                    }
                    me.party_sync();
                    me.render_party_card();
                    me.set_controls();
                }
                Err(e) => me.toast(&e),
            }
        });
    }

    fn party_react(self: &Rc<Self>, emoji: &'static str) {
        let Some(server) = self.rms.party.borrow().as_ref().map(|l| l.server.clone()) else { return };
        let me = Rc::downgrade(self);
        bg(move || servers::party_react(&server, emoji).map_err(|f| f.words(&server.name)), move |done| {
            if let (Some(me), Err(e)) = (me.upgrade(), done) {
                me.toast(&e);
            }
        });
    }

    /// A server's song into its party's queue (it starts at once when nothing plays).
    pub(super) fn add_to_party(self: &Rc<Self>, t: &Track) {
        let Some(server) = servers::of_track(t) else { return };
        let (title, song) = (t.title.clone(), t.clone());
        let me = Rc::downgrade(self);
        bg(move || servers::party_add(&server, &song).map_err(|f| f.words(&server.name)), move |done| {
            let Some(me) = me.upgrade() else { return };
            match done {
                Ok(p) => {
                    if let Some(l) = me.rms.party.borrow_mut().as_mut() {
                        l.party = p;
                    }
                    me.party_sync();
                    me.render_party_card();
                    me.toast(&format!("Added “{title}” to the party"));
                }
                Err(e) => me.toast(&e),
            }
        });
    }

    /// The song clicked on a server's page: into its party when you're in it, else played.
    pub(super) fn play_from_server(self: &Rc<Self>, list: &Rc<Vec<Track>>, i: usize) {
        let Some(t) = list.get(i) else { return };
        if servers::of_track(t).is_some_and(|s| self.in_party_on(&s.address)) {
            self.add_to_party(t);
        } else {
            self.play_list((**list).clone(), i);
        }
    }

    /// Out of the party (its WebSocket closes). `back`: your own music as it was.
    pub(super) fn leave_party(self: &Rc<Self>, back: bool) {
        let Some(mut link) = self.rms.party.borrow_mut().take() else { return };
        self.rms.party_gen.set(self.rms.party_gen.get() + 1);
        if let Some(id) = link.start.take() {
            id.remove();
        }
        if let Some(id) = link.hold.take() {
            id.remove();
        }
        crate::log::note(&format!("left the party on {}", link.server.name));
        if let Some(p) = self.player.borrow().as_ref() {
            p.stop();
        }
        if let (true, Some(b)) = (back, link.before.take()) {
            if let Some(t) = b.current {
                *self.queue.borrow_mut() = b.queue;
                self.index.set(b.index);
                let gen = self.play_gen.get() + 1;
                self.play_gen.set(gen);
                *self.current.borrow_mut() = Some(t.clone());
                self.show_current(&t);
                self.load_lyrics(&t, gen);
                self.restored.set(true);
                self.restore_at.set((b.at > 1.0).then_some(b.at));
                self.seek.set_value(b.at);
                self.pos.set_label(&clock(b.at));
                self.set_quality(None, false);
                self.render_queue();
                self.update_voice_status();
                if b.playing {
                    self.toggle_play(); // on from where it was
                }
            }
        }
        self.set_controls();
        if self.rms.shown.borrow().as_deref() == Some(link.server.address.as_str()) {
            self.render_server();
        }
    }
}
