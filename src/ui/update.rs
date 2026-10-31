//! The update pop-up (owner, 2026-10-31: "all of the app doesn't have a modal
//! opening when there's an update avail ... with a remind me later option").
//! It comes up by itself a little after the start when a newer version is out
//! (or from the menu: Look for Updates): Update Now downloads it while the music
//! plays, then Restart to Update (pressed by you, never by itself) puts it on and
//! opens the new version; Remind Me Later hides it for a day; Skip This Version
//! until the next one. See crate::update for how each kind of install updates.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::*;
use crate::update::{self, FlatpakStep, Install, Release};

const UPDATE_CSS: &str = "
.update-mark { min-width: 56px; min-height: 56px; border-radius: 999px; background: alpha(#F38A35, 0.18); color: #F38A35; }
.update-body .bullet { color: #F38A35; font-weight: 800; }
button.update-main, button.update-second { min-height: 44px; border-radius: 999px; font-weight: 700; }
";

/// Where an update is.
#[derive(Clone, Debug)]
enum Step {
    Out(Release),
    Downloading(Release, f64),
    Ready(Release),
    Restarting(Release),
    Failed(Option<Release>, String),
}

impl Step {
    fn release(&self) -> Option<&Release> {
        match self {
            Step::Out(r) | Step::Downloading(r, _) | Step::Ready(r) | Step::Restarting(r) => Some(r),
            Step::Failed(r, _) => r.as_ref(),
        }
    }
}

/// The pop-up's state, for the one window (window thread only).
struct Updater {
    install: Install,
    step: RefCell<Option<Step>>,
    dialog: RefCell<Option<adw::Dialog>>,
    /// The download's progress bar and words, updated in place while it goes.
    going: RefCell<Option<(gtk::ProgressBar, gtk::Label)>>,
    /// Stops a download (Cancel the Download).
    stop: RefCell<Arc<AtomicBool>>,
    /// Update Now was pressed: the pop-up comes back when it's ready, even if hidden meanwhile.
    following: Cell<bool>,
    /// The pop-up was closed with a button (not with Esc or its close button).
    decided: Cell<bool>,
}

thread_local! {
    static UPDATER: Rc<Updater> = Rc::new(Updater {
        install: Install::here(),
        step: RefCell::new(None),
        dialog: RefCell::new(None),
        going: RefCell::new(None),
        stop: RefCell::new(Arc::new(AtomicBool::new(false))),
        following: Cell::new(false),
        decided: Cell::new(false),
    });
}

fn updater() -> Rc<Updater> {
    UPDATER.with(Rc::clone)
}

fn mb(bytes: u64) -> String {
    if bytes == 0 {
        String::new()
    } else if bytes >= 1 << 30 {
        format!("{:.1} GB", bytes as f64 / 1e9)
    } else {
        format!("{} MB", (bytes + 500_000) / 1_000_000)
    }
}

impl Ui {
    /// Looks for a newer version: at the start (`asked` false: a little after it,
    /// at most every 6 hours, and the pop-up only when it isn't put off), or from
    /// the menu (`asked`: now, and it says so when this is the newest).
    pub(super) fn look_for_updates(self: &Rc<Self>, asked: bool) {
        if let Ok(fake) = std::env::var("RADIANCE_SHOT_UPDATE") {
            return self.update_shot(&fake); // a design check of the pop-up
        }
        if std::env::var_os("RADIANCE_SHOT").is_some() {
            return; // screenshot copies keep out
        }
        let u = updater();
        if !asked {
            bg(update::clean, |_| {}); // the download that updated to this version
            // what the last look found, at once (its pop-up waits its turn), then a new look if due
            if let Some(r) = update::remembered().filter(|r| update::newer(&r.version, update::current())) {
                let step = if update::ready(&r, &u.install) { Step::Ready(r) } else { Step::Out(r) };
                self.offer(step, false);
            }
            if !update::due() {
                return;
            }
        }
        let me = Rc::downgrade(self);
        let install = u.install.clone();
        let wait = if asked { 0 } else { 8 }; // after the start, not during it
        glib::timeout_add_seconds_local_once(wait, move || {
            bg(
                move || update::latest(&install),
                move |found| {
                    let Some(me) = me.upgrade() else { return };
                    match found {
                        Ok(r) => {
                            update::looked(&r);
                            if update::newer(&r.version, update::current()) {
                                let ready = update::ready(&r, &updater().install);
                                me.offer(if ready { Step::Ready(r) } else { Step::Out(r) }, asked);
                            } else if asked {
                                me.toast(&format!("You have the newest version ({})", update::current()));
                            }
                        }
                        Err(e) => {
                            crate::log::note(&format!("updates: {e}"));
                            if asked {
                                me.toast(&format!("Couldn't look for updates: {e}"));
                            }
                        }
                    }
                },
            );
        });
    }

    /// A newer version to offer: the pop-up, unless it's put off (Remind Me
    /// Later, Skip This Version) and not asked for. It waits while the window is
    /// hidden (music playing on) or another dialog is up.
    fn offer(self: &Rc<Self>, step: Step, asked: bool) {
        let u = updater();
        // one under way (downloading, ready) stays as it is
        if matches!(&*u.step.borrow(), Some(Step::Downloading(..) | Step::Restarting(..))) {
            return;
        }
        let version = step.release().map(|r| r.version.clone()).unwrap_or_default();
        *u.step.borrow_mut() = Some(step);
        if asked || update::asks(&version) {
            self.pop_up_when_free();
        }
    }

    fn pop_up_when_free(self: &Rc<Self>) {
        // not over the first open's setup (the sunrise held) nor before its guide has been shown
        let first_open = self.setup_holding.get() || (!self.settings.borrow().guide_seen && std::env::var_os("RADIANCE_SHOT").is_none());
        if self.window.is_visible() && self.window.visible_dialog().is_none() && !self.starting.get() && !first_open {
            return self.pop_up();
        }
        let me = Rc::downgrade(self);
        glib::timeout_add_seconds_local_once(30, move || {
            if let Some(me) = me.upgrade() {
                if updater().dialog.borrow().is_none() {
                    me.pop_up_when_free();
                }
            }
        });
    }

    /// Shows the pop-up (or brings it to the step it's at).
    fn pop_up(self: &Rc<Self>) {
        let u = updater();
        if u.dialog.borrow().is_some() {
            return self.render_update();
        }
        thread_local! { static STYLED: Cell<bool> = const { Cell::new(false) }; }
        if !STYLED.with(|s| s.replace(true)) {
            let css = gtk::CssProvider::new();
            css.load_from_string(UPDATE_CSS);
            if let Some(display) = gdk::Display::default() {
                gtk::style_context_add_provider_for_display(&display, &css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
            }
        }
        let dialog = adw::Dialog::builder().title("Update").content_width(420).build();
        let view = adw::ToolbarView::new();
        view.add_top_bar(&adw::HeaderBar::builder().show_title(false).build());
        dialog.set_child(Some(&view));
        u.decided.set(false);
        dialog.connect_closed(move |_| {
            let u = updater();
            u.dialog.borrow_mut().take();
            u.going.borrow_mut().take();
            // closed without a button (Esc, its close button): Remind Me Later, before a download
            if !u.decided.get() && matches!(&*u.step.borrow(), Some(Step::Out(_) | Step::Ready(_))) {
                update::remind_later();
                u.following.set(false);
            }
        });
        *u.dialog.borrow_mut() = Some(dialog.clone());
        self.render_update();
        dialog.present(Some(&self.window));
    }

    fn close_update(&self) {
        let u = updater();
        u.decided.set(true);
        let d = u.dialog.borrow().clone();
        if let Some(d) = d {
            d.close();
        }
    }

    /// Moves the update to `step`: the pop-up (if it's up) follows.
    fn set_update_step(self: &Rc<Self>, step: Step) {
        let u = updater();
        let same_kind = matches!((&*u.step.borrow(), &step), (Some(Step::Downloading(..)), Step::Downloading(..)));
        *u.step.borrow_mut() = Some(step.clone());
        if same_kind {
            if let (Step::Downloading(r, done), Some((bar, words))) = (&step, &*u.going.borrow()) {
                bar.set_fraction(*done);
                words.set_text(&downloading_words(r, *done, &u.install));
            }
            return;
        }
        if u.dialog.borrow().is_some() {
            self.render_update();
        } else if u.following.get() && matches!(step, Step::Ready(_) | Step::Failed(..)) {
            self.pop_up_when_free(); // done while hidden: back to say so
        }
    }

    /// The pop-up's inside for the step it's at.
    fn render_update(self: &Rc<Self>) {
        let u = updater();
        let Some(dialog) = u.dialog.borrow().clone() else { return };
        let Some(step) = u.step.borrow().clone() else { return };
        let Some(view) = dialog.child().and_downcast::<adw::ToolbarView>() else { return };
        u.going.borrow_mut().take();
        let body = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(10).margin_start(28).margin_end(28).margin_bottom(28).css_classes(["update-body"]).build();
        let playing = self.player.borrow().as_ref().is_some_and(|p| p.playing());
        let me = Rc::downgrade(self);
        let act = move |f: fn(&Rc<Ui>, &Step)| {
            let me = me.clone();
            move |_: &gtk::Button| {
                let (Some(me), Some(step)) = (me.upgrade(), updater().step.borrow().clone()) else { return };
                f(&me, &step);
            }
        };
        match &step {
            Step::Out(r) => {
                head(&body, "software-update-available-symbolic", &format!("Radiance Music {} is out", r.version), "The newest open beta.");
                bullets(&body, r);
                let (label, icon) = match &u.install {
                    Install::Other => ("Open the Download Page".to_string(), "web-browser-symbolic"),
                    Install::Flatpak => ("Update Now".to_string(), "software-update-available-symbolic"),
                    _ if r.size > 0 => (format!("Update Now ({})", mb(r.size)), "software-update-available-symbolic"),
                    _ => ("Update Now".to_string(), "software-update-available-symbolic"),
                };
                body.append(&gap(8));
                body.append(&button(icon, &label, Kind::Main, act(Ui::update_now)));
                if u.install != Install::Other {
                    body.append(&note("Music goes on playing while it downloads."));
                }
                body.append(&button("alarm-symbolic", "Remind Me Later", Kind::Second, act(|me, _| me.update_later())));
                body.append(&button("notifications-disabled-symbolic", "Skip This Version", Kind::Quiet, act(|me, s| me.update_skip(s))));
            }
            Step::Downloading(r, done) => {
                head(&body, "folder-download-symbolic", &format!("Downloading {}…", r.version), "Music goes on playing. You can hide this: it goes on downloading.");
                let bar = gtk::ProgressBar::builder().fraction(*done).margin_top(8).build();
                let words = gtk::Label::builder().label(downloading_words(r, *done, &u.install)).xalign(0.0).css_classes(["dim-label", "caption"]).build();
                body.append(&bar);
                body.append(&words);
                *u.going.borrow_mut() = Some((bar, words));
                body.append(&gap(8));
                body.append(&button("go-down-symbolic", "Hide", Kind::Second, act(|me, _| me.close_update())));
                if u.install != Install::Flatpak {
                    body.append(&button("window-close-symbolic", "Cancel the Download", Kind::Quiet, act(|me, s| me.update_cancel(s))));
                }
            }
            Step::Ready(r) => {
                let how = match &u.install {
                    Install::Windows(_) => "the installer puts it on and opens Radiance Music again by itself",
                    _ => "it opens again on the new version",
                };
                let stops = if playing { " and stops the music" } else { "" };
                head(&body, "view-refresh-symbolic", &format!("Radiance Music {} is ready", r.version), &format!("Downloaded and checked. Restarting closes the app for a moment{stops}; {how}."));
                body.append(&gap(8));
                body.append(&button("view-refresh-symbolic", "Restart to Update", Kind::Main, act(Ui::update_restart)));
                body.append(&button("alarm-symbolic", "Remind Me Later", Kind::Second, act(|me, _| me.update_later())));
                body.append(&button("notifications-disabled-symbolic", "Skip This Version", Kind::Quiet, act(|me, s| me.update_skip(s))));
            }
            Step::Restarting(r) => {
                head(&body, "view-refresh-symbolic", &format!("Updating to {}…", r.version), "Radiance Music closes now and opens again on the new version.");
                body.append(&gtk::Spinner::builder().spinning(true).width_request(32).height_request(32).halign(gtk::Align::Center).margin_top(8).build());
            }
            Step::Failed(r, why) => {
                head(&body, "dialog-warning-symbolic", why, &format!("Nothing changed: you're still on {}.", update::current()));
                body.append(&gap(8));
                body.append(&button("view-refresh-symbolic", "Try Again", Kind::Main, act(Ui::update_now)));
                body.append(&button("alarm-symbolic", "Remind Me Later", Kind::Second, act(|me, _| me.update_later())));
                if r.as_ref().is_some_and(|r| !r.page.is_empty()) {
                    body.append(&button("web-browser-symbolic", "Get It from GitHub", Kind::Quiet, act(|me, s| me.update_page(s))));
                }
            }
        }
        view.set_content(Some(&body));
    }

    /// Update Now (and Try Again): the download (or Flatpak's update) while the music plays.
    fn update_now(self: &Rc<Self>, step: &Step) {
        let Some(r) = step.release().cloned() else {
            return self.look_for_updates(true);
        };
        let u = updater();
        match u.install.clone() {
            Install::Other => self.update_page(step),
            Install::Flatpak => {
                u.following.set(true);
                self.set_update_step(Step::Downloading(r.clone(), 0.0));
                let me = Rc::downgrade(self);
                update::flatpak_update(move |s| {
                    let Some(me) = me.upgrade() else { return };
                    match s {
                        FlatpakStep::Going(f) => me.set_update_step(Step::Downloading(r.clone(), f)),
                        FlatpakStep::Done => {
                            let (r2, i) = (r.clone(), updater().install.clone());
                            bg(move || update::counted(&r2, &i), |_| {});
                            me.set_update_step(Step::Ready(r.clone()));
                        }
                        FlatpakStep::NothingNew => me.set_update_step(Step::Failed(Some(r.clone()), format!("{} isn't in the Flatpak repo yet", r.version))),
                        FlatpakStep::Failed(why) => me.set_update_step(Step::Failed(Some(r.clone()), format!("Couldn't update: {why}"))),
                    }
                });
            }
            install => {
                u.following.set(true);
                let stop = Arc::new(AtomicBool::new(false));
                *u.stop.borrow_mut() = stop.clone();
                self.set_update_step(Step::Downloading(r.clone(), 0.0));
                let (tx, rx) = async_channel::unbounded::<Result<Option<f64>, String>>();
                let (r2, s2) = (r.clone(), stop.clone());
                std::thread::spawn(move || {
                    let tx2 = tx.clone();
                    let got = update::download(&r2, &install, &s2, |have, of| {
                        if of > 0 {
                            let _ = tx2.send_blocking(Ok(Some(have as f64 / of as f64)));
                        }
                    });
                    if got.is_ok() {
                        update::counted(&r2, &install);
                    }
                    let _ = tx.send_blocking(got.map(|_| None));
                });
                let me = Rc::downgrade(self);
                glib::spawn_future_local(async move {
                    while let Ok(msg) = rx.recv().await {
                        let Some(me) = me.upgrade() else { return };
                        if stop.load(Ordering::Relaxed) {
                            return me.set_update_step(Step::Out(r.clone())); // cancelled
                        }
                        match msg {
                            Ok(Some(f)) => me.set_update_step(Step::Downloading(r.clone(), f)),
                            Ok(None) => return me.set_update_step(Step::Ready(r.clone())),
                            Err(why) => return me.set_update_step(Step::Failed(Some(r.clone()), format!("Couldn't update: {why}"))),
                        }
                    }
                });
            }
        }
    }

    /// Restart to Update: where you are is kept, the update goes on, the new version opens.
    fn update_restart(self: &Rc<Self>, step: &Step) {
        let Some(r) = step.release().cloned() else { return };
        let u = updater();
        let file = update::file(&r, &u.install);
        if u.install != Install::Flatpak && !file.exists() {
            return self.update_now(step); // its download went: again
        }
        self.set_update_step(Step::Restarting(r.clone()));
        if !self.starting.get() {
            self.save_session();
        }
        match update::put_on(&u.install, &file) {
            Ok(()) => {
                crate::log::note(&format!("updating to {}: restarting", r.version));
                let me = Rc::downgrade(self);
                // a moment for the pop-up to say so, then quit (the new version waits for it)
                glib::timeout_add_local_once(Duration::from_millis(600), move || {
                    if let Some(me) = me.upgrade() {
                        me.close_update();
                        me.quit_app();
                    }
                });
            }
            Err(why) => self.set_update_step(Step::Failed(Some(r), format!("Couldn't update: {why}"))),
        }
    }

    fn update_later(self: &Rc<Self>) {
        update::remind_later();
        updater().following.set(false);
        self.close_update();
    }

    fn update_skip(self: &Rc<Self>, step: &Step) {
        if let Some(r) = step.release() {
            update::skip(&r.version);
        }
        updater().following.set(false);
        self.close_update();
    }

    fn update_cancel(self: &Rc<Self>, _step: &Step) {
        updater().stop.borrow().store(true, Ordering::Relaxed);
        updater().following.set(false);
    }

    fn update_page(self: &Rc<Self>, step: &Step) {
        let page = step.release().map(|r| r.page.clone()).filter(|p| !p.is_empty()).unwrap_or_else(|| "https://github.com/RadianceADM/radiance-music/releases/latest".into());
        gtk::UriLauncher::new(&page).launch(Some(&self.window), gtk::gio::Cancellable::NONE, |_| {});
        self.close_update();
    }

    /// RADIANCE_SHOT_UPDATE=out|downloading|ready|failed: the pop-up at that step, made up (design checks).
    fn update_shot(self: &Rc<Self>, which: &str) {
        let r = Release {
            version: "0.7.13".into(),
            notes: "## What's new\n- **Updates pop up**: Update Now, Remind Me Later, Skip This Version\n- Restart to Update once it's downloaded; the music plays on meanwhile\n- Faster search".into(),
            page: "https://github.com/RadianceADM/radiance-music/releases".into(),
            url: "https://example.invalid/RadianceMusic.tar.gz".into(),
            size: 4_904_836,
            sha256: String::new(),
        };
        let step = match which {
            "downloading" => Step::Downloading(r, 0.42),
            "ready" => Step::Ready(r),
            "restarting" => Step::Restarting(r),
            "failed" => Step::Failed(Some(r), "Couldn't update: the download stopped".into()),
            _ => Step::Out(r),
        };
        *updater().step.borrow_mut() = Some(step);
        let me = Rc::downgrade(self);
        glib::timeout_add_local_once(Duration::from_millis(1500), move || {
            if let Some(me) = me.upgrade() {
                me.pop_up();
            }
        });
    }
}

fn downloading_words(r: &Release, done: f64, install: &Install) -> String {
    if *install == Install::Flatpak || r.size == 0 {
        format!("{}%", (done * 100.0).round() as u32)
    } else {
        format!("{} of {}", mb((r.size as f64 * done) as u64).trim(), mb(r.size))
    }
}

/// The step's icon in a soft orange circle, its title and what it means.
fn head(body: &gtk::Box, icon: &str, title: &str, text: &str) {
    let mark = gtk::Box::builder().halign(gtk::Align::Start).css_classes(["update-mark"]).build();
    mark.append(&gtk::Image::builder().icon_name(icon).pixel_size(28).hexpand(true).vexpand(true).halign(gtk::Align::Center).valign(gtk::Align::Center).build());
    body.append(&mark);
    body.append(&gtk::Label::builder().label(title).wrap(true).xalign(0.0).margin_top(6).css_classes(["title-2"]).build());
    body.append(&gtk::Label::builder().label(text).wrap(true).xalign(0.0).css_classes(["dim-label"]).build());
}

/// What's new in it: the release notes' first few points.
fn bullets(body: &gtk::Box, r: &Release) {
    for line in r.bullets(4) {
        let row = gtk::Box::builder().spacing(10).build();
        row.append(&gtk::Label::builder().label("•").valign(gtk::Align::Start).css_classes(["bullet"]).build());
        row.append(&gtk::Label::builder().label(line).wrap(true).xalign(0.0).hexpand(true).build());
        body.append(&row);
    }
}

fn note(text: &str) -> gtk::Label {
    gtk::Label::builder().label(text).wrap(true).xalign(0.0).css_classes(["dim-label", "caption"]).build()
}

fn gap(h: i32) -> gtk::Box {
    gtk::Box::builder().height_request(h).build()
}

enum Kind {
    /// The step's own: orange.
    Main,
    Second,
    /// Small and plain, under the others.
    Quiet,
}

/// A button with its icon and words (owner: every button carries an icon).
fn button(icon: &str, label: &str, kind: Kind, on: impl Fn(&gtk::Button) + 'static) -> gtk::Button {
    let b = gtk::Button::builder().child(&adw::ButtonContent::builder().icon_name(icon).label(label).can_shrink(true).build()).build();
    match kind {
        Kind::Main => b.add_css_class("update-main"),
        Kind::Second => b.add_css_class("update-second"),
        Kind::Quiet => {
            b.add_css_class("flat");
            b.set_halign(gtk::Align::Center);
        }
    }
    if matches!(kind, Kind::Main) {
        b.add_css_class("suggested-action");
    }
    b.connect_clicked(on);
    b
}
