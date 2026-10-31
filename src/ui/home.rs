//! The shell (a sidebar of pages, like Apple Music's), Home, the library grids
//! (albums, artists, genres) and a group's own page, Search's browse tiles, and
//! picking up where you left off.

use super::*;
use crate::session;

/// The sidebar: (page, label, icon, heading above it).
const NAV: [(&str, &str, &str, Option<&str>); 9] = [
    ("home", "Home", "go-home-symbolic", None),
    ("search", "Search", "system-search-symbolic", None),
    ("playing", "Now Playing", "audio-x-generic-symbolic", None),
    ("liked", "Liked Songs", "heart-filled-symbolic", Some("Library")),
    ("history", "History", "document-open-recent-symbolic", None),
    ("albums", "Albums", "media-optical-symbolic", None),
    ("artists", "Artists", "avatar-default-symbolic", None),
    ("genres", "Genres", "view-grid-symbolic", None),
    // its own app: opens Speaker EQ (the whole computer's sound)
    ("eq", "Speaker EQ", "equalizer-symbolic", Some("Sound")),
];

/// Search's tiles before anything is typed: (what it says, what it searches).
const BROWSE: [(&str, &str); 12] = [
    ("Pinoy hip-hop", "pinoy hip hop"),
    ("OPM", "OPM hits"),
    ("R&B", "r&b"),
    ("Pop", "pop hits"),
    ("K-pop", "kpop"),
    ("Lo-fi", "lofi beats"),
    ("Live concerts", "live concert"),
    ("Jazz", "jazz"),
    ("Classical", "classical"),
    ("Rock", "rock"),
    ("Acoustic", "acoustic"),
    ("Reggae", "reggae"),
];

/// Cards in a row on Home.
const ROW_CARDS: usize = 20;

/// Home's sections, top to bottom.
#[derive(Clone, Copy)]
pub(super) enum HomePart {
    Hero,
    Mix,
    Recent,
    Liked,
    Albums,
    Artists,
    Genres,
}
const HOME_PARTS: [HomePart; 7] = [HomePart::Hero, HomePart::Mix, HomePart::Recent, HomePart::Liked, HomePart::Albums, HomePart::Artists, HomePart::Genres];

impl HomePart {
    /// Its bit in Ui::home_stale.
    pub(super) const fn bit(self) -> u8 {
        1 << self as u8
    }
}
/// All of Home.
pub(super) const ALL_OF_HOME: u8 = (1 << HOME_PARTS.len()) - 1;
const CARD: i32 = 180;
const PHONE_CARD: i32 = 136;
/// The window's least height: the title bar and the player bar, the mini player.
const MINI_HEIGHT: i32 = 84;

/// The two dark looks (picked in the menu; only while the style is dark).
/// Stone: warm greys.
pub(super) const STONE: &str = "
:root {
  --window-bg-color: #1C1917; --view-bg-color: #1C1917; --headerbar-bg-color: #1C1917;
  --headerbar-shade-color: alpha(black, 0.35);
  --sidebar-bg-color: #0C0A09; --secondary-sidebar-bg-color: #0C0A09;
  --card-bg-color: alpha(#ffffff, 0.05); --popover-bg-color: #292524; --dialog-bg-color: #292524;
  --window-fg-color: #F5F5F4; --view-fg-color: #F5F5F4; --sidebar-fg-color: #F5F5F4;
}
";
/// True black: the art does all the colour.
pub(super) const BLACK: &str = "
:root {
  --window-bg-color: #000000; --view-bg-color: #000000; --headerbar-bg-color: #000000;
  --headerbar-shade-color: alpha(#ffffff, 0.08);
  --sidebar-bg-color: #000000; --secondary-sidebar-bg-color: #000000; --sidebar-border-color: #1A1A1A;
  --card-bg-color: #111111; --popover-bg-color: #161616; --dialog-bg-color: #161616;
  --window-fg-color: #FFFFFF; --view-fg-color: #FFFFFF; --sidebar-fg-color: #FFFFFF;
}
";

pub(super) const CSS: &str = "
:root { --sidebar-backdrop-color: var(--sidebar-bg-color); --headerbar-backdrop-color: var(--headerbar-bg-color); }
.inset-shell:backdrop, .inset-shell:backdrop .playbar, .inset-shell .sidebar-pane:backdrop { background-color: var(--sidebar-bg-color); }
.inset-shell, .inset-shell .playbar { background-color: var(--sidebar-bg-color); }
.inset-page { margin: 8px 8px 8px 0; border-radius: 14px; background-color: var(--view-bg-color); }
/* fullscreen: the page edge to edge, a video on black */
window.fullscreen-player .inset-page { margin: 0; border-radius: 0; }
window.fullscreen-player .page-area { border-radius: 0; }
.full-video { background-color: black; }
/* no lines between the panes: the sidebar, the card and the side panel just meet */
.inset-shell .sidebar-pane, .inset-shell .content-pane { box-shadow: none; border-style: none; }
.inset-shell separator { background: none; min-width: 0; min-height: 0; }
/* the side panel is part of the card (its colour), not a grey box inside it */
.inset-page .sidebar-pane { background-color: transparent; }
.inset-page > headerbar, .inset-page headerbar { background: none; box-shadow: none; }
.card-art, .round-art, .fan-art { background-color: alpha(#ffffff, 0.06); }
.brand { font-weight: 800; font-size: 1.05em; }
.nav-section { font-weight: 700; font-size: 0.85em; opacity: 0.55; margin: 14px 12px 2px 12px; }
.page-title { font-size: 2.1em; font-weight: 800; letter-spacing: -1px; }
.section-title { font-size: 1.45em; font-weight: 800; letter-spacing: -0.5px; }
.hero { border-radius: 26px; }
.hero-scrim { background: linear-gradient(to right, alpha(black, 0.85) 0%, alpha(black, 0.5) 45%, alpha(black, 0.05) 100%); }
.hero-title { font-size: 2.6em; font-weight: 800; letter-spacing: -1.5px; color: #ffffff; }
.hero-sub { font-size: 1.1em; color: alpha(#ffffff, 0.8); }
.pill-button { border-radius: 999px; padding: 8px 20px; font-weight: 700; }
.card-button { padding: 6px; border-radius: 16px; background: none; }
.card-button:hover .card-art { filter: brightness(1.08); }
.card-art { border-radius: 10px; }
.round-art { border-radius: 999px; }
.fan-art { border-radius: 10px; box-shadow: 0 14px 30px alpha(black, 0.5); }
.card-title { font-weight: 700; }
.card-sub { opacity: 0.62; font-size: 0.92em; }
.tile { border-radius: 16px; padding: 0; }
.tile-label { font-size: 1.25em; font-weight: 800; color: #ffffff; margin: 14px 16px; }
.tile-scrim { background: linear-gradient(to top, alpha(black, 0.55), alpha(black, 0.05)); }
.browse-tile { border-radius: 16px; min-height: 110px; color: #ffffff; font-size: 1.2em; font-weight: 800; }
.b0 { background: linear-gradient(135deg, #F38A35, #B8452A); }
.b1 { background: linear-gradient(135deg, #9B2D3F, #3A1016); }
.b2 { background: linear-gradient(135deg, #C2367A, #5B1A5E); }
.b3 { background: linear-gradient(135deg, #17907E, #0C3B4A); }
.b4 { background: linear-gradient(135deg, #7A4FD6, #2B1B5C); }
.b5 { background: linear-gradient(135deg, #FFAE63, #C46A1D); }
";

fn greeting() -> String {
    let hour = glib::DateTime::now_local().map(|d| d.hour()).unwrap_or(12);
    let part = match hour {
        5..=11 => "Good morning",
        12..=17 => "Good afternoon",
        _ => "Good evening",
    };
    // (RADIANCE_SHOT_ANON: screenshots for others say no name)
    let name = if std::env::var_os("RADIANCE_SHOT_ANON").is_some() {
        String::new()
    } else {
        glib::real_name().to_string_lossy().split_whitespace().next().unwrap_or_default().to_string()
    };
    if name.is_empty() || name.eq_ignore_ascii_case("unknown") {
        part.to_string()
    } else {
        format!("{part}, {name}")
    }
}

fn clear(b: &gtk::Box) {
    while let Some(c) = b.first_child() {
        b.remove(&c);
    }
}

impl Ui {
    // ------------------------------------------------------------------ shell

    /// The window: sidebar | page (+ the queue panel), the player bar below.
    pub(super) fn shell(self: &Rc<Self>, bar: &gtk::CenterBox, menu_button: &gtk::MenuButton) {
        // sidebar
        let brand = gtk::Box::builder().spacing(8).build();
        brand.append(&gtk::Image::builder().icon_name(APP_ID).pixel_size(22).build());
        brand.append(&gtk::Label::builder().label("Radiance Music").css_classes(["brand"]).build());
        let side_header = adw::HeaderBar::builder().show_end_title_buttons(false).title_widget(&brand).build();
        for (page, label, icon, heading) in NAV {
            if let Some(h) = heading {
                let head = gtk::ListBoxRow::builder().selectable(false).activatable(false).build();
                head.set_child(Some(&gtk::Label::builder().label(h).xalign(0.0).css_classes(["nav-section"]).build()));
                self.nav.append(&head);
            }
            let line = gtk::Box::builder().spacing(12).build();
            line.append(&gtk::Image::from_icon_name(icon));
            line.append(&gtk::Label::builder().label(label).xalign(0.0).build());
            let row = gtk::ListBoxRow::builder().child(&line).name(page).build();
            self.nav.append(&row);
        }
        let me = Rc::downgrade(self);
        self.nav.connect_row_selected(move |_, row| {
            if let (Some(me), Some(row)) = (me.upgrade(), row) {
                let page = row.widget_name();
                if let Some(id) = page.strip_prefix("playlist:").and_then(|i| i.parse::<i64>().ok()) {
                    if me.playlist_now.get() != Some(id) || me.stack.visible_child_name().as_deref() != Some("playlist") {
                        me.open_playlist(id);
                    }
                } else if let Some(address) = page.strip_prefix("server:") {
                    if me.shown_server().as_deref() != Some(address) || me.stack.visible_child_name().as_deref() != Some("server") {
                        me.open_server(address);
                    }
                } else if page == "new-playlist" {
                    me.new_playlist(None);
                } else if page == "eq" {
                    me.open_speaker_eq();
                    me.sync_nav(); // the page you were on stays picked
                } else if !page.is_empty() && me.stack.visible_child_name().as_deref() != Some(page.as_str()) {
                    me.stack.set_visible_child_name(&page);
                }
            }
        });
        let side_scroll = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&self.nav).build();
        let side = adw::ToolbarView::new();
        side.add_top_bar(&side_header);
        side.set_content(Some(&side_scroll));
        // bottom left: the playing song's mixer (when it's split), then what's being split
        let corner = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
        corner.append(&self.mixer_list);
        corner.append(&self.split_card);
        side.add_bottom_bar(&corner);

        // content
        let content_header = adw::HeaderBar::builder().show_start_title_buttons(false).title_widget(&gtk::Box::new(gtk::Orientation::Horizontal, 0)).build();
        let show_side = gtk::ToggleButton::builder().icon_name("sidebar-show-symbolic").tooltip_text("Show the sidebar").visible(false).build();
        content_header.pack_start(&show_side);
        content_header.pack_end(menu_button);
        content_header.pack_end(&self.offline_pill);
        let content = adw::ToolbarView::new();
        content.add_top_bar(&content_header);
        content.add_top_bar(&self.banner);
        content.set_content(Some(&self.split));
        // the page sits on the sidebar's colour as a card with soft corners (an inset layout)
        content.add_css_class("inset-page");
        content.set_overflow(gtk::Overflow::Hidden);

        let nav_split = adw::OverlaySplitView::builder().min_sidebar_width(220.0).max_sidebar_width(260.0).build();
        nav_split.set_sidebar(Some(&side));
        nav_split.set_content(Some(&content));
        show_side.bind_property("active", &nav_split, "show-sidebar").bidirectional().build();
        // a narrow window: the sidebar slides over, from the button
        // Window sizes. Only ONE breakpoint applies at a time (the last added of those that
        // match), so each narrower one repeats the wider ones' setters, widest added first.
        // Under 1300: the side panel slides over the page instead of sitting beside it.
        let tight = adw::Breakpoint::new(adw::BreakpointCondition::parse("max-width: 1300sp").unwrap());
        tight.add_setter(&self.split, "collapsed", Some(&true.to_value()));
        self.window.add_breakpoint(tight);
        // Under 1060: also no room for art and lyrics side by side (the lyrics stay in the panel's tab).
        let snug = adw::Breakpoint::new(adw::BreakpointCondition::parse("max-width: 1060sp").unwrap());
        snug.add_setter(&self.split, "collapsed", Some(&true.to_value()));
        snug.add_setter(&self.lyrics_scroll, "visible", Some(&false.to_value()));
        self.window.add_breakpoint(snug);
        // Under 860: also the sidebar slides over, from its button, and Now Playing's art
        // stands above (its lyrics are in the panel's tab).
        let (np, left, middle) = self.np_parts.borrow().clone().expect("Now Playing is built before the shell");
        // the bar's quality pill, by its slot (see layout)
        let quality = self.bar_quality.parent().expect("the bar is built before the shell");
        let narrow = adw::Breakpoint::new(adw::BreakpointCondition::parse("max-width: 860sp").unwrap());
        narrow.add_setter(&self.split, "collapsed", Some(&true.to_value()));
        narrow.add_setter(&self.lyrics_scroll, "visible", Some(&false.to_value()));
        narrow.add_setter(&nav_split, "collapsed", Some(&true.to_value()));
        narrow.add_setter(&show_side, "visible", Some(&true.to_value()));
        narrow.add_setter(&np, "orientation", Some(&gtk::Orientation::Vertical.to_value()));
        narrow.add_setter(&left, "halign", Some(&gtk::Align::Center.to_value()));
        // the bar keeps the mute button; the volume slider and the quality badge make room
        narrow.add_setter(&self.volume, "visible", Some(&false.to_value()));
        narrow.add_setter(&quality, "visible", Some(&false.to_value()));
        self.window.add_breakpoint(narrow);
        // Under 600, a phone: all of that, a smaller cover and page margins, and the bar keeps
        // what fits (the song, play / skip, the seek bar, Like and the queue; the system's
        // volume does the rest). Cards and the hero are drawn smaller too (see `phone`).
        let phone = adw::Breakpoint::new(adw::BreakpointCondition::parse("max-width: 600sp").unwrap());
        phone.add_setter(&self.split, "collapsed", Some(&true.to_value()));
        phone.add_setter(&self.lyrics_scroll, "visible", Some(&false.to_value()));
        phone.add_setter(&nav_split, "collapsed", Some(&true.to_value()));
        phone.add_setter(&show_side, "visible", Some(&true.to_value()));
        phone.add_setter(&np, "orientation", Some(&gtk::Orientation::Vertical.to_value()));
        phone.add_setter(&left, "halign", Some(&gtk::Align::Center.to_value()));
        phone.add_setter(&np, "margin-start", Some(&12i32.to_value()));
        phone.add_setter(&np, "margin-end", Some(&12i32.to_value()));
        phone.add_setter(&np, "margin-top", Some(&12i32.to_value()));
        phone.add_setter(&np, "spacing", Some(&16i32.to_value()));
        phone.add_setter(&left, "width-request", Some(&(-1i32).to_value()));
        phone.add_setter(&self.cover, "width-request", Some(&240i32.to_value()));
        phone.add_setter(&self.cover, "height-request", Some(&240i32.to_value()));
        phone.add_setter(&middle, "width-request", Some(&(-1i32).to_value()));
        let phone_hides = [self.volume.upcast_ref::<gtk::Widget>(), self.mute.upcast_ref(), quality.upcast_ref(), self.lyrics_button.upcast_ref(), self.shuffle.upcast_ref(), self.repeat_button.upcast_ref(), self.bar_byline.upcast_ref(), self.pos.upcast_ref(), self.len.upcast_ref()];
        for w in phone_hides {
            phone.add_setter(w, "visible", Some(&false.to_value()));
        }
        let me = Rc::downgrade(self);
        phone.connect_apply(move |_| {
            if let Some(me) = me.upgrade() {
                me.set_phone(true);
            }
        });
        let me = Rc::downgrade(self);
        phone.connect_unapply(move |_| {
            if let Some(me) = me.upgrade() {
                me.set_phone(false);
            }
        });
        self.window.add_breakpoint(phone);
        // A short window (under 240): a mini player, just the song, its controls and the
        // window's own buttons (the pages and the title bar step aside). Wide, the bar as
        // it is; narrow as well, the phone's bar. Taller than that, the pages scroll.
        let mini = adw::Breakpoint::new(adw::BreakpointCondition::parse("max-height: 240sp").unwrap());
        mini.add_setter(&nav_split, "visible", Some(&false.to_value()));
        mini.add_setter(&self.window_buttons, "visible", Some(&true.to_value()));
        // the lyrics and the queue open panels the mini player doesn't have
        mini.add_setter(&self.lyrics_button, "visible", Some(&false.to_value()));
        mini.add_setter(&self.queue_button, "visible", Some(&false.to_value()));
        mini.add_setter(&self.volume, "visible", Some(&false.to_value()));
        mini.add_setter(&quality, "visible", Some(&false.to_value()));
        self.window.add_breakpoint(mini);
        let mini_phone = adw::Breakpoint::new(adw::BreakpointCondition::parse("max-height: 240sp and max-width: 600sp").unwrap());
        mini_phone.add_setter(&nav_split, "visible", Some(&false.to_value()));
        mini_phone.add_setter(&self.window_buttons, "visible", Some(&true.to_value()));
        mini_phone.add_setter(&self.queue_button, "visible", Some(&false.to_value()));
        mini_phone.add_setter(&middle, "width-request", Some(&(-1i32).to_value()));
        for w in phone_hides {
            mini_phone.add_setter(w, "visible", Some(&false.to_value()));
        }
        self.window.add_breakpoint(mini_phone);
        // never narrower than a phone (360); as short as the mini player
        self.window.set_width_request(360);
        self.window.set_height_request(MINI_HEIGHT);

        let view = adw::ToolbarView::new();
        view.add_css_class("inset-shell");
        view.set_content(Some(&nav_split));
        view.add_bottom_bar(bar);
        view.set_bottom_bar_style(adw::ToolbarStyle::Flat); // the bar is part of the frame, the sidebar's colour
        self.window.set_content(Some(&view));
        *self.shell_parts.borrow_mut() = Some((nav_split.clone(), content.clone(), view.clone())); // fullscreen hides them

        // the sidebar follows the page shown (also when the app moves there)
        let me = Rc::downgrade(self);
        self.stack.connect_visible_child_name_notify(move |st| {
            let (Some(me), Some(name)) = (me.upgrade(), st.visible_child_name()) else { return };
            let name = match name.as_str() {
                "group" => me.group_back.borrow().clone(),
                "playlist" => me.playlist_now.get().map(|id| format!("playlist:{id}")).unwrap_or_default(),
                "server" => me.shown_server().map(|a| format!("server:{a}")).unwrap_or_default(),
                _ => name.to_string(),
            };
            let mut child = me.nav.first_child();
            while let Some(c) = child {
                if let Some(row) = c.downcast_ref::<gtk::ListBoxRow>() {
                    if row.widget_name() == name && !row.is_selected() {
                        me.nav.select_row(Some(row));
                    }
                }
                child = c.next_sibling();
            }
            if name == "home" && me.home_stale.get() != 0 {
                me.refresh_home();
            }
            if name == "history" {
                me.render_history();
            }
        });
        self.stack.set_visible_child_name("home");
        if let Some(first) = self.nav.row_at_index(0) {
            self.nav.select_row(Some(&first));
        }
    }

    // ------------------------------------------------------------------ cards

    /// A phone-sized window or not: the cards, the hero and the grids' margins drawn for it.
    fn set_phone(self: &Rc<Self>, on: bool) {
        if self.phone.replace(on) == on {
            return;
        }
        let m = if on { 12 } else { 28 };
        for c in self.grid_columns.borrow().iter() {
            c.set_margin_start(m);
            c.set_margin_end(m);
        }
        for w in self.row_extras.borrow().iter().filter_map(|w| w.upgrade()) {
            w.set_visible(!on);
        }
        self.render_home();
        self.render_library();
    }

    /// A card's side: smaller on a phone, so two fit side by side.
    fn card_size(&self) -> i32 {
        if self.phone.get() { PHONE_CARD } else { CARD }
    }

    /// A song as artwork with its title and artist; click plays (from it, on
    /// through its row), right-click for its menu.
    fn card(self: &Rc<Self>, t: &Track, place: Place) -> gtk::Button {
        let pic = gtk::Picture::builder().content_fit(gtk::ContentFit::Cover).can_shrink(true).overflow(gtk::Overflow::Hidden).css_classes(["card-art"]).build();
        let side = self.card_size();
        pic.set_size_request(side, side);
        let weak = pic.downgrade();
        self.set_art(&art_source(t), (CARD * 2) as u32, move |tex| {
            if let Some(p) = weak.upgrade() {
                p.set_paintable(Some(tex));
            }
        });
        let b = self.card_frame(&pic, &t.title, &t.credit());
        let me = Rc::downgrade(self);
        let p = place.clone();
        b.connect_clicked(move |_| {
            if let Some(me) = me.upgrade() {
                me.play_from(&p);
            }
        });
        self.attach_menu(b.upcast_ref(), place);
        b
    }

    fn card_frame(&self, art: &impl IsA<gtk::Widget>, title: &str, sub: &str) -> gtk::Button {
        let column = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(4).width_request(self.card_size()).build();
        column.append(art);
        let t = gtk::Label::builder().label(title).xalign(0.0).ellipsize(gtk::pango::EllipsizeMode::End).max_width_chars(18).margin_top(6).css_classes(["card-title"]).build();
        let s = gtk::Label::builder().label(sub).xalign(0.0).ellipsize(gtk::pango::EllipsizeMode::End).max_width_chars(20).css_classes(["card-sub"]).build();
        column.append(&t);
        if !sub.is_empty() {
            column.append(&s);
        }
        gtk::Button::builder().child(&column).css_classes(["card-button"]).tooltip_text(if sub.is_empty() { title.to_string() } else { format!("{title}\n{sub}") }).build()
    }

    /// An album or an artist (round): opens its page.
    fn group_card(self: &Rc<Self>, name: &str, songs: Vec<Track>, round: bool, from: &str) -> gtk::Button {
        let pic = gtk::Picture::builder().content_fit(gtk::ContentFit::Cover).can_shrink(true).overflow(gtk::Overflow::Hidden).build();
        pic.add_css_class(if round { "round-art" } else { "card-art" });
        let side = self.card_size();
        pic.set_size_request(side, side);
        let weak = pic.downgrade();
        self.set_art(&art_source(&songs[0]), (CARD * 2) as u32, move |tex| {
            if let Some(p) = weak.upgrade() {
                p.set_paintable(Some(tex));
            }
        });
        let n = songs.len();
        let sub = if round { format!("{n} song{}", if n == 1 { "" } else { "s" }) } else { songs[0].credit() };
        let b = self.card_frame(&pic, name, &sub);
        let me = Rc::downgrade(self);
        let (name, from) = (name.to_string(), from.to_string());
        b.connect_clicked(move |_| {
            if let Some(me) = me.upgrade() {
                me.open_group(&name, songs.clone(), &from);
            }
        });
        b
    }

    /// A genre: painted with its own songs' colours.
    fn genre_tile(self: &Rc<Self>, name: &str, songs: Vec<Track>, wide: i32) -> gtk::Button {
        let wash = gtk::Picture::builder().content_fit(gtk::ContentFit::Cover).can_shrink(true).build();
        let weak = wash.downgrade();
        let src = art_source(&songs[0]);
        let me = Rc::downgrade(self);
        bg_art(move || art::backdrop(&src), move |px| {
            if let (Some(px), Some(p), Some(_me)) = (px, weak.upgrade(), me.upgrade()) {
                p.set_paintable(Some(&texture(px)));
            }
        });
        let tile = gtk::Overlay::builder().overflow(gtk::Overflow::Hidden).css_classes(["tile"]).build();
        tile.set_size_request(wide, 120);
        tile.set_child(Some(&gtk::Box::new(gtk::Orientation::Vertical, 0)));
        tile.add_overlay(&wash);
        tile.set_clip_overlay(&wash, true);
        let scrim = gtk::Box::builder().css_classes(["tile-scrim"]).build();
        tile.add_overlay(&scrim);
        let label = gtk::Label::builder().label(name).xalign(0.0).valign(gtk::Align::End).halign(gtk::Align::Start).wrap(true).wrap_mode(gtk::pango::WrapMode::WordChar).css_classes(["tile-label"]).build();
        tile.add_overlay(&label);
        tile.set_measure_overlay(&label, true);
        let b = gtk::Button::builder().child(&tile).css_classes(["card-button"]).build();
        let me = Rc::downgrade(self);
        let name = name.to_string();
        b.connect_clicked(move |_| {
            if let Some(me) = me.upgrade() {
                me.open_group(&name, songs.clone(), "genres");
            }
        });
        b
    }

    /// A titled row of cards that scrolls sideways, with ‹ › buttons.
    fn carousel(&self, title: &str, cards: Vec<gtk::Widget>, extra: Option<gtk::Widget>) -> gtk::Box {
        let section = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(10).build();
        let head = gtk::Box::builder().spacing(6).build();
        head.append(&gtk::Label::builder().label(title).xalign(0.0).hexpand(true).css_classes(["section-title"]).build());
        if let Some(e) = extra {
            head.append(&e);
        }
        let strip = gtk::Box::builder().spacing(12).build();
        for c in &cards {
            strip.append(c);
        }
        let scroll = gtk::ScrolledWindow::builder().vscrollbar_policy(gtk::PolicyType::Never).hscrollbar_policy(gtk::PolicyType::External).child(&strip).build();
        for (icon, tip, dir) in [("go-previous-symbolic", "Back", -1.0), ("go-next-symbolic", "More", 1.0)] {
            let b = gtk::Button::builder().icon_name(icon).tooltip_text(tip).css_classes(["flat", "circular"]).build();
            let adj = scroll.hadjustment();
            let s = scroll.downgrade();
            b.connect_clicked(move |_| {
                let Some(s) = s.upgrade() else { return };
                let to = (adj.value() + dir * adj.page_size() * 0.85).clamp(adj.lower(), (adj.upper() - adj.page_size()).max(0.0));
                let target = adw::PropertyAnimationTarget::new(&adj, "value");
                adw::TimedAnimation::new(&s, adj.value(), to, 380, target).play();
            });
            head.append(&b);
        }
        section.append(&head);
        section.append(&scroll);
        section
    }

    // ------------------------------------------------------------------ home

    pub(super) fn home_page(self: &Rc<Self>) -> gtk::ScrolledWindow {
        let clamp = adw::Clamp::builder().maximum_size(1480).tightening_threshold(1100).child(&self.home_box).build();
        gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&clamp).build()
    }

    /// Home again soon (after likes, the mix or the history change); if Home
    /// isn't on screen, when it next is.
    pub(super) fn render_home_later(self: &Rc<Self>) {
        self.home_later_parts(ALL_OF_HOME);
    }

    /// Those sections of Home again soon (`parts`: HomePart bits), the rest
    /// left as they are; if Home isn't on screen, when it next is.
    pub(super) fn home_later_parts(self: &Rc<Self>, parts: u8) {
        self.home_stale.set(self.home_stale.get() | parts);
        if self.home_stale.get() == 0 || self.starting.get() || self.stack.visible_child_name().as_deref() != Some("home") {
            return;
        }
        if let Some(id) = self.home_later.borrow_mut().take() {
            id.remove();
        }
        let me = Rc::downgrade(self);
        let id = glib::timeout_add_local_once(Duration::from_millis(400), move || {
            if let Some(me) = me.upgrade() {
                me.home_later.borrow_mut().take();
                me.refresh_home();
            }
        });
        self.home_later.replace(Some(id));
    }

    /// The sections of Home that changed, built again in their places.
    pub(super) fn refresh_home(self: &Rc<Self>) {
        let stale = self.home_stale.replace(0);
        if stale == ALL_OF_HOME || self.home_sections.borrow().iter().all(Option::is_none) {
            return self.render_home();
        }
        crate::watch::doing("building Home");
        let t = Instant::now();
        for (i, part) in HOME_PARTS.into_iter().enumerate().filter(|(_, p)| stale & p.bit() != 0) {
            let new = self.home_section(part);
            let old = self.home_sections.borrow_mut()[i].take();
            match (&old, &new) {
                (Some(old), Some(new)) => {
                    self.home_box.insert_child_after(new, old.prev_sibling().as_ref());
                    self.home_box.remove(old);
                }
                (Some(old), None) => self.home_box.remove(old),
                (None, Some(new)) => {
                    // after the nearest section above it that's there (or first)
                    let above = self.home_sections.borrow()[..i].iter().rev().flatten().next().cloned();
                    self.home_box.insert_child_after(new, above.as_ref());
                }
                (None, None) => {}
            }
            self.home_sections.borrow_mut()[i] = new;
        }
        if std::env::var_os("RADIANCE_SHOT_SPLASH").is_some() {
            eprintln!("Home: sections {stale:#09b} again in {} ms", t.elapsed().as_millis());
        }
    }

    pub(super) fn render_home(self: &Rc<Self>) {
        crate::watch::doing("building Home");
        self.begin_home();
        for part in HOME_PARTS {
            self.home_part(part);
        }
    }

    /// Home as the splash's work, a section a frame (the start: it keeps moving between them).
    pub(super) fn render_home_behind(self: &Rc<Self>, splash: &super::splash::Splash) {
        let gen = Rc::new(Cell::new(0));
        let (me, g) = (Rc::downgrade(self), gen.clone());
        splash.then("Home", move || {
            if let Some(me) = me.upgrade() {
                g.set(me.begin_home());
            }
        });
        for part in HOME_PARTS {
            let (me, g) = (Rc::downgrade(self), gen.clone());
            splash.then("a Home section", move || {
                // (a newer build of Home took over: this one stops)
                if let Some(me) = me.upgrade().filter(|me| me.home_gen.get() == g.get()) {
                    crate::watch::doing("building Home");
                    me.home_part(part);
                }
            });
        }
    }

    /// A new build of Home: empty, and its number.
    fn begin_home(&self) -> u32 {
        self.home_stale.set(0);
        clear(&self.home_box);
        *self.home_sections.borrow_mut() = Default::default();
        let gen = self.home_gen.get().wrapping_add(1);
        self.home_gen.set(gen);
        gen
    }

    /// One of Home's sections, added at the bottom.
    fn home_part(self: &Rc<Self>, part: HomePart) {
        let section = self.home_section(part);
        if let Some(s) = &section {
            self.home_box.append(s);
        }
        self.home_sections.borrow_mut()[part as usize] = section;
    }

    /// One of Home's sections (None: nothing in it to show).
    fn home_section(self: &Rc<Self>, part: HomePart) -> Option<gtk::Widget> {
        let section = |title: &str, cards: Vec<gtk::Widget>, extra: Option<gtk::Widget>| (!cards.is_empty()).then(|| self.carousel(title, cards, extra).upcast());
        match part {
            HomePart::Hero => Some(self.hero()),
            HomePart::Mix => {
                let mix = Rc::new(self.mix.borrow().clone());
                let cards = mix.iter().take(ROW_CARDS).enumerate().map(|(i, t)| self.card(t, Place::List(mix.clone(), i)).upcast()).collect();
                let fresh = gtk::Button::builder().icon_name("view-refresh-symbolic").tooltip_text("New picks").css_classes(["flat", "circular"]).build();
                let me = Rc::downgrade(self);
                fresh.connect_clicked(move |_| {
                    if let Some(me) = me.upgrade() {
                        me.refresh_mix();
                    }
                });
                section("Made for you", cards, Some(fresh.upcast()))
            }
            HomePart::Recent => {
                let history = Rc::new(session::recent(&self.history.borrow()));
                let cards = history.iter().take(ROW_CARDS).enumerate().map(|(i, t)| self.card(t, Place::List(history.clone(), i)).upcast()).collect();
                section("Recently played", cards, None)
            }
            HomePart::Liked => {
                let liked = Rc::new(self.liked.borrow().clone());
                let cards = liked.iter().take(ROW_CARDS).enumerate().map(|(i, t)| self.card(t, Place::Liked(liked.clone(), i)).upcast()).collect();
                let all = gtk::Button::builder().label("See all").css_classes(["flat"]).build();
                let me = Rc::downgrade(self);
                all.connect_clicked(move |_| {
                    if let Some(me) = me.upgrade() {
                        me.stack.set_visible_child_name("liked");
                    }
                });
                section("Your liked songs", cards, Some(all.upcast()))
            }
            HomePart::Albums => {
                let liked = self.liked.borrow().clone();
                let albums = self
                    .groups_by(&liked, LikedView::Albums)
                    .into_iter()
                    .filter(|(name, _)| name != "Singles")
                    .take(ROW_CARDS)
                    .map(|(name, songs)| self.group_card(&name, songs, false, "albums").upcast())
                    .collect();
                section("Your albums", albums, None)
            }
            HomePart::Artists => {
                let liked = self.liked.borrow().clone();
                let mut artists = self.groups_by(&liked, LikedView::Artists);
                artists.sort_by_key(|(_, songs)| std::cmp::Reverse(songs.len()));
                let artists = artists.into_iter().take(ROW_CARDS).map(|(name, songs)| self.group_card(&name, songs, true, "artists").upcast()).collect();
                section("Your artists", artists, None)
            }
            HomePart::Genres => {
                let liked = self.liked.borrow().clone();
                let mut genres = self.groups_by(&liked, LikedView::Genres);
                genres.retain(|(name, _)| !name.ends_with('…'));
                genres.sort_by_key(|(_, songs)| std::cmp::Reverse(songs.len()));
                let wide = if self.phone.get() { PHONE_CARD } else { 260 };
                let tiles = genres.into_iter().take(ROW_CARDS).map(|(name, songs)| self.genre_tile(&name, songs, wide).upcast()).collect();
                section("Your genres", tiles, None)
            }
        }
    }

    /// The top of Home: a greeting over your mix's own colours, its covers
    /// fanned out like records, and play.
    fn hero(self: &Rc<Self>) -> gtk::Widget {
        let mix = self.mix.borrow().clone();
        let liked = self.liked.borrow().clone();
        let covers: Vec<Track> = if mix.is_empty() { liked.iter().take(4).cloned().collect() } else { mix.iter().take(4).cloned().collect() };
        let hero = gtk::Overlay::builder().overflow(gtk::Overflow::Hidden).css_classes(["hero"]).margin_top(6).build();
        hero.set_child(Some(&gtk::Box::new(gtk::Orientation::Vertical, 0)));
        if let Some(first) = covers.first() {
            let wash = gtk::Picture::builder().content_fit(gtk::ContentFit::Cover).can_shrink(true).build();
            let weak = wash.downgrade();
            let src = art_source(first);
            bg_art(move || art::backdrop(&src), move |px| {
                if let (Some(px), Some(p)) = (px, weak.upgrade()) {
                    p.set_paintable(Some(&texture(px)));
                }
            });
            hero.add_overlay(&wash);
            hero.set_clip_overlay(&wash, true);
        } else {
            hero.add_css_class("b1");
        }
        let scrim = gtk::Box::builder().css_classes(["hero-scrim"]).build();
        hero.add_overlay(&scrim);

        let roomy = if self.phone.get() { 20 } else { 40 };
        let content = gtk::Box::builder().spacing(24).margin_top(34).margin_bottom(34).margin_start(roomy).margin_end(roomy).build();
        let words = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(10).valign(gtk::Align::Center).hexpand(true).build();
        words.append(&gtk::Label::builder().label(greeting()).xalign(0.0).wrap(true).wrap_mode(gtk::pango::WrapMode::WordChar).css_classes(["hero-title"]).build());
        let (sub, has_mix) = if !mix.is_empty() {
            (format!("Your mix is ready: {} songs like the ones you liked.", mix.len()), true)
        } else if !liked.is_empty() {
            ("Finding songs like the ones you liked…".to_string(), false)
        } else {
            ("Like a few songs and a mix made from them shows up here.".to_string(), false)
        };
        words.append(&gtk::Label::builder().label(&sub).xalign(0.0).wrap(true).wrap_mode(gtk::pango::WrapMode::WordChar).max_width_chars(46).css_classes(["hero-sub"]).build());
        let buttons = gtk::Box::builder().spacing(10).margin_top(10).build();
        if has_mix {
            let play = gtk::Button::builder().label("Play your mix").css_classes(["suggested-action", "pill-button"]).build();
            let shuffle = gtk::Button::builder().label("Shuffle").css_classes(["pill-button"]).build();
            let me = Rc::downgrade(self);
            play.connect_clicked(move |_| {
                if let Some(me) = me.upgrade() {
                    let mix = me.mix.borrow().clone();
                    me.play_list(mix, 0);
                }
            });
            let me = Rc::downgrade(self);
            shuffle.connect_clicked(move |_| {
                if let Some(me) = me.upgrade() {
                    let mix = shuffled(me.mix.borrow().clone());
                    me.play_list(mix, 0);
                }
            });
            buttons.append(&play);
            buttons.append(&shuffle);
        } else {
            let find = gtk::Button::builder().label("Search for music").css_classes(["suggested-action", "pill-button"]).build();
            let me = Rc::downgrade(self);
            find.connect_clicked(move |_| {
                if let Some(me) = me.upgrade() {
                    me.stack.set_visible_child_name("search");
                    me.search.grab_focus();
                }
            });
            buttons.append(&find);
        }
        words.append(&buttons);
        content.append(&words);

        // the covers, fanned (fixed size: never resizes the page)
        let fan = gtk::Fixed::builder().halign(gtk::Align::End).valign(gtk::Align::Center).build();
        fan.set_size_request(400, 250);
        let spots = [(0.0, 52.0, -9.0), (78.0, 30.0, -3.0), (156.0, 40.0, 3.5), (234.0, 20.0, 9.0)];
        for (t, (x, y, turn)) in covers.iter().zip(spots) {
            let pic = gtk::Picture::builder().content_fit(gtk::ContentFit::Cover).can_shrink(true).overflow(gtk::Overflow::Hidden).css_classes(["fan-art"]).build();
            pic.set_size_request(160, 160);
            let weak = pic.downgrade();
            self.set_art(&art_source(t), 320, move |tex| {
                if let Some(p) = weak.upgrade() {
                    p.set_paintable(Some(tex));
                }
            });
            fan.put(&pic, 0.0, 0.0);
            let turn_at = gtk::gsk::Transform::new()
                .translate(&gtk::graphene::Point::new(x + 80.0, y + 80.0))
                .rotate(turn)
                .translate(&gtk::graphene::Point::new(-80.0, -80.0));
            fan.set_child_transform(&pic, Some(&turn_at));
        }
        if !self.phone.get() {
            content.append(&fan); // no room on a phone
        }
        hero.add_overlay(&content);
        hero.set_measure_overlay(&content, true);
        hero.upcast()
    }

    // ------------------------------------------------------------------ library

    /// Albums, Artists and Genres as grids.
    pub(super) fn grid_page(&self, title: &str, flow: &gtk::FlowBox) -> gtk::ScrolledWindow {
        let column = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(18).margin_top(18).margin_bottom(24).margin_start(28).margin_end(28).build();
        self.grid_columns.borrow_mut().push(column.clone());
        column.append(&gtk::Label::builder().label(title).xalign(0.0).css_classes(["page-title"]).build());
        column.append(flow);
        gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&column).build()
    }

    pub(super) fn render_library(self: &Rc<Self>) {
        self.library_gen.set(self.library_gen.get().wrapping_add(1));
        let liked = self.liked.borrow().clone();
        self.albums_flow.remove_all();
        for (name, songs) in self.groups_by(&liked, LikedView::Albums) {
            self.albums_flow.insert(&self.group_card(&name, songs, false, "albums"), -1);
        }
        self.artists_flow.remove_all();
        for (name, songs) in self.groups_by(&liked, LikedView::Artists) {
            self.artists_flow.insert(&self.group_card(&name, songs, true, "artists"), -1);
        }
        self.genres_flow.remove_all();
        for (name, songs) in self.groups_by(&liked, LikedView::Genres) {
            let wide = if self.phone.get() { PHONE_CARD } else { 220 };
            self.genres_flow.insert(&self.genre_tile(&name, songs, wide), -1);
        }
    }

    /// The library's grids as the splash's work (the start): a dozen cards a frame.
    pub(super) fn render_library_behind(self: &Rc<Self>, splash: &super::splash::Splash) {
        const CARDS_A_FRAME: usize = 12;
        let gen = self.library_gen.get().wrapping_add(1);
        self.library_gen.set(gen);
        // (render_library meanwhile: it fills them all itself, this stops)
        let ours = move |me: &Rc<Ui>| me.library_gen.get() == gen;
        let liked = self.liked.borrow().clone();
        self.albums_flow.remove_all();
        self.artists_flow.remove_all();
        self.genres_flow.remove_all();
        for (kind, view) in [LikedView::Albums, LikedView::Artists, LikedView::Genres].into_iter().enumerate() {
            let groups = Rc::new(self.groups_by(&liked, view));
            for from in (0..groups.len()).step_by(CARDS_A_FRAME) {
                let (me, groups) = (Rc::downgrade(self), groups.clone());
                splash.then("the library", move || {
                    let Some(me) = me.upgrade().filter(ours) else { return };
                    for (name, songs) in &groups[from..(from + CARDS_A_FRAME).min(groups.len())] {
                        match kind {
                            0 => me.albums_flow.insert(&me.group_card(name, songs.clone(), false, "albums"), -1),
                            1 => me.artists_flow.insert(&me.group_card(name, songs.clone(), true, "artists"), -1),
                            _ => {
                                let wide = if me.phone.get() { PHONE_CARD } else { 220 };
                                me.genres_flow.insert(&me.genre_tile(name, songs.clone(), wide), -1);
                            }
                        }
                    }
                });
            }
        }
    }

    /// An album's, artist's or genre's own page: its art, play / shuffle, its songs.
    fn open_group(self: &Rc<Self>, name: &str, songs: Vec<Track>, from: &str) {
        self.group_back.replace(from.to_string());
        clear(&self.group_box);
        let back = gtk::Button::builder()
            .child(&adw::ButtonContent::builder().icon_name("go-previous-symbolic").label(NAV.iter().find(|n| n.0 == from).map(|n| n.1).unwrap_or("Back")).build())
            .halign(gtk::Align::Start)
            .css_classes(["flat"])
            .build();
        let me = Rc::downgrade(self);
        let f = from.to_string();
        back.connect_clicked(move |_| {
            if let Some(me) = me.upgrade() {
                me.stack.set_visible_child_name(&f);
            }
        });
        self.group_box.append(&back);
        let head = gtk::Box::builder().spacing(28).build();
        let pic = gtk::Picture::builder().content_fit(gtk::ContentFit::Cover).can_shrink(true).overflow(gtk::Overflow::Hidden).build();
        pic.add_css_class(if from == "artists" { "round-art" } else { "card-art" });
        pic.set_size_request(220, 220);
        let weak = pic.downgrade();
        self.set_art(&art_source(&songs[0]), 440, move |tex| {
            if let Some(p) = weak.upgrade() {
                p.set_paintable(Some(tex));
            }
        });
        head.append(&pic);
        let words = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(8).valign(gtk::Align::End).build();
        words.append(&gtk::Label::builder().label(name).xalign(0.0).wrap(true).wrap_mode(gtk::pango::WrapMode::WordChar).css_classes(["page-title"]).build());
        let n = songs.len();
        let sub = match from {
            "albums" => format!("{} · {n} song{}", songs[0].credit(), if n == 1 { "" } else { "s" }),
            _ => format!("{n} song{}", if n == 1 { "" } else { "s" }),
        };
        words.append(&gtk::Label::builder().label(&sub).xalign(0.0).css_classes(["dim-label"]).build());
        let buttons = gtk::Box::builder().spacing(10).margin_top(8).build();
        let play = gtk::Button::builder().label("Play").css_classes(["suggested-action", "pill-button"]).build();
        let shuffle = gtk::Button::builder().label("Shuffle").css_classes(["pill-button"]).build();
        let list = Rc::new(songs);
        let (me, l) = (Rc::downgrade(self), list.clone());
        play.connect_clicked(move |_| {
            if let Some(me) = me.upgrade() {
                me.play_list((*l).clone(), 0);
            }
        });
        let (me, l) = (Rc::downgrade(self), list.clone());
        shuffle.connect_clicked(move |_| {
            if let Some(me) = me.upgrade() {
                me.play_list(shuffled((*l).clone()), 0);
            }
        });
        buttons.append(&play);
        buttons.append(&shuffle);
        words.append(&buttons);
        head.append(&words);
        self.group_box.append(&head);
        let songs_list = adw::PreferencesGroup::new();
        for (i, t) in list.iter().enumerate() {
            songs_list.add(&self.liked_row(t, list.clone(), i));
        }
        self.group_box.append(&songs_list);
        self.stack.set_visible_child_name("group");
    }

    pub(super) fn group_page(&self) -> gtk::ScrolledWindow {
        let clamp = adw::Clamp::builder().maximum_size(1240).tightening_threshold(900).child(&self.group_box).build();
        gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&clamp).build()
    }

    // ------------------------------------------------------------------ search

    /// Before anything is typed: tiles to browse by.
    pub(super) fn browse_page(self: &Rc<Self>) -> gtk::ScrolledWindow {
        let column = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(14).margin_top(14).margin_bottom(24).margin_start(18).margin_end(18).build();
        column.append(&gtk::Label::builder().label("Browse").xalign(0.0).css_classes(["section-title"]).build());
        let flow = gtk::FlowBox::builder().selection_mode(gtk::SelectionMode::None).homogeneous(true).column_spacing(14).row_spacing(14).min_children_per_line(2).max_children_per_line(6).build();
        for (i, (label, query)) in BROWSE.iter().enumerate() {
            let b = gtk::Button::builder().label(*label).css_classes(["browse-tile", &format!("b{}", i % 6)]).build();
            let me = Rc::downgrade(self);
            let q = query.to_string();
            b.connect_clicked(move |_| {
                if let Some(me) = me.upgrade() {
                    me.go_search(&q);
                }
            });
            flow.insert(&b, -1);
        }
        column.append(&flow);
        let clamp = adw::Clamp::builder().maximum_size(1240).tightening_threshold(900).child(&column).build();
        gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&clamp).build()
    }

    // ------------------------------------------------------------------ where you left off

    /// The last queue and song, shown ready to go on from the same spot.
    pub(super) fn restore_session(self: &Rc<Self>) {
        let s = session::load();
        self.history.replace(s.history);
        let Some(t) = s.queue.get(s.index).cloned() else { return };
        *self.queue.borrow_mut() = s.queue;
        self.index.set(s.index);
        let t = self.saved_copy(&t);
        *self.current.borrow_mut() = Some(t.clone());
        self.show_current(&t);
        self.load_lyrics(&t, self.play_gen.get()); // its lyrics up already, before play is pressed
        self.restored.set(true);
        self.restore_at.set((s.position > 1.0).then_some(s.position));
        self.seek.set_value(s.position);
        self.pos.set_label(&clock(s.position));
        self.set_quality(None, false);
        self.render_queue();
        self.set_controls();
        self.update_voice_status(); // its mixer (when split) and split button, before play is pressed
    }

    pub(super) fn save_session(&self) {
        crate::watch::doing("saving where you are");
        if std::env::var_os("RADIANCE_SHOT").is_some() {
            return; // a screenshot copy never writes where you are
        }
        if self.in_party() {
            return; // a party's song isn't where you are (your own music was kept as you joined)
        }
        let position = self.player.borrow().as_ref().and_then(|p| p.position()).or(self.restore_at.get()).unwrap_or(0.0);
        session::save(&session::Session {
            queue: self.queue.borrow().clone(),
            index: self.index.get(),
            position,
            history: self.history.borrow().clone(),
        });
        self.last_save.set(Some(Instant::now()));
    }

    /// A song started: into the history (Home's "Recently played").
    pub(super) fn note_played(self: &Rc<Self>, t: &Track) {
        if self.history.borrow().first().is_none_or(|(x, _)| x.key() != t.key()) {
            super::count_play(t); // the same song again right away isn't counted twice
        }
        session::played(&mut self.history.borrow_mut(), t);
        self.save_session();
        self.home_later_parts(HomePart::Recent.bit());
        if self.stack.visible_child_name().as_deref() == Some("history") {
            self.render_history();
        }
    }

    // ------------------------------------------------------------------ history

    pub(super) fn history_page(&self) -> gtk::ScrolledWindow {
        let clamp = adw::Clamp::builder().maximum_size(1240).tightening_threshold(900).child(&self.history_box).build();
        gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&clamp).build()
    }

    /// Every play, newest first, by day: Today, Yesterday, then the date.
    pub(super) fn render_history(self: &Rc<Self>) {
        crate::watch::doing("building History");
        clear(&self.history_box);
        self.history_box.append(&gtk::Label::builder().label("History").xalign(0.0).css_classes(["page-title"]).build());
        let plays = self.history.borrow().clone();
        if plays.is_empty() {
            self.history_box.append(&gtk::Label::builder().label("Songs you play show up here, with when you played them.").xalign(0.0).css_classes(["dim-label"]).build());
            return;
        }
        let today = glib::DateTime::now_local().ok();
        let day_of = |at: i64| glib::DateTime::from_unix_local(at).ok();
        let label_for = |d: &glib::DateTime| -> String {
            let Some(t) = &today else { return d.format("%A, %B %-d").map(|s| s.to_string()).unwrap_or_default() };
            let days = (t.ymd().0 as i64 * 372 + t.ymd().1 as i64 * 31 + t.ymd().2 as i64) - (d.ymd().0 as i64 * 372 + d.ymd().1 as i64 * 31 + d.ymd().2 as i64);
            match t.difference(d).as_hours() {
                _ if days == 0 => "Today".into(),
                _ if days == 1 => "Yesterday".into(),
                h if h < 24 * 7 => d.format("%A").map(|s| s.to_string()).unwrap_or_default(),
                _ => d.format("%A, %B %-d, %Y").map(|s| s.to_string()).unwrap_or_default(),
            }
        };
        let all: Rc<Vec<Track>> = Rc::new(plays.iter().map(|(t, _)| t.clone()).collect());
        let mut group: Option<(String, adw::PreferencesGroup)> = None;
        for (i, (t, at)) in plays.iter().enumerate().take(400) {
            let when = day_of(*at);
            let name = when.as_ref().filter(|_| *at > 0).map(&label_for).unwrap_or_else(|| "Earlier".into());
            if group.as_ref().map(|g| g.0 != name).unwrap_or(true) {
                let g = adw::PreferencesGroup::builder().title(&name).build();
                self.history_box.append(&g);
                group = Some((name, g));
            }
            let row = self.track_row(t, Place::List(all.clone(), i));
            if let Some(d) = when.filter(|_| *at > 0) {
                let time = d.format("%-I:%M %p").map(|s| s.to_string()).unwrap_or_default();
                row.add_suffix(&gtk::Label::builder().label(&time).css_classes(["dim-label", "numeric"]).valign(gtk::Align::Center).build());
            }
            if let Some((_, g)) = &group {
                g.add(&row);
            }
        }
    }
}
