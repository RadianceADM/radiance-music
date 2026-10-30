//! How to use it, the first time the app opens (owner, 2026-10-31: "add a easy
//! how to use, upon first enter on app"): five cards to page through, a few
//! plain words each, as the phone app's. Skip any time; the main menu's How to
//! Use It shows them again.

use super::*;

/// The cards: an icon, a title, a few words.
const CARDS: [(&str, &str, &str); 5] = [
    ("system-search-symbolic", "Search Any Song", "Type a song or an artist at the top. Lossless plays first when there is one; YouTube Music has the rest."),
    ("heart-filled-symbolic", "Like It, It's Yours", "Click the heart: the song, its cover, its lyrics and its music video are saved in your Music folder and play with no internet."),
    ("edit-cut-symbolic", "Split It, Mix It", "Split a liked song into drums, bass, other and vocals right on this computer, then turn each one up or down in the Mixer. No vocals makes it karaoke."),
    ("format-justify-center-symbolic", "Sing Along", "In Now Playing the lyrics light up word by word as they're sung, big in the middle, with the background moving to the beat."),
    ("phone-symbolic", "On Your Phone Too", "Get Radiance Music for Android at music.radianceph.com, then Share Songs with a Phone (main menu) sends it your songs, with their parts and lyrics."),
];

impl Ui {
    /// The guide, over the window; closing it any way counts as seen.
    pub(super) fn show_guide(self: &Rc<Self>) {
        let carousel = adw::Carousel::builder().hexpand(true).vexpand(true).allow_scroll_wheel(true).build();
        for (icon, title, words) in CARDS {
            let page = adw::StatusPage::builder().icon_name(icon).title(title).description(words).hexpand(true).build();
            page.add_css_class("compact");
            carousel.append(&page);
        }
        let dots = adw::CarouselIndicatorDots::builder().carousel(&carousel).build();
        let skip = gtk::Button::builder().label("Skip").css_classes(["flat"]).build();
        let next = gtk::Button::builder().css_classes(["suggested-action", "pill"]).build();
        let next_label = adw::ButtonContent::builder().icon_name("go-next-symbolic").label("Next").build();
        next.set_child(Some(&next_label));
        let bar = gtk::CenterBox::builder().margin_start(18).margin_end(18).margin_bottom(18).build();
        bar.set_start_widget(Some(&skip));
        bar.set_center_widget(Some(&dots));
        bar.set_end_widget(Some(&next));
        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        body.append(&carousel);
        body.append(&bar);
        let view = adw::ToolbarView::new();
        view.add_top_bar(&adw::HeaderBar::builder().show_title(false).build());
        view.set_content(Some(&body));
        let dialog = adw::Dialog::builder().title("How to Use It").content_width(560).content_height(520).child(&view).build();

        // the last card says Start Listening
        let last = CARDS.len() as f64 - 1.0;
        let label = next_label.clone();
        carousel.connect_position_notify(move |c| {
            let end = c.position() >= last - 0.01;
            label.set_label(if end { "Start Listening" } else { "Next" });
            label.set_icon_name(if end { "media-playback-start-symbolic" } else { "go-next-symbolic" });
        });
        let (c, d) = (carousel.clone(), dialog.clone());
        next.connect_clicked(move |_| {
            let at = c.position().round() as u32;
            if at + 1 >= c.n_pages() {
                d.close();
            } else {
                c.scroll_to(&c.nth_page(at + 1), true);
            }
        });
        let d = dialog.clone();
        skip.connect_clicked(move |_| {
            d.close();
        });
        let me = Rc::downgrade(self);
        dialog.connect_closed(move |_| {
            if let Some(me) = me.upgrade() {
                if !me.settings.borrow().guide_seen {
                    me.settings.borrow_mut().guide_seen = true;
                    settings::save(&me.settings.borrow());
                }
            }
        });
        dialog.present(Some(&self.window));
    }
}
