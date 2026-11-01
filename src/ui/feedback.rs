//! The feedback pop-up (owner, 2026-11-01: "Are you enjoying Radiance Music?"
//! and five ratings; sending asks for a Google sign-in). It comes up by itself
//! at a free moment (a song's end, a pause) when crate::feedback says it's time,
//! or from the menu: Rate Radiance Music. Anyone can fill it in; Send opens the
//! browser to sign in when needed and keeps the answers meanwhile.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::*;
use crate::feedback::{self, Account, Answer, Failure};

const FEEDBACK_CSS: &str = "
.rate-mark { min-width: 48px; min-height: 48px; border-radius: 999px; background: alpha(#F38A35, 0.18); color: #F38A35; }
button.rate-star { min-width: 36px; min-height: 36px; padding: 0; border-radius: 999px; color: alpha(currentColor, 0.3); }
button.rate-star.on { color: #F38A35; }
button.rate-pill { min-height: 44px; border-radius: 999px; font-weight: 700; }
button.rate-pill:checked { background: #F38A35; color: #1a1a1a; }
button.rate-main, button.rate-second { min-height: 44px; border-radius: 999px; font-weight: 700; }
.rate-comment { background: transparent; padding: 10px; }
.rate-problem { color: #FFAE63; }
";

/// The pop-up's widgets and answers (window thread only; one at a time).
#[derive(Default)]
struct Form {
    dialog: Option<adw::Dialog>,
    pages: Option<gtk::Stack>,
    enjoying: Option<bool>,
    stars: [u8; 5],
    pills: Vec<gtk::ToggleButton>,
    star_buttons: Vec<Vec<gtk::Button>>,
    comment: Option<gtk::TextView>,
    send: Option<gtk::Button>,
    cancel: Option<gtk::Button>,
    problem: Option<gtk::Label>,
    who: Option<gtk::Label>,
    sign_out: Option<gtk::Button>,
    account: Option<Account>,
    busy: bool,
    stop: Option<Arc<AtomicBool>>,
}

thread_local! {
    static FORM: RefCell<Form> = RefCell::new(Form::default());
}

fn with_form<T>(f: impl FnOnce(&mut Form) -> T) -> T {
    FORM.with(|c| f(&mut c.borrow_mut()))
}

impl Ui {
    /// A free moment (a song ended, or the music was paused): the pop-up may come up by itself.
    pub(super) fn feedback_moment(self: &Rc<Self>) {
        let first_open = self.setup_holding.get() || !self.settings.borrow().guide_seen;
        if std::env::var_os("RADIANCE_SHOT").is_some()
            || first_open
            || self.starting.get()
            || !self.window.is_visible()
            || self.window.visible_dialog().is_some()
            || self.in_party()
            || with_form(|f| f.dialog.is_some())
        {
            return;
        }
        if feedback::time_to_ask() {
            feedback::asked_now(); // shown: not again for 30 days, whatever is pressed
            crate::log::note("feedback: asking (a free moment, at random)");
            self.rate_app();
        }
    }

    /// RADIANCE_SHOT_RATE=form|filled|problem|thanks|thanks-plain: the pop-up at that step, made up (design checks).
    pub(super) fn rate_shot(self: &Rc<Self>, which: &str) {
        let which = which.to_string();
        let me = Rc::downgrade(self);
        glib::timeout_add_local_once(Duration::from_millis(1500), move || {
            let Some(me) = me.upgrade() else { return };
            me.rate_app();
            if which != "form" {
                let pick = with_form(|f| f.pills.first().cloned());
                if let Some(yes) = pick {
                    yes.set_active(true);
                }
                with_form(|f| f.stars = [5, 4, 5, 3, 4]);
                (0..5).for_each(show_stars);
                if let Some(c) = with_form(|f| f.comment.clone()) {
                    c.buffer().set_text("Love the Mixer. More lyrics for OPM songs, please!");
                }
            }
            match which.as_str() {
                "problem" => say_problem(&Failure::Offline.words()),
                "thanks" => me.rate_thanks(true),
                "thanks-plain" => me.rate_thanks(false),
                _ => {}
            }
        });
    }

    /// Opens the pop-up (by itself, or the menu's Rate Radiance Music).
    pub(super) fn rate_app(self: &Rc<Self>) {
        if with_form(|f| f.dialog.is_some()) {
            return;
        }
        thread_local! { static STYLED: Cell<bool> = const { Cell::new(false) }; }
        if !STYLED.with(|s| s.replace(true)) {
            let css = gtk::CssProvider::new();
            css.load_from_string(FEEDBACK_CSS);
            if let Some(display) = gdk::Display::default() {
                gtk::style_context_add_provider_for_display(&display, &css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
            }
        }
        let dialog = adw::Dialog::builder().title("Rate Radiance Music").content_width(460).build();
        let view = adw::ToolbarView::new();
        view.add_top_bar(&adw::HeaderBar::builder().show_title(false).build());
        let pages = gtk::Stack::builder().transition_type(gtk::StackTransitionType::Crossfade).vhomogeneous(false).build();
        pages.add_named(&self.rate_form(), Some("form"));
        let scroll = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).propagate_natural_height(true).child(&pages).build();
        view.set_content(Some(&scroll));
        dialog.set_child(Some(&view));
        dialog.connect_closed(|_| {
            with_form(|f| {
                if let Some(stop) = f.stop.take() {
                    stop.store(true, Ordering::Relaxed); // a sign-in still waiting for the browser
                }
                *f = Form::default();
            });
        });
        with_form(|f| {
            f.dialog = Some(dialog.clone());
            f.pages = Some(pages);
        });
        dialog.present(Some(&self.window));
        // who's signed in (the keyring may take a moment: off the window's thread)
        let me = Rc::downgrade(self);
        bg(feedback::signed_in, move |account| {
            if me.upgrade().is_some() && with_form(|f| f.dialog.is_some()) {
                with_form(|f| f.account = account);
                show_who();
            }
        });
    }

    fn rate_form(self: &Rc<Self>) -> gtk::Box {
        let body = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(9).margin_start(24).margin_end(24).margin_bottom(20).build();
        head(&body, "starred-symbolic", "Are you enjoying Radiance Music?", "A minute of your time helps make the next version better.");
        // Yes! / Not really
        let pills = gtk::Box::builder().spacing(10).homogeneous(true).margin_top(4).build();
        let yes = pill("face-smile-symbolic", "Yes!");
        let no = pill("face-sad-symbolic", "Not really");
        no.set_group(Some(&yes));
        for (b, answer) in [(&yes, true), (&no, false)] {
            b.connect_toggled(move |b| {
                if b.is_active() {
                    with_form(|f| f.enjoying = Some(answer));
                    say_problem("");
                }
            });
            pills.append(b);
        }
        body.append(&pills);
        // the five ratings
        let grid = gtk::Grid::builder().row_spacing(2).column_spacing(12).margin_top(6).build();
        let mut all = Vec::new();
        for (row, (_, label)) in feedback::AREAS.iter().enumerate() {
            grid.attach(&gtk::Label::builder().label(*label).xalign(0.0).hexpand(true).build(), 0, row as i32, 1, 1);
            let stars = gtk::Box::builder().spacing(2).build();
            let mut buttons = Vec::new();
            for n in 1..=5u8 {
                let b = gtk::Button::builder().icon_name("non-starred-symbolic").tooltip_text(format!("{label}: {n} of 5")).css_classes(["flat", "rate-star"]).build();
                b.connect_clicked(move |_| {
                    with_form(|f| f.stars[row] = n);
                    show_stars(row);
                    say_problem("");
                });
                stars.append(&b);
                buttons.push(b);
            }
            grid.attach(&stars, 1, row as i32, 1, 1);
            all.push(buttons);
        }
        body.append(&grid);
        // anything else
        body.append(&gtk::Label::builder().label("Anything to tell us? (optional)").xalign(0.0).margin_top(6).css_classes(["heading"]).build());
        let comment = gtk::TextView::builder().wrap_mode(gtk::WrapMode::WordChar).accepts_tab(false).css_classes(["rate-comment"]).build();
        comment.buffer().connect_changed(|buf| {
            // the admin takes 2000 characters
            if buf.char_count() as usize > feedback::MOST_COMMENT {
                let mut end = buf.end_iter();
                let start = buf.iter_at_offset(feedback::MOST_COMMENT as i32);
                buf.delete(&mut start.clone(), &mut end);
            }
        });
        // its own small scroll: 3 lines to 6, however narrow the window
        let comment_box = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .min_content_height(64)
            .max_content_height(132)
            .propagate_natural_height(true)
            .child(&comment)
            .css_classes(["card"])
            .build();
        body.append(&comment_box);
        let problem = gtk::Label::builder().wrap(true).xalign(0.0).visible(false).css_classes(["rate-problem"]).build();
        body.append(&problem);
        // Send (signing in first when needed)
        let me = Rc::downgrade(self);
        let send = button("mail-send-symbolic", "Send", Kind::Main, move |_| {
            if let Some(me) = me.upgrade() {
                me.rate_send();
            }
        });
        send.set_margin_top(6);
        body.append(&send);
        let cancel = button("process-stop-symbolic", "Cancel Sign-In", Kind::Quiet, |_| {
            if let Some(stop) = with_form(|f| f.stop.clone()) {
                stop.store(true, Ordering::Relaxed);
            }
        });
        cancel.set_visible(false);
        body.append(&cancel);
        let who_row = gtk::Box::builder().spacing(8).build();
        let who = gtk::Label::builder().use_markup(true).wrap(true).xalign(0.0).hexpand(true).css_classes(["dim-label", "caption"]).build();
        who_row.append(&who);
        let me = Rc::downgrade(self);
        let sign_out = button("system-log-out-symbolic", "Sign Out", Kind::Quiet, move |_| {
            if me.upgrade().is_none() {
                return;
            }
            let Some(account) = with_form(|f| f.account.take()) else { return };
            show_who();
            bg(
                move || {
                    feedback::sign_out_there(&account.token);
                    feedback::forget();
                },
                |_| {},
            );
        });
        sign_out.set_visible(false);
        who_row.append(&sign_out);
        body.append(&who_row);
        // Not Now / Don't Ask Again
        let later = gtk::Box::builder().spacing(10).homogeneous(true).margin_top(4).build();
        later.append(&button("alarm-symbolic", "Not Now", Kind::Second, |_| {
            feedback::asked_now();
            close_form();
        }));
        later.append(&button("action-unavailable-symbolic", "Don't Ask Again", Kind::Second, |_| {
            feedback::never_again();
            close_form();
        }));
        body.append(&later);
        with_form(|f| {
            f.pills = vec![yes, no];
            f.star_buttons = all;
            f.comment = Some(comment);
            f.send = Some(send);
            f.cancel = Some(cancel);
            f.problem = Some(problem);
            f.who = Some(who);
            f.sign_out = Some(sign_out);
        });
        show_who();
        body
    }

    /// Send: the answers checked, a sign-in first if there's none, then off to the admin.
    fn rate_send(self: &Rc<Self>) {
        let (enjoying, stars, busy) = with_form(|f| (f.enjoying, f.stars, f.busy));
        if busy {
            return;
        }
        let Some(enjoying) = enjoying.filter(|_| stars.iter().all(|n| *n > 0)) else {
            return say_problem("Choose Yes! or Not really, and give each one its stars.");
        };
        let comment = with_form(|f| f.comment.as_ref().map(|c| {
            let b = c.buffer();
            b.text(&b.start_iter(), &b.end_iter(), false).to_string()
        }))
        .unwrap_or_default();
        let answer = Answer { enjoying, ratings: stars, comment };
        match with_form(|f| f.account.clone()) {
            Some(account) => self.rate_deliver(account, answer),
            None => self.rate_sign_in(answer),
        }
    }

    /// The browser's Google sign-in (PKCE + a loopback address), then the answers go.
    fn rate_sign_in(self: &Rc<Self>, answer: Answer) {
        let lb = match feedback::loopback() {
            Ok(lb) => lb,
            Err(e) => return say_problem(&format!("Couldn't start the sign-in on this computer ({e}).")),
        };
        let p = feedback::pkce();
        let url = feedback::sign_in_url(&p, lb.port);
        let stop = Arc::new(AtomicBool::new(false));
        with_form(|f| f.stop = Some(stop.clone()));
        set_busy(true, "Finish signing in with Google in your browser…");
        crate::log::note("feedback: signing in through the browser");
        gtk::UriLauncher::new(&url).launch(Some(&self.window), gio::Cancellable::NONE, |_| {});
        let me = Rc::downgrade(self);
        bg(
            move || {
                let code = lb.wait(&p.state, &stop, feedback::SIGN_IN_WAIT)?;
                let account = feedback::exchange(&code, &p.verifier)?;
                feedback::keep(&account);
                Ok::<Account, Failure>(account)
            },
            move |got| {
                with_form(|f| f.stop = None);
                if with_form(|f| f.dialog.is_none()) {
                    return; // closed meanwhile
                }
                let Some(me) = me.upgrade() else { return };
                match got {
                    Ok(account) => {
                        with_form(|f| f.account = Some(account.clone()));
                        show_who();
                        me.rate_deliver(account, answer);
                    }
                    Err(f) => {
                        set_busy(false, "");
                        say_problem(&f.words());
                    }
                }
            },
        );
    }

    fn rate_deliver(self: &Rc<Self>, account: Account, answer: Answer) {
        set_busy(true, "Sending…");
        let token = account.token.clone();
        let me = Rc::downgrade(self);
        bg(
            move || feedback::send(&token, &answer),
            move |got| {
                if with_form(|f| f.dialog.is_none()) {
                    return;
                }
                let Some(me) = me.upgrade() else { return };
                set_busy(false, "");
                match got {
                    Ok(share) => {
                        feedback::sent();
                        crate::log::note("feedback: sent");
                        me.rate_thanks(share);
                    }
                    Err(Failure::SignInAgain) => {
                        // the admin doesn't take this sign-in anymore: forget it, Send signs in again
                        with_form(|f| f.account = None);
                        bg(feedback::forget, |_| {});
                        show_who();
                        say_problem(&Failure::SignInAgain.words());
                    }
                    Err(f) => say_problem(&f.words()),
                }
            },
        );
    }

    /// Thanks (and, at four or five stars overall, Share Radiance Music).
    fn rate_thanks(self: &Rc<Self>, share: bool) {
        let body = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(12).margin_start(24).margin_end(24).margin_bottom(24).valign(gtk::Align::Start).build();
        if share {
            head(&body, "face-smile-big-symbolic", "Thanks!", "Glad you're enjoying it. Know someone who'd like it too?");
            let me = Rc::downgrade(self);
            let b = button("send-to-symbolic", "Share Radiance Music", Kind::Main, move |_| {
                if let Some(me) = me.upgrade() {
                    me.window.clipboard().set_text(feedback::SITE);
                    me.toast("Link copied: music.radianceph.com. Paste it in a chat or a post.");
                }
            });
            b.set_margin_top(6);
            body.append(&b);
            body.append(&button("object-select-symbolic", "Done", Kind::Second, |_| close_form()));
        } else {
            head(&body, "face-smile-big-symbolic", "Thanks!", "Thanks for telling us. It helps make the next version better.");
            let b = button("object-select-symbolic", "Done", Kind::Main, |_| close_form());
            b.set_margin_top(6);
            body.append(&b);
        }
        if let Some(pages) = with_form(|f| f.pages.clone()) {
            pages.add_named(&body, Some("thanks"));
            pages.set_visible_child_name("thanks");
        }
    }
}

fn close_form() {
    if let Some(d) = with_form(|f| f.dialog.clone()) {
        d.close();
    }
}

fn show_stars(row: usize) {
    with_form(|f| {
        let n = f.stars[row];
        if let Some(buttons) = f.star_buttons.get(row) {
            for (i, b) in buttons.iter().enumerate() {
                let on = (i as u8) < n;
                b.set_icon_name(if on { "starred-symbolic" } else { "non-starred-symbolic" });
                if on {
                    b.add_css_class("on");
                } else {
                    b.remove_css_class("on");
                }
            }
        }
    });
}

fn show_who() {
    with_form(|f| {
        let (Some(who), Some(out)) = (&f.who, &f.sign_out) else { return };
        let kept = format!("Your name and email are kept with your rating (<a href=\"{}\">Privacy</a>).", feedback::PRIVACY);
        match &f.account {
            Some(a) => {
                let name = if a.email.is_empty() { a.name.clone() } else if a.name.is_empty() { a.email.clone() } else { format!("{} ({})", a.name, a.email) };
                who.set_markup(&format!("Sending as {}. {kept}", glib::markup_escape_text(&name)));
                out.set_visible(true);
            }
            None => {
                who.set_markup(&format!("Send opens your browser to sign in with Google. {kept}"));
                out.set_visible(false);
            }
        }
    });
}

fn say_problem(text: &str) {
    with_form(|f| {
        if let Some(p) = &f.problem {
            p.set_label(text);
            p.set_visible(!text.is_empty());
        }
    });
}

/// Sending or signing in: Send shows it (and can't be pressed twice); Cancel Sign-In while the browser has it.
fn set_busy(busy: bool, words: &str) {
    with_form(|f| {
        f.busy = busy;
        if let Some(send) = &f.send {
            send.set_sensitive(!busy);
            let content = if busy {
                let row = gtk::Box::builder().spacing(8).halign(gtk::Align::Center).build();
                row.append(&adw::Spinner::new());
                row.append(&gtk::Label::builder().label(words).wrap(true).build());
                row.upcast::<gtk::Widget>()
            } else {
                adw::ButtonContent::builder().icon_name("mail-send-symbolic").label("Send").can_shrink(true).build().upcast::<gtk::Widget>()
            };
            send.set_child(Some(&content));
        }
        if let Some(cancel) = &f.cancel {
            cancel.set_visible(busy && f.stop.is_some());
        }
    });
    if busy {
        say_problem("");
    }
}

/// The step's icon in a soft orange circle, its title and what it means.
fn head(body: &gtk::Box, icon: &str, title: &str, text: &str) {
    let mark = gtk::Box::builder().halign(gtk::Align::Start).valign(gtk::Align::Start).css_classes(["rate-mark"]).build();
    mark.append(&gtk::Image::builder().icon_name(icon).pixel_size(26).hexpand(true).vexpand(true).halign(gtk::Align::Center).valign(gtk::Align::Center).build());
    body.append(&mark);
    body.append(&gtk::Label::builder().label(title).wrap(true).xalign(0.0).margin_top(6).css_classes(["title-2"]).build());
    body.append(&gtk::Label::builder().label(text).wrap(true).xalign(0.0).css_classes(["dim-label"]).build());
}

fn pill(icon: &str, label: &str) -> gtk::ToggleButton {
    gtk::ToggleButton::builder().child(&adw::ButtonContent::builder().icon_name(icon).label(label).can_shrink(true).build()).css_classes(["rate-pill"]).build()
}

enum Kind {
    Main,
    Second,
    /// Small and plain.
    Quiet,
}

/// A button with its icon and words (owner: every button carries an icon).
fn button(icon: &str, label: &str, kind: Kind, on: impl Fn(&gtk::Button) + 'static) -> gtk::Button {
    let b = gtk::Button::builder().child(&adw::ButtonContent::builder().icon_name(icon).label(label).can_shrink(true).build()).build();
    match kind {
        Kind::Main => {
            b.add_css_class("rate-main");
            b.add_css_class("suggested-action");
        }
        Kind::Second => b.add_css_class("rate-second"),
        Kind::Quiet => {
            b.add_css_class("flat");
            b.set_halign(gtk::Align::Center);
        }
    }
    b.connect_clicked(on);
    b
}
