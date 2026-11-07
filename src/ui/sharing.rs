//! Share Songs with a Phone (the menu): a QR code the phone scans (Radiance
//! Music for Android: Settings → Get songs), to take this
//! computer's liked songs with their parts, lyrics and covers, or to send the
//! phone's here. Only what the other side hasn't got goes.

use super::*;
use crate::share::{self, Event, Share};
use radiance_core::share_words as sw;

impl Ui {
    pub(super) fn sharing_dialog(self: &Rc<Self>) {
        let dialog = adw::Dialog::builder().title("Share Songs with a Phone").content_width(440).build();
        let view = adw::ToolbarView::new();
        view.add_top_bar(&adw::HeaderBar::new());
        let body = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(14).margin_top(6).margin_bottom(24).margin_start(24).margin_end(24).build();
        let scroll = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).propagate_natural_height(true).child(&body).build();
        view.set_content(Some(&scroll));
        dialog.set_child(Some(&view));

        let way = adw::ToggleGroup::new();
        way.add(adw::Toggle::builder().name("to").label("To the Phone").build());
        way.add(adw::Toggle::builder().name("from").label("From the Phone").build());
        way.set_active_name(Some("to"));
        body.append(&way);

        // no "With music videos": phones have no music videos (owner, 2026-11-08: "final na walang music video sa mobile")

        let qr = gtk::DrawingArea::builder().content_width(280).content_height(280).halign(gtk::Align::Center).build();
        let text: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let t = text.clone();
        qr.set_draw_func(move |_, cr, w, h| draw_qr(cr, w, h, t.borrow().as_deref()));
        body.append(&qr);

        let caption = gtk::Label::builder().wrap(true).justify(gtk::Justification::Center).css_classes(["dim-label"]).build();
        body.append(&caption);
        let bar = gtk::ProgressBar::builder().visible(false).show_text(true).build();
        body.append(&bar);
        let note = gtk::Label::builder().wrap(true).visible(false).css_classes(["caption"]).justify(gtk::Justification::Center).build();
        body.append(&note);

        // a firewall that may keep the phone out: let it in, once, for the home network
        if firewall_on() && !self.settings.borrow().share_firewall_open {
            let (title, subtitle) = if cfg!(windows) {
                (
                    sw::FIREWALL_WINDOWS,
                    sw::firewall_sub(true, *share::PORTS.start(), *share::PORTS.end()),
                )
            } else {
                (sw::FIREWALL_LINUX, sw::firewall_sub(false, *share::PORTS.start(), *share::PORTS.end()))
            };
            let row = adw::ActionRow::builder().title(title).subtitle(subtitle).build();
            let allow = gtk::Button::builder().child(&adw::ButtonContent::builder().icon_name("object-select-symbolic").label("Let It In").build()).valign(gtk::Align::Center).build();
            row.add_suffix(&allow);
            let list = gtk::ListBox::builder().css_classes(["boxed-list"]).selection_mode(gtk::SelectionMode::None).build();
            list.append(&row);
            body.append(&list);
            let (me, list_w) = (Rc::downgrade(self), list.downgrade());
            allow.connect_clicked(move |b| {
                b.set_sensitive(false);
                let (me, list_w, b) = (me.clone(), list_w.clone(), b.downgrade());
                bg(open_firewall, move |ok| {
                    let Some(me) = me.upgrade() else { return };
                    if ok {
                        me.settings.borrow_mut().share_firewall_open = true;
                        settings::save(&me.settings.borrow());
                        if let Some(l) = list_w.upgrade() {
                            l.set_visible(false);
                        }
                        me.toast(sw::LET_IN_TOAST);
                    } else if let Some(b) = b.upgrade() {
                        b.set_sensitive(true);
                    }
                });
            });
        }

        let share: Rc<RefCell<Option<Share>>> = Rc::new(RefCell::new(None));
        let (tx, rx) = async_channel::unbounded::<Event>();
        let name = sw::device_name(&glib::host_name());

        // a new QR for the way picked
        let show = {
            let (me, share, text, qr, caption, bar, note, way, tx) = (Rc::downgrade(self), share.clone(), text.clone(), qr.downgrade(), caption.downgrade(), bar.downgrade(), note.downgrade(), way.downgrade(), tx.clone());
            Rc::new(move || {
                let (Some(me), Some(qr), Some(caption), Some(bar), Some(note), Some(way)) = (me.upgrade(), qr.upgrade(), caption.upgrade(), bar.upgrade(), note.upgrade(), way.upgrade()) else { return };
                share.borrow_mut().take(); // the old QR stops working
                bar.set_visible(false);
                note.set_visible(false);
                let to_phone = way.active_name().as_deref() == Some("to");
                let saved: Vec<Track> = me.liked.borrow().iter().filter(|t| t.file.as_ref().is_some_and(|f| Path::new(f).exists())).cloned().collect();
                let opened = if to_phone { Share::send(saved.clone(), tx.clone()) } else { Share::receive(tx.clone()) };
                match opened.as_ref().map(|s| s.ticket(&name)) {
                    Ok(Some(words)) => {
                        *text.borrow_mut() = Some(words);
                        caption.set_text(&if to_phone {
                            sw::to_phone_caption(saved.len())
                        } else {
                            sw::FROM_PHONE_CAPTION.to_string()
                        });
                    }
                    Ok(None) => {
                        *text.borrow_mut() = None;
                        caption.set_text(sw::NO_NETWORK);
                    }
                    Err(e) => {
                        *text.borrow_mut() = None;
                        caption.set_text(&sw::couldnt_get_ready(&e.to_string()));
                    }
                }
                *share.borrow_mut() = opened.ok();
                qr.queue_draw();
            })
        };
        show();
        let s = show.clone();
        way.connect_active_name_notify(move |_| s());

        let (me, bar_w, note_w) = (Rc::downgrade(self), bar.downgrade(), note.downgrade());
        glib::spawn_future_local(async move {
            while let Ok(event) = rx.recv().await {
                let (Some(me), Some(bar), Some(note)) = (me.upgrade(), bar_w.upgrade(), note_w.upgrade()) else { break };
                match event {
                    Event::Progress { bytes, total, songs, of, now } => {
                        bar.set_visible(true);
                        if total > 0 {
                            bar.set_fraction((bytes as f64 / total as f64).clamp(0.0, 1.0));
                        } else {
                            bar.pulse();
                        }
                        let mut words = sw::progress(songs, of, bytes, total, now.as_deref());
                        if of > 0 && songs == of {
                            words = sw::done(of);
                        }
                        bar.set_text(Some(&words));
                    }
                    Event::Received(t) => me.received_from_phone(t),
                    Event::Gentle(on) => {
                        note.set_visible(on);
                        note.set_text(sw::GENTLE);
                    }
                }
            }
        });

        // closed: the QR stops working
        dialog.connect_closed(move |_| {
            share.borrow_mut().take();
        });
        dialog.present(Some(&self.window));
    }

    /// A song from the phone, all here: in Liked Songs, playing its file here.
    fn received_from_phone(self: &Rc<Self>, t: Track) {
        let key = t.key();
        {
            let mut liked = self.liked.borrow_mut();
            match liked.iter_mut().find(|x| x.key() == key || x.same_song(&t)) {
                Some(x) => x.file = t.file.clone(),
                None => liked.insert(0, t.clone()),
            }
        }
        library::store(&self.liked.borrow());
        crate::log::note(&format!("from the phone: {}", t.title));
        self.refresh_hearts();
        self.render_liked();
        self.refresh_mix_later();
    }
}

/// The QR: soft round dots and rounded eyes (no hard squares), dark on white so every camera reads it.
fn draw_qr(cr: &gtk::cairo::Context, w: i32, h: i32, text: Option<&str>) {
    let side = w.min(h) as f64;
    let (x0, y0) = ((w as f64 - side) / 2.0, (h as f64 - side) / 2.0);
    let round = |x: f64, y: f64, s: f64, r: f64| {
        cr.new_sub_path();
        cr.arc(x + s - r, y + r, r, -std::f64::consts::FRAC_PI_2, 0.0);
        cr.arc(x + s - r, y + s - r, r, 0.0, std::f64::consts::FRAC_PI_2);
        cr.arc(x + r, y + s - r, r, std::f64::consts::FRAC_PI_2, std::f64::consts::PI);
        cr.arc(x + r, y + r, r, std::f64::consts::PI, 1.5 * std::f64::consts::PI);
        cr.close_path();
    };
    cr.set_source_rgb(1.0, 1.0, 1.0);
    round(x0, y0, side, 22.0);
    let _ = cr.fill();
    // the modules from the engine (both apps), drawn here as round dots
    let Some((n, dark)) = text.and_then(share::qr_modules) else { return };
    let pad = side * 0.06;
    let cell = (side - 2.0 * pad) / n as f64;
    let (ox, oy) = (x0 + pad, y0 + pad);
    let eye = |x: usize, y: usize| (x < 7 && y < 7) || (x >= n - 7 && y < 7) || (x < 7 && y >= n - 7);
    cr.set_source_rgb(0x0C as f64 / 255.0, 0x0A as f64 / 255.0, 0x09 as f64 / 255.0); // the Stone look's deepest (no navy)
    for y in 0..n {
        for x in 0..n {
            if dark[y * n + x] && !eye(x, y) {
                cr.new_sub_path();
                cr.arc(ox + (x as f64 + 0.5) * cell, oy + (y as f64 + 0.5) * cell, cell * 0.46, 0.0, std::f64::consts::TAU);
            }
        }
    }
    let _ = cr.fill();
    for (ex, ey) in [(0, 0), (n - 7, 0), (0, n - 7)] {
        let (x, y) = (ox + ex as f64 * cell, oy + ey as f64 * cell);
        cr.set_source_rgb(0x0C as f64 / 255.0, 0x0A as f64 / 255.0, 0x09 as f64 / 255.0); // the Stone look's deepest (no navy)
        round(x, y, 7.0 * cell, 2.2 * cell);
        let _ = cr.fill();
        cr.set_source_rgb(1.0, 1.0, 1.0);
        round(x + cell, y + cell, 5.0 * cell, 1.5 * cell);
        let _ = cr.fill();
        cr.set_source_rgb(0x0C as f64 / 255.0, 0x0A as f64 / 255.0, 0x09 as f64 / 255.0); // the Stone look's deepest (no navy)
        round(x + 2.0 * cell, y + 2.0 * cell, 3.0 * cell, 1.1 * cell);
        let _ = cr.fill();
    }
}

fn firewall_on() -> bool {
    radiance_engine::firewall::blocks()
}

fn open_firewall() -> bool {
    radiance_engine::firewall::let_in()
}
