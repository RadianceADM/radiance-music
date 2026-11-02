//! Menu → Report a Problem… (see report.rs): what will be sent, shown in full
//! before anything goes; an optional note; Send. Afterwards the report's code
//! to quote, with Copy.

use super::*;
use crate::report::{self, Report, MOST_NOTE};

impl Ui {
    pub(crate) fn report_problem(self: &Rc<Self>) {
        let dialog = adw::Dialog::builder().title("Report a Problem").content_width(560).build();
        let header = adw::HeaderBar::new();
        let view = adw::ToolbarView::new();
        view.add_top_bar(&header);
        let body = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(12).margin_top(8).margin_bottom(20).margin_start(20).margin_end(20).build();
        body.append(
            &gtk::Label::builder()
                .label("Tell us what went wrong. Nothing is sent until you press Send, and you can see below exactly what goes.")
                .wrap(true)
                .xalign(0.0)
                .build(),
        );
        body.append(&gtk::Label::builder().label("What happened? (you can leave it empty)").xalign(0.0).css_classes(["heading"]).build());
        let note = gtk::TextView::builder().wrap_mode(gtk::WrapMode::WordChar).accepts_tab(false).top_margin(8).bottom_margin(8).left_margin(8).right_margin(8).build();
        note.buffer().connect_changed(|b| {
            // at most MOST_NOTE characters, as the admin takes them
            let text = b.text(&b.start_iter(), &b.end_iter(), false);
            if text.chars().count() > MOST_NOTE {
                let mut end = b.start_iter();
                end.forward_chars(MOST_NOTE as i32);
                let mut stop = b.end_iter();
                b.delete(&mut end, &mut stop);
            }
        });
        body.append(&gtk::ScrolledWindow::builder().child(&note).min_content_height(96).css_classes(["card"]).build());
        let sent_text = gtk::TextView::builder().editable(false).cursor_visible(false).monospace(true).wrap_mode(gtk::WrapMode::WordChar).top_margin(8).bottom_margin(8).left_margin(8).right_margin(8).build();
        sent_text.buffer().set_text("Reading the app's log…");
        let shown = gtk::Expander::builder().label("What will be sent").child(&gtk::ScrolledWindow::builder().child(&sent_text).min_content_height(220).css_classes(["card"]).build()).build();
        body.append(&shown);
        let status = gtk::Label::builder().wrap(true).xalign(0.0).visible(false).build();
        body.append(&status);
        let buttons = gtk::Box::builder().spacing(8).halign(gtk::Align::End).build();
        let cancel = gtk::Button::builder().child(&adw::ButtonContent::builder().icon_name("window-close-symbolic").label("Cancel").build()).build();
        let send = gtk::Button::builder().child(&adw::ButtonContent::builder().icon_name("mail-send-symbolic").label("Send").build()).css_classes(["suggested-action"]).sensitive(false).build();
        buttons.append(&cancel);
        buttons.append(&send);
        body.append(&buttons);
        view.set_content(Some(&gtk::ScrolledWindow::builder().child(&body).propagate_natural_height(true).hscrollbar_policy(gtk::PolicyType::Never).build()));
        dialog.set_child(Some(&view));
        let d = dialog.downgrade();
        cancel.connect_clicked(move |_| {
            if let Some(d) = d.upgrade() {
                d.close();
            }
        });

        // what goes, gathered off the window's thread (it reads the log)
        let gathered: Rc<RefCell<Option<Report>>> = Rc::default();
        let (g, t, s) = (gathered.clone(), sent_text.downgrade(), send.downgrade());
        bg(Report::gather, move |r| {
            if let Some(t) = t.upgrade() {
                t.buffer().set_text(&format!(
                    "Version: {}\nInstall: {}\nSystem: {}\nInstall ID: {} (made once, at random)\n\nThe app's recent log (your name, home folder and email addresses taken out):\n\n{}",
                    r.version,
                    r.platform,
                    r.system,
                    r.install,
                    if r.log.trim().is_empty() { "(empty)" } else { r.log.as_str() }
                ));
            }
            if let Some(s) = s.upgrade() {
                s.set_sensitive(true);
            }
            g.replace(Some(r));
        });

        let (me, d) = (Rc::downgrade(self), dialog.downgrade());
        send.connect_clicked(move |b| {
            let (Some(me), Some(r)) = (me.upgrade(), gathered.borrow().clone()) else { return };
            let buf = note.buffer();
            let words = buf.text(&buf.start_iter(), &buf.end_iter(), false).to_string();
            b.set_sensitive(false);
            status.set_visible(true);
            status.remove_css_class("error");
            status.set_label("Sending…");
            let (b, status, d, body) = (b.downgrade(), status.downgrade(), d.clone(), body.downgrade());
            bg(move || report::send(&r, &words), move |result| match result {
                Ok(code) => {
                    crate::log::note(&format!("problem report sent: {code}"));
                    if let (Some(body), Some(d)) = (body.upgrade(), d.upgrade()) {
                        me.report_sent(&body, &d, &code);
                    }
                }
                Err(f) => {
                    if let Some(status) = status.upgrade() {
                        status.add_css_class("error");
                        status.set_label(&f.words());
                    }
                    if let Some(b) = b.upgrade() {
                        b.set_sensitive(true);
                    }
                }
            });
        });
        dialog.present(Some(&self.window));
    }

    /// The thank-you: the report's code to quote, Copy, Close.
    fn report_sent(&self, body: &gtk::Box, dialog: &adw::Dialog, code: &str) {
        while let Some(c) = body.first_child() {
            body.remove(&c);
        }
        body.append(&gtk::Image::builder().icon_name("emblem-ok-symbolic").pixel_size(48).margin_top(12).build());
        body.append(&gtk::Label::builder().label("Sent. Thank you!").css_classes(["title-2"]).build());
        body.append(
            &gtk::Label::builder()
                .label(format!("Your report is {code}. If you write to us about it (hello@radianceph.com), quote that."))
                .wrap(true)
                .justify(gtk::Justification::Center)
                .build(),
        );
        let row = gtk::Box::builder().spacing(8).halign(gtk::Align::Center).margin_top(8).build();
        let copy = gtk::Button::builder().child(&adw::ButtonContent::builder().icon_name("edit-copy-symbolic").label("Copy the Code").build()).build();
        let close = gtk::Button::builder().child(&adw::ButtonContent::builder().icon_name("window-close-symbolic").label("Close").build()).css_classes(["suggested-action"]).build();
        let code = code.to_string();
        copy.connect_clicked(move |b| b.clipboard().set_text(&code));
        let d = dialog.downgrade();
        close.connect_clicked(move |_| {
            if let Some(d) = d.upgrade() {
                d.close();
            }
        });
        row.append(&copy);
        row.append(&close);
        body.append(&row);
    }
}
