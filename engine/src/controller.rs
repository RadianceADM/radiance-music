//! The engine as one controller, for an app that isn't GTK (the Windows app, issue #45):
//! commands in (search, play this list, pause, …), a state to read and change events out.
//!
//! It runs on a thread of its own with a GLib main context of its own (GStreamer's bus and the
//! player's timers need one): commands are posted to it, background work comes back to it, and
//! what an app reads is a snapshot behind a lock. One controller at a time per process.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use radiance_core::search;
use radiance_core::words::{self, Kind};

use crate::sources::Sources;
use crate::track::Track;

/// What changed (read it again with the getter of the same name).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Change {
    Now,
    Position,
    Queue,
    Results,
    Home,
    Lyrics,
    Online,
    Problem,
}

/// Told when something changed: on the engine's thread, so it must hand it on quickly.
pub trait Listener: Send + Sync + 'static {
    fn changed(&self, what: Change);
}

/// A list on screen that a song is played from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListRef {
    /// Search: your liked songs that match.
    Mine,
    /// Search: the free lossless libraries.
    Lossless,
    /// Search: YouTube Music.
    Songs,
    /// Home: Made for you.
    Mix,
    /// Home: Recently played.
    Recent,
    /// Home: Your liked songs.
    Liked,
    /// The queue.
    Queue,
}

/// Search's lists for `query` (None: still being searched).
#[derive(Clone, Debug, Default)]
pub struct Results {
    pub query: String,
    pub mine: Vec<Track>,
    pub lossless: Option<Vec<Track>>,
    pub songs: Option<Vec<Track>>,
}

/// Home: the greeting, the mix's note, and its rows.
#[derive(Clone, Debug, Default)]
pub struct HomeState {
    pub greeting: String,
    pub mix_note: String,
    pub mix: Vec<Track>,
    pub recent: Vec<Track>,
    pub liked: Vec<Track>,
}

static RUNNING: AtomicBool = AtomicBool::new(false);

/// What an app reads (written on the engine's thread).
struct View {
    results: Results,
    home: HomeState,
    online: bool,
    problem: Option<String>,
}

/// Between the app's threads and the engine's.
pub(crate) struct Shared {
    listener: Arc<dyn Listener>,
    pub(crate) sources: Arc<dyn Sources>,
    quitting: AtomicBool,
    view: Mutex<View>,
}

impl Shared {
    /// Tells the app (never once it's quitting).
    fn tell(&self, what: Change) {
        if !self.quitting.load(Ordering::SeqCst) {
            self.listener.changed(what);
        }
    }

    fn view<T>(&self, f: impl FnOnce(&mut View) -> T) -> T {
        f(&mut self.view.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

/// The engine running on its thread. Every method of it runs there.
pub struct Controller {
    shared: Arc<Shared>,
    context: glib::MainContext,
    thread: Mutex<Option<JoinHandle<()>>>,
}

thread_local! {
    static ENGINE: RefCell<Option<Rc<Engine>>> = const { RefCell::new(None) };
}

/// Runs `f` with the engine (on its thread; nothing once it's gone).
fn with_engine(f: impl FnOnce(&Rc<Engine>)) {
    if let Some(e) = ENGINE.with(|x| x.borrow().clone()) {
        f(&e);
    }
}

impl Controller {
    /// Starts the engine: its thread, Home's lists. Err when one runs already.
    pub fn start(listener: Arc<dyn Listener>, sources: Arc<dyn Sources>) -> Result<Controller, String> {
        if RUNNING.swap(true, Ordering::SeqCst) {
            return Err("Radiance Music's engine is already running".into());
        }
        let shared = Arc::new(Shared {
            listener,
            sources,
            quitting: AtomicBool::new(false),
            view: Mutex::new(View { results: Results::default(), home: HomeState::default(), online: true, problem: None }),
        });
        let context = glib::MainContext::new();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<()>();
        let (s, ctx) = (shared.clone(), context.clone());
        let thread = std::thread::Builder::new()
            .name("radiance-engine".into())
            .spawn(move || {
                let _ = ctx.clone().with_thread_default(move || {
                    let main = glib::MainLoop::new(Some(&ctx), false);
                    let engine = Rc::new(Engine::new(s, ctx, main.clone()));
                    ENGINE.with(|e| *e.borrow_mut() = Some(engine.clone()));
                    engine.start_up();
                    let _ = ready_tx.send(());
                    main.run();
                    engine.shut_down();
                    ENGINE.with(|e| e.borrow_mut().take());
                });
            })
            .map_err(|e| {
                RUNNING.store(false, Ordering::SeqCst);
                e.to_string()
            })?;
        if ready_rx.recv().is_err() {
            let _ = thread.join();
            RUNNING.store(false, Ordering::SeqCst);
            return Err("Radiance Music's engine didn't start".into());
        }
        Ok(Controller { shared, context, thread: Mutex::new(Some(thread)) })
    }

    /// Posts `f` to the engine's thread (dropped once it's quitting).
    fn post(&self, f: impl FnOnce(&Rc<Engine>) + Send + 'static) {
        if !self.shared.quitting.load(Ordering::SeqCst) {
            self.context.invoke(move || with_engine(f));
        }
    }

    /// Searches (the free lossless libraries and YouTube Music); fewer than 2 letters clear it.
    pub fn search(&self, q: &str) {
        let q = q.to_string();
        self.post(move |e| e.search(q));
    }

    /// Home's mix made again.
    pub fn refresh_mix(&self) {
        self.post(|e| e.refresh_mix());
    }

    pub fn results(&self) -> Results {
        self.shared.view(|v| v.results.clone())
    }

    pub fn home(&self) -> HomeState {
        self.shared.view(|v| v.home.clone())
    }

    /// The internet seemed reachable the last time it was asked.
    pub fn online(&self) -> bool {
        self.shared.view(|v| v.online)
    }

    /// What went wrong last (a song that won't play), in plain words.
    pub fn problem(&self) -> Option<String> {
        self.shared.view(|v| v.problem.clone())
    }

    /// Stops the engine: no more changes are told, its thread ends. Safe to call twice.
    pub fn quit(&self) {
        let Some(thread) = self.thread.lock().unwrap_or_else(|e| e.into_inner()).take() else { return };
        self.shared.quitting.store(true, Ordering::SeqCst);
        self.context.invoke(|| with_engine(|e| e.main.quit()));
        let _ = thread.join();
        RUNNING.store(false, Ordering::SeqCst);
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        self.quit();
    }
}

/// The engine's own state, on its thread.
struct Engine {
    shared: Arc<Shared>,
    context: glib::MainContext,
    main: glib::MainLoop,
    search_gen: Cell<u64>,
    query: RefCell<String>,
    /// What the search found, as found: (lossless, YouTube Music).
    found: RefCell<(Option<Vec<Track>>, Option<Vec<Track>>)>,
    liked: RefCell<Vec<Track>>,
    history: RefCell<Vec<(Track, i64)>>,
    mix: RefCell<Vec<Track>>,
    mix_gen: Cell<u64>,
}

impl Engine {
    fn new(shared: Arc<Shared>, context: glib::MainContext, main: glib::MainLoop) -> Engine {
        Engine {
            shared,
            context,
            main,
            search_gen: Cell::new(0),
            query: RefCell::default(),
            found: RefCell::new((Some(Vec::new()), Some(Vec::new()))),
            liked: RefCell::default(),
            history: RefCell::default(),
            mix: RefCell::default(),
            mix_gen: Cell::new(0),
        }
    }

    /// Work off the engine's thread; `done` back on it with what it made.
    fn bg<T: Send + 'static>(&self, work: impl FnOnce() -> T + Send + 'static, done: impl FnOnce(&Rc<Engine>, T) + Send + 'static) {
        let context = self.context.clone();
        std::thread::spawn(move || {
            let out = work();
            context.invoke(move || with_engine(|e| done(e, out)));
        });
    }

    fn start_up(self: &Rc<Self>) {
        *self.liked.borrow_mut() = crate::db::liked();
        *self.history.borrow_mut() = crate::session::load().history;
        self.publish_home();
        self.refresh_mix();
    }

    fn shut_down(&self) {}

    // ------------------------------------------------------------------ search

    fn search(self: &Rc<Self>, q: String) {
        let q = q.trim().to_string();
        let gen = self.search_gen.get() + 1;
        self.search_gen.set(gen);
        *self.query.borrow_mut() = q.clone();
        if q.chars().count() < 2 {
            *self.found.borrow_mut() = (Some(Vec::new()), Some(Vec::new()));
            self.publish_results();
            return;
        }
        *self.found.borrow_mut() = (None, None);
        self.publish_results();
        for kind in [Kind::Lossless, Kind::Songs] {
            let sources = self.shared.sources.clone();
            let q = q.clone();
            self.bg(
                move || {
                    let found = sources.search(kind, &q);
                    // YouTube Music found nothing: is it the song, or the internet?
                    let reached = kind != Kind::Songs || !found.is_empty() || sources.reachable();
                    (found, reached)
                },
                move |e, (found, reached)| {
                    if kind == Kind::Songs {
                        e.set_online(reached);
                    }
                    if e.search_gen.get() != gen {
                        return; // typed on since
                    }
                    {
                        let mut f = e.found.borrow_mut();
                        if kind == Kind::Lossless {
                            f.0 = Some(found);
                        } else {
                            f.1 = Some(found);
                        }
                    }
                    e.publish_results();
                },
            );
        }
    }

    fn publish_results(&self) {
        let query = self.query.borrow().clone();
        let (lossless, songs) = self.found.borrow().clone();
        let a = search::arrange(&self.liked.borrow(), lossless, songs, None, &query);
        let mine = if query.chars().count() < 2 { Vec::new() } else { a.mine };
        self.shared.view(|v| v.results = Results { query, mine, lossless: a.lossless, songs: a.youtube });
        self.shared.tell(Change::Results);
    }

    fn set_online(&self, on: bool) {
        let changed = self.shared.view(|v| std::mem::replace(&mut v.online, on) != on);
        if changed {
            self.shared.tell(Change::Online);
        }
    }

    // ------------------------------------------------------------------ home

    fn publish_home(&self) {
        let hour = glib::DateTime::now_local().map(|d| d.hour()).unwrap_or(12).max(0) as u32;
        let name = glib::real_name().to_string_lossy().to_string();
        let liked = self.liked.borrow().clone();
        let mix = self.mix.borrow().clone();
        let home = HomeState {
            greeting: words::greeting(hour, &name),
            mix_note: words::mix_note(mix.len(), liked.len()),
            recent: crate::session::recent(&self.history.borrow()),
            mix,
            liked,
        };
        self.shared.view(|v| v.home = home);
        self.shared.tell(Change::Home);
    }

    fn refresh_mix(self: &Rc<Self>) {
        let gen = self.mix_gen.get() + 1;
        self.mix_gen.set(gen);
        let liked = self.liked.borrow().clone();
        if liked.is_empty() {
            self.mix.borrow_mut().clear();
            self.publish_home();
            return;
        }
        let history = self.history.borrow().clone();
        self.bg(
            move || crate::home::made_for_you(&liked, &history, &crate::db::counts()),
            move |e, mix| {
                if e.mix_gen.get() == gen {
                    *e.mix.borrow_mut() = mix;
                    e.publish_home();
                }
            },
        );
    }
}
