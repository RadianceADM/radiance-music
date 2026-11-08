//! Listening parties for the Windows beta (stage 4c-2), as the Linux app's (src/ui/rms.rs: start_party, join_party,
//! party_news, party_sync, party_load, party_listen, party_play_pause, party_skip, party_seek, party_act, add_to_party,
//! leave_party): a server's party card, joining one (your own music waits and comes back as it was when you leave),
//! the player kept in step with everyone by the server's clock (servers::step: a jump, or a wait just that long), its
//! controls (the host's, or everyone's when the host allows), reactions, hearing it here or not, and the end.
//! The words are core's (server_words), the protocol the engine's (servers.rs).

use super::*;
use crate::servers::{self, Failure, Joined, Live, Me, Party};
use radiance_core::server_words as rw;

/// A song whose jumps land early is jumped this much (s) past the spot, then waits.
const PAST_THE_SPOT: f64 = 1.0;
/// In a party, how often the player is checked against it.
const PARTY_CHECK: Duration = Duration::from_secs(1);
/// A shown server's party, asked about this often while its events line is down.
const PARTY_LOOK: Duration = Duration::from_secs(10);

/// A reaction in the party (the last minute's, newest first).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PartyReaction {
    pub emoji: String,
    /// Its picture's name (data/icons/rms-react-*.svg; "face-smile-symbolic" for one the app doesn't know).
    pub icon: String,
    pub by: String,
    /// "Ana: Fire".
    pub tip: String,
}

/// A song up next in the party: its entry, the song, "artist · added by NAME", and what you can do with it.
#[derive(Clone, Debug)]
pub struct PartyNext {
    pub entry: u64,
    pub track: Track,
    pub line: String,
    pub earlier: bool,
    pub later: bool,
    pub take_out: bool,
}

/// The party you're in, as its view shows it (`server` empty: you're in none).
#[derive(Clone, Debug, Default)]
pub struct PartyState {
    pub server: String,
    pub name: String,
    pub heading: String,
    /// Playing, Paused or Quiet.
    pub pill: String,
    /// The party plays (for everyone).
    pub playing: bool,
    /// In time or catching up, and how well the server's clock is known.
    pub sync_line: String,
    pub lost: bool,
    /// What plays: the song and "artist · added by NAME".
    pub now: Option<(Track, String)>,
    /// You play, pause and skip it (the host, or everyone when the host allows).
    pub control: bool,
    pub host: bool,
    /// End the Party (the host, or the server's owner).
    pub can_end: bool,
    pub everyone_controls: bool,
    /// Heard here (off: the party plays on without you hearing it).
    pub hearing: bool,
    /// Under the controls when only the host has them ("" else).
    pub controls_note: String,
    pub reactions: Vec<PartyReaction>,
    /// "Listening (N)" and each listener's chip.
    pub listening: String,
    pub members: Vec<String>,
    pub up_next: Vec<PartyNext>,
    /// The bar's play button's tooltip.
    pub bar_tip: String,
}

/// A server page's party card (the Linux app's render_party_card) when you're not in its party.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PartyCard {
    pub heading: String,
    pub line: String,
    /// Start a Party offered (with Let everyone play, pause and skip).
    pub can_start: bool,
    /// Join's label ("" : no party to join).
    pub join: String,
    pub joining: bool,
    /// Start a Party asked, its answer not back yet (the button greyed out meanwhile, never after).
    pub starting: bool,
    /// "Joining leaves NAME's party." when you're in another server's.
    pub leaves: String,
    /// You're in this server's party: PartyState is its view.
    pub in_it: bool,
}

/// Your own music as it was when you joined (it comes back when you leave).
struct Before {
    queue: Vec<Track>,
    index: usize,
    current: Option<Track>,
    at: f64,
    playing: bool,
}

/// The party you're in (the Linux app's InParty).
struct InParty {
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
    /// Waiting (paused) to let the party catch up, and a song's start on the dot.
    hold: Option<glib::SourceId>,
    start: Option<glib::SourceId>,
    drift_ms: Option<f64>,
    lost: bool,
    before: Option<Before>,
    checks: u32,
    /// The party's song that wouldn't play here (its entry, when), tried again after RETRY_FAILED, and the entry
    /// whose failure was told (one toast a song, never one a second).
    failed: Option<(u64, Instant)>,
    told: Option<u64>,
}

/// A party song that wouldn't play here is tried again this long after (not every second).
const RETRY_FAILED: Duration = Duration::from_secs(5);

/// The parties' part of the engine.
#[derive(Default)]
pub(super) struct Parties {
    now: RefCell<Option<InParty>>,
    /// Bumped as you join or leave (a left party's news and checks are let go).
    gen: Cell<u64>,
    joining: Cell<bool>,
    /// Start a Party asked on this server, its answer not back yet.
    starting: RefCell<Option<String>>,
    /// Each joined server's party as last asked (its card, when you're not in it).
    known: RefCell<HashMap<String, Party>>,
    /// The server whose page is open (its party asked about while its events line is down).
    shown: RefCell<Option<String>>,
    watching: Cell<bool>,
}

impl Engine {
    pub(super) fn in_party(&self) -> bool {
        self.party.now.borrow().is_some()
    }

    pub(super) fn in_party_on(&self, address: &str) -> bool {
        self.party.now.borrow().as_ref().is_some_and(|l| l.server.address == address)
    }

    /// The bar in a party: (you control it, a song is in it, it plays and is heard here).
    pub(super) fn party_bar(&self) -> Option<(bool, bool, bool)> {
        let link = self.party.now.borrow();
        let l = link.as_ref()?;
        Some((l.party.can_control(l.me.person), l.party.song.is_some(), l.party.playing && l.listening))
    }

    /// A server's party asked for (its card says what's on).
    pub(super) fn party_ask(self: &Rc<Self>, server: &Joined) {
        let (s, address) = (server.clone(), server.address.clone());
        self.bg(move || servers::party(&s), move |e, p| {
            if let Ok(p) = p {
                e.party_known(&address, p);
            }
        });
    }

    pub(super) fn party_known(&self, address: &str, p: Party) {
        let changed = self.party.known.borrow().get(address) != Some(&p);
        if changed {
            self.party.known.borrow_mut().insert(address.to_string(), p);
            self.publish_party();
        }
    }

    /// The server page open now ("" : none): its party is asked about now, and every PARTY_LOOK while its events
    /// line is down.
    pub(super) fn party_shown(self: &Rc<Self>, address: String) {
        let shown = (!address.is_empty()).then_some(address);
        self.party.shown.replace(shown.clone());
        let Some(address) = shown else { return };
        if let Some(s) = self.rms_joined(&address) {
            self.party_ask(&s);
        }
        if self.party.watching.replace(true) {
            return;
        }
        let me = Rc::downgrade(self);
        glib::timeout_add_local(PARTY_LOOK, move || {
            let Some(e) = me.upgrade() else { return glib::ControlFlow::Break };
            let Some(address) = e.party.shown.borrow().clone() else {
                e.party.watching.set(false);
                return glib::ControlFlow::Break;
            };
            // (in it, its WebSocket says; its events line says when one starts or ends)
            if !e.in_party_on(&address) && !e.rms_live(&address) {
                if let Some(s) = e.rms_joined(&address) {
                    e.party_ask(&s);
                }
            }
            glib::ControlFlow::Continue
        });
    }

    /// Start a Party: you're in it as its host at once.
    pub(super) fn party_start(self: &Rc<Self>, address: String, everyone: bool) {
        let Some(server) = self.rms_joined(&address) else { return };
        self.party.starting.replace(Some(address));
        self.publish_party();
        let s = server.clone();
        self.bg(move || servers::party_start(&s, everyone), move |e, started: Result<Party, Failure>| {
            e.party.starting.replace(None);
            match started {
                Ok(p) => {
                    e.party_known(&server.address, p);
                    e.toast(rw::PARTY_ON_TOAST.to_string());
                    e.party_join(server.address.clone());
                }
                Err(f) => e.toast(f.words(&server.name)),
            }
            e.publish_party(); // (Start a Party can be pressed again)
        });
    }

    /// End the Party, for everyone (the app asked first; its WebSocket says it's over, and everyone goes back).
    pub(super) fn party_end(self: &Rc<Self>) {
        let Some(server) = self.party.now.borrow().as_ref().map(|l| l.server.clone()) else { return };
        let s = server.clone();
        self.bg(move || servers::party_end(&s), move |e, ended| {
            if let Err(f) = ended {
                e.toast(f.words(&server.name));
            }
        });
    }

    /// Joins a server's party: who you are there and its clock measured, its WebSocket opened; your own music waits.
    pub(super) fn party_join(self: &Rc<Self>, address: String) {
        let Some(server) = self.rms_joined(&address) else { return };
        if self.party.joining.replace(true) {
            return;
        }
        self.publish_party();
        let s = server.clone();
        self.bg(
            move || {
                let who = servers::me(&s)?;
                let clock = servers::clock_offset(&s);
                Ok::<_, Failure>((who, clock))
            },
            move |e, found: Result<(Me, Option<(f64, f64)>), Failure>| {
                e.party.joining.set(false);
                let (who, clock) = match found {
                    Ok(x) => x,
                    Err(f) => {
                        e.toast(f.words(&server.name));
                        return e.publish_party();
                    }
                };
                let Some((offset, rtt)) = clock else {
                    e.toast(rw::not_answering_try_again(&server.name));
                    return e.publish_party();
                };
                // another server's party: this one now, and your own music as it was before the first (the session
                // file has it already: never overwritten by that party's song)
                let switching = e.in_party();
                let carried = e.party_out();
                if !switching {
                    e.save_session();
                }
                // your own music, as it is (it comes back when you leave)
                let before = carried.unwrap_or_else(|| {
                    let player = e.player.borrow();
                    let q = e.queue.borrow();
                    Before {
                        queue: q.tracks().to_vec(),
                        index: q.index(),
                        current: e.current.borrow().clone(),
                        at: player.as_ref().and_then(|p| p.position()).or(e.restore_at.get()).unwrap_or(e.position.get()),
                        playing: !e.restored.get() && player.as_ref().is_some_and(|p| p.playing()),
                    }
                });
                e.let_go_of_video();
                if let Some(p) = e.player.borrow().as_ref() {
                    p.stop();
                }
                e.restored.set(false);
                let gen = e.party.gen.get() + 1;
                e.party.gen.set(gen);
                let (tx, rx) = async_channel::unbounded::<Live>();
                let socket = servers::listen(&server, tx);
                crate::log::note(&format!("joined {}'s party (clock {offset:+.0} ms, ±{:.0} ms)", server.name, rtt / 2.0));
                let party = e.party.known.borrow().get(&server.address).cloned().unwrap_or_default();
                e.party.now.replace(Some(InParty {
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
                    start: None,
                    drift_ms: None,
                    lost: false,
                    before: Some(before),
                    checks: 0,
                    failed: None,
                    told: None,
                }));
                let w = Rc::downgrade(e);
                glib::spawn_future_local(async move {
                    while let Ok(live) = rx.recv().await {
                        let Some(e) = w.upgrade() else { break };
                        if e.party.gen.get() != gen {
                            break; // left (or another party since)
                        }
                        e.party_news(live);
                    }
                });
                // in step: checked every second; the clock measured again once a minute
                let w = Rc::downgrade(e);
                glib::timeout_add_local(PARTY_CHECK, move || {
                    let Some(e) = w.upgrade() else { return glib::ControlFlow::Break };
                    if e.party.gen.get() != gen || !e.in_party() {
                        return glib::ControlFlow::Break;
                    }
                    e.party_sync();
                    e.publish_party();
                    let again = e.party.now.borrow_mut().as_mut().and_then(|l| {
                        l.checks += 1;
                        (l.checks % 60 == 0).then(|| l.server.clone())
                    });
                    if let Some(s) = again {
                        e.bg(move || servers::clock_offset(&s), move |e, c| {
                            if e.party.gen.get() != gen {
                                return; // (another party since: not its clock)
                            }
                            if let (Some((offset, rtt)), Some(l)) = (c, e.party.now.borrow_mut().as_mut()) {
                                l.offset = offset;
                                l.rtt = rtt;
                            }
                        });
                    }
                    glib::ControlFlow::Continue
                });
                e.party_sync();
                e.publish_queue();
                e.publish_now();
                e.publish_party();
            },
        );
    }

    /// What the party's WebSocket said.
    fn party_news(self: &Rc<Self>, live: Live) {
        match live {
            Live::State(v) => {
                let p = servers::parse_party(&v);
                let Some(address) = self.party.now.borrow().as_ref().map(|l| l.server.address.clone()) else { return };
                if !p.on {
                    // (the host who ended it is told no news of their own)
                    let host = self.party.now.borrow().as_ref().filter(|l| l.party.host.0 != l.me.person).map(|l| l.party.host.1.clone()).unwrap_or_default();
                    self.party_leave(true);
                    self.toast(rw::party_ended(&host));
                    self.party_known(&address, p);
                    return;
                }
                if let Some(l) = self.party.now.borrow_mut().as_mut() {
                    l.party = p.clone();
                    l.lost = false;
                }
                self.party.known.borrow_mut().insert(address, p);
                self.party_sync();
                self.publish_now();
                self.publish_party();
            }
            Live::Removed(why) => {
                self.party_leave(true);
                self.toast(rw::removed_from_party(&why));
            }
            Live::Lost | Live::Back => {
                if let Some(l) = self.party.now.borrow_mut().as_mut() {
                    l.lost = matches!(live, Live::Lost);
                }
                self.publish_party();
            }
            // (the events line's: not on the party's)
            Live::Hello(..) | Live::Library | Live::Queue(_) | Live::Me | Live::Party => {}
        }
    }

    /// Plays the party's song where everyone is (server time, through the clock offset): loads it when it's new,
    /// starts it on the dot when it's still to start, and, more than servers::IN_STEP off, jumps (or, a little ahead,
    /// waits just that long: a song whose jumps land early is jumped past the spot, then waits). The Linux app's.
    fn party_sync(self: &Rc<Self>) {
        let (song, party, server, entry, listening, offset, lead, jumped, holding) = {
            let link = self.party.now.borrow();
            let Some(l) = link.as_ref() else { return };
            (l.party.song.clone(), l.party.clone(), l.server.clone(), l.entry, l.listening, l.offset, l.lead, l.jumped, l.hold.is_some())
        };
        // the party's song that wouldn't play here: left alone a moment, then loaded again (not tried every second)
        let failed = self.party.now.borrow().as_ref().and_then(|l| l.failed);
        if let (Some((was, when)), Some(now)) = (failed, song.as_ref()) {
            if was == now.entry && when.elapsed() < RETRY_FAILED {
                return;
            }
            if let Some(l) = self.party.now.borrow_mut().as_mut() {
                l.failed = None;
                if was == now.entry {
                    l.entry = None; // (loaded again below)
                }
            }
            return self.party_sync();
        }
        let player = self.player.borrow();
        let Some(p) = player.as_ref() else { return };
        let Some(song) = song else {
            // nothing playing in the party: quiet here too
            if entry.is_some() {
                p.stop();
                if let Some(l) = self.party.now.borrow_mut().as_mut() {
                    l.entry = None;
                }
                drop(player);
                self.loading.set(false);
                self.publish_now();
            }
            return;
        };
        let want = party.position_ms(servers::now_ms() + offset).unwrap_or(0.0) / 1000.0;
        let length = song.duration_ms.map(|d| d / 1000.0);
        if entry != Some(song.entry) {
            // a new song: loaded where the party is (or at its start, to begin on the dot)
            if let Some(l) = self.party.now.borrow_mut().as_mut() {
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
            let near = 0.3 + self.party.now.borrow().as_ref().map(|l| l.slack).unwrap_or(0.0);
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
            let no_start = self.party.now.borrow().as_ref().is_some_and(|l| l.start.is_none());
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
            let slack = self.party.now.borrow().as_ref().map(|l| l.slack).unwrap_or(0.0);
            p.seek(want + lead + slack);
            p.set_playing(true);
            if let Some(l) = self.party.now.borrow_mut().as_mut() {
                l.jumped = Some(Instant::now());
                l.seeked = true;
            }
            drop(player);
            self.publish_now();
            return;
        }
        let Some(have) = p.position() else { return };
        if jumped.is_some_and(|j| j.elapsed() < Duration::from_millis(400)) {
            return; // a jump still landing
        }
        let drift = have - want;
        let mut link = self.party.now.borrow_mut();
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
            crate::log::note(&format!("[party] here {have:.3} s, the party {want:.3} s ({:+.0} ms): {step:?}", drift * 1000.0));
        }
        if step == servers::Step::Jump {
            p.seek(want + l.lead + l.slack);
            l.jumped = Some(Instant::now());
            l.seeked = true;
        } else if let servers::Step::Wait(wait) = step {
            p.set_playing(false);
            let w = Rc::downgrade(self);
            let id = glib::timeout_add_local_once(Duration::from_secs_f64(wait), move || {
                let Some(e) = w.upgrade() else { return };
                let go = {
                    let mut link = e.party.now.borrow_mut();
                    let Some(l) = link.as_mut() else { return };
                    l.hold = None;
                    l.jumped = Some(Instant::now());
                    l.seeked = false;
                    l.listening && l.party.playing
                };
                if go {
                    if let Some(p) = e.player.borrow().as_ref() {
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
            let Some(e) = w.upgrade() else { return };
            let go = {
                let mut link = e.party.now.borrow_mut();
                let Some(l) = link.as_mut() else { return };
                l.start = None;
                l.jumped = Some(Instant::now());
                l.seeked = false;
                l.listening && l.party.playing
            };
            if go {
                if let Some(p) = e.player.borrow().as_ref() {
                    p.set_playing(true);
                }
                e.publish_now();
            }
        });
        if let Some(l) = self.party.now.borrow_mut().as_mut() {
            if let Some(old) = l.start.replace(id) {
                old.remove();
            }
        }
    }

    /// The party's song into the player (shown in the bar and Now Playing like any song).
    fn party_load(self: &Rc<Self>, t: &Track, from: f64, play: bool) {
        self.replan();
        let gen = self.play_gen.get() + 1;
        self.play_gen.set(gen);
        self.kbps.set(0);
        self.restored.set(false);
        self.restore_at.set(None);
        self.queue.borrow_mut().restore(vec![t.clone()], 0);
        *self.current.borrow_mut() = Some(t.clone());
        self.position.set(from);
        self.load_lyrics(t, gen);
        self.note_played(t);
        self.current_uri.replace(String::new());
        let uri = playing::plain_uri(t, self.relay.as_ref(), &self.streams.borrow());
        let Some(uri) = uri else {
            // (never the last song playing on under this one's name)
            if let Some(p) = self.player.borrow().as_ref() {
                p.stop();
            }
            self.loading.set(false);
            self.party_failed(rw::LEFT_ITS_SERVER);
            self.publish_now();
            return;
        };
        self.loading.set(true);
        *self.quality.borrow_mut() = quality::text(Heard::Loading);
        self.current_uri.replace(uri.clone());
        if let Some(p) = self.player.borrow().as_ref() {
            p.load_at(&uri, from);
            if !play {
                p.set_playing(false); // ready, quiet until it's time
            }
        }
        if play {
            if let Some(l) = self.party.now.borrow_mut().as_mut() {
                l.jumped = Some(Instant::now());
                l.seeked = true;
            }
        }
        self.publish_queue();
        self.publish_now();
    }

    /// Hearing the party here, or not (it plays on); back on, in step at once.
    pub(super) fn party_hear(self: &Rc<Self>, on: bool) {
        if let Some(l) = self.party.now.borrow_mut().as_mut() {
            l.listening = on;
            l.jumped = None;
            if let Some(id) = l.hold.take() {
                id.remove();
            }
        }
        self.party_sync();
        self.publish_now();
        self.publish_party();
    }

    /// The bar's play button in a party: play / pause for everyone (when you control it), else hearing it here or not.
    pub(super) fn party_play_pause(self: &Rc<Self>) {
        let (control, playing, listening) = {
            let link = self.party.now.borrow();
            let Some(l) = link.as_ref() else { return };
            (l.party.can_control(l.me.person), l.party.playing, l.listening)
        };
        if control && listening {
            self.party_act(if playing { "pause" } else { "play" }, serde_json::json!({}));
        } else {
            self.party_hear(!listening);
        }
    }

    /// Next in a party: skips for everyone (when you control it); there's no going back.
    pub(super) fn party_skip(self: &Rc<Self>, by: i32) {
        let (control, host) = {
            let link = self.party.now.borrow();
            let Some(l) = link.as_ref() else { return };
            (l.party.can_control(l.me.person), l.party.host.1.clone())
        };
        if by > 0 && control {
            self.party_act("skip", serde_json::json!({}));
        } else if by > 0 {
            self.toast(rw::host_skips(&host));
        }
    }

    /// The seek bar in a party: for everyone (when you control it); else it stays where the party is.
    pub(super) fn party_seek(self: &Rc<Self>, at: f64) {
        let control = self.party.now.borrow().as_ref().is_some_and(|l| l.party.can_control(l.me.person));
        if control {
            self.party_act("seek", serde_json::json!({ "position_ms": (at.max(0.0) * 1000.0) as i64 }));
        } else {
            self.publish_now(); // (the bar goes back to where the party is)
        }
    }

    /// A control for everyone in the party (play, pause, skip, seek, reorder, remove, allow).
    pub(super) fn party_act(self: &Rc<Self>, action: &'static str, extra: serde_json::Value) {
        let Some(server) = self.party.now.borrow().as_ref().map(|l| l.server.clone()) else { return };
        let (s, gen) = (server.clone(), self.party.gen.get());
        self.bg(move || servers::party_control(&s, action, extra), move |e, done: Result<Party, Failure>| match done {
            _ if e.party.gen.get() != gen => {} // (left, or another party since: not its state)
            Ok(p) => {
                if let Some(l) = e.party.now.borrow_mut().as_mut() {
                    l.party = p;
                }
                e.party_sync();
                e.publish_now();
                e.publish_party();
            }
            Err(f) => e.toast(f.words(&server.name)),
        });
    }

    pub(super) fn party_react(self: &Rc<Self>, emoji: String) {
        let Some(server) = self.party.now.borrow().as_ref().map(|l| l.server.clone()) else { return };
        let s = server.clone();
        self.bg(move || servers::party_react(&s, &emoji), move |e, done| {
            if let Err(f) = done {
                e.toast(f.words(&server.name));
            }
        });
    }

    /// A server's song into its party's queue (it starts at once when nothing plays).
    pub(super) fn party_add(self: &Rc<Self>, t: Track) {
        let Some(server) = servers::of_track(&t) else { return };
        let (s, gen) = (server.clone(), self.party.gen.get());
        let title = t.title.clone();
        self.bg(move || servers::party_add(&s, &t), move |e, done: Result<Party, Failure>| match done {
            _ if e.party.gen.get() != gen => {} // (left, or another party since)
            Ok(p) => {
                if let Some(l) = e.party.now.borrow_mut().as_mut() {
                    l.party = p;
                }
                e.party_sync();
                e.publish_party();
                e.toast(rw::added_to_party(&title));
            }
            Err(f) => e.toast(f.words(&server.name)),
        });
    }

    /// Out of the party (its WebSocket closes). `back`: your own music as it was.
    pub(super) fn party_leave(self: &Rc<Self>, back: bool) {
        if !self.in_party() {
            return;
        }
        let before = self.party_out();
        if let (true, Some(b)) = (back, before) {
            if let Some(t) = b.current {
                self.queue.borrow_mut().set_tracks(b.queue, b.index);
                let gen = self.play_gen.get() + 1;
                self.play_gen.set(gen);
                *self.current.borrow_mut() = Some(t.clone());
                self.load_lyrics(&t, gen);
                self.restored.set(true);
                self.restore_at.set((b.at > 1.0).then_some(b.at));
                self.position.set(b.at);
                if b.playing {
                    self.toggle_play(); // on from where it was
                }
            } else {
                self.queue.borrow_mut().set_tracks(b.queue, b.index);
                self.current.replace(None);
            }
        }
        self.publish_queue();
        self.publish_now();
        self.publish_party();
    }

    /// Out of the party (its WebSocket closes, its timers stop, the player quiet): your own music as it was when you
    /// joined (None: not in a party).
    fn party_out(self: &Rc<Self>) -> Option<Before> {
        let mut link = self.party.now.borrow_mut().take()?;
        self.party.gen.set(self.party.gen.get() + 1);
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
        self.loading.set(false);
        self.quality.borrow_mut().clear();
        link.before.take()
    }

    /// The party's song won't play, or ended (true: handled here). A song's end waits for the server's next one.
    pub(super) fn party_event(self: &Rc<Self>, ev: &crate::player::Event) -> bool {
        if !self.in_party() {
            return false;
        }
        match ev {
            crate::player::Event::Ready => false,
            crate::player::Event::Ended => true,
            crate::player::Event::Error(e) => {
                let (reason, _) = trouble::why_not(e, self.shared.view(|v| v.online));
                self.loading.set(false);
                self.party_failed(reason);
                self.publish_now();
                true
            }
        }
    }

    /// The party's song won't play here: said once for that song, and tried again after RETRY_FAILED.
    fn party_failed(&self, reason: &str) {
        let tell = {
            let mut link = self.party.now.borrow_mut();
            let Some(l) = link.as_mut() else { return };
            let Some(entry) = l.entry.or(l.party.song.as_ref().map(|s| s.entry)) else { return };
            l.failed = Some((entry, Instant::now()));
            l.told.replace(entry) != Some(entry)
        };
        if tell {
            self.toast(rw::party_song_cant_play(reason));
        }
    }

    /// The party's line: in time or catching up, and how well the clock is known (the Linux app's show_sync).
    fn party_sync_line(l: &InParty) -> String {
        if l.lost {
            return rw::LOST_PARTY.to_string();
        }
        let clock = ((l.rtt / 2.0).round() as i64).max(1);
        let text = if !l.listening {
            rw::NOT_HEARD_HERE.to_string()
        } else if l.party.song.is_none() {
            rw::WAITING_FOR_SONG.to_string()
        } else if !l.party.playing {
            rw::paused_by(l.party.host.0 == l.me.person, &l.party.host.1)
        } else {
            match (l.party.position_ms(servers::now_ms() + l.offset), l.drift_ms) {
                (Some(p), _) if p < 0.0 => rw::STARTING.to_string(),
                (_, Some(d)) if d.abs() <= servers::IN_STEP * 1000.0 => rw::in_time(d),
                (_, Some(d)) => rw::catching_up(d),
                _ => rw::GETTING_IN_STEP.to_string(),
            }
        };
        rw::sync_line(&text, clock)
    }

    /// The party view and every server page's party card, as they show now.
    pub(super) fn publish_party(&self) {
        let state = match self.party.now.borrow().as_ref() {
            None => PartyState::default(),
            Some(l) => {
                let p = &l.party;
                let me = l.me.person;
                let control = p.can_control(me);
                let host = p.host.0 == me;
                let now_ms = servers::now_ms() + l.offset;
                let n = p.queue.len();
                PartyState {
                    server: l.server.address.clone(),
                    name: l.server.name.clone(),
                    heading: rw::in_party_heading(host, &p.host.1),
                    pill: rw::party_pill(p.playing, p.song.is_some()).to_string(),
                    playing: p.playing,
                    sync_line: Self::party_sync_line(l),
                    lost: l.lost,
                    now: p.song.as_ref().map(|e| (e.track(&l.server), rw::added_by(&e.artist, &e.added_by.1))),
                    control,
                    host,
                    can_end: host || l.me.owner,
                    everyone_controls: p.everyone_controls,
                    hearing: l.listening,
                    controls_note: if control { String::new() } else { rw::host_controls(&p.host.1) },
                    // the last minute's, newest first
                    reactions: p
                        .reactions
                        .iter()
                        .rev()
                        .filter(|r| now_ms - r.at_server_ms < 60_000.0)
                        .take(6)
                        .map(|r| {
                            let (icon, name) = servers::reaction_icon(&r.emoji);
                            PartyReaction { emoji: r.emoji.clone(), icon: icon.to_string(), by: r.by.clone(), tip: rw::reaction_tip(&r.by, name) }
                        })
                        .collect(),
                    listening: rw::listening_heading(p.members.len()),
                    members: p.members.iter().map(|m| rw::member(&m.name, m.host)).collect(),
                    up_next: p
                        .queue
                        .iter()
                        .enumerate()
                        .map(|(i, e)| PartyNext {
                            entry: e.entry,
                            track: e.track(&l.server),
                            line: rw::added_by(&e.artist, &e.added_by.1),
                            earlier: control && i > 0,
                            later: control && i + 1 < n,
                            take_out: control || e.added_by.0 == me,
                        })
                        .collect(),
                    bar_tip: rw::bar_tooltip(control, p.playing && l.listening).to_string(),
                }
            }
        };
        let cards: HashMap<String, PartyCard> = self.settings.borrow().servers.iter().map(|s| (s.address.clone(), self.party_card_for(&s.address))).collect();
        self.shared.view(|v| {
            v.party = state;
            v.party_cards = cards;
        });
        self.shared.tell(Change::Party);
    }

    /// A server page's party card (the Linux app's render_party_card).
    fn party_card_for(&self, address: &str) -> PartyCard {
        let Some(server) = self.rms_joined(address) else { return PartyCard::default() };
        if self.in_party_on(address) {
            return PartyCard { in_it: true, ..Default::default() };
        }
        let who = self.rms_me(address);
        let other = self.party.now.borrow().as_ref().map(|l| rw::joining_leaves(&l.party.host.1)).unwrap_or_default();
        match self.party.known.borrow().get(address) {
            None => PartyCard { heading: rw::PARTY_HEADING.into(), line: rw::asking_party(&server.name), ..Default::default() },
            Some(p) if !p.on => {
                let can = who.as_ref().is_none_or(|m| m.can_host);
                PartyCard {
                    heading: rw::PARTY_HEADING.into(),
                    line: if can { rw::NO_PARTY.into() } else { format!("{} {}", rw::NO_PARTY, rw::hosting_off(&server.name)) },
                    can_start: can,
                    starting: self.party.starting.borrow().as_deref() == Some(address),
                    ..Default::default()
                }
            }
            Some(p) => {
                let yours = who.as_ref().is_some_and(|m| m.person == p.host.0);
                let now = match &p.song {
                    Some(e) => rw::playing(&e.title, &e.artist),
                    None => rw::NOTHING_YET.to_string(),
                };
                PartyCard {
                    heading: rw::party_heading(yours, &p.host.1),
                    line: rw::party_line(&now, &rw::listening(p.members.len())),
                    join: if yours { rw::JOIN_YOUR_PARTY } else { rw::JOIN_THE_PARTY }.to_string(),
                    joining: self.party.joining.get(),
                    starting: self.party.starting.borrow().as_deref() == Some(address),
                    leaves: other,
                    ..Default::default()
                }
            }
        }
    }
}
