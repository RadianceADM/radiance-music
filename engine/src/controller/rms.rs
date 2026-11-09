//! Radiance Music Servers for the Windows beta (stage 4c-1), as the Linux app's session in src/ui/rms.rs: the
//! joined servers (settings.json, shared with today's app) known from the start, what each lets you do and its
//! songs fetched quietly, its events line telling what changes (no asking again and again; every 30 s for one
//! whose line is down), joining by an invite, leaving, and a server that moved to a new address kept as it was.
//! The words are core's (server_words), the protocol the engine's (servers.rs).

use super::*;
use crate::servers::{self, Failure, Joined, Live, Me, Song};
use radiance_core::server_words as rw;

/// Only when a server's events line is down: its page is asked again this often.
const FALLBACK: Duration = Duration::from_secs(30);

/// A joined server in Servers…: its row (the Linux app's server_row).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ServerRow {
    pub address: String,
    pub name: String,
    /// "address · you're NAME" (or "· checking…" until it says, or what went wrong).
    pub subtitle: String,
    /// Play its songs and join its parties, add songs, split songs, host a party.
    pub can: [bool; 4],
    pub owner: bool,
    /// What went wrong last, in plain words (None: all's well).
    pub problem: Option<String>,
    /// This computer's key doesn't work there anymore.
    pub signed_out: bool,
    /// It said who you are there (`can` is its answer, not a guess).
    pub known: bool,
    /// How many songs it has (None: not fetched yet).
    pub songs: Option<usize>,
}

/// Servers… as it shows: the joined servers, the line under the invite box (its words, and whether it's an
/// error), and a join under way.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ServersState {
    pub rows: Vec<ServerRow>,
    pub note: Option<(String, bool)>,
    pub joining: bool,
}

/// A server's page (the Linux app's render_server and filter_server): its name, the line under it, what you can do
/// there, the problem card, its songs as shown (each with its line: byline · added by), and the line over them.
#[derive(Clone, Debug, Default)]
pub struct ServerPage {
    pub address: String,
    pub name: String,
    pub line: String,
    pub permissions: String,
    pub problem: Option<String>,
    pub signed_out: bool,
    pub fetching: bool,
    pub can_add: bool,
    pub songs: Vec<(Track, String)>,
    pub note: String,
}

/// A joined server, as last heard from it.
#[derive(Default)]
pub(super) struct Known {
    me: Option<Me>,
    songs: Option<Vec<Song>>,
    problem: Option<Failure>,
    fetching: bool,
    /// Asked for again while fetching (an event came): once more when it's done.
    again: bool,
    /// Its events line is open: no asking again and again.
    live: bool,
}

/// The servers' part of the engine.
#[derive(Default)]
pub(super) struct Rms {
    known: RefCell<HashMap<String, Known>>,
    /// Each joined server's events line (by its address).
    lines: RefCell<HashMap<String, servers::Socket>>,
    fallback: Cell<bool>,
    joining: Cell<bool>,
    note: RefCell<Option<(String, bool)>>,
}

impl Engine {
    pub(super) fn rms_joined(&self, address: &str) -> Option<Joined> {
        self.settings.borrow().servers.iter().find(|s| s.address == address).cloned()
    }

    /// The joined list saved, and the player told (a server song's key goes on as it plays).
    fn rms_save(&self) {
        settings::save(&self.settings.borrow());
        servers::set_joined(&self.settings.borrow().servers);
    }

    /// Who this computer is on a server, as last heard.
    pub(super) fn rms_me(&self, address: &str) -> Option<Me> {
        self.rms.known.borrow().get(address).and_then(|k| k.me.clone())
    }

    /// A server's events line is open (it says when its party starts or ends).
    pub(super) fn rms_live(&self, address: &str) -> bool {
        self.rms.known.borrow().get(address).is_some_and(|k| k.live)
    }

    fn rms_know(&self, address: &str, f: impl FnOnce(&mut Known)) {
        f(self.rms.known.borrow_mut().entry(address.to_string()).or_default());
    }

    /// At the start: the joined servers to the player, each one's songs and what it lets you do (asked quietly),
    /// its events line, and whether it moved (a domain added: it says where it is now).
    pub(super) fn rms_start(self: &Rc<Self>) {
        let list = self.settings.borrow().servers.clone();
        servers::set_joined(&list);
        for s in list {
            self.rms_fetch(&s);
            self.rms_open_line(&s);
            let was = s.clone();
            self.bg(move || servers::moved(&s), move |e, now| {
                if let Some(address) = now {
                    let moved = Joined { address, ..was.clone() };
                    e.rms_moved(&was, &moved);
                }
            });
        }
        self.rms_fallback();
        self.rms_publish();
    }

    /// A server's events line: what changes there is told.
    fn rms_open_line(self: &Rc<Self>, server: &Joined) {
        if self.rms.lines.borrow().contains_key(&server.address) || std::env::var_os("RADIANCE_SHOT").is_some() {
            return;
        }
        let (tx, rx) = async_channel::unbounded::<Live>();
        let socket = servers::events(server, tx);
        self.rms.lines.borrow_mut().insert(server.address.clone(), socket);
        let (me, address) = (Rc::downgrade(self), server.address.clone());
        glib::spawn_future_local(async move {
            while let Ok(live) = rx.recv().await {
                let Some(e) = me.upgrade() else { break };
                e.rms_news(&address, live);
            }
        });
    }

    fn rms_close_line(&self, address: &str) {
        self.rms.lines.borrow_mut().remove(address); // (its thread closes the line)
        self.rms_know(address, |k| k.live = false);
    }

    /// What a server's events line said.
    fn rms_news(self: &Rc<Self>, address: &str, live: Live) {
        let Some(server) = self.rms_joined(address) else { return };
        match live {
            Live::Hello(id, name) => {
                self.rms_know(address, |k| k.live = true);
                // its id (an older join didn't keep it) and its name, as it says now
                let renamed = !name.is_empty() && name != server.name;
                if (id.is_some() && server.id.is_none()) || renamed {
                    if let Some(s) = self.settings.borrow_mut().servers.iter_mut().find(|s| s.address == address) {
                        s.id = s.id.clone().or(id);
                        if renamed {
                            s.name = name;
                        }
                    }
                    self.rms_save();
                    self.rms_publish();
                }
            }
            Live::Library | Live::Me => self.rms_fetch(&server),
            Live::Back => {
                self.rms_know(address, |k| k.live = true);
                self.rms_fetch(&server); // what changed while it was down
            }
            Live::Lost => self.rms_know(address, |k| k.live = false),
            Live::Removed(_) => {
                self.rms_close_line(address);
                self.rms_fetch(&server); // (its row and page say so, in plain words)
            }
            Live::Party => self.party_ask(&server),
            Live::Queue(_) | Live::State(_) => {} // (the split queue: stage 4c-2's Task 4b)
        }
    }

    /// Every FALLBACK, the servers whose events line is down are asked again.
    fn rms_fallback(self: &Rc<Self>) {
        if self.rms.fallback.replace(true) {
            return;
        }
        let me = Rc::downgrade(self);
        glib::timeout_add_local(FALLBACK, move || {
            let Some(e) = me.upgrade() else { return glib::ControlFlow::Break };
            for s in e.settings.borrow().servers.clone() {
                let live = e.rms.known.borrow().get(&s.address).is_some_and(|k| k.live);
                if !live {
                    e.rms_fetch(&s);
                }
            }
            glib::ControlFlow::Continue
        });
    }

    /// A server's songs and what it lets you do, asked on a worker (asked again while asking: once more after).
    fn rms_fetch(self: &Rc<Self>, server: &Joined) {
        let busy = self.rms.known.borrow().get(&server.address).is_some_and(|k| k.fetching);
        if busy {
            self.rms_know(&server.address, |k| k.again = true);
            return;
        }
        self.rms_know(&server.address, |k| k.fetching = true);
        self.rms_publish();
        let (s, address) = (server.clone(), server.address.clone());
        self.bg(
            move || {
                let who = servers::me(&s);
                let songs = who.as_ref().ok().map(|_| servers::library(&s));
                let party = who.as_ref().ok().and_then(|_| servers::party(&s).ok());
                (who, songs, party)
            },
            move |e, (who, songs, party): (Result<Me, Failure>, Option<Result<Vec<Song>, Failure>>, Option<servers::Party>)| {
                if let Some(p) = party {
                    e.party_known(&address, p);
                }
                // its id, for an older join that didn't keep it
                if let Some(id) = who.as_ref().ok().and_then(|m| m.server_id.clone()) {
                    let learnt = e.settings.borrow_mut().servers.iter_mut().find(|s| s.address == address && s.id.is_none()).map(|s| s.id = Some(id)).is_some();
                    if learnt {
                        e.rms_save();
                    }
                }
                let mut again = false;
                e.rms_know(&address, |k| {
                    k.fetching = false;
                    again = std::mem::take(&mut k.again);
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
                });
                if k_songs_missing(&e.rms.known.borrow(), &address) {
                    // nothing fetched yet this time: its songs as last sent (the cache the library keeps)
                    let cached = servers::cached(&address);
                    if !cached.is_empty() {
                        e.rms_know(&address, |k| k.songs = Some(cached));
                    }
                }
                e.rms_publish();
                if again {
                    if let Some(s) = e.rms_joined(&address) {
                        e.rms_fetch(&s);
                    }
                }
            },
        );
    }

    /// Join (Servers…'s invite box): a server this computer is on already says so; one already joined at another
    /// address moves here; joined, it's in the sidebar with its songs on the way.
    pub(super) fn rms_join(self: &Rc<Self>, text: String) {
        let (address, code) = match servers::parse_invite(&text) {
            Ok(x) => x,
            Err(e) => return self.rms_note(e, true),
        };
        if let Some(s) = self.rms_joined(&address) {
            return self.rms_note(rw::already_on(&s.name), false);
        }
        if self.rms.joining.replace(true) {
            return;
        }
        self.rms_note(rw::JOINING.to_string(), false);
        let device = glib::host_name().to_string();
        let joined = self.settings.borrow().servers.clone();
        self.bg(move || servers::join_or_move(&address, &code, &device, &joined), |e, done| {
            e.rms.joining.set(false);
            match done {
                Ok((now, Some(was))) => {
                    e.rms_note(rw::moved_line(&now.name, &now.address), false);
                    e.rms_moved(&was, &now);
                }
                Ok((joined, None)) => {
                    crate::log::note(&format!("joined {} ({})", joined.name, joined.address));
                    e.settings.borrow_mut().servers.push(joined.clone());
                    e.rms_save();
                    e.rms_note(rw::joined_line(&joined.name), false);
                    e.rms_fetch(&joined);
                    e.rms_open_line(&joined);
                    e.toast(rw::joined_toast(&joined.name));
                }
                Err(err) => e.rms_note(err, true),
            }
        });
    }

    fn rms_note(&self, words: String, error: bool) {
        self.rms.note.replace(Some((words, error)));
        self.rms_publish();
    }

    /// Leave: the server stops taking this computer's key, then it's forgotten here (not answering: forgotten anyway,
    /// and the toast says so).
    pub(super) fn rms_leave(self: &Rc<Self>, address: String) {
        let Some(server) = self.rms_joined(&address) else { return };
        let s = server.clone();
        self.bg(move || servers::leave(&s), move |e, told| e.rms_forget(&server, told.is_ok()));
    }

    fn rms_forget(self: &Rc<Self>, s: &Joined, told: bool) {
        if self.in_party_on(&s.address) {
            self.party_leave(true); // (its party goes with it)
        }
        self.settings.borrow_mut().servers.retain(|x| x.address != s.address);
        self.rms_save();
        self.rms_close_line(&s.address);
        self.rms.known.borrow_mut().remove(&s.address);
        crate::log::note(&format!("left {} ({})", s.name, s.address));
        self.rms.note.replace(None);
        self.rms_publish();
        self.toast(rw::left_toast(&s.name, told));
    }

    /// The same server (its id) at a new address: kept as it was (its key, its songs' keys) at the new one.
    fn rms_moved(self: &Rc<Self>, was: &Joined, now: &Joined) {
        if self.in_party_on(&was.address) {
            self.party_leave(true); // (its controls would go to the old address)
        }
        crate::log::note(&format!("{} moved from {} to {}", now.name, was.address, now.address));
        if let Some(s) = self.settings.borrow_mut().servers.iter_mut().find(|s| s.address == was.address) {
            *s = now.clone();
        }
        self.rms_save();
        self.rms_close_line(&was.address);
        let known = self.rms.known.borrow_mut().remove(&was.address);
        if let Some(k) = known {
            self.rms.known.borrow_mut().insert(now.address.clone(), k);
        }
        self.rms_open_line(now);
        self.rms_fetch(now);
        self.toast(rw::moved_toast(&now.name, &now.address));
    }

    /// Add to NAME (a saved song's menu, beta.11, the Linux app's add_to_server): the song, its cover, lyrics and parts
    /// sent to the server; a toast says what went (or that it was there already).
    pub(super) fn rms_add_to(self: &Rc<Self>, key: &str, address: &str) {
        let Some(server) = self.rms_joined(address) else { return };
        let Some(t) = self.find_track(key) else { return };
        let t = playing::saved_copy(&self.liked.borrow(), &t);
        self.toast(rw::adding(&t.title, &server.name));
        let (s, title) = (server.clone(), t.title.clone());
        self.bg(move || servers::upload(&s, &t), move |e, added| {
            match added {
                Ok(a) if a.skipped && a.extras.is_empty() && a.parts.is_empty() => e.toast(rw::has_already(&server.name, &title)),
                // what went: its extras, its parts (when this computer had split it)
                Ok(a) if a.skipped => e.toast(rw::had_already(&server.name, &title, &rw::what_went(&a.extras, !a.parts.is_empty()))),
                Ok(a) => {
                    crate::log::note(&format!("added “{title}” to {}{}", server.name, if a.parts.is_empty() { "" } else { " with its parts" }));
                    e.toast(rw::on_it_now(&title, &server.name, !a.parts.is_empty()));
                }
                Err(err) => e.toast(err),
            }
            // (with its events line open, the server says so itself)
            if !e.rms_live(&server.address) {
                e.rms_fetch(&server);
            }
        });
    }

    /// Refresh (a server's page): asked again now.
    pub(super) fn rms_refresh(self: &Rc<Self>, address: String) {
        if let Some(s) = self.rms_joined(&address) {
            self.rms_fetch(&s);
        }
    }

    /// Servers… and each server's page, as they show now.
    fn rms_publish(&self) {
        let list = self.settings.borrow().servers.clone();
        let known = self.rms.known.borrow();
        let mut rows = Vec::new();
        let mut pages = HashMap::new();
        for s in &list {
            let k = known.get(&s.address);
            let me = k.and_then(|k| k.me.as_ref());
            let problem = k.and_then(|k| k.problem.as_ref());
            let songs = k.and_then(|k| k.songs.as_ref());
            let can = me.map(|m| [true, m.can_add, m.can_split, m.can_host]).unwrap_or([true, false, false, false]);
            let subtitle = match (me, problem) {
                (_, Some(f)) => f.words(&s.name),
                (Some(m), None) => rw::row_subtitle(&s.address, m.owner, &m.name),
                (None, None) => rw::row_checking(&s.address),
            };
            rows.push(ServerRow {
                address: s.address.clone(),
                name: s.name.clone(),
                subtitle,
                can,
                owner: me.is_some_and(|m| m.owner),
                problem: problem.map(|f| f.words(&s.name)),
                signed_out: problem.is_some_and(|f| f.signed_out()),
                known: me.is_some(),
                songs: songs.map(|l| l.len()),
            });
            let tracks: Vec<(Track, String)> = songs
                .map(|l| {
                    l.iter()
                        .map(|song| {
                            let t = song.track(s);
                            let line = match &song.added_by {
                                Some(by) => rw::added_by(&t.byline(), by),
                                None => t.byline(),
                            };
                            (t, line)
                        })
                        .collect()
                })
                .unwrap_or_default();
            pages.insert(
                s.address.clone(),
                ServerPage {
                    address: s.address.clone(),
                    name: s.name.clone(),
                    line: rw::page_sub(songs.map(|l| l.len()), me.map(|m| (m.owner, m.name.as_str()))),
                    permissions: me.map(|m| rw::permissions_line(m.owner, m.can_add, m.can_split, m.can_host)).unwrap_or_default(),
                    problem: problem.map(|f| f.words(&s.name)),
                    signed_out: problem.is_some_and(|f| f.signed_out()),
                    fetching: k.is_some_and(|k| k.fetching),
                    can_add: me.is_some_and(|m| m.can_add),
                    songs: tracks,
                    note: String::new(),
                },
            );
        }
        let state = ServersState { rows, note: self.rms.note.borrow().clone(), joining: self.rms.joining.get() };
        self.shared.view(|v| {
            v.servers = state;
            v.server_pages = pages;
        });
        self.shared.tell(Change::Servers);
        self.publish_party(); // (the party cards follow what each server lets you do)
    }
}

fn k_songs_missing(known: &HashMap<String, Known>, address: &str) -> bool {
    known.get(address).is_some_and(|k| k.songs.is_none())
}

/// A server's page as shown for `q` (its search): the songs that match, and the line over them (the Linux app's
/// filter_server; the party's line comes with stage 4c-2).
pub(super) fn page_for(mut page: ServerPage, q: &str) -> ServerPage {
    let q = q.trim();
    let all = page.songs.len();
    if !q.is_empty() {
        page.songs.retain(|(t, _)| radiance_core::track::matches(t, q));
    }
    let shown = page.songs.len();
    page.note = if page.fetching && all == 0 && page.problem.is_none() {
        rw::getting_songs(&page.name)
    } else if all == 0 && page.problem.is_some() {
        String::new() // (the problem card says why)
    } else if all == 0 && page.can_add {
        rw::no_songs_add(&page.name)
    } else if all == 0 {
        rw::no_songs(&page.name)
    } else if shown == 0 {
        rw::no_match(&page.name, q)
    } else if !q.is_empty() {
        rw::some_match(shown, all)
    } else if page.problem.is_some() {
        rw::as_last_sent(&page.name)
    } else {
        rw::SONGS_HINT.to_string()
    };
    page
}

/// A song's menu's server section (the Linux menu's): Save to Your Library for a server's song not saved here; Add to
/// NAME, for one of your saved songs, on each joined server that lets you add songs (or hasn't said yet), never its own.
pub(super) fn server_items(t: &Track, liked: bool, saved: bool, rows: &[ServerRow]) -> Vec<radiance_core::menu::Item> {
    use radiance_core::menu::{Action, Item};
    let mut v = vec![];
    if t.source == Source::Server && t.file.is_none() && !liked {
        v.push(Item { label: rw::SAVE_TO_LIBRARY.to_string(), action: Action::Like });
    }
    if saved {
        let own = servers::of_track(t).map(|s| s.address);
        for r in rows.iter().filter(|r| (!r.known || r.can[1]) && own.as_deref() != Some(r.address.as_str()) && !t.id.starts_with(&format!("{}/", r.address))) {
            v.push(Item { label: rw::add_to(&r.name), action: Action::AddToServer(r.address.clone()) });
        }
    }
    v
}
