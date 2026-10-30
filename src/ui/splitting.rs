//! Splitting (the menu): set splitting up on this computer, how many cores
//! it may use, what it needs and how long it takes.

use super::*;
use crate::setup;

impl Ui {
    pub(super) fn splitting_dialog(self: &Rc<Self>) {
        let dialog = adw::PreferencesDialog::builder().title("Splitting").content_width(600).build();
        let page = adw::PreferencesPage::new();
        dialog.add(&page);
        self.fill_splitting(&page, &dialog);
        dialog.present(Some(&self.window));
    }

    /// The page's groups, made again when setting up finishes.
    fn fill_splitting(self: &Rc<Self>, page: &adw::PreferencesPage, dialog: &adw::PreferencesDialog) {
        let here = adw::PreferencesGroup::builder()
            .title("On this computer")
            .description("Splits a song into drums, bass, other and vocals (for the Mixer, No vocals and Vocals only) and times every word of its lyrics, here on this computer. Nothing is sent anywhere. Like a song and it's saved, then split.")
            .build();
        page.add(&here);
        let check = setup::check();
        let total = check.cores;
        match self.settings.borrow().local_split.clone() {
            Some(local) => {
                here.add(&adw::ActionRow::builder().title("Ready").subtitle("Demucs splits, MMS times the words. It runs at idle priority: whatever you're doing always goes first.").build());
                let cores = adw::SpinRow::builder()
                    .title("Cores it may use")
                    .subtitle(format!("Of the {total} this computer has. More is quicker; fewer leaves more for everything else."))
                    .adjustment(&gtk::Adjustment::new(local.threads.min(total as u32) as f64, 1.0, total as f64, 1.0, 1.0, 0.0))
                    .build();
                let me = Rc::downgrade(self);
                cores.connect_value_notify(move |row| {
                    let Some(me) = me.upgrade() else { return };
                    let n = row.value().round() as u32;
                    if let Some(l) = me.settings.borrow_mut().local_split.as_mut() {
                        l.threads = n;
                    }
                    settings::save(&me.settings.borrow());
                    crate::log::note(&format!("splitting may use {n} cores"));
                });
                here.add(&cores);
            }
            None => {
                let problems = check.problems();
                let row = adw::ActionRow::builder()
                    .title("Not set up")
                    .subtitle(if problems.is_empty() {
                        "Setting up downloads about 2.5 GB (PyTorch, Demucs and two models) and takes 10 to 30 minutes, in the background.".to_string()
                    } else {
                        problems.join("\n")
                    })
                    .build();
                let set_up = gtk::Button::builder().label("Set Up").valign(gtk::Align::Center).css_classes(["suggested-action"]).sensitive(problems.is_empty()).build();
                row.add_suffix(&set_up);
                here.add(&row);
                let bar = gtk::ProgressBar::builder().visible(false).margin_top(10).show_text(true).build();
                here.add(&bar);
                let (me, dialog_w, row_w, bar_w) = (Rc::downgrade(self), dialog.downgrade(), row.downgrade(), bar.downgrade());
                set_up.connect_clicked(move |b| {
                    b.set_sensitive(false);
                    let button = b.downgrade();
                    let Some(bar) = bar_w.upgrade() else { return };
                    bar.set_visible(true);
                    bar.set_fraction(0.0);
                    bar.set_text(Some("Starting…"));
                    let (tx, rx) = async_channel::unbounded::<(String, f64)>();
                    let bw = bar.downgrade();
                    glib::spawn_future_local(async move {
                        while let Ok((what, done)) = rx.recv().await {
                            let Some(bar) = bw.upgrade() else { break };
                            bar.set_fraction(done);
                            bar.set_text(Some(&what));
                        }
                    });
                    crate::log::note("setting splitting up on this computer");
                    let threads = setup::half_the_cores();
                    let (me, dialog_w, row_w) = (me.clone(), dialog_w.clone(), row_w.clone());
                    bg(
                        move || {
                            setup::install(threads, &move |what, done| {
                                let _ = tx.try_send((what.to_string(), done));
                            })
                        },
                        move |done| {
                            let Some(me) = me.upgrade() else { return };
                            match done {
                                Ok(local) => {
                                    crate::log::note("splitting is set up on this computer");
                                    me.settings.borrow_mut().local_split = Some(local);
                                    settings::save(&me.settings.borrow());
                                    me.toast("Splitting is set up: liked songs split here now");
                                    me.update_voice_status();
                                    // shown again, set up
                                    if let Some(dialog) = dialog_w.upgrade() {
                                        dialog.close();
                                        me.splitting_dialog();
                                    }
                                    me.split_liked_waiting();
                                }
                                Err(e) => {
                                    crate::log::note(&format!("setting splitting up failed: {e}"));
                                    if let Some(row) = row_w.upgrade() {
                                        row.set_title("Setting up stopped");
                                        row.set_subtitle(&format!("{e}\nThe details are in ~/.local/share/radiance-music/demucs.install.log. Set Up again to carry on."));
                                    }
                                    if let Some(b) = button.upgrade() {
                                        b.set_sensitive(true);
                                    }
                                }
                            }
                        },
                    );
                });
            }
        }
        if let Some(server) = self.settings.borrow().split.clone() {
            let srv = adw::PreferencesGroup::builder().title("Your split server").description("Set in settings.json: songs split there first; this computer helps when it's set up.").build();
            srv.add(&adw::ActionRow::builder().title(&server.host).subtitle("Splits, word timing and a library copy of each split song").build());
            page.add(&srv);
        }

        // what it needs, against this computer
        let needs = adw::PreferencesGroup::builder().title("What it needs").description("Minimum: it works. Recommended: a 4-minute song splits in about the time it plays.").build();
        let mark = |ok: bool| if ok { "✓ this computer" } else { "✗ this computer" };
        let rows = [
            ("Processor", format!("{} cores minimum · {}+ recommended (64-bit)", setup::MIN_CORES, setup::GOOD_CORES), format!("{} {} cores", mark(check.cores >= setup::MIN_CORES), check.cores)),
            ("Memory", format!("{:.0} GB minimum · {:.0} GB recommended", setup::MIN_RAM_GB, setup::GOOD_RAM_GB), format!("{} {:.1} GB", mark(check.ram_gb + 0.5 >= setup::MIN_RAM_GB), check.ram_gb)),
            ("Free space", format!("{:.0} GB (the install and its models), plus your songs' parts", setup::MIN_FREE_GB), format!("{} {:.1} GB free", mark(check.free_gb >= setup::MIN_FREE_GB), check.free_gb)),
            ("Python", "Python 3.9 or newer, with venv".to_string(), match &check.python {
                Some(v) => format!("✓ this computer: {v}"),
                None => "✗ this computer: not found".to_string(),
            }),
        ];
        for (title, need, have) in rows {
            needs.add(&adw::ActionRow::builder().title(title).subtitle(format!("{need}\n{have}")).subtitle_lines(2).build());
        }
        page.add(&needs);

        // how long it takes, measured
        let times = adw::PreferencesGroup::builder().title("How long it takes").description(setup::MEASURED_ON).build();
        for &(cores, split_min, words_min, peak_gb) in setup::TIMINGS {
            times.add(
                &adw::ActionRow::builder()
                    .title(format!("{cores} {}", if cores == 1 { "core" } else { "cores" }))
                    .subtitle(format!("Splitting a 4-minute song: {} · timing its words: {} · memory: {peak_gb:.1} GB at most", minutes(split_min), minutes(words_min)))
                    .build(),
            );
        }
        page.add(&times);
    }

    /// Liked songs waiting for a splitter: in line now.
    pub(super) fn split_liked_waiting(self: &Rc<Self>) {
        let todo: Vec<Track> = self.liked.borrow().iter().filter(|t| t.file.is_some() && !t.video && !split::has_parts(t)).cloned().collect();
        for t in todo.iter().take(200) {
            self.request_split(t);
        }
    }
}

/// 3.5 -> "about 3½ min", 0.75 -> "about 45 s".
fn minutes(m: f64) -> String {
    if m < 1.0 {
        format!("about {:.0} s", (m * 60.0 / 5.0).round() * 5.0)
    } else {
        format!("about {:.0} min", m.round().max(1.0))
    }
}
