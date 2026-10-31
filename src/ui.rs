//! The window: Search (lossless first, YouTube Music as the fallback), Liked
//! (with "For you", songs like the ones you liked), Now playing (album art over
//! its own blurred colours, and synced lyrics) and EQ; the player bar below, and
//! the queue in a side panel. Every song has a menu: right-click, long-press or
//! the Menu key.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant};

use adw::prelude::*;
use gtk::{gdk, gio, glib, graphene};

mod backdrop;
mod lens;
mod home;
mod rms;
mod sharing;
mod splash;
mod splitting;
mod guide;
mod update;
use backdrop::Backdrop;
use lens::Lens;

use crate::beat::{self, Plan};
use crate::lyrics::{self, Lyrics};
use crate::player::{Event, Player};
use crate::relay::Relay;
use crate::settings::{self, Settings, Transition, FADE_RANGE};
use crate::split::{self, Voice};
use crate::track::{clock, file_key, letter_of, matches, Source, Track};
use crate::{align, archive, art, genre, library, mix, servers, storage, youtube, APP_ID};


/// Same pause as every search-as-you-type: no request per keystroke.
const SEARCH_DEBOUNCE_MS: u64 = 300;
/// Art sizes in pixels: rows show 48 px (sharp at 2x), the big cover up to 360 px.
const THUMB: u32 = 96;
const COVER: u32 = 720;
/// A YouTube stream address found ahead (for the next song) stays good this long.
const STREAM_FRESH: Duration = Duration::from_secs(3600);
/// The queue panel lists this many songs from the one playing.
const QUEUE_ROWS: usize = 100;
/// Songs played before, above the queue (to go back to one stuck in your head).
const RECENT_ROWS: usize = 5;

const CSS: &str = "
levelbar.part-meter trough { min-height: 6px; border-radius: 3px; }
levelbar.part-meter block { min-height: 6px; border-radius: 3px; }
levelbar.part-meter block.filled { background: #F38A35; }
:root { --accent-bg-color: #E8782A; --accent-fg-color: #ffffff; --accent-color: #B85A14; }
@media (prefers-color-scheme: dark) {
  :root { --accent-bg-color: #F38A35; --accent-fg-color: #1a0f05; --accent-color: #FFAE63; }
}
.pill { padding: 2px 9px; border-radius: 999px; font-size: 0.8em; font-weight: 700; }
.offline-pill { padding: 3px 11px; border-radius: 999px; background: alpha(@window_fg_color, 0.1); font-size: 0.85em; font-weight: 700; }
.pill.lossless { background: alpha(@success_color, 0.18); color: @success_color; }
.pill.compressed { background: alpha(@window_fg_color, 0.1); }
.pill.split { background: alpha(#F38A35, 0.2); color: #FFAE63; }
.badge { margin: 0 3px; }
.az-rail { padding: 8px 0; border-radius: 999px; background: alpha(@window_fg_color, 0.05); }
.az-rail label { font-size: 0.74em; font-weight: 800; color: var(--accent-color); padding: 0 7px; min-height: 15px; border-radius: 999px; }
.az-rail label.empty { opacity: 0.28; }
.az-rail label.on { background: var(--accent-bg-color); color: var(--accent-fg-color); }
.split-bar progress { background-color: #F38A35; }
.page-area { border-radius: 14px; }
.split-card { margin: 8px; padding: 10px 12px; border-radius: 12px; background: alpha(#ffffff, 0.06); }
.suggest { padding: 4px 12px; border-radius: 999px; }
.suggest.liked-suggest { color: var(--accent-color); }
.split-bar text { font-size: 0.85em; }
.badge.lossless { color: @success_color; }
.badge.split { color: #F38A35; }
.badge.lyrics-words { color: #F38A35; }
.badge.lyrics-text { opacity: 0.55; }
.badge.compressed, .badge.video, .badge.waiting { opacity: 0.6; }
.liked { color: #e5364f; }
.art { border-radius: 6px; }
.cover { border-radius: 16px; }
.art-card { border-radius: 16px; box-shadow: 0 12px 40px alpha(black, 0.45); }
.art-caption { padding: 72px 20px 16px 20px; background-image: linear-gradient(to top, alpha(black, 0.8), alpha(black, 0.5) 50%, alpha(black, 0)); }
.art-caption label { color: #ffffff; text-shadow: 0 1px 8px alpha(black, 0.7); }
.art-caption .dim-label { color: alpha(#ffffff, 0.85); }
.np-title { font-size: 1.7em; font-weight: 800; }
.lyrics, .lyrics row { background: none; }
.lyrics row { padding: 0; }
.lyrics-scroll undershoot.top, .lyrics-scroll undershoot.bottom { background: none; box-shadow: none; border: none; }
.lyric { font-size: 2.05em; font-weight: 800; padding: 10px 18px; opacity: 0.34; transition: opacity 350ms; }
.lyric.now { opacity: 1; }
.lyric-plain { font-size: 1.1em; padding: 12px 16px; }
.side-lyric { font-size: 1.4em; padding: 7px 14px; }
.side-plain { padding: 10px 14px; }
.playbar { padding: 6px 14px; }
.now-row { background: color-mix(in srgb, var(--accent-bg-color) 14%, transparent); }
.played-row { opacity: 0.7; }
.dragged { opacity: 0.35; }
.drop-above { box-shadow: inset 0 3px 0 0 #F38A35; }
.drop-below { box-shadow: inset 0 -3px 0 0 #F38A35; }
.played-row:hover { opacity: 1; }

/* Now playing: the art's own colours, blurred (moved from code, see drift) */
.np-scrim { background: linear-gradient(to bottom, alpha(black, 0.30), alpha(black, 0.55)); }
.np-page label { color: #ffffff; }
.np-page list.boxed-list, .np-page list.boxed-list > row, .np-page list.boxed-list row { background-color: alpha(black, 0.28); color: #ffffff; }
.np-page list.boxed-list { border-radius: 12px; box-shadow: 0 0 0 1px alpha(#ffffff, 0.08); }
.np-page toggle-group { background-color: alpha(black, 0.28); }
.np-page toggle-group toggle { color: #ffffff; }
.np-page toggle-group toggle:checked { background-color: alpha(#ffffff, 0.22); color: #ffffff; }
.np-page button.flat, .np-page row button { color: #ffffff; }
.np-page .pill-button, .np-page button.split-now { background-color: alpha(#ffffff, 0.16); color: #ffffff; }
.np-page .dim-label { color: alpha(#ffffff, 0.72); opacity: 1; }
.np-page .pill.compressed { background: alpha(#ffffff, 0.18); }
.np-page .pill.lossless { background: alpha(#57e389, 0.25); color: #8ff0a4; }
";

/// Details for RADIANCE_DEBUG=1 (never shown in the window).
fn debug(text: &str) {
    crate::log::note(text);
    if std::env::var_os("RADIANCE_DEBUG").is_some() {
        eprintln!("[debug] {text}");
    }
}

/// A scrolling page whose groups use the screen: up to 1240 px wide (a
/// PreferencesPage stays a 600 px column even on a big screen).
fn wide_page(groups: &[&adw::PreferencesGroup]) -> gtk::ScrolledWindow {
    let column = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(28).margin_top(18).margin_bottom(24).margin_start(18).margin_end(18).build();
    for g in groups {
        column.append(*g);
    }
    let clamp = adw::Clamp::builder().maximum_size(1240).tightening_threshold(900).child(&column).build();
    gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&clamp).build()
}

/// Fades a widget's opacity to `to` over `ms`.
fn fade(w: &impl IsA<gtk::Widget>, to: f64, ms: u32) {
    let w = w.upcast_ref::<gtk::Widget>();
    let target = adw::PropertyAnimationTarget::new(w, "opacity");
    adw::TimedAnimation::new(w, w.opacity(), to, ms, target).play();
}

thread_local! {
    /// The dark look's colours (Stone or True black), swapped live.
    static LOOK: gtk::CssProvider = gtk::CssProvider::new();
}

/// Stone or True black while the style is dark; the system's own when light.
fn apply_look(look: &str) {
    let dark = adw::StyleManager::default().is_dark();
    let css = match (dark, look) {
        (false, _) => "",
        (true, "black") => home::BLACK,
        (true, _) => home::STONE,
    };
    LOOK.with(|l| l.load_from_string(css));
}

/// What's New, from CHANGELOG.md (built in): the latest `versions` entries as
/// the markup the About window takes (paragraphs and lists; **bold** as <em>).
fn release_notes(changelog: &str, versions: usize) -> String {
    let esc = |t: &str| t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let inline = |t: &str| {
        let mut out = String::new();
        for (i, part) in esc(t).split("**").enumerate() {
            if i % 2 == 1 {
                out += &format!("<em>{part}</em>");
            } else {
                out += part;
            }
        }
        out
    };
    let mut html = String::new();
    let mut items: Vec<String> = vec![];
    let mut shown = 0;
    let flush = |html: &mut String, items: &mut Vec<String>| {
        if !items.is_empty() {
            *html += "<ul>";
            for i in items.drain(..) {
                *html += &format!("<li>{}</li>", inline(&i));
            }
            *html += "</ul>";
        }
    };
    for line in changelog.lines() {
        if let Some(head) = line.strip_prefix("## ") {
            flush(&mut html, &mut items);
            shown += 1;
            if shown > versions {
                break;
            }
            html += &format!("<p>{}</p>", esc(head.trim()));
        } else if shown == 0 {
            continue;
        } else if let Some(item) = line.strip_prefix("- ") {
            items.push(item.trim().to_string());
        } else if line.starts_with("  ") && !items.is_empty() {
            let last = items.last_mut().unwrap();
            last.push(' ');
            last.push_str(line.trim());
        }
    }
    flush(&mut html, &mut items);
    html
}

/// The A to Z rail's letters, in the order songs are filed.
const AZ: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ#";

/// Runs `work` on a worker thread, then `done` with its result on the main loop.
fn bg<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static, done: impl FnOnce(T) + 'static) {
    let (tx, rx) = async_channel::bounded(1);
    std::thread::spawn(move || {
        let _ = tx.send_blocking(work());
    });
    glib::spawn_future_local(async move {
        if let Ok(v) = rx.recv().await {
            done(v);
        }
    });
}

/// Covers, fetched and scaled by two workers at a low priority, the newest
/// asked first (what's just been put on screen). The start asks for a few
/// hundred at once (Home, Liked Songs, the library); a thread each took every
/// core, and the window (and the splash) stuttered.
fn bg_art<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static, done: impl FnOnce(T) + 'static) {
    use std::sync::{Arc, Condvar, Mutex, OnceLock};
    type Job = Box<dyn FnOnce() + Send>;
    static POOL: OnceLock<Arc<(Mutex<Vec<Job>>, Condvar)>> = OnceLock::new();
    let pool = POOL.get_or_init(|| {
        let pool = Arc::new((Mutex::new(Vec::<Job>::new()), Condvar::new()));
        for _ in 0..2 {
            let p = pool.clone();
            let _ = std::thread::Builder::new().name("radiance-art".into()).spawn(move || {
                // this thread only, below the window's (Linux: a thread is its own task)
                #[cfg(unix)]
                unsafe {
                    libc::setpriority(libc::PRIO_PROCESS, 0, 10)
                };
                loop {
                    let job = {
                        let (lock, ready) = &*p;
                        let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
                        while q.is_empty() {
                            q = ready.wait(q).unwrap_or_else(|e| e.into_inner());
                        }
                        q.pop()
                    };
                    if let Some(job) = job {
                        job();
                    }
                }
            });
        }
        pool
    });
    let (tx, rx) = async_channel::bounded(1);
    let (lock, ready) = &**pool;
    lock.lock().unwrap_or_else(|e| e.into_inner()).push(Box::new(move || {
        let _ = tx.send_blocking(work());
    }));
    ready.notify_one();
    glib::spawn_future_local(async move {
        if let Ok(v) = rx.recv().await {
            done(v);
        }
    });
}

fn texture(px: art::Pixels) -> gdk::Texture {
    let size = px.size as i32;
    let bytes = glib::Bytes::from_owned(px.rgba);
    gdk::MemoryTexture::new(size, size, gdk::MemoryFormat::R8g8b8a8, &bytes, px.size as usize * 4).upcast()
}

/// How much you like a song by what you do: played often, more; skipped
/// often, less (1 for a song with no plays or skips; between 0.2 and 8).
fn liking(t: &Track) -> f64 {
    let (plays, skips) = counts_of(t);
    ((1.0 + plays as f64) / (1.0 + 2.0 * skips as f64)).clamp(0.2, 8.0)
}

/// Skipped more than played (at least twice): left out of mixes and radios.
fn often_skipped(t: &Track) -> bool {
    let (plays, skips) = counts_of(t);
    skips >= 2 && skips > plays
}

/// A shuffle that still listens to you: random, but songs you play a lot tend
/// to come early and songs you skip tend to come late (each song's place is
/// u^(1/w), u random, w how much you like it: weighted random order).
fn shuffled(v: Vec<Track>) -> Vec<Track> {
    let mut x = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(7) | 1;
    let mut keyed: Vec<(f64, Track)> = v
        .into_iter()
        .map(|t| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let u = ((x >> 11) as f64 / (1u64 << 53) as f64).max(1e-12);
            (u.powf(1.0 / liking(&t)), t)
        })
        .collect();
    keyed.sort_by(|a, b| b.0.total_cmp(&a.0));
    keyed.into_iter().map(|(_, t)| t).collect()
}

/// Repeat: off, the whole queue, or this song.
#[derive(Clone, Copy, PartialEq)]
enum Repeat {
    Off,
    All,
    One,
}

/// A saved song's own cover, else the source's.
fn art_source(t: &Track) -> String {
    t.file
        .as_ref()
        .map(|f| library::beside(f, "jpg"))
        .filter(|p| p.exists())
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| t.art.clone())
}

/// "Offline", small in the top bar (shown while there's no internet).
fn offline_pill() -> gtk::Box {
    let pill = gtk::Box::builder()
        .spacing(6)
        .valign(gtk::Align::Center)
        .css_classes(["offline-pill"])
        .visible(false)
        .tooltip_text("No internet: your saved songs still play. Searching, streaming and lyrics come back with it.")
        .build();
    pill.append(&gtk::Image::from_icon_name("network-offline-symbolic"));
    pill.append(&gtk::Label::new(Some("Offline")));
    pill
}

fn khz(rate: i32) -> String {
    if rate % 1000 == 0 {
        format!("{} kHz", rate / 1000)
    } else {
        format!("{:.1} kHz", rate as f64 / 1000.0)
    }
}

/// The song's page on the web, to share.
fn link(t: &Track) -> String {
    match t.source {
        Source::YouTube => format!("https://music.youtube.com/watch?v={}", t.id),
        // its server's address (a song's own address needs this computer's key)
        Source::Server => servers::of_track(t).map(|s| s.address).unwrap_or_else(|| t.id.split("/files/").next().unwrap_or(&t.id).to_string()),
        Source::Archive => t
            .id
            .strip_prefix("https://archive.org/download/")
            .and_then(|rest| rest.split('/').next())
            .map(|item| format!("https://archive.org/details/{item}"))
            .unwrap_or_else(|| t.id.clone()),
    }
}

/// A saved song's own Lyricsfile first, else LRCLIB, else its old .lrc (worker
/// thread). A song saved before word timing asks LRCLIB once more and keeps the
/// answer as a Lyricsfile, so its words light up too.
///
/// Lines without their words' times get them from the song's Vocals part once
/// it's been split (see align.rs); a saved song keeps that in its Lyricsfile.
fn lyrics_for(t: &Track) -> Lyrics {
    lyrics_and_source(t).0
}

/// lyrics_for, and whether its words were timed from the vocals just now.
fn lyrics_and_source(t: &Track) -> (Lyrics, bool) {
    let file = t.file.as_deref();
    let kept = fs::read_to_string(lyrics_file(t)).ok().and_then(|y| lyrics::parse_lyricsfile(&y)).or_else(|| server_lyrics(t));
    let fresh = kept.is_none();
    let mut l = kept.unwrap_or_else(|| {
        let found = lyrics::find(t);
        match (file, &found) {
            (None, _) | (_, Lyrics::Timed { .. }) => found,
            (Some(f), _) => match fs::read_to_string(library::beside(f, "lrc")) {
                Ok(text) => {
                    let lines = lyrics::parse_lrc(&text);
                    if lines.is_empty() { Lyrics::Plain(text) } else { Lyrics::Timed { lines, exact: false } }
                }
                Err(_) => found,
            },
        }
    });
    let mut from_vocals = false;
    if let Lyrics::Timed { lines, exact } = &mut l {
        if let (false, Some(vocals)) = (*exact, split::ready_part(t, "vocals")) {
            if align::words_from_vocals(lines, &gio::File::for_path(&vocals).uri()) {
                *exact = true;
                from_vocals = true;
            }
        }
    }
    if let (Some(f), true, Lyrics::Timed { .. }) = (file, fresh || from_vocals, &l) {
        if let Some((ext, text)) = lyrics::to_file(&l, Some(t), if from_vocals { &[lyrics::VOCALS_NOTE] } else { &[] }) {
            let _ = fs::write(library::beside(f, ext), text);
        }
    }
    (l, from_vocals)
}

/// A server's song (not saved): the lyrics its server keeps for it, a Lyricsfile
/// kept here too (in the cache, as any song's), else its .lrc text (worker thread).
fn server_lyrics(t: &Track) -> Option<Lyrics> {
    if t.source != Source::Server || t.file.is_some() {
        return None;
    }
    let (yaml, text) = servers::lyrics_text(t)?;
    if yaml {
        let l = lyrics::parse_lyricsfile(&text)?;
        let _ = fs::write(lyrics_file(t), text);
        return Some(l);
    }
    let lines = lyrics::parse_lrc(&text);
    Some(if lines.is_empty() { Lyrics::Plain(text) } else { Lyrics::Timed { lines, exact: false } })
}

/// A split song's lyrics for the server's library (worker thread): its
/// Lyricsfile (words timed from its vocals when they can be), else plain text.
fn hosted_lyrics(t: &Track) -> Option<(&'static str, String)> {
    let (l, from_vocals) = lyrics_and_source(t);
    if let Some(text) = t.file.as_deref().and_then(|f| fs::read_to_string(library::beside(f, "lyrics.yaml")).ok()) {
        return Some(("lyrics.lyricsfile.yaml", text));
    }
    match &l {
        Lyrics::Timed { .. } => lyrics::to_file(&l, Some(t), if from_vocals { &[lyrics::VOCALS_NOTE] } else { &[] }).map(|(_, y)| ("lyrics.lyricsfile.yaml", y)),
        Lyrics::Plain(text) => Some(("lyrics.txt", text.clone())),
        Lyrics::None => None,
    }
}

/// A liked song's official music video, saved next to it (worker thread; minutes).
/// Ok(None): it has none (remembered, not looked for again).
pub(crate) fn keep_video(t: &Track) -> Result<Option<String>, String> {
    let file = t.file.as_deref().map(Path::new).ok_or("not saved")?;
    let (dir, base) = (file.parent().ok_or("no folder")?, file.file_stem().and_then(|s| s.to_str()).ok_or("no name")?);
    let video = if t.video { Some(t.clone()) } else { youtube::counterpart(t) };
    let Some(v) = video.filter(|v| v.video) else {
        crate::db::add_video(&format!("none:{}", t.key()), &t.key(), "");
        return Ok(None);
    };
    let path = youtube::save_video(&v.id, dir, base)?;
    crate::db::add_video(&v.id, &t.key(), &path);
    Ok(Some(path))
}

/// A liked song's lyrics, kept next to it: a Lyricsfile when timed, else the
/// plain text as .lrc (worker thread).
fn keep_lyrics_file(t: &Track) {
    let Some(f) = t.file.as_deref() else { return };
    recheck_lyrics(t, f);
    if library::beside(f, "lyrics.yaml").exists() || library::beside(f, "lrc").exists() {
        return;
    }
    if let (Lyrics::Plain(text), _) = lyrics_and_source(t) {
        let _ = fs::write(library::beside(f, "lrc"), text);
    }
}

/// Lyrics kept before the app checked the artist could be another song's with
/// the same title. Once per song they're looked up again; if they don't match,
/// the old files go to the Trash (they can be brought back) and the right
/// lyrics, or none, take their place (worker thread).
fn recheck_lyrics(t: &Track, f: &str) {
    if crate::db::lyrics_checked(&t.key()) || hand_edited(t) {
        return; // checked, or yours
    }
    let kept: Vec<std::path::PathBuf> = ["lyrics.yaml", "lrc"].iter().map(|e| library::beside(f, e)).filter(|p| p.exists()).collect();
    let words = |l: &Lyrics| -> HashSet<String> {
        let text = match l {
            Lyrics::Timed { lines, .. } => lines.iter().map(|x| x.text.clone()).collect::<Vec<_>>().join(" "),
            Lyrics::Plain(p) => p.clone(),
            Lyrics::None => String::new(),
        };
        text.split_whitespace().map(|w| w.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect::<String>()).filter(|w| w.len() > 2).collect()
    };
    let old = kept.iter().find_map(|p| {
        let text = fs::read_to_string(p).ok()?;
        lyrics::parse_lyricsfile(&text).or_else(|| Some(if lyrics::parse_lrc(&text).is_empty() { Lyrics::Plain(text.clone()) } else { Lyrics::Timed { lines: lyrics::parse_lrc(&text), exact: false } }))
    });
    let fresh = lyrics::find(t);
    if matches!(fresh, Lyrics::None) {
        return; // nothing to compare with (or offline): the kept ones stay, checked again next time
    }
    if let Some(old) = old {
        let (a, b) = (words(&old), words(&fresh));
        let shared = a.intersection(&b).count() as f64 / a.len().max(1) as f64;
        if shared < 0.5 {
            debug(&format!("lyrics of {}: the kept ones were another song's; replaced", t.title));
            for p in &kept {
                let _ = gio::File::for_path(p).trash(gio::Cancellable::NONE);
            }
            match lyrics::to_file(&fresh, Some(t), &[]) {
                Some((ext, text)) => {
                    let _ = fs::write(library::beside(f, ext), text);
                }
                None => {}
            }
        }
    }
    crate::db::set_lyrics_checked(&t.key());
}

/// Where splitting and word timing happen: the split server (yours, if you
/// set one up), else this computer.
#[derive(Clone)]
pub(crate) enum Aligner {
    Server(settings::SplitServer),
    Here(settings::LocalSplitter),
}

impl Aligner {
    pub(crate) fn of(s: &settings::Settings) -> Option<Aligner> {
        match (&s.split, &s.local_split) {
            (Some(server), _) => Some(Aligner::Server(server.clone())),
            (None, Some(local)) => Some(Aligner::Here(local.clone())),
            _ => None,
        }
    }
}

/// A song's lyrics looked up again, as asked (worker thread; minutes when it's
/// split): the kept ones go to the Trash (they can be brought back), yours
/// too; the ones found take their place; a split song gets every word timed
/// to its Vocals part on the server. Ok(true): timed. Err: none found (the
/// kept ones stay).
pub(crate) fn relook_lyrics(t: &Track, aligner: Option<&Aligner>) -> Result<bool, String> {
    let fresh = lyrics::find(t);
    if matches!(fresh, Lyrics::None) {
        return Err("No other lyrics found for this song: yours stay. Edit Lyrics… to type them in.".into());
    }
    let lrc = t.file.as_deref().map(|f| library::beside(f, "lrc"));
    for p in [Some(lyrics_path(t)), lrc.clone()].into_iter().flatten().filter(|p| p.exists()) {
        gio::File::for_path(&p).trash(gio::Cancellable::NONE).map_err(|e| format!("Couldn't move the old lyrics to the Trash: {e}"))?;
    }
    let written = match (&fresh, &lrc) {
        (Lyrics::Plain(text), Some(lrc)) => fs::write(lrc, text),
        (Lyrics::Plain(_), None) => Ok(()), // a song not saved: looked up each time
        _ => match lyrics::to_file(&fresh, Some(t), &[]) {
            Some((_, text)) => fs::write(lyrics_file(t), text),
            None => Ok(()),
        },
    };
    written.map_err(|e| format!("Couldn't keep the new lyrics: {e}"))?;
    crate::db::set_lyrics_checked(&t.key());
    crate::log::note(&format!("lyrics of “{}” looked up again: {}", t.title, match &fresh { Lyrics::Timed { lines, .. } => format!("{} timed lines", lines.len()), Lyrics::Plain(p) => format!("{} lines of text", p.lines().count()), Lyrics::None => "none".into() }));
    match (aligner, split::has_parts(t)) {
        (Some(a), true) => time_words(t, a).map(|_| true),
        _ => Ok(false),
    }
}

/// Keeps a split song in the server's library, its words timed first (worker thread).
pub(crate) fn keep_on_server(t: &Track, server: &settings::SplitServer, timing: &dyn Fn()) {
    if let Err(e) = split::host_copy(t, server, hosted_lyrics(t)) {
        debug(&format!("library copy of {}: {e}", t.title));
        return;
    }
    split::mark_hosted(&t.key());
    // the words' real times, from MMS on the server (the quick ones stand in till then)
    if !aligned_by_mms(t) {
        timing();
        if let Err(e) = time_words(t, &Aligner::Server(server.clone())) {
            debug(&format!("word times for {}: {e}", t.title));
        }
    }
}

/// The lyrics were edited by hand (in the lyrics editor): never replaced by a lookup.
fn hand_edited(t: &Track) -> bool {
    fs::read_to_string(lyrics_path(t)).is_ok_and(|y| y.contains(lyrics::HAND_NOTE))
}

/// Every word's time from MMS, on the split server or this computer (worker
/// thread; minutes): for lyrics edited by hand, their lines (and singers,
/// kept); else LRCLIB's or YouTube Music's. The Lyricsfile is kept for the song.
fn time_words(t: &Track, aligner: &Aligner) -> Result<(), String> {
    let mine = if hand_edited(t) {
        fs::read_to_string(lyrics_path(t)).ok().and_then(|y| lyrics::parse_lyricsfile(&y))
    } else {
        None
    };
    let source = mine.clone().unwrap_or_else(|| lyrics::find(t));
    let given = match &source {
        Lyrics::Timed { lines, exact } if mine.is_some() || !exact => serde_json::json!({
            "lines": lines.iter().filter(|l| !l.text.trim().is_empty()).map(|l| serde_json::json!({ "start": l.start, "end": l.end, "text": l.text })).collect::<Vec<_>>()
        }),
        Lyrics::Plain(text) => serde_json::json!({ "plain": text }),
        _ => return Ok(()), // none, or the song's own word times already
    };
    let yaml = match aligner {
        Aligner::Server(server) => split::align_on_server(t, server, &given)?,
        Aligner::Here(local) => split::align_here(t, local, &given)?,
    };
    let Some(Lyrics::Timed { mut lines, exact }) = lyrics::parse_lyricsfile(&yaml) else { return Err("nothing came back".into()) };
    let Some(Lyrics::Timed { lines: hand, .. }) = mine else {
        let _ = fs::write(lyrics_file(t), yaml);
        return Ok(());
    };
    // yours: the same lines in the same order, each with its singer again
    for (line, was) in lines.iter_mut().zip(hand.iter().filter(|l| !l.text.trim().is_empty())) {
        line.singer = was.singer.clone();
    }
    let mms = "word times: MMS forced alignment (Meta, torchaudio MMS_FA) on the song's Vocals part";
    if let Some((_, text)) = lyrics::to_file(&Lyrics::Timed { lines, exact }, Some(t), &[lyrics::HAND_NOTE, mms]) {
        let _ = fs::write(lyrics_file(t), text);
    }
    Ok(())
}

/// Where a song's Lyricsfile is kept: next to a saved song, else in the cache.
fn lyrics_file(t: &Track) -> std::path::PathBuf {
    let p = lyrics_path(t);
    if let Some(dir) = p.parent() {
        let _ = fs::create_dir_all(dir);
    }
    p
}

fn lyrics_path(t: &Track) -> std::path::PathBuf {
    match t.file.as_deref() {
        Some(f) => library::beside(f, "lyrics.yaml"),
        None => glib::user_cache_dir().join("radiance-music/lyrics").join(format!("{}.lyrics.yaml", crate::track::file_name(&t.key()))),
    }
}

/// What lyrics a song has on this computer.
#[derive(Clone, Copy, PartialEq)]
enum LyricsKind {
    /// Each word timed (a Lyricsfile with words).
    Words,
    /// Each line timed.
    Lines,
    /// The text only.
    Text,
}

thread_local! {
    /// Each song's plays and skips (from the database, kept up to date here).
    static COUNTS: RefCell<Option<HashMap<String, (usize, usize)>>> = const { RefCell::new(None) };
}

/// A song's (plays, skips).
fn counts_of(t: &Track) -> (usize, usize) {
    COUNTS.with(|c| c.borrow_mut().get_or_insert_with(crate::db::counts).get(&t.key()).copied().unwrap_or_default())
}

fn count_play(t: &Track) {
    COUNTS.with(|c| c.borrow_mut().get_or_insert_with(crate::db::counts).entry(t.key()).or_default().0 += 1);
}

fn count_skip(t: &Track) {
    crate::db::add_skip(&t.key());
    COUNTS.with(|c| c.borrow_mut().get_or_insert_with(crate::db::counts).entry(t.key()).or_default().1 += 1);
}

/// "Played 12 times · skipped 3 times" (or less, or nothing).
fn counts_text(t: &Track) -> String {
    let (plays, skips) = counts_of(t);
    let times = |n: usize| if n == 1 { "once".to_string() } else { format!("{n} times") };
    match (plays, skips) {
        (0, 0) => String::new(),
        (p, 0) => format!("Played {}", times(p)),
        (p, s) => format!("Played {} · skipped {}", times(p), times(s)),
    }
}

thread_local! {
    /// Lyrics files already looked at (by when they last changed).
    static LYRICS_KINDS: RefCell<HashMap<std::path::PathBuf, (std::time::SystemTime, LyricsKind)>> = RefCell::default();
}

/// The lyrics kept for a song (its Lyricsfile, else its .lrc), if any.
fn lyrics_kind(t: &Track) -> Option<LyricsKind> {
    let mut files = vec![lyrics_path(t)];
    if let Some(f) = t.file.as_deref() {
        files.push(library::beside(f, "lrc"));
    }
    for p in files {
        let Some(changed) = fs::metadata(&p).ok().and_then(|m| m.modified().ok()) else { continue };
        if let Some(kind) = LYRICS_KINDS.with(|k| k.borrow().get(&p).filter(|(at, _)| *at == changed).map(|(_, kind)| *kind)) {
            return Some(kind);
        }
        let Ok(text) = fs::read_to_string(&p) else { continue };
        let kind = if p.extension().is_some_and(|e| e == "yaml") {
            if text.contains("\n    words:\n      - ") {
                LyricsKind::Words
            } else if text.contains("\nlines:") {
                LyricsKind::Lines
            } else {
                LyricsKind::Text
            }
        } else if lyrics::parse_lrc(&text).is_empty() {
            LyricsKind::Text
        } else {
            LyricsKind::Lines
        };
        LYRICS_KINDS.with(|k| k.borrow_mut().insert(p, (changed, kind)));
        return Some(kind);
    }
    None
}

/// Its words were timed by MMS already.
fn aligned_by_mms(t: &Track) -> bool {
    fs::read_to_string(lyrics_file(t)).is_ok_and(|y| y.contains("MMS forced alignment"))
}

/// Words light this much ahead of the sound, and a line comes to the middle
/// this much before it's sung (it has to get there first).
const WORD_LEAD: f64 = 0.08;
const LINE_LEAD: f64 = 0.35;

/// The line being sung in Now Playing is drawn this much bigger than the rest
/// (less when a long line wouldn't fit the view).
const LYRIC_GROW: f32 = 2.5;

/// A timed lyric line on screen: its words' places in the text (in characters),
/// its two labels, and how many characters were lit when last drawn.
struct LyricView {
    line: lyrics::Line,
    /// Who sings it, shown above it when the voice changes from the line before.
    head: Option<String>,
    spans: Vec<(usize, usize)>,
    big: gtk::Label,
    side: gtk::Label,
    sung: Cell<Option<usize>>,
}

impl LyricView {
    /// The singer's name, small, above the line (when it's a new voice).
    fn head_markup(&self) -> String {
        self.head.as_deref().map(|h| format!("<span size=\"55%\" alpha=\"75%\" weight=\"bold\">{}</span>\n", glib::markup_escape_text(h))).unwrap_or_default()
    }

    /// The line not being sung.
    fn resting_markup(&self) -> String {
        format!("{}{}", self.head_markup(), glib::markup_escape_text(&self.line.text))
    }

    /// How much of the line has been sung at `at` (characters): whole words
    /// once they're over, the word being sung in proportion to its time.
    fn sung_chars(&self, at: f64) -> usize {
        if at >= self.line.end || self.spans.is_empty() {
            return self.line.text.chars().count();
        }
        // each word lights whole, the moment it's sung (no sweep across its letters)
        let at = at + WORD_LEAD;
        let mut k = 0;
        for (w, &(_, b)) in self.line.words.iter().zip(&self.spans) {
            if at >= w.start {
                k = b;
            } else {
                break;
            }
        }
        k
    }
}

/// Where each word sits in the line's text, in characters (words that can't be
/// found follow on from the last one).
fn word_spans(text: &str, words: &[lyrics::Word]) -> Vec<(usize, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let mut pos = 0;
    words
        .iter()
        .map(|w| {
            let word: Vec<char> = w.text.trim().chars().collect();
            let found = (pos..=chars.len().saturating_sub(word.len())).find(|&i| !word.is_empty() && chars[i..i + word.len()] == word[..]);
            let a = found.unwrap_or(pos).min(chars.len());
            let b = (a + word.len()).min(chars.len());
            pos = b;
            (a, b)
        })
        .collect()
}

/// A saved song's own sample rate, kept when it plays among its parts: Opus
/// is always 48 kHz; a FLAC says in its STREAMINFO (None: the mixer picks).
fn file_rate(path: &str) -> Option<u32> {
    let low = path.to_lowercase();
    if low.ends_with(".opus") {
        return Some(48_000);
    }
    if !low.ends_with(".flac") {
        return None;
    }
    use std::io::Read;
    let mut head = [0u8; 42];
    fs::File::open(path).ok()?.read_exact(&mut head).ok()?;
    // "fLaC", then STREAMINFO's header (4) and its first 10 bytes; the rate is the next 20 bits
    if &head[..4] != b"fLaC" {
        return None;
    }
    let b = &head[18..21];
    let rate = (u32::from(b[0]) << 12) | (u32::from(b[1]) << 4) | (u32::from(b[2]) >> 4);
    (8_000..=768_000).contains(&rate).then_some(rate)
}

/// A line with its first `sung` characters lit and the rest still faint.
fn karaoke(text: &str, sung: usize) -> String {
    let cut = text.char_indices().nth(sung).map(|(i, _)| i).unwrap_or(text.len());
    let (done, rest) = text.split_at(cut);
    if rest.is_empty() {
        return glib::markup_escape_text(done).to_string();
    }
    format!("{}<span fgalpha=\"40%\">{}</span>", glib::markup_escape_text(done), glib::markup_escape_text(rest))
}

/// A small icon that explains itself in a card when the pointer rests on it.
fn badge(icon: &str, class: &str, title: &str, body: &str) -> gtk::Widget {
    let img = gtk::Image::builder().icon_name(icon).pixel_size(16).valign(gtk::Align::Center).css_classes(["badge", class]).build();
    let card = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(4).margin_top(6).margin_bottom(6).margin_start(8).margin_end(8).build();
    card.append(&gtk::Label::builder().label(title).xalign(0.0).css_classes(["heading"]).build());
    card.append(&gtk::Label::builder().label(body).xalign(0.0).wrap(true).wrap_mode(gtk::pango::WrapMode::WordChar).max_width_chars(34).css_classes(["dim-label"]).build());
    let pop = gtk::Popover::builder().child(&card).autohide(false).can_focus(false).position(gtk::PositionType::Top).build();
    pop.set_parent(&img);
    let motion = gtk::EventControllerMotion::new();
    let p = pop.downgrade();
    motion.connect_enter(move |_, _, _| {
        if let Some(p) = p.upgrade() {
            p.popup();
        }
    });
    let p = pop.downgrade();
    motion.connect_leave(move |_| {
        if let Some(p) = p.upgrade() {
            p.popdown();
        }
    });
    img.add_controller(motion);
    img.update_property(&[gtk::accessible::Property::Label(title), gtk::accessible::Property::Description(body)]);
    img.connect_destroy(move |_| pop.unparent());
    img.upcast()
}

/// What a song is, as one icon: lossless, compressed or a video.
fn quality_badge(t: &Track) -> gtk::Widget {
    if t.video {
        badge("camera-video-symbolic", "video", "Music video", "From YouTube; plays with its picture in Now Playing (up to 720p).")
    } else if t.source == Source::Server {
        let name = servers::of_track(t).map(|s| s.name).unwrap_or_else(|| "a Radiance Music Server".into());
        badge("network-server-symbolic", if t.lossless() { "lossless" } else { "compressed" }, &format!("From {name} · {}", t.format), "Streams from the server you joined. Like it to save it, with its parts, lyrics and cover, into your own library.")
    } else if t.lossless() {
        let what = if t.format == "24-bit FLAC" { "24-bit FLAC" } else { "FLAC" };
        badge("lossless-symbolic", "lossless", &format!("Lossless · {what}"), "The full recording from the Internet Archive, nothing thrown away. Played first.")
    } else {
        badge("weather-overcast-symbolic", "compressed", "Compressed · Opus", "Streams from YouTube Music, about 130–160 kbps. No lossless copy of this song was found.")
    }
}

/// How liked songs are grouped (Albums, Artists, Genres pages and Home).
#[derive(Clone, Copy, PartialEq)]
enum LikedView {
    Albums,
    Artists,
    Genres,
}

/// Where a song is shown: what its menu offers, and what clicking it plays.
#[derive(Clone)]
enum Place {
    /// Search results or the "For you" mix: plays on through that list.
    List(Rc<Vec<Track>>, usize),
    Liked(Rc<Vec<Track>>, usize),
    /// A song in a playlist (its id), which plays on through it.
    Playlist(i64, Rc<Vec<Track>>, usize),
    /// A song on a Radiance Music Server's page: plays on through its songs (in its party: joins the party's queue).
    Server(Rc<Vec<Track>>, usize),
    /// A song in the queue, by position.
    Queue(usize),
    /// A song played before (above the queue): plays again now, then the queue carries on.
    Again(Track),
    /// The song playing (the player bar).
    Current,
}

pub struct Ui {
    pub window: adw::ApplicationWindow,
    stack: adw::ViewStack,
    toasts: adw::ToastOverlay,
    /// What the app can't work without and is missing (GStreamer's parts, yt-dlp): said until
    /// it's fixed, with where to read how (the button)
    banner: adw::Banner,
    banner_help: RefCell<String>,
    /// "Offline" in the top bar while there's no internet; `online` as GIO last said
    offline_pill: gtk::Box,
    online: Cell<bool>,
    /// The lyrics shown are "none" because the internet was off (looked for again when it's back)
    lyrics_offline: Cell<bool>,
    /// Songs that wouldn't play, in a row (see `unavailable`)
    fails: Cell<u32>,
    /// The sunrise is held for the first start's setup (the guide waits for it)
    setup_holding: Cell<bool>,
    /// The first start's splitter setup, waiting to try again (when the internet's back, or soon)
    setup_waiting: RefCell<Option<Box<dyn FnOnce()>>>,
    split: adw::OverlaySplitView,
    // search
    search: gtk::SearchEntry,
    search_view: gtk::Stack,
    lossless: adw::PreferencesGroup,
    lossless_spin: gtk::Spinner,
    fallback: adw::PreferencesGroup,
    fallback_spin: gtk::Spinner,
    videos: adw::PreferencesGroup,
    videos_spin: gtk::Spinner,
    found_videos: RefCell<Option<Vec<Track>>>,
    result_rows: RefCell<Vec<(adw::PreferencesGroup, adw::ActionRow)>>,
    found_lossless: RefCell<Option<Vec<Track>>>,
    found_youtube: RefCell<Option<Vec<Track>>>,
    search_gen: Cell<u64>,
    debounce: RefCell<Option<glib::SourceId>>,
    // liked + for you
    liked_group: adw::PreferencesGroup,
    liked_rows: RefCell<Vec<gtk::Widget>>,
    /// The songs of those rows, in their order (A to Z).
    liked_shown: RefCell<Rc<Vec<Track>>>,
    /// Counts the listings of Liked Songs and fillings of the library's grids:
    /// one done bit by bit stops when a newer one starts.
    liked_gen: Cell<u32>,
    library_gen: Cell<u32>,
    /// Liked Songs' own search: only the songs that match stay listed.
    liked_search: gtk::SearchEntry,
    /// "Fix missing": liked songs whose file or music video isn't on this computer, saved.
    fix_button: gtk::Button,
    /// On the Search page: the liked songs that match, before what's online.
    in_liked: adw::PreferencesGroup,
    /// Beside Liked Songs: A to Z, then #; and the first row under each letter.
    az_rail: gtk::Box,
    liked_letters: RefCell<Vec<(char, gtk::Widget)>>,
    /// Saving liked songs' videos and lyrics now, and asked again meanwhile.
    extras_running: Cell<bool>,
    extras_again: Cell<bool>,

    /// Genres being looked up for liked songs.
    genres_busy: Cell<bool>,
    liked: RefCell<Vec<Track>>,
    saving: RefCell<HashSet<String>>,
    hearts: RefCell<Vec<(String, glib::WeakRef<gtk::Button>)>>,
    mix: RefCell<Vec<Track>>,
    mix_gen: Cell<u64>,
    mix_later: RefCell<Option<glib::SourceId>>,
    // queue panel
    queue_group: adw::PreferencesGroup,
    recent_group: adw::PreferencesGroup,
    recent_rows: RefCell<Vec<adw::ActionRow>>,
    queue_rows: RefCell<Vec<adw::ActionRow>>,
    queue_button: gtk::ToggleButton,
    // now playing
    backdrop: Backdrop,
    backdrop_n: Cell<u32>,
    // home, library, group pages, the sidebar
    nav: gtk::ListBox,
    home_box: gtk::Box,
    /// Home's sections waiting to be built again (HomePart bits).
    home_stale: Cell<u8>,
    /// Home's sections as they're shown (by HomePart; None: not shown).
    home_sections: RefCell<[Option<gtk::Widget>; 7]>,
    home_later: RefCell<Option<glib::SourceId>>,
    /// Counts Home's builds: a build done bit by bit stops when a newer one starts.
    home_gen: Cell<u32>,
    /// Still starting (behind the splash): Home waits to be built once, at the end.
    starting: Cell<bool>,
    albums_flow: gtk::FlowBox,
    artists_flow: gtk::FlowBox,
    genres_flow: gtk::FlowBox,
    group_box: gtk::Box,
    group_back: RefCell<String>,
    // where you left off
    history: RefCell<Vec<(Track, i64)>>,
    history_box: gtk::Box,
    /// The last session's song is shown but not loaded yet (play loads it).
    restored: Cell<bool>,
    restore_at: Cell<Option<f64>>,
    last_save: Cell<Option<Instant>>,
    cover: gtk::Stack,
    cover_frame: adw::Clamp,
    /// The video's picture height (720 …), while a video plays.
    video_height: Cell<u32>,
    np_text: gtk::Box,
    volume: gtk::Scale,
    mute: gtk::ToggleButton,
    np_title: gtk::Label,
    np_byline: gtk::Label,
    np_quality: gtk::Label,
    lyrics_list: Lens,
    lyrics_scroll: gtk::ScrolledWindow,
    /// The timed lyrics on screen, line by line (Now playing's and the side panel's).
    lyric_lines: RefCell<Vec<LyricView>>,
    /// The side panel's own copy of the lyrics (its Lyrics tab), in step.
    side_lyrics_list: Lens,
    side_lyrics_scroll: gtk::ScrolledWindow,
    side_tabs: adw::ToggleGroup,
    side_stack: gtk::Stack,
    /// The side panel's Splits tab: the song being split, then those waiting.
    split_list: gtk::ListBox,
    split_now_row: RefCell<Option<adw::ActionRow>>,
    split_queue_btn: gtk::Button,
    /// At the bottom of the sidebar while anything is split or waiting.
    split_card: gtk::Box,
    /// Search suggestions under the search box (your artists, or what's being typed).
    suggest_flow: gtk::FlowBox,
    suggest_gen: Cell<u64>,
    /// The playlist page, and which playlist it shows.
    playlist_box: gtk::Box,
    playlist_now: Cell<Option<i64>>,
    lyrics_button: gtk::Button,
    /// The lines being sung now, and the first of them (kept in the middle).
    lyric_now: RefCell<Vec<usize>>,
    lyric_lead: Cell<Option<usize>>,
    /// When Now Playing's lyrics last faded (in or out), and how long its sung
    /// line has been out of sight: a check puts them back if they stay hidden.
    lyrics_faded: Cell<Option<Instant>>,
    lyrics_lost: Cell<u32>,
    lyrics_check: Cell<Option<Instant>>,
    sing_ms: Cell<f64>,
    /// The song's position at the last reading, and when that was.
    song_at: Cell<Option<(f64, Instant)>>,
    sing_calls: Cell<u32>,
    /// The song whose beat the backdrop follows (its key), asked for once.
    pulse_for: RefCell<String>,
    /// The song (play_gen) whose lyrics have arrived.
    lyrics_found: Cell<u64>,
    // the bar
    bar_song: gtk::Box,
    bar_art: gtk::Image,
    bar_title: gtk::Label,
    bar_byline: gtk::Label,
    bar_quality: gtk::Label,
    bar_heart: gtk::Button,
    play: gtk::Button,
    prev: gtk::Button,
    next: gtk::Button,
    shuffle: gtk::ToggleButton,
    repeat_button: gtk::Button,
    repeat: Cell<Repeat>,
    /// The queue in its own order, while shuffle is on.
    unshuffled: RefCell<Option<Vec<Track>>>,
    seek: gtk::Scale,
    pos: gtk::Label,
    len: gtk::Label,
    // playing
    player: RefCell<Option<Player>>,
    relay: Option<Relay>,
    queue: RefCell<Vec<Track>>,
    index: Cell<usize>,
    current: RefCell<Option<Track>>,
    kbps: Cell<u32>,
    quality_known: Cell<bool>,
    play_gen: Cell<u64>,
    streams: RefCell<HashMap<String, (String, u32, Instant)>>,
    /// YouTube songs whose stream is being looked up ahead.
    finding: RefCell<HashSet<String>>,
    textures: RefCell<HashMap<(String, u32), gdk::Texture>>,

    // between songs
    settings: RefCell<Settings>,
    /// The address the song playing came from (for the DJ mix's analysis).
    current_uri: RefCell<String>,
    /// The DJ mix plan for (this song, the next one).
    plan: RefCell<Option<(String, String, Plan)>>,
    analysing: RefCell<Option<(String, String)>>,
    /// The next song cued on the other deck (its key).
    cued: RefCell<Option<String>>,
    mix_timer: RefCell<Option<glib::SourceId>>,
    fade_label: gtk::Label,
    // no vocals / vocals only
    voice: Cell<Voice>,
    voice_group: adw::ToggleGroup,
    /// The mixer: one slider per part (drums, bass, other, vocals), and whether
    /// the parts are what's playing.
    mixer: gtk::Box,
    /// The mixer's row (closed at first), shown for songs split into parts.
    mixer_list: gtk::ListBox,
    part_scales: RefCell<Vec<(String, gtk::Scale)>>,
    part_meters: RefCell<Vec<(String, gtk::LevelBar)>>,
    six_check: RefCell<Option<gtk::CheckButton>>,
    part_gains: RefCell<HashMap<String, f64>>,
    /// The Mixer's effects: the Speaker EQ's low cut, high cut and reverb for
    /// the device playing (each 0 off .. 1 the most), which device that is
    /// (None: not one of the EQ's), a change on its way, and when one was made.
    fx: [gtk::Scale; 3],
    fx_device: Cell<Option<usize>>,
    /// That speaker's name, as Speaker EQ calls it.
    fx_name: RefCell<Option<String>>,
    /// A phone-sized window (see the phone breakpoint): cards and the hero drawn smaller.
    phone: Cell<bool>,
    /// What the window-size breakpoints change in Now Playing and the bar:
    /// its two halves' box, the art's column, the bar's middle.
    np_parts: RefCell<Option<(gtk::Box, gtk::Box, gtk::Box)>>,
    /// The library grids' columns (their margins shrink on a phone).
    grid_columns: RefCell<Vec<gtk::Box>>,
    /// Song rows' extra marks (split, quality, plays): hidden on a phone, where the title needs the room.
    row_extras: RefCell<Vec<glib::WeakRef<gtk::Widget>>>,
    /// The window's own buttons at the end of the bar, for the mini player (the title bar steps aside).
    window_buttons: gtk::WindowControls,
    fx_later: RefCell<Option<glib::SourceId>>,
    fx_touched: Cell<Option<Instant>>,
    mixer_row: adw::ExpanderRow,
    /// The desktop's media controls (top bar, lock screen, media keys).
    mpris: RefCell<Option<Rc<crate::mpris::Mpris>>>,
    /// Quitting for real (Quit), not just closing the window while music plays.
    quitting: Cell<bool>,
    /// Fullscreen: on or not, its button on the art, the video filling the screen,
    /// what the sidebar and side panel were, the player bar's hide timer and
    /// where the pointer last was, and the shell's parts it hides (sidebar
    /// split, the page with its header, the whole with the bar).
    full: Cell<bool>,
    full_button: gtk::Button,
    full_picture: gtk::Picture,
    full_was: Cell<(bool, bool)>,
    bar_hide: RefCell<Option<glib::SourceId>>,
    pointer: Cell<(f64, f64)>,
    shell_parts: RefCell<Option<(adw::OverlaySplitView, adw::ToolbarView, adw::ToolbarView)>>,
    /// Autoplay: the song songs like it were last asked for (once per song), and
    /// whether the queue ran out before they came (then they start at once).
    autoplay_for: RefCell<Option<String>>,
    ran_out: Cell<bool>,
    on_stems: Cell<bool>,
    mixer_lock: Cell<bool>,
    /// Song | Music video, when the other version was found.
    /// On the art: the song's official video (or back to the song), when there is one.
    video_button: gtk::Button,
    counterpart: RefCell<Option<(String, Track)>>,
    voice_status: gtk::Label,
    /// The half of a split song playing now, if one is.
    on_half: Cell<Option<Voice>>,
    splitting: RefCell<Option<Track>>,
    split_queue: RefCell<Vec<Track>>,
    /// Splits asked for by hand (their results are announced; auto-splits of
    /// liked songs are quiet).
    split_asked: RefCell<HashSet<String>>,
    split_button: gtk::Button,
    /// Pause / resume the split queue (Liked Songs' header).
    split_toggle: gtk::Button,
    split_bar: gtk::ProgressBar,
    /// Every row's split mark, to update in place.
    split_marks: RefCell<Vec<(Track, glib::WeakRef<gtk::Box>)>>,
    split_progress: Cell<f64>,
    /// The Radiance Music Servers joined: their pages, their split queues, the party you're in.
    rms: rms::Rms,
}

impl Ui {
    pub fn build(app: &adw::Application) -> Rc<Ui> {
        let provider = gtk::CssProvider::new();
        provider.load_from_string(&format!("{CSS}\n{}\n{}", home::CSS, rms::CSS));
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(&display, &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
            gtk::style_context_add_provider_for_display(&display, &LOOK.with(|l| l.clone()), gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1);
        }
        // never bigger than the screen: Windows doesn't shrink a window to fit as GNOME
        // does, so on a 1366×768 laptop the player bar was below the taskbar
        let (width, height) = gdk::Display::default()
            .and_then(|d| d.monitors().item(0)?.downcast::<gdk::Monitor>().ok())
            .map(|m| m.geometry())
            .map_or((1100, 780), |g| (1100.min(g.width() - 40).max(360), 780.min(g.height() - 110).max(294)));
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Radiance Music")
            .icon_name(APP_ID)
            .default_width(width)
            .default_height(height)
            .build();
        let button = |icon: &str, tip: &str| {
            gtk::Button::builder().icon_name(icon).tooltip_text(tip).valign(gtk::Align::Center).css_classes(["flat", "circular"]).build()
        };
        let label = |classes: &[&str]| {
            let l = gtk::Label::builder().xalign(0.0).ellipsize(gtk::pango::EllipsizeMode::End).build();
            classes.iter().for_each(|c| l.add_css_class(c));
            l
        };
        let spinner = || gtk::Spinner::builder().spinning(true).visible(false).build();
        let grid = || {
            gtk::FlowBox::builder()
                .selection_mode(gtk::SelectionMode::None)
                .homogeneous(true)
                .column_spacing(10)
                .row_spacing(18)
                .min_children_per_line(2)
                .max_children_per_line(12)
                .valign(gtk::Align::Start)
                .build()
        };
        let ui = Rc::new(Ui {
            window: window.clone(),
            // sized by the page on screen, not the widest of them all (small windows fit)
            stack: adw::ViewStack::builder().hhomogeneous(false).vhomogeneous(false).build(),
            toasts: adw::ToastOverlay::new(),
            banner: adw::Banner::builder().revealed(false).build(),
            banner_help: RefCell::default(),
            offline_pill: offline_pill(),
            online: Cell::new(true),
            lyrics_offline: Cell::new(false),
            fails: Cell::new(0),
            setup_holding: Cell::new(false),
            setup_waiting: RefCell::default(),
            split: adw::OverlaySplitView::builder()
                .sidebar_position(gtk::PackType::End)
                .show_sidebar(false)
                .min_sidebar_width(320.0)
                .max_sidebar_width(400.0)
                .build(),
            search: gtk::SearchEntry::builder().placeholder_text("Search any song or artist").hexpand(true).build(),
            search_view: gtk::Stack::new(),
            lossless: adw::PreferencesGroup::builder().title("Lossless").build(),
            lossless_spin: spinner(),
            fallback: adw::PreferencesGroup::builder().title("YouTube Music").build(),
            fallback_spin: spinner(),
            videos: adw::PreferencesGroup::builder().title("Videos").build(),
            videos_spin: spinner(),
            found_videos: RefCell::default(),
            result_rows: RefCell::default(),
            found_lossless: RefCell::default(),
            found_youtube: RefCell::default(),
            search_gen: Cell::new(0),
            debounce: RefCell::default(),
            liked_group: adw::PreferencesGroup::builder().title("Liked songs").build(),
            liked_rows: RefCell::default(),
            liked_shown: RefCell::default(),
            liked_gen: Cell::new(0),
            library_gen: Cell::new(0),
            fix_button: gtk::Button::builder()
                .tooltip_text("Saves every liked song whose file or music video isn't on this computer (with its lyrics and cover), in the background")
                .css_classes(["pill-button"])
                .visible(false)
                .build(),
            liked_search: gtk::SearchEntry::builder().placeholder_text("Search your liked songs").hexpand(true).build(),
            in_liked: adw::PreferencesGroup::builder().title("Liked Songs").visible(false).build(),
            az_rail: gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .homogeneous(true)
                .valign(gtk::Align::Center)
                .margin_end(8)
                .margin_start(2)
                .css_classes(["az-rail"])
                .tooltip_text("Jump to a letter")
                .build(),
            liked_letters: RefCell::default(),
            extras_running: Cell::new(false),
            extras_again: Cell::new(false),

            genres_busy: Cell::new(false),
            liked: RefCell::new(library::load()),
            saving: RefCell::default(),
            hearts: RefCell::default(),
            mix: RefCell::default(),
            mix_gen: Cell::new(0),
            mix_later: RefCell::default(),
            queue_group: adw::PreferencesGroup::builder().title("Queue").build(),
            recent_group: adw::PreferencesGroup::builder()
                .title("Played before")
                .description("Go back to one: it plays now, then the queue carries on")
                .visible(false)
                .build(),
            recent_rows: RefCell::default(),
            queue_rows: RefCell::default(),
            queue_button: gtk::ToggleButton::builder()
                .icon_name("view-list-bullet-symbolic")
                .tooltip_text("Queue: what plays next")
                .valign(gtk::Align::Center)
                .css_classes(["flat", "circular"])
                .build(),
            backdrop: Backdrop::default(),
            backdrop_n: Cell::new(0),
            nav: gtk::ListBox::builder().css_classes(["navigation-sidebar"]).build(),
            home_box: gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(34).margin_top(8).margin_bottom(28).margin_start(28).margin_end(28).build(),
            home_stale: Cell::new(home::ALL_OF_HOME),
            home_sections: RefCell::default(),
            home_later: RefCell::default(),
            home_gen: Cell::new(0),
            starting: Cell::new(true),
            albums_flow: grid(),
            artists_flow: grid(),
            genres_flow: grid(),
            group_box: gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(22).margin_top(10).margin_bottom(28).margin_start(28).margin_end(28).build(),
            group_back: RefCell::new("albums".into()),
            history: RefCell::default(),
            history_box: gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(24).margin_top(18).margin_bottom(24).margin_start(28).margin_end(28).build(),
            restored: Cell::new(false),
            restore_at: Cell::new(None),
            last_save: Cell::new(None),
            cover: gtk::Stack::builder().transition_type(gtk::StackTransitionType::Crossfade).transition_duration(500).build(),
            cover_frame: adw::Clamp::builder().maximum_size(480).build(),
            video_height: Cell::new(0),
            volume: gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.02),
            mute: gtk::ToggleButton::builder()
                .icon_name("audio-volume-high-symbolic")
                .tooltip_text("Mute")
                .valign(gtk::Align::Center)
                .css_classes(["flat", "circular"])
                .build(),
            np_text: gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(10).build(),
            // long titles wrap instead of taking the lyrics' room
            np_title: gtk::Label::builder().xalign(0.0).wrap(true).wrap_mode(gtk::pango::WrapMode::WordChar).max_width_chars(26).css_classes(["np-title"]).label("Nothing playing").build(),
            np_byline: gtk::Label::builder().xalign(0.0).wrap(true).wrap_mode(gtk::pango::WrapMode::WordChar).max_width_chars(40).css_classes(["dim-label"]).build(),
            np_quality: gtk::Label::builder().halign(gtk::Align::Start).css_classes(["pill", "compressed"]).visible(false).build(),
            lyrics_list: Lens::default(),
            lyrics_scroll: gtk::ScrolledWindow::builder().hexpand(true).vexpand(true).width_request(320).hscrollbar_policy(gtk::PolicyType::Never).css_classes(["lyrics-scroll"]).build(),
            lyric_lines: RefCell::default(),
            side_lyrics_list: Lens::default(),
            side_lyrics_scroll: gtk::ScrolledWindow::builder().vexpand(true).hscrollbar_policy(gtk::PolicyType::Never).css_classes(["lyrics-scroll"]).build(),
            side_tabs: adw::ToggleGroup::builder().margin_top(10).margin_bottom(6).margin_start(12).margin_end(12).build(),
            side_stack: gtk::Stack::builder().transition_type(gtk::StackTransitionType::Crossfade).transition_duration(220).vexpand(true).build(),
            split_list: gtk::ListBox::builder().selection_mode(gtk::SelectionMode::None).css_classes(["boxed-list"]).margin_start(12).margin_end(12).margin_bottom(12).build(),
            split_now_row: RefCell::default(),
            split_card: gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(4).css_classes(["split-card"]).visible(false).build(),
            split_queue_btn: gtk::Button::builder().icon_name("view-list-bullet-symbolic").tooltip_text("The split queue").valign(gtk::Align::Start).css_classes(["flat", "circular"]).visible(false).build(),
            suggest_flow: gtk::FlowBox::builder().selection_mode(gtk::SelectionMode::None).column_spacing(8).row_spacing(8).min_children_per_line(1).max_children_per_line(12).build(),
            suggest_gen: Cell::new(0),
            playlist_box: gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(18).margin_top(18).margin_bottom(24).margin_start(18).margin_end(18).build(),
            playlist_now: Cell::new(None),
            lyrics_button: gtk::Button::builder()
                .icon_name("format-justify-center-symbolic")
                .tooltip_text("Lyrics")
                .valign(gtk::Align::Center)
                .css_classes(["flat", "circular"])
                .build(),
            lyric_now: RefCell::default(),
            lyric_lead: Cell::new(None),
            lyrics_faded: Cell::new(None),
            lyrics_lost: Cell::new(0),
            lyrics_check: Cell::new(None),
            sing_ms: Cell::new(0.0),
            song_at: Cell::new(None),
            sing_calls: Cell::new(0),
            pulse_for: RefCell::default(),
            lyrics_found: Cell::new(0),
            bar_song: gtk::Box::new(gtk::Orientation::Horizontal, 10),
            bar_art: gtk::Image::builder().icon_name("audio-x-generic-symbolic").pixel_size(48).overflow(gtk::Overflow::Hidden).css_classes(["art"]).build(),
            bar_title: label(&["heading"]),
            bar_byline: label(&["dim-label", "caption"]),
            bar_quality: gtk::Label::builder().css_classes(["pill", "compressed"]).valign(gtk::Align::Center).visible(false).build(),
            bar_heart: button("heart-outline-symbolic", "Like: saves the song"),
            play: button("media-playback-start-symbolic", "Play"),
            prev: button("media-skip-backward-symbolic", "Previous"),
            next: button("media-skip-forward-symbolic", "Next"),
            shuffle: gtk::ToggleButton::builder()
                .icon_name("media-playlist-shuffle-symbolic")
                .tooltip_text("Shuffle")
                .valign(gtk::Align::Center)
                .css_classes(["flat", "circular"])
                .build(),
            repeat_button: button("media-playlist-repeat-symbolic", "Repeat: off"),
            repeat: Cell::new(Repeat::Off),
            unshuffled: RefCell::default(),
            seek: gtk::Scale::builder().orientation(gtk::Orientation::Horizontal).hexpand(true).draw_value(false).build(),
            pos: label(&["numeric", "caption"]),
            len: label(&["numeric", "caption"]),
            player: RefCell::new(None),
            relay: Relay::start().ok(),
            queue: RefCell::default(),
            index: Cell::new(0),
            current: RefCell::new(None),
            kbps: Cell::new(0),
            quality_known: Cell::new(false),
            play_gen: Cell::new(0),
            streams: RefCell::default(),
            finding: RefCell::default(),
            textures: RefCell::default(),

            settings: RefCell::new(settings::load()),
            current_uri: RefCell::default(),
            plan: RefCell::default(),
            analysing: RefCell::default(),
            cued: RefCell::default(),
            mix_timer: RefCell::default(),
            fade_label: gtk::Label::builder().xalign(0.0).hexpand(true).build(),
            voice: Cell::new(Voice::Song),
            voice_group: adw::ToggleGroup::builder().halign(gtk::Align::Start).margin_top(6).build(),
            mixer: gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(2).margin_top(4).visible(false).build(),
            mixer_list: gtk::ListBox::builder().selection_mode(gtk::SelectionMode::None).css_classes(["boxed-list"]).margin_start(8).margin_end(8).margin_top(8).visible(false).build(),
            part_scales: RefCell::default(),
            part_meters: RefCell::default(),
            six_check: RefCell::default(),
            part_gains: RefCell::new(split::PARTS.iter().map(|p| (p.to_string(), 1.0)).collect()),
            fx: std::array::from_fn(|_| gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.02)),
            fx_device: Cell::new(None),
            fx_name: RefCell::new(None),
            phone: Cell::new(false),
            np_parts: RefCell::new(None),
            grid_columns: RefCell::default(),
            row_extras: RefCell::default(),
            window_buttons: gtk::WindowControls::builder().side(gtk::PackType::End).decoration_layout(":minimize,close").visible(false).build(),
            fx_later: RefCell::default(),
            fx_touched: Cell::new(None),
            mpris: RefCell::default(),
            quitting: Cell::new(false),
            full: Cell::new(false),
            full_button: gtk::Button::builder().icon_name("view-fullscreen-symbolic").tooltip_text("Fullscreen (F11)").css_classes(["osd", "circular"]).build(),
            full_picture: gtk::Picture::builder().content_fit(gtk::ContentFit::Contain).can_shrink(true).hexpand(true).vexpand(true).build(),
            full_was: Cell::new((true, false)),
            bar_hide: RefCell::default(),
            pointer: Cell::new((0.0, 0.0)),
            shell_parts: RefCell::default(),
            autoplay_for: RefCell::default(),
            ran_out: Cell::new(false),
            mixer_row: adw::ExpanderRow::builder().title("Mixer").subtitle("Each part as loud as you like").subtitle_lines(1).expanded(false).build(),
            on_stems: Cell::new(false),
            mixer_lock: Cell::new(false),
            video_button: gtk::Button::builder()
                .icon_name("camera-video-symbolic")
                .tooltip_text("Watch the music video")
                .halign(gtk::Align::End)
                .valign(gtk::Align::Start)
                .margin_end(10)
                .margin_top(10)
                .css_classes(["osd", "circular"])
                .visible(false)
                .build(),
            counterpart: RefCell::default(),
            voice_status: gtk::Label::builder().xalign(0.0).wrap(true).wrap_mode(gtk::pango::WrapMode::WordChar).css_classes(["dim-label", "caption"]).build(),
            on_half: Cell::new(None),
            splitting: RefCell::default(),
            split_queue: RefCell::default(),
            split_asked: RefCell::default(),
            split_bar: gtk::ProgressBar::builder().show_text(true).visible(false).margin_top(4).css_classes(["split-bar"]).build(),
            split_progress: Cell::new(0.0),
            split_marks: RefCell::default(),
            rms: rms::Rms::default(),
            split_toggle: gtk::Button::builder().valign(gtk::Align::Center).css_classes(["flat"]).visible(false).build(),
            split_button: gtk::Button::builder()
                .label("Split this song")
                .tooltip_text("Splits it into the voice and the music on the split server (about 3 minutes, while it plays). Then No vocals and Vocals only switch at once.")
                .halign(gtk::Align::Start)
                .css_classes(["pill-button"])
                .visible(false)
                .build(),
        });

        let me = Rc::downgrade(&ui);
        match Player::new(move |ev| {
            if let Some(me) = me.upgrade() {
                me.on_player(ev);
            }
        }) {
            Ok(p) => *ui.player.borrow_mut() = Some(p),
            Err(e) => {
                debug(&format!("player: {e}"));
                ui.cant_work(
                    "Radiance Music can't play sound here: a part of GStreamer, which plays the music, is missing",
                    "https://github.com/RadianceADM/radiance-music#to-play-music",
                );
            }
        }
        ui.layout();
        ui.connect();
        apply_look(&ui.settings.borrow().look);
        let me = Rc::downgrade(&ui);
        adw::StyleManager::default().connect_dark_notify(move |_| {
            if let Some(me) = me.upgrade() {
                apply_look(&me.settings.borrow().look);
            }
        });
        library::set_music_dir(ui.settings.borrow().music_dir.clone().map(std::path::PathBuf::from));
        // the joined servers' keys, before any of their covers or songs are fetched (the last song's, say)
        servers::set_joined(&ui.settings.borrow().servers);
        // the rest behind the splash, a step at a time (it keeps moving between them)
        let look = if adw::StyleManager::default().is_dark() { ui.settings.borrow().look.clone() } else { "stone".into() };
        let splash = splash::Splash::cover(&ui.window, splash::Look::of(&look));
        ui.start_up(splash);
        let u = ui.clone(); // the window keeps the app's state alive
        window.connect_close_request(move |w| {
            if !u.starting.get() {
                u.save_session(); // (still starting: where you were isn't back yet, keep it as it was)
            }
            // closed while music plays: it plays on (the media controls, or opening the app, bring it
            // back; on Windows the controls are its own, and opening it again reaches this copy
            // through one_copy in main.rs)
            let playing = u.player.borrow().as_ref().is_some_and(|p| p.playing());
            if playing && !u.quitting.get() && std::env::var_os("RADIANCE_SHOT").is_none() {
                w.set_visible(false);
                crate::log::note("window closed: the music plays on");
                return glib::Propagation::Stop;
            }
            if let Some(p) = u.player.borrow().as_ref() {
                p.stop();
            }
            glib::Propagation::Proceed
        });
        ui
    }

    /// What starting takes once the window is up. Behind the splash every step
    /// has a moment of its own (the splash moves on between them) and Home is
    /// built a section at a time; without one, all of it at once.
    fn start_up(self: &Rc<Self>, splash: Option<splash::Splash>) {
        type Step = Box<dyn FnOnce(&Rc<Ui>)>;
        let steps: Vec<(&'static str, Step)> = vec![
            ("search", Box::new(|ui| ui.render_results(""))),
            ("the last song", Box::new(|ui| {
                ui.restore_session();
                ui.render_playlist_nav();
                ui.start_servers();
                ui.update_suggestions();
            })),
            ("the queue", Box::new(|ui| {
                ui.backfill_library();
                ui.save_extras(Duration::from_secs(120));
                let me = Rc::downgrade(ui);
                glib::timeout_add_local_once(Duration::from_secs(20), move || {
                    if let Some(me) = me.upgrade() {
                        me.auto_split_liked(); // after the app has settled
                    }
                });
                ui.render_queue();
                ui.refresh_mix();
            })),
        ];
        // the splitter's scripts as this app has them (a split on this computer uses its copy)
        if self.settings.borrow().local_split.is_some() {
            std::thread::spawn(crate::setup::refresh_scripts);
        }
        let began = Instant::now();
        self.get_splitter_ready(splash.as_ref());
        let Some(splash) = splash else {
            for (_, step) in steps {
                step(self);
            }
            self.render_liked();
            self.starting.set(false);
            self.render_home();
            self.started(began, &[]);
            return;
        };
        let took: Rc<RefCell<Vec<(&'static str, u128)>>> = Rc::default();
        let mut steps = steps.into_iter();
        let first = steps.next();
        let liked_from = Rc::new(Cell::new(Instant::now()));
        for (what, step) in first.into_iter() {
            let (me, took) = (Rc::downgrade(self), took.clone());
            splash.first(what, move || {
                if let Some(me) = me.upgrade() {
                    let t = Instant::now();
                    step(&me);
                    took.borrow_mut().push((what, t.elapsed().as_millis()));
                }
            });
        }
        let (me, from) = (Rc::downgrade(self), liked_from.clone());
        splash.then("Liked Songs", move || {
            if me.upgrade().is_some() {
                from.set(Instant::now());
            }
        });
        self.render_liked_behind(&splash);
        let (me, took2) = (Rc::downgrade(self), took.clone());
        splash.then("Liked Songs", move || {
            if me.upgrade().is_some() {
                took2.borrow_mut().push(("Liked Songs", liked_from.get().elapsed().as_millis()));
            }
        });
        for (what, step) in steps {
            let (me, took) = (Rc::downgrade(self), took.clone());
            splash.first(what, move || {
                if let Some(me) = me.upgrade() {
                    let t = Instant::now();
                    step(&me);
                    took.borrow_mut().push((what, t.elapsed().as_millis()));
                }
            });
        }
        // last, Home, a section a frame
        let home_from = Rc::new(Cell::new(Instant::now()));
        let (me, from) = (Rc::downgrade(self), home_from.clone());
        splash.then("Home", move || {
            if let Some(me) = me.upgrade() {
                me.starting.set(false);
                from.set(Instant::now());
            }
        });
        self.render_home_behind(&splash);
        let (me, s) = (Rc::downgrade(self), splash.clone());
        splash.then("the last bits", move || {
            if let Some(me) = me.upgrade() {
                took.borrow_mut().push(("Home", home_from.get().elapsed().as_millis()));
                me.started(began, &took.borrow());
            }
            s.ready();
        });
    }

    /// Splitting isn't set up on this computer yet (and there's no split
    /// server): it's set up now, without asking, like an app getting what it
    /// needs on its first start. The splash stays on the sunrise meanwhile,
    /// showing how far along it is, and goes on once it's done; a failure (no
    /// internet) is tried again shortly. A computer that can't split songs
    /// just goes on without it.
    fn get_splitter_ready(self: &Rc<Self>, splash: Option<&splash::Splash>) {
        {
            let s = self.settings.borrow();
            // (a splitter whose Python is gone, an old Windows test install's: set up again)
            if s.split.is_some() || s.local_split.as_ref().is_some_and(|l| std::path::Path::new(&l.python).exists()) {
                return;
            }
        }
        // RADIANCE_SHOT_SETUP=1: the steps acted out (screenshot checks); otherwise never in a screenshot copy
        let acted = std::env::var_os("RADIANCE_SHOT_SETUP").is_some();
        if std::env::var_os("RADIANCE_SHOT").is_some() && !acted {
            return;
        }
        if let Some(s) = splash {
            s.hold();
            self.setup_holding.set(true);
            s.progress("Getting Radiance Music ready…", 0.0);
        }
        self.set_splitter_up(splash.cloned(), acted);
    }

    fn set_splitter_up(self: &Rc<Self>, splash: Option<splash::Splash>, acted: bool) {
        let (tx, rx) = async_channel::unbounded::<(String, f64)>();
        if let Some(s) = &splash {
            let sp = s.downgrade();
            glib::spawn_future_local(async move {
                while let Ok((what, done)) = rx.recv().await {
                    let Some(s) = sp.upgrade() else { break };
                    s.progress(&what, done);
                }
            });
        }
        crate::log::note("getting splitting ready on this computer (its first start)");
        let threads = crate::setup::half_the_cores();
        let me = Rc::downgrade(self);
        bg(
            move || -> Result<Option<settings::LocalSplitter>, String> {
                let say = move |what: &str, done: f64| {
                    let mut step = what.to_string();
                    if let Some(first) = step.get(..1) {
                        step = first.to_lowercase() + &step[1..];
                    }
                    let _ = tx.try_send((format!("Getting Radiance Music ready: {step}"), done));
                };
                if acted {
                    // RADIANCE_SHOT_SETUP=fail: a failed try (what offline looks like)
                    if std::env::var("RADIANCE_SHOT_SETUP").as_deref() == Ok("fail") {
                        say("Downloading PyTorch…", 0.1);
                        std::thread::sleep(Duration::from_millis(900));
                        return Err("acted: Failed to establish a new connection: [Errno -3] Temporary failure in name resolution".into());
                    }
                    for (i, what) in ["Downloading PyTorch…", "Downloading Demucs…", "Downloading the splitting model (about 80 MB)…", "Downloading the word-timing model (about 1.2 GB)…", "Checking it works…"].iter().enumerate() {
                        say(what, i as f64 / 6.0);
                        std::thread::sleep(Duration::from_millis(900));
                    }
                    return Ok(None);
                }
                if !crate::setup::check().problems().is_empty() {
                    return Ok(None); // this computer can't split songs: on without it
                }
                crate::setup::install(threads, &say).map(Some)
            },
            move |done| {
                let Some(me) = me.upgrade() else { return };
                match done {
                    Ok(Some(local)) => {
                        crate::log::note("splitting is set up on this computer");
                        me.settings.borrow_mut().local_split = Some(local);
                        settings::save(&me.settings.borrow());
                        me.update_voice_status();
                        me.split_liked_waiting();
                        if let Some(s) = &splash {
                            s.release();
                        }
                        me.setup_holding.set(false);
                    }
                    Ok(None) => {
                        if let Some(s) = &splash {
                            s.release();
                        }
                        me.setup_holding.set(false);
                    }
                    Err(e) => {
                        crate::log::note(&format!("getting splitting ready stopped: {e}"));
                        // tried again when the internet's back, or in a while; meanwhile the sunrise
                        // says why and offers a way on (it had none offline: 2026-10-31 audit)
                        let (w1, w2) = (Rc::downgrade(&me), Rc::downgrade(&me));
                        let again = splash.clone();
                        me.setup_waiting.replace(Some(Box::new(move || {
                            if let Some(me) = w1.upgrade() {
                                me.set_splitter_up(again, acted);
                            }
                        })));
                        glib::timeout_add_local_once(Duration::from_secs(30), move || {
                            if let Some(me) = w2.upgrade() {
                                let job = me.setup_waiting.take();
                                if let Some(job) = job {
                                    job();
                                }
                            }
                        });
                        let offline = me.why_not(&e).0 == "you're offline";
                        if offline {
                            me.set_online(false, true);
                        }
                        if let Some(s) = &splash {
                            s.progress(
                                if !offline {
                                    "Couldn't get splitting ready yet. Trying again in a moment…"
                                } else {
                                    "You're offline. Splitting songs needs a one-time download: it starts when you're back online."
                                },
                                0.0,
                            );
                            s.offer(&me.setup_buttons(s, acted));
                        }
                    }
                }
            },
        );
    }

    /// Under the sunrise when getting splitting ready failed: try again now, or open the app
    /// and let it get ready in the background (as soon as it can).
    fn setup_buttons(self: &Rc<Self>, splash: &splash::Splash, acted: bool) -> gtk::Box {
        let row = gtk::Box::builder().spacing(12).halign(gtk::Align::Center).valign(gtk::Align::End).margin_bottom(56).build();
        let again = gtk::Button::builder().child(&adw::ButtonContent::builder().icon_name("view-refresh-symbolic").label("Try Again").build()).css_classes(["pill"]).build();
        let later = gtk::Button::builder()
            .child(&adw::ButtonContent::builder().icon_name("media-playback-start-symbolic").label("Open Radiance Music Now").build())
            .css_classes(["pill", "suggested-action"])
            .tooltip_text("Play music now; splitting gets ready in the background when the internet's back")
            .build();
        row.append(&again);
        row.append(&later);
        let (me, sp) = (Rc::downgrade(self), splash.downgrade());
        again.connect_clicked(move |_| {
            let Some(me) = me.upgrade() else { return };
            if let Some(s) = sp.upgrade() {
                s.clear_offer();
                s.progress("Getting Radiance Music ready…", 0.0);
            }
            let job = me.setup_waiting.take();
            if let Some(job) = job {
                job();
            }
        });
        let (me, sp) = (Rc::downgrade(self), splash.downgrade());
        later.connect_clicked(move |_| {
            let Some(me) = me.upgrade() else { return };
            if let Some(s) = sp.upgrade() {
                s.release(); // (its buttons go with it)
            }
            me.setup_holding.set(false);
            // the next try has no sunrise to report to
            let w = Rc::downgrade(&me);
            me.setup_waiting.replace(Some(Box::new(move || {
                if let Some(me) = w.upgrade() {
                    me.set_splitter_up(None, acted);
                }
            })));
        });
        row
    }

    /// Something the app can't work without is missing: a banner over the page until it's
    /// fixed (a toast was gone in 4 seconds, and then "Loading…" for ever: 2026-10-31 audit).
    fn cant_work(&self, what: &str, help: &str) {
        crate::log::note(&format!("missing: {what}"));
        self.banner.set_title(what);
        self.banner.set_button_label(Some("How to Fix It"));
        self.banner_help.replace(help.to_string());
        self.banner.set_revealed(true);
    }

    /// The internet came or went (GIO's network monitor): "Offline" in the top bar while
    /// it's gone; when it's back, what came up empty meanwhile is looked for again.
    fn set_online(self: &Rc<Self>, online: bool, changed: bool) {
        let was = self.online.replace(online);
        self.offline_pill.set_visible(!online);
        if !changed || was == online {
            return;
        }
        crate::log::note(if online { "back online" } else { "offline" });
        if !online {
            return;
        }
        self.toast("Back online");
        if !self.search.text().trim().is_empty() {
            self.run_search();
        }
        if self.lyrics_offline.get() {
            let t = self.current.borrow().clone();
            if let Some(t) = t {
                self.load_lyrics(&t, self.play_gen.get());
            }
        }
        let job = self.setup_waiting.take();
        if let Some(job) = job {
            job();
        }
    }

    /// Why a song won't play, in plain words, and whether every song would fail the same way
    /// now (then skipping on only makes a run of failures).
    fn why_not(&self, error: &str) -> (&'static str, bool) {
        let e = error.to_lowercase();
        let any = |words: &[&str]| words.iter().any(|w| e.contains(w));
        if !self.online.get() || any(&["resolve", "name resolution", "unreachable", "no route", "getaddrinfo", "connection refused", "could not connect", "network is down", "failed to establish a new connection"]) {
            ("you're offline", true)
        } else if e.contains("yt-dlp isn't installed") {
            ("yt-dlp, which plays YouTube songs, isn't installed", true)
        } else if any(&["sign in to confirm", "not a bot"]) {
            ("YouTube wants to check you're not a bot; try again in a while", true)
        } else if any(&["timed out", "timeout"]) {
            ("the internet is too slow right now", true)
        } else if any(&["missing plugin", "no decoder", "not-negotiated", "not negotiated", "no element"]) {
            ("this computer can't play this kind of file", false)
        } else if any(&["unavailable", "private video", "removed", "copyright", "not available", "terminated"]) {
            ("it isn't on YouTube anymore", false)
        } else {
            ("its source didn't answer", false)
        }
    }

    /// The start's last bits, and how long it all took (in the app log).
    fn started(self: &Rc<Self>, began: Instant, took: &[(&'static str, u128)]) {
        self.look_for_updates(false); // a newer version out? (the pop-up, a little later)
        // likes that weren't saved yet (offline, or the app closed while saving)
        let unsaved: Vec<Track> = self.liked.borrow().iter().filter(|t| t.file.is_none() && library::saves(t)).cloned().collect();
        unsaved.into_iter().for_each(|t| self.save_liked(t));
        self.start_media_controls();
        // YouTube songs need yt-dlp: missing, it's said now (not "Loading…" at the first song)
        if std::env::var_os("RADIANCE_SHOT").is_none() {
            let me = Rc::downgrade(self);
            bg(youtube::installed, move |ok| {
                if let (Some(me), false) = (me.upgrade(), ok) {
                    me.cant_work("YouTube songs can't play: yt-dlp, which gets them, isn't installed", "https://github.com/RadianceADM/radiance-music#to-play-music");
                }
            });
        }
        self.home_later_parts(0); // what changed while it was being built
        // how to use it, the first time the app opens (once the start's dark has opened)
        if !self.settings.borrow().guide_seen && std::env::var_os("RADIANCE_SHOT").is_none() {
            // (not over the sunrise while it holds for the first start's setup: after it)
            let me = Rc::downgrade(self);
            glib::timeout_add_local(Duration::from_millis(1800), move || {
                let Some(me) = me.upgrade() else { return glib::ControlFlow::Break };
                if me.setup_holding.get() {
                    return glib::ControlFlow::Continue;
                }
                me.show_guide();
                glib::ControlFlow::Break
            });
        }
        let steps: Vec<String> = took.iter().map(|(what, ms)| format!("{what} {ms} ms")).collect();
        let line = format!("started Radiance Music {} in {} ms{}", env!("CARGO_PKG_VERSION"), began.elapsed().as_millis(), if steps.is_empty() { String::new() } else { format!(" ({})", steps.join(", ")) });
        if std::env::var_os("RADIANCE_SHOT").is_some() {
            eprintln!("{line}"); // (screenshot copies keep out of the log)
        }
        crate::log::note(&line);
    }

    /// Fullscreen: Now Playing fills the screen (the sidebar, the side panel and
    /// the header step aside; the player bar fades away until the pointer moves),
    /// a video fills it whole. Off: all as it was.
    fn set_full(self: &Rc<Self>, on: bool) {
        if self.full.get() == on {
            return;
        }
        let Some((nav, page, whole)) = self.shell_parts.borrow().clone() else { return };
        self.full.set(on);
        if on {
            self.full_was.set((nav.shows_sidebar(), self.queue_button.is_active()));
            self.queue_button.set_active(false);
            nav.set_show_sidebar(false);
            page.set_reveal_top_bars(false);
            self.window.add_css_class("fullscreen-player");
            self.full_button.set_icon_name("view-restore-symbolic");
            self.full_button.set_tooltip_text(Some("Leave fullscreen (Esc)"));
            self.window.fullscreen();
            if self.stack.visible_child_name().as_deref() != Some("playing") {
                self.stack.set_visible_child_name("playing");
            }
            self.show_full_page();
            self.nudge_bar();
        } else {
            if let Some(id) = self.bar_hide.borrow_mut().take() {
                id.remove();
            }
            self.window.set_cursor(None::<&gdk::Cursor>);
            whole.set_reveal_bottom_bars(true);
            page.set_reveal_top_bars(true);
            let (sidebar, panel) = self.full_was.get();
            nav.set_show_sidebar(sidebar);
            self.queue_button.set_active(panel);
            self.window.remove_css_class("fullscreen-player");
            self.full_button.set_icon_name("view-fullscreen-symbolic");
            self.full_button.set_tooltip_text(Some("Fullscreen (F11)"));
            if self.window.is_fullscreen() {
                self.window.unfullscreen();
            }
            if self.stack.visible_child_name().as_deref() == Some("fullvideo") {
                self.stack.set_visible_child_name("playing");
            }
            self.full_picture.set_paintable(None::<&gdk::Paintable>);
        }
    }

    /// In fullscreen: a video on its own full page, a song on Now Playing.
    fn show_full_page(&self) {
        if !self.full.get() {
            return;
        }
        let video = self.current.borrow().as_ref().is_some_and(|t| t.video);
        let picture = self.cover.visible_child().and_downcast::<gtk::Picture>().and_then(|p| p.paintable());
        match (video, picture) {
            (true, Some(p)) => {
                self.full_picture.set_paintable(Some(&p));
                self.stack.set_visible_child_name("fullvideo");
            }
            _ => {
                self.full_picture.set_paintable(None::<&gdk::Paintable>);
                if self.stack.visible_child_name().as_deref() == Some("fullvideo") {
                    self.stack.set_visible_child_name("playing");
                }
            }
        }
    }

    /// In fullscreen the pointer moved: the player bar (and the pointer) show,
    /// and fade away again after 3 s still.
    fn nudge_bar(self: &Rc<Self>) {
        let Some((_, _, whole)) = self.shell_parts.borrow().clone() else { return };
        whole.set_reveal_bottom_bars(true);
        self.window.set_cursor(None::<&gdk::Cursor>);
        if let Some(id) = self.bar_hide.borrow_mut().take() {
            id.remove();
        }
        let me = Rc::downgrade(self);
        let id = glib::timeout_add_local_once(Duration::from_secs(3), move || {
            let Some(me) = me.upgrade() else { return };
            me.bar_hide.borrow_mut().take();
            if me.full.get() {
                if let Some((_, _, whole)) = me.shell_parts.borrow().clone() {
                    whole.set_reveal_bottom_bars(false);
                }
                me.window.set_cursor_from_name(Some("none"));
            }
        });
        *self.bar_hide.borrow_mut() = Some(id);
    }

    /// Quits for real: the music stops (Quit in the menu, Ctrl+Q, or the media controls).
    fn quit_app(&self) {
        self.quitting.set(true);
        self.window.set_visible(true);
        self.window.close();
    }

    /// The desktop's media controls: the song and play / pause / skip in GNOME's
    /// top bar, on the lock screen and on the keyboard's media keys.
    fn start_media_controls(self: &Rc<Self>) {
        if std::env::var_os("RADIANCE_SHOT").is_some() {
            return; // a screenshot copy stays out of them
        }
        use crate::mpris::{Command, Song, State};
        let (me, me2) = (Rc::downgrade(self), Rc::downgrade(self));
        let m = crate::mpris::Mpris::start(
            move |c| {
                let Some(me) = me.upgrade() else { return };
                let playing = me.player.borrow().as_ref().is_some_and(|p| p.playing());
                match c {
                    Command::PlayPause => me.toggle_play(),
                    Command::Play if !playing => me.toggle_play(),
                    Command::Pause if playing => me.toggle_play(),
                    Command::Play | Command::Pause => {}
                    Command::Next => me.skip(1),
                    Command::Previous => me.skip(-1),
                    Command::Raise => me.window.present(),
                    Command::Quit => me.quit_app(),
                }
            },
            move || {
                let Some(me) = me2.upgrade() else { return State::default() };
                let song = me.current.borrow().as_ref().map(|t| Song {
                    key: t.key(),
                    title: t.title.clone(),
                    artists: t.artists(),
                    album: t.album.clone(),
                    art: art_source(t),
                    length: t.seconds as i64 * 1_000_000,
                });
                let player = me.player.borrow();
                let playing = player.as_ref().is_some_and(|p| p.playing());
                let position = player.as_ref().and_then(|p| p.position()).map(|s| (s * 1e6) as i64).unwrap_or(0);
                State { playing, song, position }
            },
        );
        *self.mpris.borrow_mut() = Some(m);
    }

    fn layout(self: &Rc<Self>) {
        self.stack.set_enable_transitions(true); // the tabs fade into each other
        // Search
        let search_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let clamp = adw::Clamp::builder().maximum_size(1240).tightening_threshold(900).child(&self.search).margin_top(12).margin_bottom(6).margin_start(18).margin_end(18).build();
        search_box.append(&clamp);
        let suggest = adw::Clamp::builder().maximum_size(1240).tightening_threshold(900).child(&self.suggest_flow).margin_start(18).margin_end(18).margin_bottom(4).build();
        search_box.append(&suggest);
        let empty = self.browse_page();

        self.lossless.set_header_suffix(Some(&self.lossless_spin));
        self.fallback.set_header_suffix(Some(&self.fallback_spin));
        self.videos.set_header_suffix(Some(&self.videos_spin));
        let results = wide_page(&[&self.in_liked, &self.lossless, &self.fallback, &self.videos]);
        self.search_view.add_named(&empty, Some("empty"));
        self.search_view.add_named(&results, Some("results"));
        search_box.append(&self.search_view);
        self.stack.add_titled_with_icon(&self.home_page(), Some("home"), "Home", "go-home-symbolic");
        self.stack.add_titled_with_icon(&search_box, Some("search"), "Search", "system-search-symbolic");
        let playlist_clamp = adw::Clamp::builder().maximum_size(1240).tightening_threshold(900).child(&self.playlist_box).build();
        self.stack.add_named(&gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&playlist_clamp).build(), Some("playlist"));
        // a Radiance Music Server's songs and party (the sidebar has one entry per server)
        self.stack.add_named(&self.server_page(), Some("server"));

        // Liked, with "For you" on top

        let liked_buttons = gtk::Box::builder().spacing(8).valign(gtk::Align::Center).build();
        let play = gtk::Button::builder().label("Play").css_classes(["suggested-action", "pill-button"]).build();
        let shuffle = gtk::Button::builder().label("Shuffle").css_classes(["pill-button"]).build();
        let me = Rc::downgrade(self);
        play.connect_clicked(move |_| {
            if let Some(me) = me.upgrade() {
                let list = me.liked_listed(); // as listed: A to Z
                me.play_list(list, 0);
            }
        });
        let me = Rc::downgrade(self);
        shuffle.connect_clicked(move |_| {
            if let Some(me) = me.upgrade() {
                let list = shuffled(me.liked_listed());
                me.play_list(list, 0);
            }
        });
        liked_buttons.append(&play);
        liked_buttons.append(&shuffle);
        liked_buttons.append(&self.fix_button);
        let me = Rc::downgrade(self);
        self.fix_button.connect_clicked(move |_| {
            if let Some(me) = me.upgrade() {
                me.fix_missing();
            }
        });
        liked_buttons.append(&self.split_toggle);
        self.liked_group.set_header_suffix(Some(&liked_buttons));
        let me = Rc::downgrade(self);
        self.split_toggle.connect_clicked(move |_| {
            if let Some(me) = me.upgrade() {
                me.set_split_paused(!Self::split_paused());
            }
        });
        let liked = wide_page(&[&self.liked_group]);
        liked.set_hexpand(true);
        // the A to Z rail is its scroll bar: no other one
        liked.set_policy(gtk::PolicyType::Never, gtk::PolicyType::External);
        let list_and_rail = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        list_and_rail.append(&liked);
        list_and_rail.append(&self.az_rail_for(&liked));
        // its search stays on top as the list scrolls
        let me = Rc::downgrade(self);
        self.liked_search.connect_search_changed(move |_| {
            if let Some(me) = me.upgrade() {
                me.filter_liked();
            }
        });
        let liked_page = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let find = adw::Clamp::builder().maximum_size(1240).tightening_threshold(900).child(&self.liked_search).margin_top(12).margin_start(18).margin_end(18).build();
        liked_page.append(&find);
        liked_page.append(&list_and_rail);
        self.stack.add_titled_with_icon(&liked_page, Some("liked"), "Liked Songs", "heart-filled-symbolic");
        self.stack.add_titled_with_icon(&self.grid_page("Albums", &self.albums_flow), Some("albums"), "Albums", "media-optical-symbolic");
        self.stack.add_titled_with_icon(&self.grid_page("Artists", &self.artists_flow), Some("artists"), "Artists", "avatar-default-symbolic");
        self.stack.add_titled_with_icon(&self.grid_page("Genres", &self.genres_flow), Some("genres"), "Genres", "view-grid-symbolic");
        self.stack.add_named(&self.group_page(), Some("group"));
        self.stack.add_titled_with_icon(&self.history_page(), Some("history"), "History", "document-open-recent-symbolic");

        // Now playing: art and lyrics over the art's own colours
        // two halves sharing the room: the art centred in one, the lyrics in the other (one, when narrow)
        let np = gtk::Box::builder().orientation(gtk::Orientation::Horizontal).homogeneous(true).spacing(36).margin_top(24).margin_bottom(24).margin_start(32).margin_end(32).build();
        let left = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(10).valign(gtk::Align::Center).width_request(300).build();
        // the split's progress and its queue: a card at the bottom of the sidebar (see shell)
        let split_line = gtk::Box::builder().spacing(6).build();
        self.voice_status.set_hexpand(true);
        split_line.append(&self.voice_status);
        split_line.append(&self.split_queue_btn);
        self.split_card.append(&split_line);
        self.split_card.append(&self.split_bar);
        // your songs being split on a server: where they are in its queue, and whose turn is next
        self.split_card.append(&self.rms.away_line);
        self.split_card.append(&self.rms.away_bar);
        let open = gtk::GestureClick::new();
        let me = Rc::downgrade(self);
        open.connect_released(move |_, _, _, _| {
            if let Some(me) = me.upgrade() {
                me.side_tabs.set_active_name(Some("splits"));
                me.queue_button.set_active(true);
            }
        });
        self.split_card.add_controller(open);
        self.split_card.set_cursor_from_name(Some("pointer"));
        self.split_card.set_tooltip_text(Some("The split queue"));
        // the song's title, artists and quality on the art itself, over a soft shade
        let art = gtk::Overlay::builder().child(&self.cover).css_classes(["art-card"]).overflow(gtk::Overflow::Hidden).build();
        let caption = gtk::Box::builder().orientation(gtk::Orientation::Vertical).valign(gtk::Align::End).css_classes(["art-caption"]).can_target(false).build();
        caption.append(&self.np_text);
        art.add_overlay(&caption);
        let art_buttons = gtk::Box::builder().spacing(8).halign(gtk::Align::End).valign(gtk::Align::Start).margin_end(10).margin_top(10).build();
        self.video_button.set_margin_end(0);
        self.video_button.set_margin_top(0);
        art_buttons.append(&self.video_button);
        art_buttons.append(&self.full_button);
        art.add_overlay(&art_buttons);
        let me = Rc::downgrade(self);
        self.full_button.connect_clicked(move |_| {
            if let Some(me) = me.upgrade() {
                me.set_full(!me.full.get());
            }
        });
        // a double-click on the art: fullscreen (again: back)
        let twice = gtk::GestureClick::new();
        let me = Rc::downgrade(self);
        twice.connect_pressed(move |_, n, _, _| {
            if let (2, Some(me)) = (n, me.upgrade()) {
                me.set_full(!me.full.get());
            }
        });
        art.add_controller(twice);
        self.cover_frame.set_child(Some(&art));
        self.cover_frame.set_halign(gtk::Align::Center);
        self.cover.set_size_request(320, 320);
        left.append(&self.cover_frame);
        self.np_text.append(&self.np_title);
        self.np_text.append(&self.np_byline);
        self.np_text.append(&self.np_quality);

        for v in [Voice::Song, Voice::NoVocals, Voice::VocalsOnly] {
            let toggle = adw::Toggle::builder().name(v.label()).label(v.label()).build();
            self.voice_group.add(toggle);
        }
        self.voice_group.set_active_name(Some(Voice::Song.label()));
        // the song's official video: a small button on the art (again: back to the song)
        let me = Rc::downgrade(self);
        self.video_button.connect_clicked(move |_| {
            if let Some(me) = me.upgrade() {
                let on_video = me.current.borrow().as_ref().is_some_and(|t| t.video);
                me.switch_version(!on_video);
            }
        });

        // six parts or four: ticked song by song (the split again, with Piano and Guitar apart)
        let six = gtk::CheckButton::builder().label("6 parts: Piano and Guitar too").tooltip_text("Split this song again into six parts: the beat's piano and guitar get sliders of their own. Only this song; untick to go back to four.").css_classes(["caption"]).build();
        let six_row = gtk::Box::builder().margin_bottom(2).build();
        six_row.append(&six);
        six_row.set_widget_name("six");
        self.mixer.append(&six_row);
        let me = Rc::downgrade(self);
        six.connect_toggled(move |b| {
            if let Some(me) = me.upgrade() {
                if !me.mixer_lock.get() {
                    me.six_toggled(b.is_active());
                }
            }
        });
        six_row.set_visible(split::SIX_PARTS_ON); // (put away for a later release)
        *self.six_check.borrow_mut() = Some(six);
        for part in ["drums", "bass", "other", "piano", "guitar", "music", "vocals"] {
            let row = gtk::Box::builder().spacing(8).build();
            let name = match part {
                "drums" => "Drums",
                "bass" => "Bass",
                "other" => "Other",
                "piano" => "Piano",
                "guitar" => "Guitar",
                "music" => "Music",
                _ => "Vocals",
            };
            row.append(&gtk::Label::builder().label(name).xalign(0.0).width_chars(7).css_classes(["caption"]).build());
            // how much of it this song has, right now (a part can be nearly empty in a song)
            let meter = gtk::LevelBar::builder().min_value(0.0).max_value(1.0).value(0.0).valign(gtk::Align::Center).width_request(28).css_classes(["part-meter"]).build();
            meter.remove_offset_value(Some(gtk::LEVEL_BAR_OFFSET_LOW));
            meter.remove_offset_value(Some(gtk::LEVEL_BAR_OFFSET_HIGH));
            meter.remove_offset_value(Some(gtk::LEVEL_BAR_OFFSET_FULL));
            row.append(&meter);
            self.part_meters.borrow_mut().push((part.to_string(), meter));
            let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.05);
            scale.set_value(1.0);
            scale.set_draw_value(false);
            scale.set_hexpand(true);
            scale.set_size_request(80, -1); // fits the sidebar; it grows with it
            scale.set_tooltip_text(Some(&format!("{name}: how loud (never louder than the song itself)")));
            let me = Rc::downgrade(self);
            let p = part.to_string();
            scale.connect_value_changed(move |sc| {
                if let Some(me) = me.upgrade() {
                    if !me.mixer_lock.get() {
                        me.part_gains.borrow_mut().insert(p.clone(), sc.value());
                        me.apply_mix();
                    }
                }
            });
            row.append(&scale);
            row.set_widget_name(part);
            self.mixer.append(&row);
            self.part_scales.borrow_mut().push((part.to_string(), scale));
        }
        // the Speaker EQ's effects for the device playing (every song; off at the left)
        for (i, name) in ["Low cut", "High cut", "Reverb"].into_iter().enumerate() {
            let row = gtk::Box::builder().spacing(10).margin_top(if i == 0 { 6 } else { 0 }).build();
            row.append(&gtk::Label::builder().label(name).xalign(0.0).width_chars(8).css_classes(["caption"]).build());
            let scale = &self.fx[i];
            scale.set_value(0.0);
            scale.set_draw_value(false);
            scale.set_hexpand(true);
            scale.set_size_request(80, -1);
            let me = Rc::downgrade(self);
            scale.connect_value_changed(move |_| {
                if let Some(me) = me.upgrade() {
                    if !me.mixer_lock.get() {
                        me.fx_changed();
                    }
                }
            });
            row.append(scale);
            row.set_widget_name("fx");
            self.mixer.append(&row);
        }
        self.fx_tooltips();
        self.voice_group.set_tooltip_text(Some("No vocals: the music without the singer (karaoke). Vocals only: just the voice. Split once per song by Demucs; it carries on to the next songs."));
        // the mixer: closed until opened; Reset puts every part back at full
        let mixer_row = self.mixer_row.clone();
        let reset = gtk::Button::builder().icon_name("view-refresh-symbolic").tooltip_text("Reset: every part at full, the effects off (the song as it is)").valign(gtk::Align::Center).css_classes(["flat", "circular"]).build();
        let me = Rc::downgrade(self);
        reset.connect_clicked(move |_| {
            if let Some(me) = me.upgrade() {
                me.part_gains.borrow_mut().clear();
                me.mixer_lock.set(true);
                me.fx.iter().for_each(|s| s.set_value(0.0));
                me.mixer_lock.set(false);
                me.fx_changed();
                me.show_mixer_values();
                me.apply_mix();
            }
        });
        mixer_row.add_suffix(&reset);
        let me = Rc::downgrade(self);
        glib::timeout_add_local(Duration::from_millis(250), move || {
            let Some(me) = me.upgrade() else { return glib::ControlFlow::Break };
            if me.mixer_row.is_expanded() && me.mixer.is_mapped() {
                me.update_meters();
            }
            glib::ControlFlow::Continue
        });
        let me = Rc::downgrade(self);
        mixer_row.connect_expanded_notify(move |r| {
            if let (true, Some(me)) = (r.is_expanded(), me.upgrade()) {
                me.refresh_effects();
                // a server's song: its parts on their way here, ready for the first slider
                let t = me.current.borrow().clone();
                if let Some(t) = t {
                    me.server_parts_ready(&t);
                }
            }
        });
        self.mixer.set_visible(true);
        self.mixer.set_margin_start(12);
        self.mixer.set_margin_end(12);
        self.mixer.set_margin_bottom(8);
        mixer_row.add_row(&self.mixer);
        self.mixer_list.append(&mixer_row);
        // shown at the bottom left of the sidebar, over the split card (see shell)
        left.append(&self.split_button);
        let me = Rc::downgrade(self);
        self.split_button.connect_clicked(move |_| {
            if let Some(me) = me.upgrade() {
                let t = me.current.borrow().clone();
                if let Some(t) = t {
                    me.split_now(&t);
                }
            }
        });
        self.update_voice_status();
        left.set_halign(gtk::Align::Center);
        let art_half = adw::Clamp::builder().maximum_size(500).tightening_threshold(500).hexpand(true).child(&left).build();
        np.append(&art_half);
        self.lyrics_list.set_grow(LYRIC_GROW);
        let lyrics_clamp = adw::Clamp::builder().maximum_size(640).child(&self.lyrics_list).build();
        self.lyrics_scroll.set_child(Some(&lyrics_clamp));
        np.append(&self.lyrics_scroll);
        let scrim = gtk::Box::builder().css_classes(["np-scrim"]).can_target(false).build();
        // The backdrop is drawn under everything but never sizes anything (its
        // pictures are scaled past the page edges): the page takes its size from
        // the content alone. (As the overlay's main child it grew the window.)
        let page = gtk::Overlay::builder().css_classes(["np-page"]).overflow(gtk::Overflow::Hidden).build();
        page.set_child(Some(&gtk::Box::new(gtk::Orientation::Vertical, 0)));
        page.add_overlay(&self.backdrop);
        page.set_clip_overlay(&self.backdrop, true);
        page.add_overlay(&scrim);
        // a short window: the page scrolls (the lyrics keep their own scrolling inside)
        let np_scroll = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).child(&np).build();
        page.add_overlay(&np_scroll);
        page.set_measure_overlay(&np_scroll, true);
        self.stack.add_titled_with_icon(&page, Some("playing"), "Now playing", "audio-x-generic-symbolic");
        // fullscreen, a video: the same picture, filling the screen
        let full_page = gtk::Box::builder().css_classes(["full-video"]).build();
        full_page.append(&self.full_picture);
        let twice = gtk::GestureClick::new();
        let me = Rc::downgrade(self);
        twice.connect_pressed(move |_, n, _, _| {
            if let (2, Some(me)) = (n, me.upgrade()) {
                me.set_full(false);
            }
        });
        full_page.add_controller(twice);
        self.stack.add_named(&full_page, Some("fullvideo"));
        // a big screen: the cover may grow to 540 when its half has the room (it
        // never makes the page wider: with the side panel open on a laptop it stays smaller)
        let big = adw::Breakpoint::new(adw::BreakpointCondition::parse("min-width: 1400sp").unwrap());
        big.add_setter(&self.cover_frame, "maximum-size", Some(&540i32.to_value()));
        big.add_setter(&art_half, "maximum-size", Some(&540i32.to_value()));
        self.window.add_breakpoint(big);
        // the narrower windows' breakpoints (home.rs) stack art above the lyrics, and more
        *self.np_parts.borrow_mut() = Some((np.clone(), left.clone(), gtk::Box::new(gtk::Orientation::Vertical, 0)));


        // the queue, in a side panel
        // (it sits under the window's own header: no second title bar)
        let queue_page = adw::PreferencesPage::new();
        queue_page.add(&self.recent_group);
        queue_page.add(&self.queue_group);
        Self::drag_scrolls(&queue_page);
        self.side_tabs.add(adw::Toggle::builder().name("queue").label("Queue").build());
        self.side_tabs.add(adw::Toggle::builder().name("lyrics").label("Lyrics").build());
        self.side_tabs.add(adw::Toggle::builder().name("splits").label("Splits").build());
        self.side_tabs.set_active_name(Some("queue"));
        self.side_stack.add_named(&queue_page, Some("queue"));
        self.side_lyrics_scroll.set_child(Some(&self.side_lyrics_list));
        self.side_stack.add_named(&self.side_lyrics_scroll, Some("lyrics"));
        let splits = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(8).build();
        let pause = gtk::Button::builder().css_classes(["flat"]).halign(gtk::Align::Start).margin_start(12).build();
        pause.set_action_name(Some("win.split-pause"));
        let label = gtk::Label::new(Some("Pause or resume splitting"));
        pause.set_child(Some(&label));
        splits.append(&pause);
        splits.append(&self.split_list);
        self.side_stack.add_named(&gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&splits).build(), Some("splits"));
        let stack = self.side_stack.downgrade();
        self.side_tabs.connect_active_name_notify(move |g| {
            if let (Some(stack), Some(name)) = (stack.upgrade(), g.active_name()) {
                stack.set_visible_child_name(&name);
            }
        });
        let side = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
        side.append(&self.side_tabs);
        side.append(&self.side_stack);
        self.split.set_sidebar(Some(&side));
        self.seek_on_click(&self.side_lyrics_list);
        self.connect_split_queue_button();
        // the bar's lyrics button: the side panel, on its Lyrics tab (again: closes it)
        let me = Rc::downgrade(self);
        self.lyrics_button.connect_clicked(move |_| {
            if let Some(me) = me.upgrade() {
                let open_on_lyrics = me.queue_button.is_active() && me.side_tabs.active_name().as_deref() == Some("lyrics");
                if open_on_lyrics {
                    me.queue_button.set_active(false);
                } else {
                    me.side_tabs.set_active_name(Some("lyrics"));
                    me.queue_button.set_active(true);
                }
            }
        });
        self.toasts.set_child(Some(&self.stack));
        self.split.set_content(Some(&self.toasts));
        self.toasts.add_css_class("page-area");
        self.toasts.set_overflow(gtk::Overflow::Hidden);
        self.queue_button.bind_property("active", &self.split, "show-sidebar").bidirectional().sync_create().build();

        // the bar
        let bar = gtk::CenterBox::builder().css_classes(["playbar"]).build();
        self.bar_song.append(&self.bar_art);
        let names = gtk::Box::builder().orientation(gtk::Orientation::Vertical).valign(gtk::Align::Center).build();
        names.append(&self.bar_title);
        names.append(&self.bar_byline);
        self.bar_title.set_label("Nothing playing");
        self.bar_title.set_max_width_chars(28);
        self.bar_byline.set_max_width_chars(32);
        self.bar_song.append(&names);
        bar.set_start_widget(Some(&self.bar_song));
        let middle = gtk::Box::builder().orientation(gtk::Orientation::Vertical).width_request(300).build();
        if let Some(parts) = self.np_parts.borrow_mut().as_mut() {
            parts.2 = middle.clone();
        }
        let controls = gtk::Box::builder().spacing(6).halign(gtk::Align::Center).build();
        self.play.remove_css_class("flat");
        self.play.add_css_class("suggested-action");
        controls.append(&self.shuffle);
        controls.append(&self.prev);
        controls.append(&self.play);
        controls.append(&self.next);
        controls.append(&self.repeat_button);
        self.repeat_button.set_opacity(0.55);
        middle.append(&controls);
        let timeline = gtk::Box::builder().spacing(6).build();
        self.pos.set_label("0:00");
        self.len.set_label("0:00");
        timeline.append(&self.pos);
        timeline.append(&self.seek);
        timeline.append(&self.len);
        middle.append(&timeline);
        bar.set_center_widget(Some(&middle));
        let end = gtk::Box::builder().spacing(6).build();
        let level = self.settings.borrow().volume;
        self.volume.set_value(level);
        self.volume.set_draw_value(false);
        self.volume.set_size_request(110, -1);
        self.volume.set_valign(gtk::Align::Center);
        self.volume.set_tooltip_text(Some("Volume (Radiance Music only; 100% = your system volume, never louder)"));
        if let Some(p) = self.player.borrow().as_ref() {
            p.set_volume(level);
        }
        end.append(&self.mute);
        end.append(&self.volume);
        // in a slot of its own: the window sizes hide the slot (the song's quality shows or hides the pill
        // inside it, and used to override them: a phone's bar grew wider than the phone)
        let quality_slot = gtk::Box::builder().valign(gtk::Align::Center).build();
        quality_slot.append(&self.bar_quality);
        end.append(&quality_slot);
        end.append(&self.bar_heart);
        end.append(&self.lyrics_button);
        end.append(&self.queue_button);
        end.append(&self.window_buttons);
        bar.set_end_widget(Some(&end));
        self.attach_menu(self.bar_song.upcast_ref(), Place::Current);
        let open = gtk::GestureClick::builder().button(gdk::BUTTON_PRIMARY).build();
        let me = Rc::downgrade(self);
        open.connect_released(move |_, _, _, _| {
            if let Some(me) = me.upgrade() {
                if me.current.borrow().is_some() {
                    me.stack.set_visible_child_name("playing");
                }
            }
        });
        self.bar_song.add_controller(open);
        self.bar_song.set_cursor_from_name(Some("pointer"));
        self.bar_song.set_tooltip_text(Some("Open Now Playing"));

        // the menu (between songs, the music folder, About)
        let menu = gio::Menu::new();
        let between = gio::Menu::new();
        between.append(Some("No Transition"), Some("win.transition::off"));
        between.append(Some("Crossfade"), Some("win.transition::crossfade"));
        between.append(Some("DJ Mix (by the Beat)"), Some("win.transition::dj"));
        let slider = gio::MenuItem::new(None, None);
        slider.set_attribute_value("custom", Some(&"fade".to_variant()));
        between.append_item(&slider);
        between.append(Some("Autoplay When the Queue Ends"), Some("win.autoplay"));
        menu.append_section(Some("Between Songs"), &between);
        let looks = gio::Menu::new();
        looks.append(Some("Stone"), Some("win.look::stone"));
        looks.append(Some("True Black"), Some("win.look::black"));
        menu.append_section(Some("Look"), &looks);
        let splitting = gio::Menu::new();
        splitting.append(Some("Splitting…"), Some("win.splitting"));
        splitting.append(Some("Pause Splitting"), Some("win.split-pause"));
        menu.append_section(None, &splitting);
        let app = gio::Menu::new();
        app.append(Some("Share Songs with a Phone…"), Some("win.share-songs"));
        app.append(Some("Servers…"), Some("win.servers"));
        app.append(Some("Storage and Folder"), Some("win.storage"));
        app.append(Some("Open the Music Folder"), Some("win.music-folder"));
        app.append(Some("How to Use It"), Some("win.guide"));
        app.append(Some("Look for Updates"), Some("win.updates"));
        app.append(Some("About Radiance Music"), Some("win.about"));
        menu.append_section(None, &app);
        let leave = gio::Menu::new();
        leave.append(Some("Quit (stops the music)"), Some("win.quit"));
        menu.append_section(None, &leave);
        let menu_button = gtk::MenuButton::builder().icon_name("open-menu-symbolic").menu_model(&menu).tooltip_text("Main menu").primary(true).build();
        if let Some(pop) = menu_button.popover().and_downcast::<gtk::PopoverMenu>() {
            pop.add_child(&self.fade_row(), "fade");
        }
        self.shell(&bar, &menu_button);
        self.set_controls();
    }

    fn connect(self: &Rc<Self>) {
        // the internet: GIO says when it comes and goes (Linux and Windows alike); an empty
        // search or a song that won't load also asks for itself (net::reachable)
        let monitor = gio::NetworkMonitor::default();
        self.set_online(monitor.is_network_available(), false);
        let me = Rc::downgrade(self);
        monitor.connect_network_changed(move |_, available| {
            if let Some(me) = me.upgrade() {
                me.set_online(available, true);
            }
        });
        let me = Rc::downgrade(self);
        self.banner.connect_button_clicked(move |_| {
            if let Some(me) = me.upgrade() {
                let help = me.banner_help.borrow().clone();
                gtk::UriLauncher::new(&help).launch(Some(&me.window), gio::Cancellable::NONE, |_| {});
            }
        });
        let me = Rc::downgrade(self);
        self.search.connect_search_changed(move |_| {
            if let Some(me) = me.upgrade() {
                me.search_changed();
            }
        });
        let me = Rc::downgrade(self);
        self.play.connect_clicked(move |_| {
            if let Some(me) = me.upgrade() {
                me.toggle_play();
            }
        });
        let me = Rc::downgrade(self);
        self.next.connect_clicked(move |_| {
            if let Some(me) = me.upgrade() {
                me.skip(1);
            }
        });
        let me = Rc::downgrade(self);
        self.prev.connect_clicked(move |_| {
            if let Some(me) = me.upgrade() {
                me.skip(-1);
            }
        });
        let me = Rc::downgrade(self);
        self.volume.connect_value_changed(move |s| {
            if let Some(me) = me.upgrade() {
                me.mute.set_active(false);
                me.apply_volume();
                me.settings.borrow_mut().volume = s.value();
                settings::save(&me.settings.borrow());
            }
        });
        let me = Rc::downgrade(self);
        self.mute.connect_toggled(move |_| {
            if let Some(me) = me.upgrade() {
                me.apply_volume();
            }
        });
        let me = Rc::downgrade(self);
        self.shuffle.connect_toggled(move |_| {
            if let Some(me) = me.upgrade() {
                me.shuffle_changed();
            }
        });
        let me = Rc::downgrade(self);
        self.repeat_button.connect_clicked(move |_| {
            if let Some(me) = me.upgrade() {
                me.next_repeat();
            }
        });
        let me = Rc::downgrade(self);
        self.bar_heart.connect_clicked(move |_| {
            if let Some(me) = me.upgrade() {
                let t = me.current.borrow().clone();
                if let Some(t) = t {
                    me.toggle_like(&t);
                }
            }
        });
        let me = Rc::downgrade(self);
        self.seek.connect_change_value(move |_, _, v| {
            if let Some(me) = me.upgrade() {
                if me.in_party() {
                    me.party_seek(v); // for everyone, when you control the party
                    return glib::Propagation::Stop;
                }
                me.cued.replace(None);
                if let Some(p) = me.player.borrow().as_ref() {
                    p.seek(v);
                }
            }
            glib::Propagation::Proceed
        });
        self.seek_on_click(&self.lyrics_list);
        let me = Rc::downgrade(self);
        self.voice_group.connect_active_name_notify(move |g| {
            if let Some(me) = me.upgrade() {
                let v = match g.active_name().as_deref() {
                    Some("No vocals") => Voice::NoVocals,
                    Some("Vocals only") => Voice::VocalsOnly,
                    _ => Voice::Song,
                };
                if v != me.voice.get() {
                    me.voice.set(v);
                    me.voice_changed();
                }
            }
        });
        // the backdrop drifts only while on screen
        let me = Rc::downgrade(self);
        self.stack.connect_visible_child_name_notify(move |_| {
            if let Some(me) = me.upgrade() {
                me.update_drift();
            }
        });
        // the device playing and its effects, read again now and then (the Mixer in step)
        let me = Rc::downgrade(self);
        glib::timeout_add_seconds_local(5, move || match me.upgrade() {
            Some(me) => {
                if me.window.is_visible() {
                    me.refresh_effects();
                }
                glib::ControlFlow::Continue
            }
            None => glib::ControlFlow::Break,
        });
        self.refresh_effects();
        // window actions
        let quit = gio::SimpleAction::new("quit", None);
        let me = Rc::downgrade(self);
        quit.connect_activate(move |_, _| {
            if let Some(me) = me.upgrade() {
                me.quit_app();
            }
        });
        self.window.add_action(&quit);
        if let Some(app) = self.window.application() {
            app.set_accels_for_action("win.quit", &["<Control>q"]);
        }
        let guide = gio::SimpleAction::new("guide", None);
        let me = Rc::downgrade(self);
        guide.connect_activate(move |_, _| {
            if let Some(me) = me.upgrade() {
                me.show_guide();
            }
        });
        self.window.add_action(&guide);
        let about = gio::SimpleAction::new("about", None);
        let me = Rc::downgrade(self);
        about.connect_activate(move |_, _| {
            if let Some(me) = me.upgrade() {
                me.about();
            }
        });
        self.window.add_action(&about);
        let updates = gio::SimpleAction::new("updates", None);
        let me = Rc::downgrade(self);
        updates.connect_activate(move |_, _| {
            if let Some(me) = me.upgrade() {
                me.look_for_updates(true);
            }
        });
        self.window.add_action(&updates);
        let me = Rc::downgrade(self);
        let folder = gio::SimpleAction::new("music-folder", None);
        folder.connect_activate(move |_, _| {
            if let Some(me) = me.upgrade() {
                let dir = library::music_dir();
                let _ = fs::create_dir_all(&dir);
                gtk::FileLauncher::new(Some(&gio::File::for_path(&dir))).launch(Some(&me.window), gio::Cancellable::NONE, |_| {});
            }
        });
        self.window.add_action(&folder);
        let me = Rc::downgrade(self);
        let share_songs = gio::SimpleAction::new("share-songs", None);
        let me2 = Rc::downgrade(self);
        share_songs.connect_activate(move |_, _| {
            if let Some(me) = me2.upgrade() {
                me.sharing_dialog();
            }
        });
        self.window.add_action(&share_songs);
        let servers_action = gio::SimpleAction::new("servers", None);
        let me2 = Rc::downgrade(self);
        servers_action.connect_activate(move |_, _| {
            if let Some(me) = me2.upgrade() {
                me.servers_dialog();
            }
        });
        self.window.add_action(&servers_action);
        let splitting = gio::SimpleAction::new("splitting", None);
        splitting.connect_activate(move |_, _| {
            if let Some(me) = me.upgrade() {
                me.splitting_dialog();
            }
        });
        self.window.add_action(&splitting);
        let me = Rc::downgrade(self);
        let storage = gio::SimpleAction::new("storage", None);
        storage.connect_activate(move |_, _| {
            if let Some(me) = me.upgrade() {
                me.storage_dialog();
            }
        });
        self.window.add_action(&storage);
        let me = Rc::downgrade(self);
        let mode = self.settings.borrow().transition;
        let transition = gio::SimpleAction::new_stateful("transition", Some(glib::VariantTy::STRING), &mode.id().to_variant());
        transition.connect_activate(move |a, v| {
            let (Some(me), Some(id)) = (me.upgrade(), v.and_then(|v| v.str().map(String::from))) else { return };
            a.set_state(&id.to_variant());
            me.settings.borrow_mut().transition = Transition::from_id(&id);
            settings::save(&me.settings.borrow());
            me.replan();
            me.toast(match Transition::from_id(&id) {
                Transition::Off => "Songs play one after another",
                Transition::Crossfade => "Songs crossfade",
                Transition::DjMix => "DJ mix: beats lined up between songs (a crossfade when the tempos are too far apart)",
            });
        });
        self.window.add_action(&transition);
        let me = Rc::downgrade(self);
        let look = self.settings.borrow().look.clone();
        let look_action = gio::SimpleAction::new_stateful("look", Some(glib::VariantTy::STRING), &look.to_variant());
        look_action.connect_activate(move |a, v| {
            let (Some(me), Some(id)) = (me.upgrade(), v.and_then(|v| v.str().map(String::from))) else { return };
            a.set_state(&id.to_variant());
            me.settings.borrow_mut().look = id.clone();
            settings::save(&me.settings.borrow());
            apply_look(&id);
        });
        self.window.add_action(&look_action);
        let me = Rc::downgrade(self);
        let autoplay = gio::SimpleAction::new_stateful("autoplay", None, &self.settings.borrow().autoplay.to_variant());
        autoplay.connect_activate(move |a, _| {
            let Some(me) = me.upgrade() else { return };
            let on = !me.settings.borrow().autoplay;
            a.set_state(&on.to_variant());
            me.settings.borrow_mut().autoplay = on;
            settings::save(&me.settings.borrow());
            me.toast(if on { "Autoplay on: when the queue ends, songs like the last one play on" } else { "Autoplay off: the music stops when the queue ends" });
            *me.autoplay_for.borrow_mut() = None;
            me.render_queue();
            me.autoplay_if_last();
        });
        self.window.add_action(&autoplay);
        let me = Rc::downgrade(self);
        let pause = gio::SimpleAction::new_stateful("split-pause", None, &Self::split_paused().to_variant());
        pause.connect_activate(move |_, _| {
            if let Some(me) = me.upgrade() {
                me.set_split_paused(!Self::split_paused());
            }
        });
        self.window.add_action(&pause);
        // F11: fullscreen; Esc: out of it
        let keys = gtk::ShortcutController::new();
        let me = Rc::downgrade(self);
        keys.add_shortcut(gtk::Shortcut::new(
            gtk::ShortcutTrigger::parse_string("F11"),
            Some(gtk::CallbackAction::new(move |_, _| {
                if let Some(me) = me.upgrade() {
                    me.set_full(!me.full.get());
                }
                glib::Propagation::Stop
            })),
        ));
        let me = Rc::downgrade(self);
        keys.add_shortcut(gtk::Shortcut::new(
            gtk::ShortcutTrigger::parse_string("Escape"),
            Some(gtk::CallbackAction::new(move |_, _| match me.upgrade() {
                Some(me) if me.full.get() => {
                    me.set_full(false);
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            })),
        ));
        self.window.add_controller(keys);
        let moved = gtk::EventControllerMotion::new();
        let me = Rc::downgrade(self);
        moved.connect_motion(move |_, x, y| {
            if let Some(me) = me.upgrade() {
                let (px, py) = me.pointer.replace((x, y));
                if me.full.get() && ((x - px).abs() > 2.0 || (y - py).abs() > 2.0) {
                    me.nudge_bar();
                }
            }
        });
        self.window.add_controller(moved);
        let me = Rc::downgrade(self);
        self.window.connect_fullscreened_notify(move |w| {
            if let Some(me) = me.upgrade() {
                if !w.is_fullscreen() && me.full.get() {
                    me.set_full(false); // left by the desktop (a shortcut, a gesture)
                }
            }
        });
        // Ctrl+F: search
        let me = Rc::downgrade(self);
        let find = gtk::ShortcutController::new();
        find.add_shortcut(gtk::Shortcut::new(
            gtk::ShortcutTrigger::parse_string("<Control>f"),
            Some(gtk::CallbackAction::new(move |_, _| {
                if let Some(me) = me.upgrade() {
                    me.stack.set_visible_child_name("search");
                    me.search.grab_focus();
                }
                glib::Propagation::Stop
            })),
        ));
        self.window.add_controller(find);

        let me = Rc::downgrade(self);
        glib::timeout_add_local(Duration::from_millis(200), move || match me.upgrade() {
            Some(me) => {
                me.tick();
                glib::ControlFlow::Continue
            }
            None => glib::ControlFlow::Break,
        });
        // the words being sung, lit as they're sung (only while lyrics are on screen)
        let me = Rc::downgrade(self);
        glib::timeout_add_local(Duration::from_millis(33), move || match me.upgrade() {
            Some(me) => {
                me.sing();
                glib::ControlFlow::Continue
            }
            None => glib::ControlFlow::Break,
        });
        for (scroll, list) in [(&self.lyrics_scroll, &self.lyrics_list), (&self.side_lyrics_scroll, &self.side_lyrics_list)] {
            list.follow(&scroll.vadjustment()); // each line's size follows where it is
            // room above the first line and below the last, so every line can sit in the middle
            let list = list.downgrade();
            scroll.vadjustment().connect_page_size_notify(move |adj| {
                let room = (adj.page_size() / 2.0 - 40.0).max(0.0) as i32;
                let list = list.clone();
                glib::idle_add_local_once(move || {
                    if let Some(list) = list.upgrade() {
                        if (list.margin_top() - room).abs() > 1 {
                            list.set_margin_top(room);
                            list.set_margin_bottom(room);
                        }
                    }
                });
            });
            // back on screen: the line being sung goes to the middle again
            let me = Rc::downgrade(self);
            scroll.connect_map(move |_| {
                if let Some(me) = me.upgrade() {
                    me.lyric_lead.set(None);
                }
            });
        }
        self.search.grab_focus();
    }

    /// The crossfade length slider in the main menu.
    fn fade_row(self: &Rc<Self>) -> gtk::Box {
        let row = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(2).margin_start(12).margin_end(12).margin_bottom(6).build();
        let fade = self.settings.borrow().fade;
        let show = |l: &gtk::Label, v: f64| l.set_label(&format!("Crossfade length: {v:.0} s"));
        show(&self.fade_label, fade);
        self.fade_label.add_css_class("caption");
        let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, FADE_RANGE.0, FADE_RANGE.1, 1.0);
        scale.set_value(fade);
        scale.set_draw_value(false);
        scale.set_size_request(220, -1);
        let me = Rc::downgrade(self);
        scale.connect_value_changed(move |s| {
            if let Some(me) = me.upgrade() {
                let v = s.value().round();
                show(&me.fade_label, v);
                me.settings.borrow_mut().fade = v;
                settings::save(&me.settings.borrow());
                me.replan();
            }
        });
        row.append(&self.fade_label);
        row.append(&scale);
        row
    }

    // ------------------------------------------------------------------ between songs

    /// The song after this one, if one plays next (repeat-one replays instead).
    fn next_track(&self) -> Option<Track> {
        if self.repeat.get() == Repeat::One {
            return None;
        }
        let queue = self.queue.borrow();
        let i = self.index.get() + 1;
        queue.get(i).cloned().or_else(|| (self.repeat.get() == Repeat::All && queue.len() > 1).then(|| queue[0].clone()))
    }

    /// The saved copy of a liked song, else the song itself.
    fn saved_copy(&self, t: &Track) -> Track {
        self.liked
            .borrow()
            .iter()
            .find(|x| x.key() == t.key() && x.file.as_ref().is_some_and(|f| Path::new(f).exists()))
            .cloned()
            .unwrap_or_else(|| t.clone())
    }

    /// Where a song plays from now: its split half for the voice picked, else itself.
    fn uri_now(&self, t: &Track) -> Option<String> {
        self.half_for(t).map(|h| h.1).or_else(|| self.uri_plain(t))
    }

    /// Where the song itself can be played from (a YouTube stream only once found).
    fn uri_plain(&self, t: &Track) -> Option<String> {
        if let Some(f) = &t.file {
            return Some(gio::File::for_path(f).uri().to_string());
        }
        match t.source {
            Source::Archive => Some(self.web_uri(&t.id)),
            Source::YouTube => self.fresh_stream(&t.id).map(|s| s.0),
            // with this computer's key (none: the server was left)
            Source::Server => servers::of_track(t).map(|_| self.web_uri(&servers::signed(&t.id))),
        }
    }

    /// An Internet Archive file's (or a Radiance Music Server's) address for the player: as
    /// it is, except on Windows, where it goes through the relay (GStreamer's TLS there can't
    /// check the Archive).
    fn web_uri(&self, url: &str) -> String {
        match &self.relay {
            Some(relay) if cfg!(windows) => relay.add_web(url),
            _ => url.to_string(),
        }
    }

    /// Settings or the order changed: plan the next transition again.
    fn replan(&self) {
        self.plan.replace(None);
        self.analysing.replace(None);
        if let Some(id) = self.mix_timer.borrow_mut().take() {
            id.remove();
        }
        if self.cued.borrow_mut().take().is_some() {
            if let Some(p) = self.player.borrow().as_ref() {
                p.drop_cue(); // it was for another song, or another kind of mix
            }
        }
    }

    /// Called five times a second while playing: plan, cue ahead, and start the
    /// mix right on time.
    fn transitions(self: &Rc<Self>, at: f64) {
        crate::watch::doing("planning the mix");
        let mode = self.settings.borrow().transition;
        let Some(player_len) = self.player.borrow().as_ref().and_then(|p| p.duration()) else { return };
        if mode == Transition::Off || player_len < 40.0 || self.player.borrow().as_ref().is_some_and(|p| p.mixing()) {
            return;
        }
        let (Some(cur), Some(next)) = (self.current.borrow().clone(), self.next_track()) else { return };
        if cur.video || next.video {
            return; // a video just ends; the next one starts
        }
        let next = self.saved_copy(&next);
        let fade = self.settings.borrow().fade;
        let keys = (cur.key(), next.key());
        // DJ mix: analyse both songs once (needs both addresses)
        if mode == Transition::DjMix {
            let planned = self.plan.borrow().as_ref().is_some_and(|(a, b, _)| (a, b) == (&keys.0, &keys.1));
            let busy = self.analysing.borrow().as_ref() == Some(&keys);
            if !planned && !busy {
                if let Some(b_uri) = self.uri_now(&next) {
                    let a_uri = self.current_uri.borrow().clone();
                    self.analysing.replace(Some(keys.clone()));
                    let me = Rc::downgrade(self);
                    let k = keys.clone();
                    bg(
                        move || {
                            let a = beat::analyse(&a_uri, (player_len - 45.0).max(0.0), 45.0);
                            let b = beat::analyse(&b_uri, 0.0, 40.0);
                            if std::env::var_os("RADIANCE_DEBUG").is_some() {
                                eprintln!("[mix] this: {a:?}\n[mix] next: {b:?}");
                            }
                            beat::plan(a, b, player_len, fade)
                        },
                        move |plan| {
                            let Some(me) = me.upgrade() else { return };
                            if me.analysing.borrow().as_ref() == Some(&k) {
                                if std::env::var_os("RADIANCE_DEBUG").is_some() {
                                    eprintln!("[mix] plan: {plan:?}");
                                }
                                me.analysing.replace(None);
                                me.plan.replace(Some((k.0, k.1, plan)));
                            }
                        },
                    );
                } else if next.source == Source::YouTube {
                    self.prefetch_next();
                }
            }
        }
        let plan = match mode {
            Transition::DjMix => self
                .plan
                .borrow()
                .as_ref()
                .filter(|(a, b, _)| (a, b) == (&keys.0, &keys.1))
                .map(|x| x.2)
                .unwrap_or_else(|| beat::plan(None, None, player_len, fade)),
            _ => beat::plan(None, None, player_len, fade),
        };
        if at > plan.at + plan.seconds / 2.0 {
            return; // jumped past it: this song just ends
        }
        // cue the next song 12 s ahead
        if plan.at - at <= 12.0 && self.cued.borrow().as_deref() != Some(keys.1.as_str()) {
            // split and saved: it comes in with its parts at hand (not in a beat-matched DJ mix: that's the decks')
            let (parts, whole, rate) = self.parts_of(&next);
            let with_parts = !parts.is_empty() && !plan.beat_matched && self.voice.get() == Voice::Song && whole.is_some();
            match (self.uri_now(&next), with_parts) {
                (Some(_), true) => {
                    if let (Some(p), Some(w)) = (self.player.borrow().as_ref(), whole.as_deref()) {
                        p.cue_parts(w, &parts, plan.from, &self.gains_for(&next), rate);
                    }
                    self.cued.replace(Some(keys.1.clone()));
                }
                (Some(uri), false) => {
                    if let Some(p) = self.player.borrow().as_ref() {
                        p.cue(&uri, plan.from, plan.rate, plan.beat_matched);
                    }
                    self.cued.replace(Some(keys.1.clone()));
                }
                (None, _) => self.prefetch_next(),
            }
        }
        // and start it on the dot
        let ready = self.player.borrow().as_ref().is_some_and(|p| p.cued());
        if ready && plan.at - at <= 0.45 && self.mix_timer.borrow().is_none() {
            let me = Rc::downgrade(self);
            let wait = ((plan.at - at).max(0.0) * 1000.0) as u64;
            let id = glib::timeout_add_local_once(Duration::from_millis(wait), move || {
                if let Some(me) = me.upgrade() {
                    me.mix_timer.borrow_mut().take();
                    me.begin_mix(plan);
                }
            });
            self.mix_timer.replace(Some(id));
        }
    }

    fn begin_mix(self: &Rc<Self>, plan: Plan) {
        let Some(next) = self.next_track() else { return };
        let next = self.saved_copy(&next);
        let Some(uri) = self.uri_now(&next) else { return };
        if let Some(p) = self.player.borrow().as_ref() {
            p.retune_cued(&self.gains_for(&next)); // the sliders as they are now, not as when it was cued
            p.mix(plan.seconds, plan.beat_matched);
        }
        if !self.player.borrow().as_ref().is_some_and(|p| p.mixing()) {
            return;
        }
        let len = self.queue.borrow().len();
        let i = self.index.get() + 1;
        self.index.set(if i < len { i } else { 0 });
        self.cued.replace(None);
        self.plan.replace(None);
        let gen = self.play_gen.get() + 1;
        self.play_gen.set(gen);
        self.kbps.set(self.fresh_stream(&next.id).map(|s| s.1).filter(|_| next.source == Source::YouTube && next.file.is_none()).unwrap_or(0));
        self.quality_known.set(false);
        let parts = self.player.borrow().as_ref().is_some_and(|p| p.is_stems());
        self.on_stems.set(parts);
        self.on_half.set(if parts { None } else { self.half_for(&next).map(|h| h.0) });
        self.current_uri.replace(uri);
        *self.current.borrow_mut() = Some(next.clone());
        self.show_current(&next);
        self.load_lyrics(&next, gen);
        self.note_played(&next);
        self.render_queue();
        self.prefetch_next();
        if self.voice.get() != Voice::Song {
            if let Some(after) = self.next_track() {
                self.request_split(&self.saved_copy(&after)); // ready by the time it plays
            }
        }
        self.update_voice_status();
    }

    // ------------------------------------------------------------------ voice

    /// The half of a song to play for the voice picked, if it's been split.
    fn half_for(&self, t: &Track) -> Option<(Voice, String)> {
        let v = self.voice.get();
        let path = split::ready(t, v)?;
        Some((v, glib::filename_to_uri(&path, None).ok()?.to_string()))
    }

    fn voice_changed(self: &Rc<Self>) {
        let v = self.voice.get();
        let Some(t) = self.current.borrow().clone() else {
            self.update_voice_status();
            return;
        };
        if split::has_parts(&t) || Self::shown_parts(&t).len() >= 4 {
            // with the four parts: the buttons are mixer presets
            let preset = |p: &str| match v {
                Voice::Song => 1.0,
                Voice::NoVocals => if p == "vocals" { 0.0 } else { 1.0 },
                Voice::VocalsOnly => if p == "vocals" { 1.0 } else { 0.0 },
            };
            for p in split::SIX {
                self.part_gains.borrow_mut().insert(p.to_string(), preset(p));
            }
            self.show_mixer_values();
            self.apply_mix();
            self.update_voice_status();
            return;
        }
        if v == Voice::Song || split::ready(&t, v).is_some() {
            self.switch_half(&t);
        } else {
            self.split_now(&t);
        }
        if v != Voice::Song {
            if let Some(next) = self.next_track() {
                self.request_split(&self.saved_copy(&next)); // ready by the time it plays
            }
        }
        self.replan();
        self.update_voice_status();
    }

    /// Each part's meter: how loud it is in the song right now, before its slider
    /// (so a part that's nearly empty in this song shows it).
    fn update_meters(&self) {
        let levels: HashMap<String, f64> = self.player.borrow().as_ref().map(|p| p.part_levels().into_iter().collect()).unwrap_or_default();
        for (name, meter) in self.part_meters.borrow().iter() {
            let db = levels.get(name).map(|p| 10.0 * p.max(1e-12).log10());
            // -60 dB (nothing) to -6 dB (as loud as a part gets)
            meter.set_value(db.map(|d| ((d + 60.0) / 54.0).clamp(0.0, 1.0)).unwrap_or(0.0));
            let label = name[..1].to_uppercase() + &name[1..];
            meter.set_tooltip_text(Some(&match db {
                Some(d) if d < -42.0 => format!("{label}: almost nothing in the song here"),
                Some(d) => format!("{label}: {d:.0} dB in the song here"),
                None => format!("{label}: how loud it is in the song (while its parts play)"),
            }));
        }
    }

    fn show_mixer_values(&self) {
        self.mixer_lock.set(true);
        for (p, sc) in self.part_scales.borrow().iter() {
            sc.set_value(self.part_gains.borrow().get(p).copied().unwrap_or(1.0));
        }
        self.mixer_lock.set(false);
    }

    /// The Mixer's effects in the Speaker EQ's terms: low cut and high cut in Hz
    /// (off at 20 Hz and 20 kHz), reverb in %; each slider off at its left.
    fn fx_values(&self) -> (f64, f64, f64) {
        use crate::effects::{HIGH_CUT, LOW_CUT};
        let v = |i: usize| self.fx[i].value().clamp(0.0, 1.0);
        let low = LOW_CUT.0 * (LOW_CUT.1 / LOW_CUT.0).powf(v(0));
        let high = HIGH_CUT.1 * (HIGH_CUT.0 / HIGH_CUT.1).powf(v(1));
        (low.round(), high.round(), (v(2) * 100.0).round())
    }

    /// The sliders where the Speaker EQ has the effects.
    fn fx_show(&self, s: &crate::effects::Sound) {
        use crate::effects::{HIGH_CUT, LOW_CUT};
        let at = [
            (s.low_cut / LOW_CUT.0).ln() / (LOW_CUT.1 / LOW_CUT.0).ln(),
            (s.high_cut / HIGH_CUT.1).ln() / (HIGH_CUT.0 / HIGH_CUT.1).ln(),
            s.reverb / 100.0,
        ];
        self.mixer_lock.set(true);
        for (sc, v) in self.fx.iter().zip(at) {
            sc.set_value(v.clamp(0.0, 1.0));
        }
        self.mixer_lock.set(false);
        self.fx_tooltips();
    }

    /// Each effect says what it does now.
    fn fx_tooltips(&self) {
        let (low, high, reverb) = self.fx_values();
        let hz = |v: f64| if v >= 1000.0 { format!("{:.1} kHz", v / 1000.0) } else { format!("{v:.0} Hz") };
        let device = self.fx_name.borrow().clone().unwrap_or_else(|| "speaker".into());
        self.fx[0].set_tooltip_text(Some(&if low <= crate::effects::LOW_CUT.0 + 0.5 { "Low cut: off".to_string() } else { format!("Low cut: takes out the bass below {}", hz(low)) }));
        self.fx[1].set_tooltip_text(Some(&if high >= crate::effects::HIGH_CUT.1 - 0.5 { "High cut: off".to_string() } else { format!("High cut: takes out the sounds above {}", hz(high)) }));
        self.fx[2].set_tooltip_text(Some(&if reverb < 0.5 { "Reverb: off".to_string() } else { format!("Reverb: {reverb:.0} %, a room around the music") }));
        for sc in &self.fx {
            let t = sc.tooltip_text().unwrap_or_default();
            sc.set_tooltip_text(Some(&format!("{t}. The Speaker EQ's, for the {device}: everything playing on it.")));
        }
    }

    /// An effect moved: the Speaker EQ set (saved, and heard at once) a moment later.
    fn fx_changed(self: &Rc<Self>) {
        self.fx_touched.set(Some(Instant::now()));
        self.fx_tooltips();
        if self.fx_later.borrow().is_some() {
            return;
        }
        let me = Rc::downgrade(self);
        let id = glib::timeout_add_local_once(Duration::from_millis(80), move || {
            let Some(me) = me.upgrade() else { return };
            me.fx_later.borrow_mut().take();
            let Some(device) = me.fx_device.get() else { return };
            let (low, high, reverb) = me.fx_values();
            bg(move || crate::effects::set_effects(device, low, high, reverb), |_| {});
        });
        *self.fx_later.borrow_mut() = Some(id);
    }

    /// Which device plays now and its effects, read again (worker thread).
    fn refresh_effects(self: &Rc<Self>) {
        let me = Rc::downgrade(self);
        bg(crate::effects::effects, move |found| {
            let Some(me) = me.upgrade() else { return };
            let was = me.fx_device.get();
            me.fx_device.set(found.as_ref().map(|p| p.device));
            me.fx_name.replace(found.as_ref().map(|p| p.name.clone()));
            // not while one is being moved (the saved values are a moment behind)
            let moving = me.fx_later.borrow().is_some() || me.fx_touched.get().is_some_and(|t| t.elapsed() < Duration::from_secs(2));
            if let (Some(p), false) = (found, moving) {
                me.fx_show(&p.sound);
            }
            if was != me.fx_device.get() {
                me.update_voice_status();
            }
        });
    }

    /// The parts, mixed as the sliders say; all at full is the song itself
    /// (its own file, the best quality). A song with its parts at hand (see
    /// player::StemPipe) only changes volumes: nothing loads, nothing jumps.
    /// The parts a song's Mixer shows: six only when it was ticked for six (split in six but
    /// not ticked: four, Other moving Piano and Guitar with it).
    fn shown_parts(t: &Track) -> Vec<&'static str> {
        let mut have = split::mix_parts(t);
        if have.is_empty() && t.file.is_none() {
            have = servers::mix_parts(t); // a server's song: its parts there
        }
        if have.len() == 6 && !split::wants_six(t) { split::PARTS.to_vec() } else { have }
    }

    /// The volumes the player gets for a song's parts, as its sliders say.
    fn gains_for(&self, t: &Track) -> HashMap<String, f64> {
        part_volumes(&self.part_gains.borrow(), split::wants_six(t))
    }

    /// Ticked (or not) for six parts: split again in six if it isn't yet; the sliders to match.
    fn six_toggled(self: &Rc<Self>, on: bool) {
        let Some(t) = self.current.borrow().clone() else { return };
        crate::db::set_six(&t.key(), on);
        crate::log::note(&format!("“{}”: {} parts", t.title, if on { "six" } else { "four" }));
        if on && !split::has_six(&t) {
            self.split_now(&self.saved_copy(&t));
            self.toast(&format!("Splitting “{}” into six parts: Piano and Guitar get sliders when it's done", t.title));
        }
        if on {
            // Piano and Guitar start where Other is (the song sounds the same until moved)
            let other = self.part_gains.borrow().get("other").copied().unwrap_or(1.0);
            self.part_gains.borrow_mut().insert("piano".into(), other);
            self.part_gains.borrow_mut().insert("guitar".into(), other);
        }
        self.show_mixer_values();
        self.update_voice_status();
        self.apply_mix();
    }

    fn apply_mix(self: &Rc<Self>) {
        let Some(t) = self.current.borrow().clone() else { return };
        let have = Self::shown_parts(&t);
        if have.is_empty() {
            return;
        }
        let gains = self.gains_for(&t);
        let full = have.iter().all(|p| gains.get(*p).is_none_or(|g| *g >= 0.999));
        // the next song, if it's cued with its parts, takes the sliders too (it's heard in a crossfade)
        if let (Some(pl), Some(next)) = (self.player.borrow().as_ref(), self.next_track()) {
            pl.retune_cued(&self.gains_for(&next));
        }
        if self.on_stems.get() && self.player.borrow().as_ref().is_some_and(|p| p.is_stems()) {
            if let Some(pl) = self.player.borrow().as_ref() {
                for (p, g) in &gains {
                    pl.set_part(p, *g);
                }
                pl.set_whole(full); // all up: the song's own file again, blended in
            }
            self.update_quality();
            return;
        }
        if full {
            return; // the song plays as itself
        }
        // a server's song: its parts come here first (then this again)
        if !self.server_parts_ready(&t) {
            return;
        }
        // a song playing as a plain file (no saved copy, or after a DJ mix): over to its parts, once
        let at = self.player.borrow().as_ref().and_then(|p| p.position()).unwrap_or(0.0);
        self.start_parts(&t, at, true);
    }

    /// A song's parts as (name, file uri), and its own file's uri and sample
    /// rate when it's saved (then it plays among its parts).
    fn parts_of(&self, t: &Track) -> (Vec<(String, String)>, Option<String>, Option<u32>) {
        let parts = split::mix_parts(t)
            .iter()
            .filter_map(|p| split::ready_part(t, p).and_then(|f| glib::filename_to_uri(&f, None).ok()).map(|u| (p.to_string(), u.to_string())))
            .collect();
        let file = t.file.as_deref().filter(|f| Path::new(f).exists());
        (parts, file.map(|f| gio::File::for_path(f).uri().to_string()), file.and_then(file_rate))
    }

    /// Plays the song with its parts at hand, from `at`. `switch`: from the
    /// song playing as a plain file (it fades out, this fades in, on from
    /// where it's got to).
    fn start_parts(self: &Rc<Self>, t: &Track, at: f64, switch: bool) {
        crate::watch::doing("starting the split parts");
        let (parts, whole, rate) = self.parts_of(t);
        let gains = self.gains_for(t);
        let res = self.player.borrow().as_ref().map(|p| p.load_parts(whole.as_deref(), &parts, at, &gains, rate, switch));
        match res {
            Some(Ok(())) => {
                debug(&format!("“{}” plays with its parts at hand{}{}", t.title, if whole.is_some() { ", its own file among them" } else { "" }, if switch { " (switched over, fading)" } else { "" }));
                self.on_stems.set(true);
                self.on_half.set(None);
                self.quality_known.set(false);
                if let Some(w) = whole {
                    self.current_uri.replace(w);
                }
                self.replan();
            }
            Some(Err(e)) => debug(&format!("stems: {e}")),
            None => {}
        }
        self.set_controls();
    }

    /// The other version (song <-> its official music video), from the same spot.
    fn switch_version(self: &Rc<Self>, want_video: bool) {
        let Some(cur) = self.current.borrow().clone() else { return };
        if cur.video == want_video {
            return;
        }
        let Some((key, other)) = self.counterpart.borrow().clone() else { return };
        if key != cur.key() {
            return;
        }
        let at = self.player.borrow().as_ref().and_then(|p| p.position()).unwrap_or(0.0);
        let i = self.index.get();
        if let Some(slot) = self.queue.borrow_mut().get_mut(i) {
            *slot = other.clone();
        }
        // the way back is known already
        self.counterpart.replace(Some((other.key(), cur.clone())));
        self.restore_at.set((at > 1.0).then_some(at));
        self.play_current();
    }

    /// Looks for the playing song's other version; shows the switch if found.
    fn find_counterpart(self: &Rc<Self>, t: &Track) {
        let known = self.counterpart.borrow().as_ref().is_some_and(|(k, _)| *k == t.key());
        self.video_button.set_icon_name(if t.video { "audio-x-generic-symbolic" } else { "camera-video-symbolic" });
        self.video_button.set_tooltip_text(Some(if t.video { "Back to the song" } else { "Watch the music video" }));
        if known {
            self.video_button.set_visible(true);
            return;
        }
        self.video_button.set_visible(false);
        if t.source != Source::YouTube {
            return;
        }
        let me = Rc::downgrade(self);
        let song = t.clone();
        bg(move || youtube::counterpart(&song), {
            let key = t.key();
            move |found| {
                let Some(me) = me.upgrade() else { return };
                if me.current.borrow().as_ref().map(|c| c.key()) != Some(key.clone()) {
                    return;
                }
                if let Some(other) = found {
                    me.counterpart.replace(Some((key, other)));
                    me.video_button.set_visible(true);
                }
            }
        });
    }

    /// Plays the song playing now as the voice picked, from where it is.
    fn switch_half(self: &Rc<Self>, t: &Track) {
        crate::watch::doing("switching to a split half");
        let at = self.player.borrow().as_ref().and_then(|p| p.position()).unwrap_or(0.0);
        let uri = match self.half_for(t) {
            Some((v, uri)) => {
                self.on_half.set(Some(v));
                uri
            }
            None => {
                self.on_half.set(None);
                match self.uri_plain(t) {
                    Some(u) => u,
                    None => return,
                }
            }
        };
        self.quality_known.set(false);
        self.current_uri.replace(uri.clone());
        if let Some(p) = self.player.borrow().as_ref() {
            p.load_onward(&uri, at); // on from where the song has got to, never back
        }
        self.set_controls();
    }

    /// Splits a song when its turn comes (liked songs, the next song). A server's
    /// song that its server splits waits to be asked for by hand.
    fn request_split(self: &Rc<Self>, t: &Track) {
        if self.splits_on_server(t).is_some() {
            return;
        }
        self.queue_split(t, false);
    }

    /// Splits a song first, before any waiting (asked for by hand, or it's playing).
    /// A server's song is split on the server when it allows that (for everyone there).
    fn split_now(self: &Rc<Self>, t: &Track) {
        if self.splits_on_server(t).is_some() {
            if Self::shown_parts(t).is_empty() {
                self.split_on_server(t);
            }
            return;
        }
        self.split_asked.borrow_mut().insert(t.key());
        self.queue_split(t, true);
    }

    fn queue_split(self: &Rc<Self>, t: &Track, first: bool) {
        // done when the four parts are there (two halves alone: split again, for the mixer)
        if !self.can_split() || t.video || split::done(t) {
            return;
        }
        let key = t.key();
        if self.splitting.borrow().as_ref().is_some_and(|x| x.key() == key) {
            return; // being split now
        }
        let mut queue = self.split_queue.borrow_mut();
        queue.retain(|x| x.key() != key);
        if first {
            queue.insert(0, t.clone());
        } else {
            queue.push(t.clone());
        }
        drop(queue);
        self.run_splits();
    }

    /// Every liked (saved) song gets split, one after another, in the background.
    fn auto_split_liked(self: &Rc<Self>) {
        if std::env::var_os("RADIANCE_SHOT").is_some() {
            return; // a screenshot copy splits nothing
        }
        let liked: Vec<Track> = self.liked.borrow().iter().filter(|t| t.file.is_some() && !t.video).cloned().collect();
        for t in liked {
            if !split::done(&t) {
                self.request_split(&t);
            }
        }
    }

    /// Splitting waits while ~/.config/radiance-music/split-paused exists
    /// (benchmarks need the server to themselves).
    fn split_paused() -> bool {
        glib::user_config_dir().join("radiance-music/split-paused").exists()
    }

    /// Pauses or resumes the split queue (the song being split finishes first).
    fn set_split_paused(self: &Rc<Self>, on: bool) {
        let flag = glib::user_config_dir().join("radiance-music/split-paused");
        if on {
            if let Some(d) = flag.parent() {
                let _ = fs::create_dir_all(d);
            }
            let _ = fs::write(&flag, "paused from the app\n");
        } else {
            let _ = fs::remove_file(&flag);
        }
        if let Some(a) = self.window.lookup_action("split-pause").and_downcast::<gio::SimpleAction>() {
            a.set_state(&on.to_variant());
        }
        self.toast(if on { "Splitting paused: the song being split finishes, the rest wait" } else { "Splitting resumed" });
        self.update_voice_status();
        if !on {
            self.run_splits();
        }
    }

    /// The pause / resume button: shown while songs wait or one is being split.
    fn show_split_toggle(&self) {
        let waiting = self.split_queue.borrow().len();
        let busy = self.splitting.borrow().is_some();
        let paused = Self::split_paused();
        self.split_toggle.set_visible(self.can_split() && (waiting > 0 || busy || paused));
        let (icon, text) = if paused {
            ("media-playback-start-symbolic", format!("Resume splitting ({waiting} waiting)"))
        } else {
            ("media-playback-pause-symbolic", format!("Pause splitting ({waiting} waiting)"))
        };
        self.split_toggle.set_child(Some(&adw::ButtonContent::builder().icon_name(icon).label(&text).build()));
    }

    /// One split at a time on the server.
    fn run_splits(self: &Rc<Self>) {
        crate::watch::doing("starting the next split");
        if self.splitting.borrow().is_some() || self.split_queue.borrow().is_empty() {
            self.update_voice_status();
            return;
        }
        if Self::split_paused() {
            self.update_voice_status();
            let me = Rc::downgrade(self);
            glib::timeout_add_local_once(Duration::from_secs(30), move || {
                if let Some(me) = me.upgrade() {
                    me.run_splits(); // try again once the pause is over
                }
            });
            return;
        }
        let t = self.split_queue.borrow_mut().remove(0);
        let (server, local) = (self.settings.borrow().split.clone(), self.settings.borrow().local_split.clone());
        if server.is_none() && local.is_none() {
            return;
        }
        let uri = self.uri_plain(&t);
        let relay = self.relay.clone();
        self.splitting.replace(Some(t.clone()));
        self.split_progress.set(0.0);
        self.update_voice_status();
        self.refresh_split_marks();
        // progress from the worker thread, shown here as it comes
        let (tx, rx) = async_channel::unbounded::<f64>();
        let phase = tx.clone(); // after the split: -1 the library copy, -2 the word times
        let report: split::Progress = std::sync::Arc::new(move |p| {
            let _ = tx.try_send(p);
        });
        let me = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            while let Ok(p) = rx.recv().await {
                let Some(me) = me.upgrade() else { break };
                me.split_progress.set(p);
                me.show_split_progress();
            }
        });
        let me = Rc::downgrade(self);
        let song = t.clone();
        bg(
            move || {
                let uri = match uri {
                    Some(u) => u,
                    None => {
                        let relay = relay.ok_or("no relay")?;
                        let s = youtube::stream(&song.id)?;
                        relay.add(s.url, s.size, s.mime)
                    }
                };
                let done = split::split(&song, &uri, server.as_ref(), local.as_ref(), report);
                match (&done, &server, &local) {
                    (Ok(()), Some(server), _) => {
                        let _ = phase.try_send(-1.0);
                        keep_on_server(&song, server, &|| {
                            let _ = phase.try_send(-2.0);
                        }); // for hosting, with its words timed
                    }
                    (Ok(()), None, Some(local)) if !aligned_by_mms(&song) => {
                        // every word timed here too
                        let _ = phase.try_send(-2.0);
                        if let Err(e) = time_words(&song, &Aligner::Here(local.clone())) {
                            debug(&format!("word times for {}: {e}", song.title));
                        }
                    }
                    _ => {}
                }
                done
            },
            move |result| {
                let Some(me) = me.upgrade() else { return };
                me.splitting.replace(None);
                let asked = me.split_asked.borrow_mut().remove(&t.key());
                match result {
                    Ok(()) => {
                        let now = me.current.borrow().as_ref().is_some_and(|c| c.key() == t.key());
                        if now && me.voice.get() != Voice::Song && me.on_half.get().is_none() {
                            me.switch_half(&t);
                            me.toast(&format!("“{}”: {}", t.title, me.voice.get().label().to_lowercase()));
                        } else if asked {
                            me.toast(&format!("“{}” is split: No vocals and Vocals only are ready", t.title));
                        }
                        if now {
                            me.refresh_lyrics(); // its words get their times from the new Vocals part
                        }
                        me.refresh_split_marks(); // its scissors, in every list
                    }
                    Err(e) => {
                        debug(&format!("split {}: {e}", t.title));
                        if asked {
                            me.toast(&format!("Couldn't split “{}” right now", t.title));
                        }
                    }
                }
                me.refresh_split_marks();
                me.run_splits();
            },
        );
    }

    /// A row's split mark: a spinner while it's being split, scissors once it is.
    fn fill_split_slot(&self, slot: &gtk::Box, t: &Track) {
        while let Some(c) = slot.first_child() {
            slot.remove(&c);
        }
        if t.video {
            return;
        }
        // its lyrics, when kept here
        match lyrics_kind(t) {
            Some(LyricsKind::Words) => slot.append(&badge("format-justify-center-symbolic", "lyrics-words", "Lyrics, word by word", "Each word lights up as it's sung, timed to the song's own vocals.")),
            Some(LyricsKind::Lines) => slot.append(&badge("format-justify-center-symbolic", "lyrics", "Lyrics, line by line", "Each line lights up as it's sung. Once the song is split, every word gets its own time.")),
            Some(LyricsKind::Text) => slot.append(&badge("format-justify-center-symbolic", "lyrics-text", "Lyrics", "The words as text. Once the song is split, they're timed to the singing.")),
            None => {}
        }
        let splitting = self.splitting.borrow().as_ref().is_some_and(|x| x.key() == t.key()) || self.away_on_server(t);
        let waiting = self.split_queue.borrow().iter().position(|x| x.key() == t.key());
        let on_server = t.source == Source::Server && t.file.is_none() && !servers::mix_parts(t).is_empty();
        if on_server {
            slot.append(&badge("edit-cut-symbolic", "split", "Split on its server", "Its parts are on the server: the Mixer, No vocals and Vocals only play them from there."));
        } else if splitting {
            let spin = gtk::Spinner::builder().spinning(true).tooltip_text("Being split into the voice and the music now").build();
            slot.append(&spin);
        } else if split::is_split(t) {
            slot.append(&badge("edit-cut-symbolic", "split", "Split", "No vocals and Vocals only are ready: switch on Now Playing, instantly."));
        } else if let Some(i) = waiting {
            slot.append(&badge("content-loading-symbolic", "waiting", "Waiting to be split", &format!("Number {} in line; liked songs are split one after another.", i + 1)));
        }
    }

    /// Every song row on screen shows its split state as it is now.
    fn refresh_split_marks(&self) {
        crate::watch::doing("updating the split marks");
        let marks: Vec<(Track, gtk::Box)> = {
            let mut m = self.split_marks.borrow_mut();
            m.retain(|(_, w)| w.upgrade().is_some());
            m.iter().filter_map(|(t, w)| w.upgrade().map(|b| (t.clone(), b))).collect()
        };
        for (t, slot) in marks {
            self.fill_split_slot(&slot, &t);
        }
    }

    /// The split's bar: how far, in words (sending, splitting, bringing back).
    fn show_split_progress(&self) {
        crate::watch::doing("showing split progress");
        let busy = self.splitting.borrow().is_some();
        self.split_bar.set_visible(busy);
        if !busy {
            return;
        }
        let p = self.split_progress.get();
        // split; then its copy goes to the split server's library and its words get their times
        let text = match p {
            p if p <= -2.0 => "Timing every word (MMS on the split server)…".to_string(),
            p if p < 0.0 => "Keeping a copy in the split server's library…".to_string(),
            p if p < 0.05 => format!("Sending the song · {:.0}%", p * 100.0),
            p if p < 0.93 => format!("Splitting · {:.0}%", p * 100.0),
            p => format!("Bringing the parts back · {:.0}%", p * 100.0),
        };
        if p < 0.0 {
            self.split_bar.pulse();
        } else {
            self.split_bar.set_fraction(p);
        }
        self.split_bar.set_text(Some(&text));
        if let Some(r) = self.split_now_row.borrow().as_ref() {
            r.set_subtitle(&text);
        }
    }

    fn update_voice_status(self: &Rc<Self>) {
        self.render_split_queue();
        self.show_split_progress();
        self.show_split_toggle();
        let have: Vec<&str> = self.current.borrow().as_ref().filter(|t| !t.video).map(Self::shown_parts).unwrap_or_default();
        let parts = !have.is_empty();
        let ticked = self.current.borrow().as_ref().is_some_and(|t| split::wants_six(t));
        if let Some(b) = self.six_check.borrow().as_ref() {
            self.mixer_lock.set(true);
            b.set_active(ticked);
            self.mixer_lock.set(false);
        }
        // the sliders for the parts this song has (six, four, or music and vocals)
        let mut row = self.mixer.first_child();
        while let Some(r) = row {
            let name = r.widget_name();
            r.set_visible(match name.as_str() {
                "fx" => self.fx_device.get().is_some(),
                "six" => split::SIX_PARTS_ON && ((parts && have.len() >= 4) || ticked),
                n => have.contains(&n),
            });
            row = r.next_sibling();
        }
        let device = self.fx_name.borrow().clone().filter(|_| self.fx_device.get().is_some());
        self.mixer_row.set_subtitle(&match (parts, device.as_deref()) {
            (true, Some(_)) => "Each part as loud as you like, and effects".to_string(),
            (true, None) => "Each part as loud as you like".to_string(),
            (false, Some(d)) => format!("Effects for the {d}"),
            (false, None) => String::new(),
        });
        self.mixer_list.set_visible(parts || device.is_some());
        let cur = self.current.borrow().clone();
        let splitting_key = self.splitting.borrow().as_ref().map(|t| t.key());
        // not while it's being split or waiting in line for it
        let queued = |t: &Track| self.split_queue.borrow().iter().any(|x| x.key() == t.key());
        let can = cur.as_ref().is_some_and(|t| {
            let there = self.splits_on_server(t).is_some();
            (self.can_split() || there)
                && !t.video
                && Self::shown_parts(t).is_empty()
                && splitting_key.as_deref() != Some(t.key().as_str())
                && !queued(t)
                && !self.away_on_server(t)
        });
        self.split_button.set_visible(can);
        let waiting = self.split_queue.borrow().len();
        let text = if Self::split_paused() && waiting > 0 {
            format!("Splitting is paused ({waiting} waiting). Resume it on Liked Songs or in the menu.")
        } else if !self.can_split() && !cur.as_ref().is_some_and(|t| self.splits_on_server(t).is_some() || !Self::shown_parts(t).is_empty()) {
            self.voice_group.set_sensitive(false);
            // (not over a server's split, in the same card)
            if self.has_away() { String::new() } else { "No vocals / Vocals only and the Mixer's parts need splitting: set it up in the menu, Splitting…".to_string() }
        } else if let Some(t) = self.splitting.borrow().as_ref() {
            let host = self.settings.borrow().split.as_ref().map(|s| s.host.clone());
            let more = if waiting > 0 { format!(" ({waiting} more waiting)") } else { String::new() };
            let both = match (host, self.settings.borrow().local_split.is_some()) {
                (Some(h), true) => format!("{h} and this computer"),
                (Some(h), false) => h,
                (None, _) => "this computer".to_string(),
            };
            format!("Splitting “{}” on {both}{more}", t.title)
        } else if self.voice.get() != Voice::Song && self.on_half.get().is_none() && self.current.borrow().is_some() {
            "Plays as the full song until it's split.".to_string()
        } else {
            String::new()
        };
        self.voice_status.set_visible(!text.is_empty());
        self.voice_status.set_label(&text);
        let busy = self.splitting.borrow().is_some() || waiting > 0;
        let away = self.show_away(); // your songs in a server's split queue
        self.split_queue_btn.set_visible(busy || away);
        self.split_card.set_visible(busy || away); // nothing to say: no card (paused shows its own line while songs wait)
    }

    fn apply_volume(&self) {
        let level = if self.mute.is_active() { 0.0 } else { self.volume.value() };
        if let Some(p) = self.player.borrow().as_ref() {
            p.set_volume(level);
        }
        let icon = match level {
            l if l <= 0.0 => "audio-volume-muted-symbolic",
            l if l < 0.34 => "audio-volume-low-symbolic",
            l if l < 0.67 => "audio-volume-medium-symbolic",
            _ => "audio-volume-high-symbolic",
        };
        self.mute.set_icon_name(icon);
        self.mute.set_tooltip_text(Some(if self.mute.is_active() { "Unmute" } else { "Mute" }));
    }

    /// Shows a page by name (screenshots; "server": the first joined server's).
    pub fn show_page(self: &Rc<Self>, page: &str) {
        if page == "server" {
            self.show_first_server();
            return;
        }
        self.stack.set_visible_child_name(page);
    }

    /// Servers… open (screenshots).
    pub fn shot_servers(self: &Rc<Self>) {
        self.servers_dialog();
    }

    /// Plays the song shown, from where it was (screenshots; silent there).
    pub fn shot_play(self: &Rc<Self>) {
        self.toggle_play();
    }

    /// Opens the side panel on a tab (screenshots).
    pub fn shot_panel(&self, tab: &str) {
        self.side_tabs.set_active_name(Some(tab));
        self.queue_button.set_active(true);
    }

    /// Searches for `q` on the Search page (screenshots).
    pub fn search_for(&self, q: &str) {
        self.go_search(q);
    }

    /// Opens the Mixer (screenshots).
    pub fn shot_mixer(&self) {
        self.mixer_row.set_expanded(true);
    }

    /// The Splitting window open (screenshots).
    pub fn shot_splitting(self: &Rc<Self>) {
        self.splitting_dialog();
    }

    /// Searches Liked Songs for `q` (screenshots).
    pub fn find_liked(&self, q: &str) {
        self.stack.set_visible_child_name("liked");
        self.liked_search.set_text(q);
    }

    fn toast(&self, text: &str) {
        self.toasts.add_toast(adw::Toast::builder().use_markup(false).title(text).timeout(4).build());
    }

    fn about(&self) {
        let about = adw::AboutDialog::builder()
            .application_name("Radiance Music")
            .application_icon(APP_ID)
            .developer_name("Radiance Innovation Centers")
            .version(env!("CARGO_PKG_VERSION"))
            .website("https://github.com/RadianceADM/radiance-music")
            .issue_url("https://github.com/RadianceADM/radiance-music/issues")
            .license_type(gtk::License::Gpl30)
            .copyright("© 2026 Radiance Innovation Centers")
            .comments(
                "Search any song and play it. Lossless recordings from the free libraries come first; \
                 YouTube Music fills in the rest. Like a song to keep it, with its synced lyrics and \
                 album art, and shape the sound with your EQ.",
            )
            .developers(vec!["Bassi https://github.com/RadianceADM"])
            .release_notes(release_notes(include_str!("../CHANGELOG.md"), 3))
            .release_notes_version(env!("CARGO_PKG_VERSION"))
            .build();
        about.add_credit_section(
            Some("Music and lyrics from"),
            &["Internet Archive https://archive.org", "YouTube Music https://music.youtube.com", "LRCLIB https://lrclib.net"],
        );
        about.add_credit_section(Some("Built with"), &["GStreamer https://gstreamer.freedesktop.org", "yt-dlp https://github.com/yt-dlp/yt-dlp", "GTK and libadwaita https://gtk.org"]);
        about.add_legal_section("yt-dlp", None, gtk::License::Custom, Some("yt-dlp is released into the public domain (The Unlicense)."));
        about.add_legal_section("GStreamer", None, gtk::License::Lgpl21, None);
        about.present(Some(&self.window));
    }

    /// Fetches art off the main thread (once per address and size) and hands it to `apply`.
    fn set_art(self: &Rc<Self>, source: &str, size: u32, apply: impl Fn(&gdk::Texture) + 'static) {
        if source.is_empty() {
            return;
        }
        let key = (source.to_string(), size);
        if let Some(t) = self.textures.borrow().get(&key) {
            apply(t);
            return;
        }
        let me = Rc::downgrade(self);
        let src = source.to_string();
        bg_art(move || art::fetch(&src, size), move |px| {
            if let (Some(px), Some(me)) = (px, me.upgrade()) {
                let t = texture(px);
                me.textures.borrow_mut().insert(key, t.clone());
                apply(&t);
            }
        });
    }

    /// The blurred art behind Now playing, faded in over the last one.
    fn set_backdrop(self: &Rc<Self>, source: &str) {
        let me = Rc::downgrade(self);
        let src = source.to_string();
        let n = self.backdrop_n.get() + 1;
        self.backdrop_n.set(n);
        bg(move || art::backdrop(&src).map(|px| (art::blob(&px), px)), move |px| {
            let (Some((patch, whole)), Some(me)) = (px, me.upgrade()) else { return };
            if me.backdrop_n.get() != n {
                return; // a newer song's art is on its way
            }
            me.backdrop.set_art(backdrop::Art { whole: texture(whole), patch: texture(patch) }); // fades in over the old one
        });
    }

    /// The backdrop drifts only while Now playing is on screen and music plays.
    fn update_drift(&self) {
        let on = self.stack.visible_child_name().as_deref() == Some("playing")
            && self.player.borrow().as_ref().is_some_and(|p| p.playing());
        self.backdrop.set_drifting(on);
    }

    // ------------------------------------------------------------------ search

    fn search_changed(self: &Rc<Self>) {
        if let Some(id) = self.debounce.borrow_mut().take() {
            id.remove();
        }
        let me = Rc::downgrade(self);
        let id = glib::timeout_add_local_once(Duration::from_millis(SEARCH_DEBOUNCE_MS), move || {
            if let Some(me) = me.upgrade() {
                me.debounce.borrow_mut().take();
                me.update_suggestions();
                me.run_search();
            }
        });
        *self.debounce.borrow_mut() = Some(id);
    }

    fn run_search(self: &Rc<Self>) {
        let q = self.search.text().trim().to_string();
        let gen = self.search_gen.get() + 1;
        self.search_gen.set(gen);
        *self.found_lossless.borrow_mut() = None;
        *self.found_youtube.borrow_mut() = None;
        *self.found_videos.borrow_mut() = None;
        if q.chars().count() < 2 {
            self.render_results("");
            return;
        }
        self.render_results(&q);
        for kind in 0..3 {
            let me = Rc::downgrade(self);
            let (q1, q2) = (q.clone(), q.clone());
            bg(
                move || {
                    let found = match kind {
                        0 => archive::search(&q1),
                        1 => youtube::search(&q1),
                        _ => youtube::search_videos(&q1),
                    };
                    // YouTube Music found nothing: is it the song, or the internet? (see net::reachable)
                    let reached = (kind != 1 || !found.is_empty()).then_some(true).unwrap_or_else(crate::net::reachable);
                    (found, reached)
                },
                move |(found, reached)| {
                    let Some(me) = me.upgrade() else { return };
                    if kind == 1 {
                        me.set_online(reached, true);
                    }
                    if me.search_gen.get() != gen {
                        return; // typed on since
                    }
                    let slot = match kind {
                        0 => &me.found_lossless,
                        1 => &me.found_youtube,
                        _ => &me.found_videos,
                    };
                    *slot.borrow_mut() = Some(found);
                    me.render_results(&q2);
                },
            );
        }
    }

    /// Searches for an artist or an album (from a song's menu).
    fn go_search(&self, q: &str) {
        self.stack.set_visible_child_name("search");
        self.search.set_text(q);
        self.search.set_position(-1);
    }

    fn render_results(self: &Rc<Self>, q: &str) {
        for (g, r) in self.result_rows.borrow_mut().drain(..) {
            g.remove(&r);
        }
        if q.is_empty() {
            self.search_view.set_visible_child_name("empty");
            return;
        }
        self.search_view.set_visible_child_name("results");
        // your own first: the liked songs that match (titles that match before artists that do)
        let mut mine: Vec<Track> = self.liked.borrow().iter().filter(|t| matches(t, q)).cloned().collect();
        let by_title = |t: &Track| matches(&Track { artist: String::new(), album: String::new(), ..t.clone() }, q);
        mine.sort_by_cached_key(|t| (!by_title(t), file_key(&t.title)));
        let yours = |t: &Track| mine.iter().any(|m| m.key() == t.key());
        let lossless = self.found_lossless.borrow().clone().map(|v| v.into_iter().filter(|t| !yours(t)).collect::<Vec<_>>());
        // a song found lossless isn't offered again from YouTube
        let youtube = self.found_youtube.borrow().clone().map(|v| {
            v.into_iter().filter(|t| !yours(t) && !lossless.as_ref().is_some_and(|l| l.iter().any(|x| x.same_song(t)))).collect::<Vec<_>>()
        });
        self.lossless_spin.set_visible(lossless.is_none());
        self.fallback_spin.set_visible(youtube.is_none());
        // offline, nothing is found anywhere: say that, not "no songs" (2026-10-31 audit)
        let off = !self.online.get();
        const OFF: &str = "You're offline: searching needs the internet. Your liked songs still play; it searches again when you're back online.";
        self.lossless.set_description(Some(&match &lossless {
            None => "Searching the free lossless libraries (Internet Archive)…".to_string(),
            Some(v) if v.is_empty() && off => OFF.to_string(),
            Some(v) if v.is_empty() => format!("The free libraries have no lossless recording of “{q}”."),
            Some(_) => "Free FLAC from the Internet Archive. Played first.".to_string(),
        }));
        self.fallback.set_description(Some(&match &youtube {
            None => "Searching YouTube Music…".to_string(),
            Some(v) if v.is_empty() && off => "YouTube Music too, once you're back online.".to_string(),
            Some(v) if v.is_empty() => format!("YouTube Music found no other songs for “{q}”."),
            Some(_) => "Compressed (Opus). The fallback when there's no lossless copy.".to_string(),
        }));
        let videos = self.found_videos.borrow().clone().map(|v| v.into_iter().filter(|t| !yours(t)).collect::<Vec<_>>());
        self.videos_spin.set_visible(videos.is_none());
        self.videos.set_description(Some(&match &videos {
            None => "Searching music videos…".to_string(),
            Some(v) if v.is_empty() && off => "Music videos too.".to_string(),
            Some(v) if v.is_empty() => format!("No music videos for “{q}”."),
            Some(_) => "Music videos from YouTube: they play with their picture in Now playing.".to_string(),
        }));
        self.in_liked.set_visible(!mine.is_empty());
        self.in_liked.set_description(Some(&format!("{} of your liked songs {}", mine.len(), if mine.len() == 1 { "matches" } else { "match" })));
        if !mine.is_empty() {
            let mine = Rc::new(mine);
            for (i, t) in mine.iter().enumerate() {
                let row = self.track_row(t, Place::Liked(mine.clone(), i));
                self.in_liked.add(&row);
                self.result_rows.borrow_mut().push((self.in_liked.clone(), row));
            }
        }
        let list: Rc<Vec<Track>> =
            Rc::new(lossless.unwrap_or_default().into_iter().chain(youtube.unwrap_or_default()).chain(videos.unwrap_or_default()).collect());
        for (i, t) in list.iter().enumerate() {
            let row = self.track_row(t, Place::List(list.clone(), i));
            let group = if t.video {
                &self.videos
            } else if t.lossless() {
                &self.lossless
            } else {
                &self.fallback
            };
            group.add(&row);
            self.result_rows.borrow_mut().push((group.clone(), row));
        }
    }

    fn track_row(self: &Rc<Self>, t: &Track, place: Place) -> adw::ActionRow {
        // plain text first, then the names (an "&" or "<3" isn't markup)
        let row = adw::ActionRow::builder().use_markup(false).title_lines(1).subtitle_lines(1).activatable(true).build();
        row.set_title(&t.title);
        row.set_subtitle(&t.byline());
        if t.artists().len() > 1 {
            row.set_tooltip_text(Some(&t.credit())); // every name, even when the line is cut short
        }
        let img = gtk::Image::builder().icon_name("audio-x-generic-symbolic").pixel_size(48).overflow(gtk::Overflow::Hidden).css_classes(["art"]).build();
        row.add_prefix(&img);
        let weak = img.downgrade();
        self.set_art(&art_source(t), THUMB, move |tex| {
            if let Some(img) = weak.upgrade() {
                img.set_paintable(Some(tex));
            }
        });
        // its split mark (kept up to date in place, in every list: see refresh_split_marks)
        let slot = gtk::Box::builder().valign(gtk::Align::Center).build();
        self.fill_split_slot(&slot, t);
        self.split_marks.borrow_mut().push((t.clone(), slot.downgrade()));
        row.add_suffix(&slot);
        let quality = quality_badge(t);
        row.add_suffix(&quality);
        let mut extras: Vec<gtk::Widget> = vec![slot.clone().upcast(), quality.upcast()];
        let (plays, _) = counts_of(t);
        if plays > 0 {
            let n = gtk::Label::builder().label(format!("{plays}×")).tooltip_text(counts_text(t)).css_classes(["dim-label", "numeric", "caption"]).build();
            row.add_suffix(&n);
            extras.push(n.upcast());
        }
        // on a phone the title needs the room
        let mut kept = self.row_extras.borrow_mut();
        if kept.len() > 4000 {
            kept.retain(|w| w.upgrade().is_some());
        }
        for w in extras {
            w.set_visible(!self.phone.get());
            kept.push(w.downgrade());
        }
        drop(kept);
        if t.seconds > 0 {
            let len = gtk::Label::builder().label(t.length()).css_classes(["dim-label", "numeric"]).build();
            row.add_suffix(&len);
        }
        row.add_suffix(&self.heart_button(t));
        let me = Rc::downgrade(self);
        let p = place.clone();
        row.connect_activated(move |_| {
            if let Some(me) = me.upgrade() {
                me.play_from(&p);
            }
        });
        self.attach_menu(row.upcast_ref(), place);
        row
    }

    fn play_from(self: &Rc<Self>, place: &Place) {
        match place {
            Place::List(list, i) | Place::Liked(list, i) | Place::Playlist(_, list, i) => self.play_list((**list).clone(), *i),
            Place::Server(list, i) => self.play_from_server(list, *i),
            Place::Queue(i) => {
                self.index.set(*i);
                self.play_current();
            }
            Place::Again(t) => self.play_again(t),
            Place::Current => {}
        }
    }

    // ------------------------------------------------------------------ song menus

    /// Right-click, long-press, or the Menu key / Shift+F10.
    fn attach_menu(self: &Rc<Self>, w: &gtk::Widget, place: Place) {
        let click = gtk::GestureClick::builder().button(gdk::BUTTON_SECONDARY).build();
        let (me, p, ww) = (Rc::downgrade(self), place.clone(), w.downgrade());
        click.connect_pressed(move |g, _, x, y| {
            g.set_state(gtk::EventSequenceState::Claimed);
            if let (Some(me), Some(w)) = (me.upgrade(), ww.upgrade()) {
                me.open_menu(&w, &p, Some((x, y)));
            }
        });
        w.add_controller(click);
        let press = gtk::GestureLongPress::builder().touch_only(true).build();
        let (me, p, ww) = (Rc::downgrade(self), place.clone(), w.downgrade());
        press.connect_pressed(move |g, x, y| {
            g.set_state(gtk::EventSequenceState::Claimed);
            if let (Some(me), Some(w)) = (me.upgrade(), ww.upgrade()) {
                me.open_menu(&w, &p, Some((x, y)));
            }
        });
        w.add_controller(press);
        let keys = gtk::ShortcutController::new();
        let (me, ww) = (Rc::downgrade(self), w.downgrade());
        keys.add_shortcut(gtk::Shortcut::new(
            gtk::ShortcutTrigger::parse_string("Menu|<Shift>F10"),
            Some(gtk::CallbackAction::new(move |_, _| {
                if let (Some(me), Some(w)) = (me.upgrade(), ww.upgrade()) {
                    me.open_menu(&w, &place, None);
                }
                glib::Propagation::Stop
            })),
        ));
        w.add_controller(keys);
    }

    fn open_menu(self: &Rc<Self>, on: &gtk::Widget, place: &Place, at: Option<(f64, f64)>) {
        let track = match place {
            Place::List(list, i) | Place::Liked(list, i) | Place::Playlist(_, list, i) | Place::Server(list, i) => list.get(*i).cloned(),
            Place::Queue(i) => self.queue.borrow().get(*i).cloned(),
            Place::Again(t) => Some(t.clone()),
            Place::Current => self.current.borrow().clone(),
        };
        let Some(t) = track else { return };
        let actions = gio::SimpleActionGroup::new();
        let menu = gio::Menu::new();
        let me = Rc::downgrade(self);
        let add = |section: &gio::Menu, label: &str, name: &str, f: Box<dyn Fn(&Rc<Ui>)>| {
            section.append(Some(label), Some(&format!("song.{name}")));
            let action = gio::SimpleAction::new(name, None);
            let me = me.clone();
            action.connect_activate(move |_, _| {
                if let Some(me) = me.upgrade() {
                    f(&me);
                }
            });
            actions.add_action(&action);
        };

        let play = gio::Menu::new();
        if let Place::Queue(i) = place {
            let i = *i;
            add(&play, "Remove from Queue", "unqueue", Box::new(move |me| me.unqueue(i)));
        }
        if !matches!(place, Place::Current) {
            let p = place.clone();
            add(&play, "Play", "play", Box::new(move |me| me.play_from(&p)));
        }
        let t1 = t.clone();
        add(&play, "Play Next", "play-next", Box::new(move |me| me.play_next(&t1)));
        let t1 = t.clone();
        add(&play, "Add to Queue", "enqueue", Box::new(move |me| me.enqueue(&t1)));
        menu.append_section(None, &play);

        let lists = gio::Menu::new();
        if let Place::Playlist(id, _, _) = place {
            let (id, key) = (*id, t.key());
            add(&lists, "Remove from This Playlist", "pl-remove", Box::new(move |me| {
                crate::db::remove_from_playlist(id, &key);
                me.open_playlist(id);
                me.render_playlist_nav();
            }));
        }
        let to = gio::Menu::new();
        for (id, name, _) in crate::db::playlists() {
            let t1 = t.clone();
            let name2 = name.clone();
            add(&to, &name, &format!("pl-add-{id}"), Box::new(move |me| {
                let added = crate::db::add_to_playlist(id, &t1);
                me.toast(&if added { format!("Added to “{name2}”") } else { format!("Already in “{name2}”") });
                me.render_playlist_nav();
            }));
        }
        let t1 = t.clone();
        add(&to, "New Playlist…", "pl-new", Box::new(move |me| me.new_playlist(Some(t1.clone()))));
        lists.append_submenu(Some("Add to Playlist"), &to);
        menu.append_section(None, &lists);

        let mine = gio::Menu::new();
        let liked = self.is_liked(&t.key());
        let t1 = t.clone();
        add(&mine, if liked { "Remove from Liked" } else { "Like" }, "like", Box::new(move |me| me.toggle_like(&t1)));
        let t1 = t.clone();
        add(&mine, "Song Radio", "radio", Box::new(move |me| me.start_radio(&t1)));
        if let Some(s) = self.splits_on_server(&t) {
            if Self::shown_parts(&t).is_empty() && !self.away_on_server(&t) {
                let t1 = t.clone();
                add(&mine, &format!("Split on {}", s.name), "split", Box::new(move |me| me.split_on_server(&t1)));
            }
        } else if self.can_split() && !t.video && !split::done(&t) {
            let t1 = t.clone();
            add(&mine, "Split Vocals", "split", Box::new(move |me| {
                me.split_now(&t1);
                me.toast(&format!("Splitting “{}”: about 3 minutes", t1.title));
            }));
        }
        menu.append_section(None, &mine);

        // a Radiance Music Server's: its party, saving its song, adding yours to it
        let server = gio::Menu::new();
        if t.source == Source::Server && servers::of_track(&t).is_some_and(|s| self.in_party_on(&s.address)) {
            let t1 = t.clone();
            add(&server, "Add to the Party", "party-add", Box::new(move |me| me.add_to_party(&t1)));
        }
        if t.source == Source::Server && t.file.is_none() && !liked {
            let t1 = t.clone();
            add(&server, "Save to Your Library", "save", Box::new(move |me| me.toggle_like(&t1)));
        }
        for (i, s) in self.servers_for(&t).into_iter().enumerate() {
            let t1 = t.clone();
            add(&server, &format!("Add to {}", s.name), &format!("to-server-{i}"), Box::new(move |me| me.add_to_server(&t1, &s)));
        }
        if server.n_items() > 0 {
            menu.append_section(None, &server);
        }

        let go = gio::Menu::new();
        let artists = t.artists();
        if artists.len() == 1 {
            let q = artists[0].clone();
            add(&go, "Go to Artist", "artist", Box::new(move |me| me.go_search(&q)));
        } else {
            for (i, a) in artists.iter().take(8).enumerate() {
                let q = a.clone();
                add(&go, &format!("Go to {a}"), &format!("artist{i}"), Box::new(move |me| me.go_search(&q)));
            }
        }
        if !t.album.is_empty() {
            let q = format!("{} {}", t.album, t.artist);
            add(&go, "Go to Album", "album", Box::new(move |me| me.go_search(q.trim())));
        }
        menu.append_section(None, &go);

        let more = gio::Menu::new();
        let t1 = t.clone();
        add(&more, "Show Lyrics", "lyrics", Box::new(move |me| me.lyrics_dialog(&t1)));
        let t1 = t.clone();
        add(&more, "Edit Lyrics…", "edit-lyrics", Box::new(move |me| me.edit_lyrics(&t1)));
        let t1 = t.clone();
        add(&more, "Find Lyrics Again", "relook-lyrics", Box::new(move |me| me.find_lyrics_again(&t1)));
        let t1 = t.clone();
        add(&more, "Track Info", "info", Box::new(move |me| me.info_dialog(&t1)));
        let t1 = t.clone();
        add(&more, "Copy Link", "link", Box::new(move |me| me.copy_link(&t1)));
        if let Some(file) = t.file.clone().filter(|f| Path::new(f).exists()) {
            add(
                &more,
                "Show in Files",
                "files",
                Box::new(move |me| {
                    gtk::FileLauncher::new(Some(&gio::File::for_path(&file))).open_containing_folder(Some(&me.window), gio::Cancellable::NONE, |_| {});
                }),
            );
        }
        menu.append_section(None, &more);

        on.insert_action_group("song", Some(&actions));
        let pop = gtk::PopoverMenu::from_model(Some(&menu));
        pop.set_parent(on);
        pop.set_has_arrow(false);
        if let Some((x, y)) = at {
            pop.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        }
        pop.connect_closed(|p| {
            let p = p.clone();
            glib::idle_add_local_once(move || p.unparent()); // after its action has run
        });
        pop.popup();
    }

    fn copy_link(&self, t: &Track) {
        self.window.clipboard().set_text(&link(t));
        self.toast("Link copied");
    }

    fn lyrics_dialog(self: &Rc<Self>, t: &Track) {
        let label = gtk::Label::builder()
            .label("Looking for lyrics…")
            .wrap(true).wrap_mode(gtk::pango::WrapMode::WordChar)
            .justify(gtk::Justification::Center)
            .selectable(true)
            .margin_top(18)
            .margin_bottom(18)
            .margin_start(18)
            .margin_end(18)
            .build();
        let scroll = gtk::ScrolledWindow::builder().child(&label).vexpand(true).hscrollbar_policy(gtk::PolicyType::Never).build();
        // wrong words? two ways to fix them, both timed to the singing once the song is split
        let again = gtk::Button::builder().label("Find Again").tooltip_text("Look the lyrics up again (another artist's song is turned away). Split songs get every word timed to the singing.").build();
        let edit = gtk::Button::builder().label("Edit…").tooltip_text("Type or paste the right words, and when each line starts. Saved as yours; split songs get every word timed to the singing.").build();
        let header = adw::HeaderBar::new();
        header.pack_start(&again);
        header.pack_end(&edit);
        let view = adw::ToolbarView::new();
        view.add_top_bar(&header);
        view.set_content(Some(&scroll));
        let dialog = adw::Dialog::builder().title(&t.title).content_width(460).content_height(620).child(&view).build();
        for (button, relook) in [(&again, true), (&edit, false)] {
            let (me, song, d) = (Rc::downgrade(self), t.clone(), dialog.downgrade());
            button.connect_clicked(move |_| {
                let Some(me) = me.upgrade() else { return };
                if let Some(d) = d.upgrade() {
                    d.close();
                }
                if relook { me.find_lyrics_again(&song) } else { me.edit_lyrics(&song) }
            });
        }
        dialog.present(Some(&self.window));
        let weak = label.downgrade();
        let t = t.clone();
        bg(move || lyrics_for(&t), move |l| {
            if let Some(label) = weak.upgrade() {
                label.set_label(&match l {
                    Lyrics::Timed { lines, .. } => lines.into_iter().map(|x| x.text).collect::<Vec<_>>().join("\n"),
                    Lyrics::Plain(text) => text,
                    Lyrics::None => "No lyrics found for this song.".into(),
                });
            }
        });
    }

    fn info_dialog(&self, t: &Track) {
        let mut lines = vec![];
        match t.artists().len() {
            0 => {}
            1 => lines.push(format!("Artist: {}", t.credit())),
            _ => lines.push(format!("Artists: {}", t.credit())),
        }
        if !t.album.is_empty() {
            lines.push(format!("Album: {}", t.album));
        }
        let counts = counts_text(t);
        if !counts.is_empty() {
            lines.push(counts);
        }
        if t.seconds > 0 {
            lines.push(format!("Length: {}", t.length()));
        }
        lines.push(format!(
            "From: {}",
            match t.source {
                Source::Server => format!("{} ({})", servers::of_track(t).map(|s| s.name).unwrap_or_else(|| "a Radiance Music Server".into()), t.format),
                _ if t.lossless() => format!("Internet Archive, lossless ({})", t.format),
                _ => "YouTube Music, compressed (Opus)".into(),
            }
        ));
        if self.current.borrow().as_ref().is_some_and(|c| c.key() == t.key()) && self.quality_known.get() {
            lines.push(format!("Playing as: {}", self.bar_quality.label()));
        }
        let saved = self.liked.borrow().iter().find(|x| x.key() == t.key()).and_then(|x| x.file.clone());
        lines.push(match saved {
            Some(f) => format!("Saved: {f}"),
            None if self.is_liked(&t.key()) => "Liked; streams (not saved as a file)".into(),
            None => "Not saved".into(),
        });
        lines.push(format!("Link: {}", link(t)));
        let dialog = adw::AlertDialog::builder().heading(&t.title).body(lines.join("\n")).build();
        dialog.add_response("copy", "Copy Link");
        dialog.add_response("close", "Close");
        dialog.set_default_response(Some("close"));
        dialog.set_close_response("close");
        let window = self.window.clone();
        let url = link(t);
        dialog.connect_response(Some("copy"), move |_, _| window.clipboard().set_text(&url));
        dialog.present(Some(&self.window));
    }

    // ------------------------------------------------------------------ queue

    /// A song played before, again now; the queue carries on after it (the
    /// song it interrupts isn't counted as skipped: going back isn't disliking).
    fn play_again(self: &Rc<Self>, t: &Track) {
        if self.current.borrow().is_none() {
            self.play_list(vec![t.clone()], 0);
            return;
        }
        let at = (self.index.get() + 1).min(self.queue.borrow().len());
        self.queue.borrow_mut().insert(at, t.clone());
        self.index.set(at);
        self.play_current(); // lists the queue again too
        self.replan();
        self.save_session();
    }

    fn play_next(self: &Rc<Self>, t: &Track) {
        if self.current.borrow().is_none() {
            self.play_list(vec![t.clone()], 0);
            return;
        }
        let at = (self.index.get() + 1).min(self.queue.borrow().len());
        self.queue.borrow_mut().insert(at, t.clone());
        self.queue_changed();
        self.prefetch_next();
        self.toast(&format!("“{}” plays next", t.title));
    }

    fn enqueue(self: &Rc<Self>, t: &Track) {
        if self.current.borrow().is_none() {
            self.play_list(vec![t.clone()], 0);
            return;
        }
        self.queue.borrow_mut().push(t.clone());
        self.queue_changed();
        self.prefetch_next();
        self.toast(&format!("Added “{}” to the queue", t.title));
    }

    fn unqueue(self: &Rc<Self>, i: usize) {
        let len = self.queue.borrow().len();
        if i >= len {
            return;
        }
        let cur = self.index.get();
        self.queue.borrow_mut().remove(i);
        if i < cur {
            self.index.set(cur - 1);
        } else if i == cur {
            // the song playing: on to the next one (now at the same place)
            if cur < len - 1 {
                self.play_current();
            } else {
                if let Some(p) = self.player.borrow().as_ref() {
                    p.stop();
                }
                *self.current.borrow_mut() = None;
                self.index.set(cur.saturating_sub(1));
            }
        }
        self.queue_changed();
    }

    fn queue_changed(self: &Rc<Self>) {
        self.replan();
        self.save_session();
        self.render_queue();
        self.set_controls();
    }

    fn render_queue(self: &Rc<Self>) {
        crate::watch::doing("listing the queue");
        for r in self.queue_rows.borrow_mut().drain(..) {
            self.queue_group.remove(&r);
        }
        self.render_recent();
        let queue = self.queue.borrow().clone();
        let cur = self.index.get();
        let playing = self.current.borrow().is_some();
        let ahead = queue.len().saturating_sub(cur + usize::from(playing));
        self.queue_group.set_description(Some(&if queue.is_empty() || (!playing && ahead == 0) {
            "Nothing queued. Play a song, or right-click any song and pick Add to Queue.".to_string()
        } else if ahead == 0 {
            if self.settings.borrow().autoplay && self.repeat.get() == Repeat::Off {
                "Autoplay is adding songs like this one next.".to_string()
            } else {
                "Nothing after this song. Right-click any song and pick Add to Queue, or start a Song Radio (or turn on Autoplay in the menu).".to_string()
            }
        } else {
            format!("{ahead} songs next")
        }));
        for (i, t) in queue.iter().enumerate().skip(cur).take(QUEUE_ROWS) {
            let row = self.track_row(t, Place::Queue(i));
            let now = i == cur && playing;
            if now {
                row.add_css_class("now-row");
                row.set_subtitle(&format!("Now playing · {}", t.byline()));
            }
            self.queue_drag(&row, i, now);
            self.queue_group.add(&row);
            self.queue_rows.borrow_mut().push(row);
        }
    }

    /// Dragging songs in the queue: any song after the one playing can be
    /// picked up (its handle shows it) and dropped above or below another
    /// (the line shows where); the song playing stays where it is.
    fn queue_drag(self: &Rc<Self>, row: &adw::ActionRow, i: usize, now: bool) {
        if !now {
            let handle = gtk::Image::builder().icon_name("list-drag-handle-symbolic").css_classes(["dim-label"]).tooltip_text("Drag to move it in the queue").build();
            row.add_prefix(&handle);
            if let Some(prefixes) = handle.parent().and_downcast::<gtk::Box>() {
                prefixes.reorder_child_after(&handle, None::<&gtk::Widget>); // before the art
            }
            let drag = gtk::DragSource::builder().actions(gdk::DragAction::MOVE).build();
            let grabbed = Rc::new(Cell::new((0.0, 0.0)));
            let g = grabbed.clone();
            drag.connect_prepare(move |_, x, y| {
                g.set((x, y));
                Some(gdk::ContentProvider::for_value(&(i as u32).to_value()))
            });
            let r = row.downgrade();
            drag.connect_drag_begin(move |src, _| {
                let Some(r) = r.upgrade() else { return };
                // the row as it looks, under the pointer where it was picked up
                let (x, y) = grabbed.get();
                src.set_icon(Some(&gtk::WidgetPaintable::new(Some(&r)).current_image()), x as i32, y as i32);
                r.add_css_class("dragged");
            });
            let r = row.downgrade();
            drag.connect_drag_end(move |_, _, _| {
                if let Some(r) = r.upgrade() {
                    r.remove_css_class("dragged");
                }
            });
            row.add_controller(drag);
        }
        // a place to drop: above this row or below it (only below the song playing)
        let target = gtk::DropTarget::new(u32::static_type(), gdk::DragAction::MOVE);
        let r = row.downgrade();
        let below = move |y: f64| now || r.upgrade().is_some_and(|r| y > r.height() as f64 / 2.0);
        let (r, b) = (row.downgrade(), below.clone());
        target.connect_motion(move |_, _, y| {
            if let Some(r) = r.upgrade() {
                let (on, off) = if b(y) { ("drop-below", "drop-above") } else { ("drop-above", "drop-below") };
                r.remove_css_class(off);
                r.add_css_class(on);
            }
            gdk::DragAction::MOVE
        });
        let r = row.downgrade();
        target.connect_leave(move |_| {
            if let Some(r) = r.upgrade() {
                r.remove_css_class("drop-above");
                r.remove_css_class("drop-below");
            }
        });
        let me = Rc::downgrade(self);
        target.connect_drop(move |_, value, _, y| {
            let Ok(from) = value.get::<u32>() else { return false };
            let to = if below(y) { i + 1 } else { i };
            let me = me.clone();
            // after the drag is over (the rows are made again)
            glib::idle_add_local_once(move || {
                if let Some(me) = me.upgrade() {
                    me.move_in_queue(from as usize, to);
                }
            });
            true
        });
        row.add_controller(target);
    }

    /// The queue's song at `from` moves to just before `to` (never above the
    /// song playing).
    fn move_in_queue(self: &Rc<Self>, from: usize, to: usize) {
        let first = self.index.get() + usize::from(self.current.borrow().is_some());
        let len = self.queue.borrow().len();
        if from < first || from >= len {
            return;
        }
        let to = to.clamp(first, len);
        if to == from || to == from + 1 {
            return; // where it was
        }
        let title = {
            let mut q = self.queue.borrow_mut();
            let t = q.remove(from);
            let title = t.title.clone();
            q.insert(if to > from { to - 1 } else { to }, t);
            title
        };
        crate::log::note(&format!("queue: “{title}” moved from place {} to {}", from + 1 - first, if to > from { to - first } else { to + 1 - first }));
        self.queue_changed();
        self.prefetch_next();
    }

    /// Dragging a song near the top or bottom of the queue scrolls it.
    fn drag_scrolls(page: &adw::PreferencesPage) {
        fn scrolled(w: &gtk::Widget) -> Option<gtk::ScrolledWindow> {
            if let Some(s) = w.downcast_ref::<gtk::ScrolledWindow>() {
                return Some(s.clone());
            }
            let mut next = w.first_child();
            while let Some(c) = next {
                if let Some(s) = scrolled(&c) {
                    return Some(s);
                }
                next = c.next_sibling();
            }
            None
        }
        let Some(scroll) = scrolled(page.upcast_ref()) else { return };
        let motion = gtk::DropControllerMotion::new();
        let speed = Rc::new(Cell::new(0.0f64));
        let timer: Rc<RefCell<Option<glib::SourceId>>> = Rc::default();
        let (s, t, view) = (speed.clone(), timer.clone(), scroll.downgrade());
        motion.connect_motion(move |m, _, y| {
            let Some(view) = view.upgrade() else { return };
            const EDGE: f64 = 56.0;
            let h = view.height() as f64;
            // up to 18 px a frame, faster the nearer the edge
            s.set(if y < EDGE { -18.0 * (EDGE - y) / EDGE } else if y > h - EDGE { 18.0 * (y - (h - EDGE)) / EDGE } else { 0.0 });
            if s.get() == 0.0 || t.borrow().is_some() {
                return;
            }
            let (s, t2, view, m) = (s.clone(), t.clone(), view.downgrade(), m.downgrade());
            *t.borrow_mut() = Some(glib::timeout_add_local(Duration::from_millis(16), move || {
                let (Some(view), Some(m)) = (view.upgrade(), m.upgrade()) else { return glib::ControlFlow::Break };
                if s.get() == 0.0 || !m.contains_pointer() {
                    t2.borrow_mut().take();
                    return glib::ControlFlow::Break;
                }
                let adj = view.vadjustment();
                adj.set_value((adj.value() + s.get()).clamp(adj.lower(), adj.upper() - adj.page_size()));
                glib::ControlFlow::Continue
            }));
        });
        motion.connect_leave(move |_| speed.set(0.0));
        page.add_controller(motion);
    }

    /// Above the queue: the last songs played before this one, the newest
    /// nearest to it.
    fn render_recent(self: &Rc<Self>) {
        for r in self.recent_rows.borrow_mut().drain(..) {
            self.recent_group.remove(&r);
        }
        let now = self.current.borrow().as_ref().map(|t| t.key());
        let mut before: Vec<Track> = crate::session::recent(&self.history.borrow()).into_iter().filter(|t| Some(t.key()) != now).take(RECENT_ROWS).collect();
        before.reverse();
        self.recent_group.set_visible(!before.is_empty());
        for t in &before {
            let row = self.track_row(t, Place::Again(t.clone()));
            row.add_css_class("played-row");
            self.recent_group.add(&row);
            self.recent_rows.borrow_mut().push(row);
        }
    }

    /// A song's radio: that song, then about 50 like it.
    fn start_radio(self: &Rc<Self>, t: &Track) {
        self.toast(&format!("Starting “{}” radio…", t.title));
        let me = Rc::downgrade(self);
        let seed = t.clone();
        let s = seed.clone();
        bg(move || mix::radio(&s), move |list| {
            let Some(me) = me.upgrade() else { return };
            if list.is_empty() {
                me.toast(&format!("YouTube Music has no radio for “{}”", seed.title));
                return;
            }
            let mut queue = vec![seed.clone()];
            queue.extend(list.into_iter().filter(|x| x.key() != seed.key() && !x.same_song(&seed) && !often_skipped(x)));
            let n = queue.len() - 1;
            if me.current.borrow().as_ref().is_some_and(|c| c.key() == seed.key()) {
                *me.queue.borrow_mut() = queue; // it's playing already: just what comes after
                me.index.set(0);
                me.queue_changed();
                me.prefetch_next();
            } else {
                me.play_list(queue, 0);
            }
            me.queue_button.set_active(true);
            me.toast(&format!("“{}” radio: {n} songs like it", seed.title));
        });
    }

    // ------------------------------------------------------------------ liked

    fn is_liked(&self, key: &str) -> bool {
        self.liked.borrow().iter().any(|t| t.key() == key)
    }

    fn heart_button(self: &Rc<Self>, t: &Track) -> gtk::Button {
        let b = gtk::Button::builder().valign(gtk::Align::Center).css_classes(["flat", "circular"]).build();
        self.paint_heart(&b, self.is_liked(&t.key()));
        self.hearts.borrow_mut().push((t.key(), b.downgrade()));
        let me = Rc::downgrade(self);
        let t = t.clone();
        b.connect_clicked(move |_| {
            if let Some(me) = me.upgrade() {
                me.toggle_like(&t);
            }
        });
        b
    }

    fn paint_heart(&self, b: &gtk::Button, liked: bool) {
        b.set_icon_name(if liked { "heart-filled-symbolic" } else { "heart-outline-symbolic" });
        if liked {
            b.add_css_class("liked");
        } else {
            b.remove_css_class("liked");
        }
        b.set_tooltip_text(Some(if liked { "Liked. Click to remove from Liked" } else { "Like" }));
    }

    fn refresh_hearts(&self) {
        self.hearts.borrow_mut().retain(|(key, w)| match w.upgrade() {
            Some(b) => {
                self.paint_heart(&b, self.is_liked(key));
                true
            }
            None => false,
        });
        let key = self.current.borrow().as_ref().map(|t| t.key());
        self.paint_heart(&self.bar_heart, key.is_some_and(|k| self.is_liked(&k)));
    }

    fn toggle_like(self: &Rc<Self>, t: &Track) {
        let key = t.key();
        let found = self.liked.borrow().iter().position(|x| x.key() == key);
        if let Some(i) = found {
            let gone = self.liked.borrow_mut().remove(i);
            library::store(&self.liked.borrow());
            if gone.file.is_some() {
                self.toast(&format!("Removed “{}” from Liked; its saved copy is in the Trash", gone.title));
            } else {
                self.toast(&format!("Removed “{}” from Liked", gone.title));
            }
            bg(move || library::unsave(&gone), |_| {});
        } else {
            let mut t = t.clone();
            t.file = None;
            self.liked.borrow_mut().insert(0, t.clone());
            library::store(&self.liked.borrow());
            if library::saves(&t) {
                self.save_liked(t);
            } else {
                self.toast(&format!("Liked “{}”", t.title));
            }
        }
        self.refresh_hearts();
        self.render_liked();
        self.refresh_mix_later();
    }

    /// Liked: saved into ~/Music/Radiance Music in the background.
    fn save_liked(self: &Rc<Self>, t: Track) {
        let key = t.key();
        if !self.saving.borrow_mut().insert(key.clone()) {
            return;
        }
        self.render_liked();
        let me = Rc::downgrade(self);
        let title = t.title.clone();
        let mut done = t.clone();
        bg(move || library::save(&t), move |result| {
            let Some(me) = me.upgrade() else { return };
            me.saving.borrow_mut().remove(&key);
            match result {
                Ok(path) => {
                    let kept = {
                        let mut liked = me.liked.borrow_mut();
                        liked.iter_mut().find(|x| x.key() == key).map(|x| x.file = Some(path.clone())).is_some()
                    };
                    if kept {
                        library::store(&me.liked.borrow());
                        me.toast(&format!("Saved “{title}”"));
                        let saved = me.liked.borrow().iter().find(|x| x.key() == key).cloned();
                        if let Some(t) = saved {
                            me.request_split(&t); // liked songs get split too
                        }
                        me.save_extras(Duration::from_secs(10)); // and keep their video (your own build)
                    } else {
                        done.file = Some(path); // removed from Liked while it was saving
                        library::unsave(&done);
                    }
                }
                Err(e) => {
                    debug(&format!("save {title}: {e}"));
                    me.toast(&format!("Couldn't save “{title}” right now. It tries again next time the app opens."));
                }
            }
            me.render_liked();
        });
    }

    fn render_liked(self: &Rc<Self>) {
        crate::watch::doing("listing Liked Songs");
        self.liked_gen.set(self.liked_gen.get().wrapping_add(1));
        for r in self.liked_rows.borrow_mut().drain(..) {
            self.liked_group.remove(&r);
        }
        // always A to Z by title (the rail beside it jumps to a letter)
        let mut sorted = self.liked.borrow().clone();
        sorted.sort_by_cached_key(|t| file_key(&t.title));
        let liked = Rc::new(sorted);
        for (i, t) in liked.iter().enumerate() {
            let row = self.liked_row(t, liked.clone(), i);
            self.liked_group.add(&row);
            self.liked_rows.borrow_mut().push(row.upcast());
        }
        *self.liked_shown.borrow_mut() = liked;
        self.filter_liked();
        self.show_missing();
        self.render_library();
        self.render_home_later();
        self.fill_genres();
    }

    /// Liked Songs as the splash's work (the start): its rows a few a frame,
    /// then what goes with them, each in a frame of its own.
    fn render_liked_behind(self: &Rc<Self>, splash: &splash::Splash) {
        const ROWS_A_FRAME: usize = 12;
        let gen = self.liked_gen.get().wrapping_add(1);
        self.liked_gen.set(gen);
        for r in self.liked_rows.borrow_mut().drain(..) {
            self.liked_group.remove(&r);
        }
        let mut sorted = self.liked.borrow().clone();
        sorted.sort_by_cached_key(|t| file_key(&t.title));
        let liked = Rc::new(sorted);
        // (render_liked meanwhile: it lists them all itself, this stops)
        let ours = move |me: &Rc<Ui>| me.liked_gen.get() == gen;
        for from in (0..liked.len()).step_by(ROWS_A_FRAME) {
            let (me, liked) = (Rc::downgrade(self), liked.clone());
            splash.then("Liked Songs rows", move || {
                let Some(me) = me.upgrade().filter(ours) else { return };
                crate::watch::doing("listing Liked Songs");
                for i in from..(from + ROWS_A_FRAME).min(liked.len()) {
                    let row = me.liked_row(&liked[i], liked.clone(), i);
                    me.liked_group.add(&row);
                    me.liked_rows.borrow_mut().push(row.upcast());
                }
            });
        }
        let (me, all) = (Rc::downgrade(self), liked.clone());
        splash.then("Liked Songs filter", move || {
            if let Some(me) = me.upgrade().filter(ours) {
                *me.liked_shown.borrow_mut() = all;
                me.filter_liked();
            }
        });
        let me = Rc::downgrade(self);
        splash.then("missing songs", move || {
            if let Some(me) = me.upgrade().filter(ours) {
                me.show_missing();
            }
        });
        self.render_library_behind(splash);
        let me = Rc::downgrade(self);
        splash.then("genres", move || {
            if let Some(me) = me.upgrade().filter(ours) {
                me.fill_genres();
            }
        });
    }

    /// Liked songs not on this computer: (their song file, their music video).
    fn missing(&self) -> (usize, usize) {
        let liked = self.liked.borrow();
        let songs = liked.iter().filter(|t| library::saves(t) && t.file.as_ref().is_none_or(|f| !Path::new(f).exists())).count();
        let videos = if cfg!(feature = "save-youtube") {
            let have = crate::db::songs_with_video(); // a saved video, or known to have none
            liked.iter().filter(|t| t.source == Source::YouTube && !t.video && !have.contains(&t.key())).count()
        } else {
            0
        };
        (songs, videos)
    }

    /// The "Fix missing" button: shown with how many are missing, while any are.
    fn show_missing(&self) {
        let (songs, videos) = self.missing();
        let n = songs.max(videos);
        self.fix_button.set_visible(n > 0 && std::env::var_os("RADIANCE_SHOT").is_none());
        let busy = !self.saving.borrow().is_empty() || self.extras_running.get();
        self.fix_button.set_label(&if busy { format!("Fixing… {n} left") } else { format!("Fix missing ({n})") });
        self.fix_button.set_sensitive(!busy);
    }

    /// Saves what's missing: the songs first (one at a time), then each one's
    /// music video, lyrics file and cover (the background saver, at once).
    fn fix_missing(self: &Rc<Self>) {
        let (songs, videos) = self.missing();
        let todo: Vec<Track> = self.liked.borrow().iter().filter(|t| library::saves(t) && t.file.as_ref().is_none_or(|f| !Path::new(f).exists())).cloned().collect();
        todo.into_iter().for_each(|t| self.save_liked(t));
        self.save_extras(Duration::from_secs(1));
        crate::log::note(&format!("fix missing: {songs} songs, {videos} videos"));
        self.toast(&match (songs, videos) {
            (0, v) => format!("Saving {v} music videos in the background"),
            (s, 0) => format!("Saving {s} songs in the background"),
            (s, v) => format!("Saving {s} songs and {v} music videos in the background"),
        });
        self.show_missing();
    }

    /// Lists only the liked songs that match Liked Songs' search (all when
    /// it's empty), and puts the A to Z rail and the count in step.
    fn filter_liked(&self) {
        let q = self.liked_search.text().trim().to_string();
        let shown = self.liked_shown.borrow().clone();
        let rows = self.liked_rows.borrow().clone();
        let mut letters: Vec<(char, gtk::Widget)> = vec![];
        let mut found = 0;
        for (row, t) in rows.iter().zip(shown.iter()) {
            let on = q.is_empty() || matches(t, &q);
            row.set_visible(on);
            if !on {
                continue;
            }
            found += 1;
            let letter = letter_of(&t.title);
            if letters.last().is_none_or(|(l, _)| *l != letter) {
                letters.push((letter, row.clone()));
            }
        }
        let dir = library::music_dir();
        self.liked_group.set_description(Some(&if shown.is_empty() {
            "Like a song (the heart) and it's kept here. Lossless songs are saved with their lyrics and album art.".to_string()
        } else if q.is_empty() {
            format!("{} songs, saved in {}", shown.len(), dir.display())
        } else if found == 0 {
            format!("None of your {} liked songs match “{q}”. Search finds it online.", shown.len())
        } else {
            format!("{found} of your {} liked songs {} “{q}”", shown.len(), if found == 1 { "matches" } else { "match" })
        }));
        // letters with no songs (listed) stay faint
        let mut child = self.az_rail.first_child();
        while let Some(c) = child {
            let has = c.downcast_ref::<gtk::Label>().is_some_and(|l| letters.iter().any(|(x, _)| l.label().starts_with(*x)));
            if has {
                c.remove_css_class("empty");
            } else {
                c.add_css_class("empty");
            }
            child = c.next_sibling();
        }
        *self.liked_letters.borrow_mut() = letters;
    }

    /// The liked songs as listed now: A to Z, only those that match its search.
    fn liked_listed(&self) -> Vec<Track> {
        let q = self.liked_search.text().trim().to_string();
        self.liked_shown.borrow().iter().filter(|t| q.is_empty() || matches(t, &q)).cloned().collect()
    }

    /// The A to Z rail: click a letter, or drag along it, to jump there.
    fn az_rail_for(self: &Rc<Self>, scroll: &gtk::ScrolledWindow) -> gtk::Box {
        for c in AZ.chars() {
            self.az_rail.append(&gtk::Label::builder().label(c.to_string()).css_classes(["empty"]).build());
        }
        // as the list scrolls, the rail lights the letter of the songs at the top
        let (me, sc) = (Rc::downgrade(self), scroll.downgrade());
        scroll.vadjustment().connect_value_changed(move |_| {
            if let (Some(me), Some(sc)) = (me.upgrade(), sc.upgrade()) {
                me.light_letter_in_view(&sc);
            }
        });
        // and from the start, and when the list changes (a like, a new size)
        let (me, sc) = (Rc::downgrade(self), scroll.downgrade());
        scroll.vadjustment().connect_changed(move |_| {
            if let (Some(me), Some(sc)) = (me.upgrade(), sc.upgrade()) {
                me.light_letter_in_view(&sc);
            }
        });
        let drag = gtk::GestureDrag::new();
        let (me, sc) = (Rc::downgrade(self), scroll.downgrade());
        drag.connect_drag_begin(move |_, _, y| {
            if let (Some(me), Some(sc)) = (me.upgrade(), sc.upgrade()) {
                me.jump_to_letter(&sc, y);
            }
        });
        let (me, sc) = (Rc::downgrade(self), scroll.downgrade());
        drag.connect_drag_update(move |g, _, dy| {
            if let (Some(me), Some(sc), Some((_, y0))) = (me.upgrade(), sc.upgrade(), g.start_point()) {
                me.jump_to_letter(&sc, y0 + dy);
            }
        });
        self.az_rail.add_controller(drag);
        self.az_rail.set_cursor_from_name(Some("pointer"));
        self.az_rail.clone()
    }

    /// Lights the rail's letter for the songs at the top of the list as it's scrolled.
    fn light_letter_in_view(&self, scroll: &gtk::ScrolledWindow) {
        let Some(view) = scroll.child() else { return };
        let letters = self.liked_letters.borrow();
        // the last letter whose first song has reached the top (a little under it)
        let at_top = letters.iter().filter(|(_, row)| row.compute_bounds(&view).is_some_and(|b| b.y() <= 40.0)).last().or(letters.first());
        let Some((letter, _)) = at_top else { return };
        let mut child = self.az_rail.first_child();
        while let Some(l) = child {
            if l.downcast_ref::<gtk::Label>().is_some_and(|x| x.label().starts_with(*letter)) {
                l.add_css_class("on");
            } else {
                l.remove_css_class("on");
            }
            child = l.next_sibling();
        }
    }

    /// The letter under `y` on the rail; the list scrolls to its first song
    /// (or the next letter that has one).
    fn jump_to_letter(&self, scroll: &gtk::ScrolledWindow, y: f64) {
        let labels: Vec<gtk::Widget> = std::iter::successors(self.az_rail.first_child(), |c| c.next_sibling()).collect();
        let n = labels.len().max(1) as f64;
        let at = ((y / self.az_rail.height().max(1) as f64) * n).floor().clamp(0.0, n - 1.0) as usize;
        let want = AZ.chars().nth(at).unwrap_or('A');
        let rank = |c: char| AZ.find(c).unwrap_or(AZ.len());
        let letters = self.liked_letters.borrow();
        let Some((letter, row)) = letters.iter().find(|(l, _)| rank(*l) >= rank(want)).or(letters.last()) else { return };
        for l in &labels {
            if l.downcast_ref::<gtk::Label>().is_some_and(|l| l.label().starts_with(*letter)) {
                l.add_css_class("on");
            } else {
                l.remove_css_class("on");
            }
        }
        let Some(view) = scroll.child() else { return };
        if let Some(b) = row.compute_bounds(&view) {
            let adj = scroll.vadjustment();
            adj.set_value((adj.value() + b.y() as f64 - 12.0).clamp(adj.lower(), (adj.upper() - adj.page_size()).max(0.0)));
        }
    }

    fn liked_row(self: &Rc<Self>, t: &Track, list: Rc<Vec<Track>>, i: usize) -> adw::ActionRow {
        let row = self.track_row(t, Place::Liked(list, i));
        if self.saving.borrow().contains(&t.key()) {
            row.set_subtitle("Saving…");
        } else if t.file.is_none() {
            row.set_subtitle(&if library::saves(t) { "Not saved yet".to_string() } else { format!("{} · streams", t.byline()) });
        }
        row
    }

    /// The liked songs by album, by each credited artist, or by genre (A to Z).
    fn groups_by(&self, liked: &[Track], view: LikedView) -> Vec<(String, Vec<Track>)> {
        let mut map: std::collections::BTreeMap<String, (String, Vec<Track>)> = Default::default();
        let mut put = |name: &str, t: &Track| {
            let e = map.entry(name.to_lowercase()).or_insert_with(|| (name.to_string(), vec![]));
            if !e.1.iter().any(|x| x.key() == t.key()) {
                e.1.push(t.clone());
            }
        };
        for t in liked {
            match view {
                LikedView::Albums => put(if t.album.is_empty() { "Singles" } else { &t.album }, t),
                LikedView::Artists => t.artists().iter().for_each(|a| put(a, t)),
                LikedView::Genres => match &t.genres {
                    Some(g) if !g.is_empty() => g.iter().for_each(|x| put(x, t)),
                    Some(_) => put("Other", t),
                    None => put("Finding the genre…", t),
                },
            }
        }
        map.into_values().collect()
    }

    /// Looks up the genres of liked songs that don't have them yet, one at a
    /// time in the background (Deezer's public catalogue).
    fn fill_genres(self: &Rc<Self>) {
        if self.genres_busy.get() {
            return;
        }
        let todo: Vec<Track> = self.liked.borrow().iter().filter(|t| t.genres.is_none()).cloned().collect();
        if todo.is_empty() {
            return;
        }
        self.genres_busy.set(true);
        let me = Rc::downgrade(self);
        bg(
            move || {
                todo.iter()
                    .map(|t| {
                        std::thread::sleep(Duration::from_millis(250)); // gently
                        (t.key(), genre::lookup(t))
                    })
                    .collect::<Vec<_>>()
            },
            move |found| {
                let Some(me) = me.upgrade() else { return };
                me.genres_busy.set(false);
                {
                    let mut liked = me.liked.borrow_mut();
                    for (key, g) in found {
                        if let Some(t) = liked.iter_mut().find(|t| t.key() == key) {
                            t.genres = Some(g);
                        }
                    }
                }
                library::store(&me.liked.borrow());
                me.render_library();
                me.home_later_parts(home::HomePart::Genres.bit());
            },
        );
    }

    // ------------------------------------------------------------------ for you

    /// New suggestions a few seconds after the likes change (not per click).
    fn refresh_mix_later(self: &Rc<Self>) {
        if let Some(id) = self.mix_later.borrow_mut().take() {
            id.remove();
        }
        let me = Rc::downgrade(self);
        let id = glib::timeout_add_local_once(Duration::from_secs(4), move || {
            if let Some(me) = me.upgrade() {
                me.mix_later.borrow_mut().take();
                me.refresh_mix();
            }
        });
        *self.mix_later.borrow_mut() = Some(id);
    }

    fn refresh_mix(self: &Rc<Self>) {
        let liked = self.liked.borrow().clone();
        let gen = self.mix_gen.get() + 1;
        self.mix_gen.set(gen);
        if liked.is_empty() {
            self.mix.borrow_mut().clear();
            self.render_mix();
            return;
        }
        self.render_mix();
        let me = Rc::downgrade(self);
        // the seeds: the likes you play most (ties: the latest), and nothing you keep skipping
        let mut seeds = liked.clone();
        seeds.sort_by(|a, b| liking(b).total_cmp(&liking(a)));
        let avoid: HashSet<String> = self.history.borrow().iter().map(|(t, _)| t).chain(liked.iter()).filter(|t| often_skipped(t)).map(|t| t.key()).collect();
        let seeds: Vec<Track> = seeds.into_iter().filter(|t| !avoid.contains(&t.key())).collect();
        bg(move || mix::from_likes(&liked, &seeds, &avoid), move |list| {
            let Some(me) = me.upgrade() else { return };
            if me.mix_gen.get() == gen {
                *me.mix.borrow_mut() = list;
                me.render_mix();
            }
        });
    }

    fn render_mix(self: &Rc<Self>) {
        self.home_later_parts(home::HomePart::Hero.bit() | home::HomePart::Mix.bit()); // the greeting and "Made for you"
    }

    // ------------------------------------------------------------------ playing

    fn play_list(self: &Rc<Self>, queue: Vec<Track>, i: usize) {
        self.note_skip_if_early(); // another song picked mid-song
        self.restored.set(false);
        self.restore_at.set(None);
        if self.shuffle.is_active() && queue.len() > 1 {
            // the song picked first, the rest in a random order
            *self.unshuffled.borrow_mut() = Some(queue.clone());
            let mut rest = queue;
            let first = rest.remove(i.min(rest.len() - 1));
            let mut order = vec![first];
            order.extend(shuffled(rest));
            *self.queue.borrow_mut() = order;
            self.index.set(0);
        } else {
            *self.unshuffled.borrow_mut() = None;
            *self.queue.borrow_mut() = queue;
            self.index.set(i);
        }
        self.play_current();
    }

    /// Shuffle on: what comes after this song, in a random order. Off: the
    /// queue's own order again, carrying on from this song.
    fn shuffle_changed(self: &Rc<Self>) {
        let on = self.shuffle.is_active();
        self.shuffle.set_tooltip_text(Some(if on { "Shuffle: on" } else { "Shuffle" }));
        let cur = self.index.get();
        if on {
            let queue = self.queue.borrow().clone();
            if queue.len() > cur + 2 {
                *self.unshuffled.borrow_mut() = Some(queue.clone());
                let mut order = queue[..=cur].to_vec();
                order.extend(shuffled(queue[cur + 1..].to_vec()));
                *self.queue.borrow_mut() = order;
            }
        } else if let Some(original) = self.unshuffled.borrow_mut().take() {
            let key = self.current.borrow().as_ref().map(|t| t.key());
            let at = key.and_then(|k| original.iter().position(|t| t.key() == k)).unwrap_or(0);
            *self.queue.borrow_mut() = original;
            self.index.set(at);
        }
        self.queue_changed();
        self.prefetch_next();
    }

    fn next_repeat(self: &Rc<Self>) {
        let (mode, icon, tip) = match self.repeat.get() {
            Repeat::Off => (Repeat::All, "media-playlist-repeat-symbolic", "Repeat: the whole queue"),
            Repeat::All => (Repeat::One, "media-playlist-repeat-song-symbolic", "Repeat: this song"),
            Repeat::One => (Repeat::Off, "media-playlist-repeat-symbolic", "Repeat: off"),
        };
        self.repeat.set(mode);
        self.replan();
        self.repeat_button.set_icon_name(icon);
        self.repeat_button.set_tooltip_text(Some(tip));
        self.repeat_button.set_opacity(if mode == Repeat::Off { 0.55 } else { 1.0 });
        if mode == Repeat::Off {
            self.repeat_button.remove_css_class("accent");
        } else {
            self.repeat_button.add_css_class("accent");
        }
        self.set_controls();
    }

    fn play_current(self: &Rc<Self>) {
        crate::watch::doing("starting a song");
        if self.in_party() {
            self.leave_party(false); // a song of your own picked: out of the party
            self.toast("You left the party to play your own music");
        }
        let Some(t) = self.queue.borrow().get(self.index.get()).cloned() else { return };
        // the saved copy when there is one: instant, and works offline
        let t = self
            .liked
            .borrow()
            .iter()
            .find(|x| x.key() == t.key() && x.file.as_ref().is_some_and(|f| Path::new(f).exists()))
            .cloned()
            .unwrap_or(t);
        let gen = self.play_gen.get() + 1;
        self.play_gen.set(gen);
        self.kbps.set(0);
        self.quality_known.set(false);
        *self.current.borrow_mut() = Some(t.clone());
        self.show_current(&t);
        self.load_lyrics(&t, gen);
        self.note_played(&t);
        self.on_half.set(None);
        self.on_stems.set(false);
        self.find_counterpart(&t);
        let have = Self::shown_parts(&t);
        let gains = self.gains_for(&t);
        let mixed = have.iter().any(|p| gains.get(*p).is_some_and(|g| *g < 0.999));
        // split and saved: it plays with its parts at hand (the Mixer is then seamless)
        let at_hand = t.file.is_some() && self.voice.get() == Voice::Song;
        if !have.is_empty() && (mixed || at_hand) && !t.video && self.server_parts_ready(&t) {
            self.update_voice_status();
            self.start_parts(&t, 0.0, false);
            self.render_queue();
            self.prefetch_next();
            return;
        }
        if self.voice.get() != Voice::Song {
            self.split_now(&t);
            if let Some(next) = self.next_track() {
                self.request_split(&self.saved_copy(&next));
            }
        }
        self.update_voice_status();
        self.video_height.set(0);
        self.cover_frame.set_maximum_size(360);
        let needs_lookup = (t.video && t.file.is_none())
            || (t.source == Source::YouTube && t.file.is_none() && self.half_for(&t).is_none() && self.fresh_stream(&t.id).is_none());
        if needs_lookup {
            if let Some(p) = self.player.borrow().as_ref() {
                p.stop(); // not the last song under this one's name while it's found
            }
        }
        if t.video && t.file.is_none() {
            self.play_video(&t, gen);
        } else if let Some((v, uri)) = self.half_for(&t) {
            self.on_half.set(Some(v));
            self.start(&uri, gen);
        } else if let Some(file) = &t.file {
            self.start(&gio::File::for_path(file).uri(), gen);
        } else if t.source == Source::Archive {
            let uri = self.web_uri(&t.id);
            self.start(&uri, gen);
        } else if t.source == Source::Server {
            match self.uri_plain(&t) {
                Some(uri) => self.start(&uri, gen),
                None => self.unavailable(&t.title, "you left its server"),
            }
        } else if let Some((uri, kbps)) = self.fresh_stream(&t.id) {
            self.kbps.set(kbps);
            self.start(&uri, gen);
        } else {
            self.find_stream(&t, Some(gen));
        }
        self.render_queue();
        self.prefetch_next();
        self.autoplay_if_last();
    }

    /// Autoplay: when the queue's last song plays, about 25 songs like it (its
    /// radio) come after it, so the music carries on. Not with Repeat on; never
    /// songs played lately, ones you keep skipping, or ones already queued.
    fn autoplay_if_last(self: &Rc<Self>) {
        if !self.settings.borrow().autoplay || self.repeat.get() != Repeat::Off || std::env::var_os("RADIANCE_SHOT").is_some() {
            return;
        }
        let Some(seed) = self.current.borrow().clone() else { return };
        if self.index.get() + 1 < self.queue.borrow().len() || self.autoplay_for.borrow().as_deref() == Some(seed.key().as_str()) {
            return;
        }
        *self.autoplay_for.borrow_mut() = Some(seed.key());
        let me = Rc::downgrade(self);
        let s = seed.clone();
        bg(move || mix::radio(&s), move |found| {
            let Some(me) = me.upgrade() else { return };
            // still the last song, still that one
            let still = me.current.borrow().as_ref().is_some_and(|c| c.key() == seed.key()) && me.index.get() + 1 >= me.queue.borrow().len();
            if !still || !me.settings.borrow().autoplay {
                return;
            }
            let queued: HashSet<String> = me.queue.borrow().iter().map(|t| t.key()).collect();
            let lately: HashSet<String> = crate::session::recent(&me.history.borrow()).into_iter().take(30).map(|t| t.key()).collect();
            let more: Vec<Track> = found
                .into_iter()
                .filter(|t| !queued.contains(&t.key()) && !lately.contains(&t.key()) && !t.same_song(&seed) && !often_skipped(t))
                .take(25)
                .collect();
            if more.is_empty() {
                return;
            }
            crate::log::note(&format!("autoplay: {} songs like “{}” after it", more.len(), seed.title));
            me.queue.borrow_mut().extend(more);
            me.queue_changed();
            if me.ran_out.replace(false) {
                me.skip(1); // the last song ended while they were on their way: on to them
            } else {
                me.prefetch_next();
            }
        });
    }

    /// A music video: its picture where the cover goes (wider), its sound as usual.
    fn play_video(self: &Rc<Self>, t: &Track, gen: u64) {
        let Some(relay) = self.relay.clone() else { return };
        let me = Rc::downgrade(self);
        let (id, title) = (t.id.clone(), t.title.clone());
        self.stack.set_visible_child_name("playing");
        let saved = crate::db::video_file(&id).map(|f| gio::File::for_path(f).uri().to_string());
        bg(
            move || match saved {
                Some(file) => Ok((file.clone(), 720, file, 0)), // saved: from the file, offline
                None => youtube::video_streams(&id).map(|(v, a)| (relay.add(v.url, v.size, v.mime), v.kbps, relay.add(a.url, a.size, a.mime), a.kbps)),
            },
            move |result| {
                let Some(me) = me.upgrade() else { return };
                if me.play_gen.get() != gen {
                    return;
                }
                let shown = match result {
                    Ok((picture, height, sound, kbps)) => {
                        me.kbps.set(kbps);
                        me.video_height.set(height);
                        me.replan();
                        me.current_uri.replace(sound.clone());
                        me.player.borrow().as_ref().map(|p| p.load_video(&picture, &sound))
                    }
                    Err(e) => Some(Err(e)),
                };
                match shown {
                    Some(Ok(paintable)) => {
                        let pic = gtk::Picture::builder()
                            .paintable(&paintable)
                            .content_fit(gtk::ContentFit::Contain)
                            .can_shrink(true)
                            .overflow(gtk::Overflow::Hidden)
                            .css_classes(["cover"])
                            .build();
                        me.cover_frame.set_maximum_size(640);
                        me.cover.add_child(&pic);
                        me.cover.set_visible_child(&pic);
                        me.set_controls();
                        me.show_full_page();
                    }
                    Some(Err(e)) => me.unavailable(&title, &e),
                    None => {}
                }
            },
        );
    }

    fn fresh_stream(&self, id: &str) -> Option<(String, u32)> {
        let streams = self.streams.borrow();
        let (uri, kbps, when) = streams.get(id)?;
        (when.elapsed() < STREAM_FRESH).then(|| (uri.clone(), *kbps))
    }

    /// Asks yt-dlp for a YouTube song's stream; plays it if `play` is still the song wanted.
    fn find_stream(self: &Rc<Self>, t: &Track, play: Option<u64>) {
        let Some(relay) = self.relay.clone() else {
            self.toast("Can't play YouTube songs: the local relay didn't start");
            return;
        };
        let (id, title) = (t.id.clone(), t.title.clone());
        if play.is_none() && !self.finding.borrow_mut().insert(id.clone()) {
            return; // already being looked up
        }
        let me = Rc::downgrade(self);
        bg(
            {
                let id = id.clone();
                move || youtube::stream(&id).map(|s| (relay.add(s.url, s.size, s.mime), s.kbps))
            },
            move |result| {
                let Some(me) = me.upgrade() else { return };
                me.finding.borrow_mut().remove(&id);
                match result {
                    Ok((uri, kbps)) => {
                        me.streams.borrow_mut().insert(id, (uri.clone(), kbps, Instant::now()));
                        if let Some(gen) = play {
                            me.kbps.set(kbps);
                            me.start(&uri, gen);
                        }
                    }
                    Err(e) if play.is_some_and(|g| g == me.play_gen.get()) => me.unavailable(&title, &e),
                    Err(_) => {}
                }
            },
        );
    }

    /// The next YouTube song's stream, found while this one plays: no wait between songs.
    fn prefetch_next(self: &Rc<Self>) {
        let next = self.queue.borrow().get(self.index.get() + 1).cloned();
        if let Some(t) = next {
            let saved = self.liked.borrow().iter().any(|x| x.key() == t.key() && x.file.is_some());
            if t.source == Source::YouTube && !saved && self.fresh_stream(&t.id).is_none() {
                self.find_stream(&t, None);
            }
        }
    }

    fn start(&self, uri: &str, gen: u64) {
        crate::watch::doing("loading a song into the player");
        if gen != self.play_gen.get() {
            return; // another song was picked meanwhile
        }
        self.replan();
        self.current_uri.replace(uri.to_string());
        if let Some(p) = self.player.borrow().as_ref() {
            p.load(uri);
        }
        self.set_controls();
    }

    fn show_current(self: &Rc<Self>, t: &Track) {
        crate::watch::doing("showing the new song");
        self.bar_title.set_label(&t.title);
        self.bar_byline.set_label(&t.byline());
        self.bar_byline.set_tooltip_text(Some(&t.byline()));
        self.bar_title.set_tooltip_text(Some(&t.title));
        self.np_title.set_label(&t.title);
        let counts = counts_text(t);
        self.np_byline.set_label(&if counts.is_empty() { t.byline() } else { format!("{}\n{counts}", t.byline()) });
        self.np_text.set_opacity(0.0);
        fade(&self.np_text, 1.0, 450);
        self.bar_art.set_icon_name(Some("audio-x-generic-symbolic"));
        let source = art_source(t);
        let weak = self.bar_art.downgrade();
        self.set_art(&source, THUMB, move |tex| {
            if let Some(img) = weak.upgrade() {
                img.set_paintable(Some(tex));
            }
        });
        let weak = self.cover.downgrade();
        self.set_art(&source, COVER, move |tex| {
            let Some(stack) = weak.upgrade() else { return };
            // the new cover fades in over the old one
            let pic = gtk::Picture::builder().paintable(tex).content_fit(gtk::ContentFit::Cover).can_shrink(true).overflow(gtk::Overflow::Hidden).css_classes(["cover"]).build();
            stack.add_child(&pic);
            stack.set_visible_child(&pic);
            let (s, keep) = (stack.downgrade(), pic.downgrade());
            glib::timeout_add_local_once(Duration::from_millis(700), move || {
                let (Some(s), Some(keep)) = (s.upgrade(), keep.upgrade()) else { return };
                if s.visible_child().as_ref() != Some(keep.upcast_ref::<gtk::Widget>()) {
                    return; // a newer cover came in meanwhile: its own turn tidies up
                }
                let mut child = s.first_child();
                while let Some(c) = child {
                    child = c.next_sibling();
                    if c != *keep.upcast_ref::<gtk::Widget>() {
                        s.remove(&c);
                    }
                }
            });
        });
        self.set_backdrop(&source);
        self.set_quality(Some("Loading…"), false);
        self.seek.set_range(0.0, t.seconds.max(1) as f64);
        self.seek.set_value(0.0);
        self.pos.set_label("0:00");
        self.len.set_label(&t.length());
        self.window.set_title(Some(&format!("{} – Radiance Music", t.title)));
        self.refresh_hearts();
        self.set_controls();
        self.show_full_page(); // fullscreen: a song on Now Playing, a video on its own page
    }

    fn set_quality(&self, text: Option<&str>, lossless: bool) {
        for l in [&self.bar_quality, &self.np_quality] {
            l.set_visible(text.is_some());
            l.set_label(text.unwrap_or_default());
            l.remove_css_class(if lossless { "compressed" } else { "lossless" });
            l.add_css_class(if lossless { "lossless" } else { "compressed" });
        }
    }

    /// What's really playing, once sound flows: "Lossless · FLAC · 24-bit · 96 kHz".
    fn update_quality(&self) {
        let Some(t) = self.current.borrow().clone() else { return };
        if self.player.borrow().as_ref().is_some_and(|p| p.hears_parts()) {
            self.set_quality(Some("Mixer · its four parts"), false);
            if let Some(d) = self.player.borrow().as_ref().and_then(|p| p.duration()) {
                self.seek.set_range(0.0, d.max(1.0));
                self.len.set_label(&clock(d));
            }
            self.quality_known.set(true);
            return;
        }
        if self.player.borrow().as_ref().is_some_and(|p| p.is_video()) {
            let h = self.video_height.get();
            let text = if self.kbps.get() > 0 { format!("Video · {h}p · Opus {} kbps", self.kbps.get()) } else { format!("Video · {h}p") };
            self.set_quality(Some(&text), false);
            if let Some(d) = self.player.borrow().as_ref().and_then(|p| p.duration()) {
                self.seek.set_range(0.0, d.max(1.0));
                self.len.set_label(&clock(d));
            }
            self.quality_known.set(true);
            return;
        }
        let Some((rate, bits)) = self.player.borrow().as_ref().and_then(|p| p.output()) else { return };
        let flac = t.lossless() || t.file.as_deref().is_some_and(|f| f.ends_with(".flac"));
        let text = if let Some(v) = self.on_half.get() {
            format!("{} · split · {bits} · {}", v.label(), khz(rate))
        } else if flac {
            if bits.is_empty() { "Lossless · FLAC".to_string() } else { format!("Lossless · FLAC · {bits} · {}", khz(rate)) }
        } else if t.source == Source::Server && t.file.is_none() {
            format!("Compressed · {}", t.format)
        } else if self.kbps.get() > 0 {
            format!("Compressed · Opus · {} kbps", self.kbps.get())
        } else {
            "Compressed · Opus".into()
        };
        self.set_quality(Some(&text), flac);
        if let Some(d) = self.player.borrow().as_ref().and_then(|p| p.duration()) {
            self.seek.set_range(0.0, d.max(1.0));
            self.len.set_label(&clock(d));
        }
        self.quality_known.set(true);
    }

    fn on_player(self: &Rc<Self>, ev: Event) {
        crate::watch::doing("handling a player event");
        let song = self.current.borrow().as_ref().map(|t| t.title.clone()).unwrap_or_default();
        match &ev {
            Event::Ready => {
                let out = self.player.borrow().as_ref().map(|p| p.sound_report()).unwrap_or_default();
                crate::log::note(&format!("playing “{song}”: {out}"));
            }
            Event::Ended => crate::log::note(&format!("ended “{song}”")),
            Event::Error(e) => crate::log::note(&format!("error playing “{song}”: {e}")),
        }
        if self.party_event(&ev) {
            return; // the party's song: its end and its problems are the party's
        }
        match ev {
            Event::Ready => {
                self.fails.set(0);
                if !self.online.get() && self.current.borrow().as_ref().is_some_and(|t| t.file.is_none()) {
                    self.set_online(true, true); // it came from the internet, so that's back
                }
                if let Some(at) = self.restore_at.take() {
                    if let Some(p) = self.player.borrow().as_ref() {
                        p.seek(at);
                    }
                }
                self.update_quality();
                self.set_controls();
            }
            Event::Ended if self.repeat.get() == Repeat::One => self.play_current(),
            Event::Ended => self.skip(1),
            Event::Error(e) => {
                let title = self.current.borrow().as_ref().map(|t| t.title.clone()).unwrap_or_default();
                self.unavailable(&title, &e);
            }
        }
    }

    /// A song that won't play: why, in plain words, and on to the next one. Not when every
    /// song would fail the same way now (offline, yt-dlp missing, YouTube's bot check), nor
    /// after three in a row: then it stops and says so, with Try Again (it skipped song after
    /// song before, saying only "isn't available": 2026-10-31 audit).
    fn unavailable(self: &Rc<Self>, title: &str, why: &str) {
        debug(&format!("unavailable {title}: {why}"));
        crate::log::note(&format!("“{title}” won't play: {why}"));
        self.quality_known.set(true);
        let fails = self.fails.get() + 1;
        self.fails.set(fails);
        // a server's song: its server not answering isn't "you're offline"
        let (reason, every_song) = self.server_reason(why).unwrap_or_else(|| {
            let (r, all) = self.why_not(why);
            (r.to_string(), all)
        });
        if reason == "you're offline" {
            self.set_online(false, true);
        }
        if why.contains("yt-dlp isn't installed") {
            self.cant_work("YouTube songs can't play: yt-dlp, which gets them, isn't installed", "https://github.com/RadianceADM/radiance-music#to-play-music");
        }
        let len = self.queue.borrow().len();
        let more = self.index.get() + 1 < len || self.repeat.get() == Repeat::All;
        if more && !every_song && fails < 3 {
            self.toast(&format!("“{title}” can't play: {reason}. On to the next song."));
            self.skip(1);
            return;
        }
        if let Some(p) = self.player.borrow().as_ref() {
            p.stop();
        }
        self.set_quality(None, false);
        self.set_controls();
        let text = if fails >= 3 && !every_song { format!("{fails} songs in a row wouldn't play: {reason}") } else { format!("“{title}” can't play: {reason}") };
        let toast = adw::Toast::builder().use_markup(false).title(text).button_label("Try Again").timeout(10).build();
        let me = Rc::downgrade(self);
        toast.connect_button_clicked(move |_| {
            if let Some(me) = me.upgrade() {
                me.fails.set(0);
                me.play_current();
            }
        });
        self.toasts.add_toast(toast);
    }

    fn toggle_play(self: &Rc<Self>) {
        crate::watch::doing("play/pause");
        if self.in_party() {
            self.party_play_pause();
            return;
        }
        if self.current.borrow().is_none() {
            return;
        }
        if self.restored.replace(false) {
            self.play_current(); // last time's song, from where it was (see Ready)
            return;
        }
        if let Some(p) = self.player.borrow().as_ref() {
            p.set_playing(!p.playing());
        }
        self.set_controls();
    }

    /// The song playing, left early (before 70 % and with more than 30 s to go): a skip.
    fn note_skip_if_early(&self) {
        let Some(t) = self.current.borrow().clone() else { return };
        let player = self.player.borrow();
        let Some(p) = player.as_ref() else { return };
        let (Some(at), Some(len)) = (p.position(), p.duration()) else { return };
        if len > 0.0 && at > 1.0 && at < 0.7 * len && len - at > 30.0 {
            count_skip(&t);
            crate::log::note(&format!("skipped “{}” at {at:.0} of {len:.0} s", t.title));
        }
    }

    fn skip(self: &Rc<Self>, by: i32) {
        crate::watch::doing("skipping");
        if self.in_party() {
            self.party_skip(by);
            return;
        }
        if by > 0 {
            self.note_skip_if_early();
        }
        let len = self.queue.borrow().len();
        let at = self.player.borrow().as_ref().and_then(|p| p.position()).unwrap_or(0.0);
        if by < 0 && at > 3.0 {
            if let Some(p) = self.player.borrow().as_ref() {
                p.seek(0.0); // back: the start of this song first
            }
            return;
        }
        let mut i = self.index.get() as i64 + by as i64;
        if i as usize >= len && by > 0 && self.repeat.get() == Repeat::All && len > 0 {
            i = 0; // repeat the queue: from the top
        }
        if i < 0 || i as usize >= len {
            if by > 0 {
                if let Some(p) = self.player.borrow().as_ref() {
                    p.stop(); // the end of the list
                }
                self.ran_out.set(true); // Autoplay's songs, if they're on their way, start when they come
                self.set_controls();
            }
            return;
        }
        self.ran_out.set(false);
        self.index.set(i as usize);
        self.play_current();
    }

    fn set_controls(&self) {
        crate::watch::doing("updating the buttons");
        let has = self.current.borrow().is_some();
        let playing = self.player.borrow().as_ref().is_some_and(|p| p.playing());
        self.play.set_sensitive(has);
        self.play.set_icon_name(if playing { "media-playback-pause-symbolic" } else { "media-playback-start-symbolic" });
        self.play.set_tooltip_text(Some(if playing { "Pause" } else { "Play" }));
        let (i, len) = (self.index.get(), self.queue.borrow().len());
        self.prev.set_sensitive(has);
        self.next.set_sensitive(has && (i + 1 < len || self.repeat.get() == Repeat::All));
        // in a party: Next skips for everyone (when you control it); there's no going back
        if let Some((control, song, playing)) = self.party_buttons() {
            // (the party's play / pause, not this computer's short waits to stay in step)
            self.play.set_icon_name(if playing { "media-playback-pause-symbolic" } else { "media-playback-start-symbolic" });
            self.prev.set_sensitive(false);
            self.next.set_sensitive(control && song);
            self.play.set_tooltip_text(Some(match (control, playing) {
                (true, true) => "Pause the party",
                (true, false) => "Play the party",
                (false, true) => "Stop hearing the party here",
                (false, false) => "Hear the party here",
            }));
        }
        self.bar_heart.set_sensitive(has);
        self.seek.set_sensitive(has);
        self.update_drift();
        if let Some(m) = self.mpris.borrow().as_ref() {
            m.changed();
        }
    }

    /// Five times a second: the time, the seek bar, the lyric being sung, the mix.
    fn tick(self: &Rc<Self>) {
        crate::watch::doing("updating the time and seek bar");
        if self.last_save.get().is_none_or(|t| t.elapsed() > Duration::from_secs(5)) && self.player.borrow().as_ref().is_some_and(|p| p.playing()) {
            self.save_session(); // where you are, in case the app closes
        }
        let Some((at, playing)) = self.player.borrow().as_ref().map(|p| (p.position(), p.playing())) else { return };
        let Some(at) = at else { return };
        if !playing {
            return;
        }
        if !self.quality_known.get() {
            self.update_quality(); // in case the "ready" message came before the sound did
        }
        if let Some((was, now)) = self.player.borrow().as_ref().and_then(|p| p.heal_volume()) {
            crate::log::note(&format!("fixed: the song was playing at volume {was:.2} (should be {now:.2}); put back"));
        }
        self.seek.set_value(at);
        self.pos.set_label(&clock(at));
        self.song_at.set(Some((at, Instant::now())));
        self.backdrop.sync(at);
        self.load_pulse(at);
        if !self.in_party() {
            self.transitions(at); // (a party's songs follow the server, never a mix)
        }
    }

    // ------------------------------------------------------------------ lyrics

    /// The old lyrics fade out; the new ones fade in once found (a "Looking for
    /// lyrics…" note meanwhile, if that takes a while).
    fn load_lyrics(self: &Rc<Self>, t: &Track, gen: u64) {
        const OUT: Duration = Duration::from_millis(200);
        self.lyrics_faded.set(Some(Instant::now()));
        fade(&self.lyrics_scroll, 0.0, 180);
        let me = Rc::downgrade(self);
        glib::timeout_add_local_once(OUT, move || {
            if let Some(me) = me.upgrade() {
                if me.play_gen.get() == gen && me.lyrics_found.get() != gen {
                    me.show_lyrics(None);
                    me.lyrics_faded.set(Some(Instant::now()));
                    fade(&me.lyrics_scroll, 1.0, 250);
                }
            }
        });
        let me = Rc::downgrade(self);
        let t = t.clone();
        let asked = Instant::now();
        bg(move || lyrics_for(&t), move |l| {
            let Some(me) = me.upgrade() else { return };
            if me.play_gen.get() != gen {
                return;
            }
            me.lyrics_found.set(gen);
            me.refresh_split_marks(); // its lyrics mark, now that they're kept
            let show = Rc::downgrade(&me);
            glib::timeout_add_local_once(OUT.saturating_sub(asked.elapsed()), move || {
                if let Some(me) = show.upgrade() {
                    if me.play_gen.get() == gen {
                        me.lyrics_scroll.set_opacity(0.0);
                        me.show_lyrics(Some(l));
                        me.lyrics_faded.set(Some(Instant::now()));
                        fade(&me.lyrics_scroll, 1.0, 400);
                    }
                }
            });
        });
    }

    /// The song's lyrics again, in place (no fade): e.g. once its words are timed.
    fn refresh_lyrics(self: &Rc<Self>) {
        let Some(t) = self.current.borrow().clone() else { return };
        let gen = self.play_gen.get();
        let me = Rc::downgrade(self);
        bg(move || lyrics_for(&t), move |l| {
            if let Some(me) = me.upgrade() {
                if me.play_gen.get() == gen {
                    me.show_lyrics(Some(l));
                }
            }
        });
    }

    fn show_lyrics(&self, l: Option<Lyrics>) {
        crate::watch::doing("putting the lyrics up");
        let timed = matches!(l, Some(Lyrics::Timed { .. }));
        for list in [&self.lyrics_list, &self.side_lyrics_list] {
            list.clear();
            list.set_lensing(timed);
        }
        self.lyric_lines.borrow_mut().clear();
        self.lyric_now.borrow_mut().clear();
        self.lyric_lead.set(None);
        let note = |text: &str| {
            for list in [&self.lyrics_list, &self.side_lyrics_list] {
                let label = gtk::Label::builder().label(text).wrap(true).wrap_mode(gtk::pango::WrapMode::WordChar).xalign(0.0).css_classes(["dim-label"]).margin_top(48).margin_start(18).build();
                list.append(&label);
            }
        };
        self.lyrics_offline.set(matches!(l, Some(Lyrics::None)) && !self.online.get());
        match l {
            None => note("Looking for lyrics…"),
            Some(Lyrics::None) if !self.online.get() => note("You're offline: the lyrics come when you're back online."),
            Some(Lyrics::None) => note("No lyrics found for this song."),
            Some(Lyrics::Plain(text)) => {
                for (list, class) in [(&self.lyrics_list, "lyric-plain"), (&self.side_lyrics_list, "side-plain")] {
                    let label = gtk::Label::builder().label(&text).wrap(true).wrap_mode(gtk::pango::WrapMode::WordChar).xalign(0.0).justify(gtk::Justification::Left).css_classes([class]).build();
                    list.append(&label);
                }
            }
            Some(Lyrics::Timed { lines, .. }) => {
                let mut voice: Option<String> = None;
                for mut line in lines {
                    if line.text.trim().is_empty() {
                        line.text = "♪".into(); // a break in the singing
                        line.words.clear();
                    }
                    let make = |class: &[&str]| {
                        gtk::Label::builder().label(&line.text).wrap(true).wrap_mode(gtk::pango::WrapMode::WordChar).xalign(0.0).justify(gtk::Justification::Left).css_classes(class.to_vec()).build()
                    };
                    let (big, side) = (make(&["lyric"]), make(&["lyric", "side-lyric"]));
                    self.lyrics_list.append(&big);
                    self.side_lyrics_list.append(&side);
                    let spans = word_spans(&line.text, &line.words);
                    // a new voice: its name above the line
                    let head = line.singer.clone().filter(|s| voice.as_deref() != Some(s.as_str()));
                    if line.singer.is_some() {
                        voice = line.singer.clone();
                    }
                    let view = LyricView { spans, big, side, sung: Cell::new(None), head, line };
                    if view.head.is_some() {
                        view.big.set_markup(&view.resting_markup());
                        view.side.set_markup(&view.resting_markup());
                    }
                    self.lyric_lines.borrow_mut().push(view);
                }
            }
        }
        self.lyrics_scroll.vadjustment().set_value(0.0);
        self.side_lyrics_scroll.vadjustment().set_value(0.0);
        let song = self.current.borrow().as_ref().map(|t| t.title.clone()).unwrap_or_default();
        crate::log::note(&format!("lyrics up for “{song}”: {} timed lines, {}", self.lyric_lines.borrow().len(), if timed { "lensed" } else { "plain or a note" }));
    }

    /// Scrolls `scroll` so `label` sits in its middle, gliding.
    fn glide_to(scroll: &gtk::ScrolledWindow, label: &gtk::Label) {
        let Some(view) = scroll.child() else { return };
        if let Some(b) = label.compute_bounds(&view) {
            let adj = scroll.vadjustment();
            // where the line settles once it's done growing (the lines above it may be shrinking back)
            let settled = label.parent().and_downcast::<Lens>().and_then(|lens| lens.settled_middle(label).and_then(|y| lens.compute_point(&view, &graphene::Point::new(0.0, y))));
            let mid = adj.value() + settled.map(|p| p.y() as f64).unwrap_or(b.y() as f64 + b.height() as f64 / 2.0);
            let to = (mid - adj.page_size() / 2.0).clamp(adj.lower(), (adj.upper() - adj.page_size()).max(0.0));
            if (to - adj.value()).abs() < 1.0 {
                return;
            }
            let target = adw::PropertyAnimationTarget::new(&adj, "value");
            adw::TimedAnimation::new(scroll, adj.value(), to, 300, target).play();
        }
    }

    /// Your own build (save-youtube): every liked song keeps its official music
    /// video (720p) and its lyrics file next to it. One at a time in the
    /// background, at most 1.5 MB/s; after the app starts and after each like.
    fn save_extras(self: &Rc<Self>, after: Duration) {
        if !cfg!(feature = "save-youtube") || std::env::var_os("RADIANCE_SHOT").is_some() {
            return;
        }
        if self.extras_running.get() {
            self.extras_again.set(true); // once this round is over
            return;
        }
        self.extras_running.set(true);
        let me = Rc::downgrade(self);
        glib::timeout_add_local_once(after, move || {
            let Some(me) = me.upgrade() else { return };
            let have = crate::db::songs_with_video();
            let todo: Vec<Track> = me.liked.borrow().iter().filter(|t| t.file.is_some() && t.source == Source::YouTube).cloned().collect();
            let weak = Rc::downgrade(&me);
            bg(
                move || {
                    for t in &todo {
                        keep_lyrics_file(t);
                        if have.contains(&t.key()) {
                            continue;
                        }
                        if let Err(e) = keep_video(t) {
                            debug(&format!("video of {}: {e}", t.title));
                        }
                    }
                },
                move |_| {
                    let Some(me) = weak.upgrade() else { return };
                    me.extras_running.set(false);
                    me.refresh_split_marks(); // lyrics marks for the files just kept
                    me.show_missing();
                    if me.extras_again.take() {
                        me.save_extras(Duration::from_secs(5));
                    }
                },
            );
        });
    }

    /// Songs split before the server kept a library: their copies go up once,
    /// one after another, a minute after the app starts.
    fn backfill_library(self: &Rc<Self>) {
        if std::env::var_os("RADIANCE_SHOT").is_some() {
            return;
        }
        let me = Rc::downgrade(self);
        glib::timeout_add_local_once(Duration::from_secs(60), move || {
            let Some(me) = me.upgrade() else { return };
            let Some(server) = me.settings.borrow().split.clone() else { return };
            let done = split::hosted();
            let todo: Vec<Track> = me
                .liked
                .borrow()
                .iter()
                .filter(|t| t.file.is_some() && split::has_parts(t) && (!done.contains(&t.key()) || !aligned_by_mms(t)))
                .cloned()
                .collect();
            if todo.is_empty() {
                return;
            }
            debug(&format!("library: {} split songs to keep on the server", todo.len()));
            let me = Rc::downgrade(&me);
            bg(
                move || {
                    for t in &todo {
                        keep_on_server(t, &server, &|| {});
                    }
                },
                move |_| {
                    if let Some(me) = me.upgrade() {
                        me.refresh_lyrics(); // the song playing may have its words timed now
                        me.refresh_split_marks(); // and every row's lyrics mark
                    }
                },
            );
        });
    }

    // ------------------------------------------------------------------ suggestions

    /// Under the search box: before typing, your artists (from your likes);
    /// while typing, your liked songs and artists that match, then what YouTube
    /// Music suggests. A click searches it.
    pub(crate) fn update_suggestions(self: &Rc<Self>) {
        crate::watch::doing("suggesting searches");
        while let Some(c) = self.suggest_flow.first_child() {
            self.suggest_flow.remove(&c);
        }
        let gen = self.suggest_gen.get() + 1;
        self.suggest_gen.set(gen);
        let q = self.search.text().trim().to_lowercase();
        let liked = self.liked.borrow().clone();
        let mut shown: Vec<String> = vec![];
        let mut pill = |me: &Rc<Self>, text: &str, mine: bool| {
            if shown.iter().any(|s| s.eq_ignore_ascii_case(text)) || text.eq_ignore_ascii_case(&q) {
                return;
            }
            shown.push(text.to_string());
            let b = gtk::Button::builder().label(text).css_classes(if mine { vec!["suggest", "liked-suggest"] } else { vec!["suggest"] }).build();
            if mine {
                b.set_tooltip_text(Some("From your liked songs"));
            }
            let (weak, query) = (Rc::downgrade(me), text.to_string());
            b.connect_clicked(move |_| {
                if let Some(me) = weak.upgrade() {
                    me.go_search(&query);
                }
            });
            me.suggest_flow.insert(&b, -1);
        };
        if q.is_empty() {
            // your artists, the most liked first
            let mut count: HashMap<String, (String, usize)> = HashMap::new();
            for t in &liked {
                for a in t.artists() {
                    count.entry(a.to_lowercase()).or_insert_with(|| (a.clone(), 0)).1 += 1;
                }
            }
            let mut top: Vec<(String, usize)> = count.into_values().collect();
            top.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            for (name, _) in top.into_iter().take(10) {
                pill(self, &name, true);
            }
            return;
        }
        for a in liked.iter().flat_map(|t| t.artists()).filter(|a| a.to_lowercase().contains(&q)).take(3) {
            pill(self, &a, true);
        }
        for t in liked.iter().filter(|t| t.title.to_lowercase().contains(&q)).take(3) {
            pill(self, &format!("{} {}", t.title, t.artists().first().cloned().unwrap_or_default()), true);
        }
        let me = Rc::downgrade(self);
        let typed = self.search.text().trim().to_string();
        bg(move || youtube::suggestions(&typed), move |found| {
            let Some(me) = me.upgrade() else { return };
            if me.suggest_gen.get() != gen {
                return; // typed on
            }
            let have: Vec<String> = std::iter::successors(me.suggest_flow.first_child(), |c| c.next_sibling())
                .filter_map(|c| c.first_child().and_then(|b| b.downcast::<gtk::Button>().ok()).and_then(|b| b.label()).map(|l| l.to_lowercase()))
                .collect();
            for s in found.into_iter().filter(|s| !have.contains(&s.to_lowercase())).take(6) {
                let b = gtk::Button::builder().label(&s).css_classes(["suggest"]).build();
                let (weak, query) = (Rc::downgrade(&me), s.clone());
                b.connect_clicked(move |_| {
                    if let Some(me) = weak.upgrade() {
                        me.go_search(&query);
                    }
                });
                me.suggest_flow.insert(&b, -1);
            }
        });
    }

    // ------------------------------------------------------------------ playlists

    /// The sidebar's Playlists: each of yours, and New Playlist.
    pub(crate) fn render_playlist_nav(self: &Rc<Self>) {
        let mut child = self.nav.first_child();
        while let Some(c) = child {
            child = c.next_sibling();
            let name = c.widget_name();
            if name.starts_with("playlist:") || name == "new-playlist" || name == "playlists-head" {
                self.nav.remove(&c);
            }
        }
        let head = gtk::ListBoxRow::builder().selectable(false).activatable(false).name("playlists-head").build();
        head.set_child(Some(&gtk::Label::builder().label("Playlists").xalign(0.0).css_classes(["nav-section"]).build()));
        self.nav.append(&head);
        let row = |icon: &str, label: &str, name: &str| {
            let line = gtk::Box::builder().spacing(12).build();
            line.append(&gtk::Image::from_icon_name(icon));
            line.append(&gtk::Label::builder().label(label).xalign(0.0).ellipsize(gtk::pango::EllipsizeMode::End).build());
            gtk::ListBoxRow::builder().child(&line).name(name).build()
        };
        for (id, name, n) in crate::db::playlists() {
            let r = row("view-list-symbolic", &name, &format!("playlist:{id}"));
            r.set_tooltip_text(Some(&format!("{name}: {n} song{}", if n == 1 { "" } else { "s" })));
            self.nav.append(&r);
        }
        self.nav.append(&row("list-add-symbolic", "New Playlist", "new-playlist"));
    }

    /// Shows a playlist: its songs, Play, Shuffle, Rename, Delete.
    pub(crate) fn open_playlist(self: &Rc<Self>, id: i64) {
        crate::watch::doing("showing a playlist");
        let Some((_, name, _)) = crate::db::playlists().into_iter().find(|p| p.0 == id) else { return };
        self.playlist_now.set(Some(id));
        while let Some(c) = self.playlist_box.first_child() {
            self.playlist_box.remove(&c);
        }
        let songs = Rc::new(crate::db::playlist_songs(id));
        let n = songs.len();
        let head = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(6).build();
        head.append(&gtk::Label::builder().label(&name).xalign(0.0).wrap(true).wrap_mode(gtk::pango::WrapMode::WordChar).css_classes(["page-title"]).build());
        head.append(&gtk::Label::builder().label(format!("{n} song{}", if n == 1 { "" } else { "s" })).xalign(0.0).css_classes(["dim-label"]).build());
        let buttons = gtk::Box::builder().spacing(10).margin_top(8).build();
        let play = gtk::Button::builder().label("Play").css_classes(["suggested-action", "pill-button"]).sensitive(n > 0).build();
        let shuffle = gtk::Button::builder().label("Shuffle").css_classes(["pill-button"]).sensitive(n > 0).build();
        let rename = gtk::Button::builder().icon_name("document-edit-symbolic").tooltip_text("Rename").css_classes(["circular"]).build();
        let delete = gtk::Button::builder().icon_name("user-trash-symbolic").tooltip_text("Delete the playlist").css_classes(["circular"]).build();
        let (me, l) = (Rc::downgrade(self), songs.clone());
        play.connect_clicked(move |_| {
            if let Some(me) = me.upgrade() {
                me.play_list((*l).clone(), 0);
            }
        });
        let (me, l) = (Rc::downgrade(self), songs.clone());
        shuffle.connect_clicked(move |_| {
            if let Some(me) = me.upgrade() {
                me.play_list(shuffled((*l).clone()), 0);
            }
        });
        let (me, current) = (Rc::downgrade(self), name.clone());
        rename.connect_clicked(move |_| {
            if let Some(me) = me.upgrade() {
                me.name_dialog("Rename Playlist", "Rename", &current, move |me, new| {
                    crate::db::rename_playlist(id, new);
                    me.render_playlist_nav();
                    me.open_playlist(id);
                });
            }
        });
        let (me, gone) = (Rc::downgrade(self), name.clone());
        delete.connect_clicked(move |_| {
            let Some(me) = me.upgrade() else { return };
            let ask = adw::AlertDialog::new(Some(&format!("Delete “{gone}”?")), Some("The playlist goes; its songs stay in your library."));
            ask.add_response("cancel", "Cancel");
            ask.add_response("delete", "Delete");
            ask.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
            ask.set_close_response("cancel");
            let weak = Rc::downgrade(&me);
            ask.connect_response(None, move |_, response| {
                if response == "delete" {
                    if let Some(me) = weak.upgrade() {
                        crate::db::delete_playlist(id);
                        me.playlist_now.set(None);
                        me.render_playlist_nav();
                        me.stack.set_visible_child_name("liked");
                    }
                }
            });
            ask.present(Some(&me.window));
        });
        for b in [&play, &shuffle] {
            buttons.append(b);
        }
        buttons.append(&rename);
        buttons.append(&delete);
        head.append(&buttons);
        self.playlist_box.append(&head);
        let group = adw::PreferencesGroup::new();
        if songs.is_empty() {
            group.set_description(Some("No songs in it yet. Add one from any song's menu: Add to Playlist."));
        }
        for (i, t) in songs.iter().enumerate() {
            group.add(&self.track_row(t, Place::Playlist(id, songs.clone(), i)));
        }
        self.playlist_box.append(&group);
        self.stack.set_visible_child_name("playlist");
    }

    /// Asks for a name (a new playlist, a rename); `done` gets it, trimmed.
    fn name_dialog(self: &Rc<Self>, heading: &str, action: &str, start: &str, done: impl Fn(&Rc<Self>, &str) + 'static) {
        let ask = adw::AlertDialog::new(Some(heading), None);
        let entry = gtk::Entry::builder().text(start).activates_default(true).placeholder_text("Playlist name").build();
        ask.set_extra_child(Some(&entry));
        ask.add_response("cancel", "Cancel");
        ask.add_response("ok", action);
        ask.set_response_appearance("ok", adw::ResponseAppearance::Suggested);
        ask.set_default_response(Some("ok"));
        ask.set_close_response("cancel");
        let weak = Rc::downgrade(self);
        let e = entry.clone();
        ask.connect_response(None, move |_, response| {
            let Some(me) = weak.upgrade() else { return };
            let name = e.text().trim().to_string();
            if response == "ok" && !name.is_empty() {
                done(&me, &name);
            }
            me.sync_nav();
        });
        ask.present(Some(&self.window));
        entry.grab_focus();
    }

    /// A new playlist (with `first` in it, from a song's menu).
    pub(crate) fn new_playlist(self: &Rc<Self>, first: Option<Track>) {
        let n = crate::db::playlists().len() + 1;
        self.name_dialog("New Playlist", "Create", &format!("My Playlist {n}"), move |me, name| {
            let Some(id) = crate::db::create_playlist(name) else { return };
            match &first {
                Some(t) => {
                    crate::db::add_to_playlist(id, t);
                    me.toast(&format!("Added to “{name}”"));
                }
                None => me.open_playlist(id),
            }
            me.render_playlist_nav();
        });
    }

    /// The sidebar shows the page that's open.
    /// Speaker EQ, its own app (bass, mid, treble and presets for each speaker).
    fn open_speaker_eq(self: &Rc<Self>) {
        let me = Rc::downgrade(self);
        bg(
            || std::process::Command::new("gtk-launch").arg("io.github.radianceadm.SpeakerEq").status().is_ok_and(|s| s.success()),
            move |ok| {
                if let (false, Some(me)) = (ok, me.upgrade()) {
                    me.toast("Speaker EQ isn't installed. It's its own app, for the whole computer's sound.");
                }
            },
        );
    }

    fn sync_nav(&self) {
        let page = match self.stack.visible_child_name().as_deref() {
            Some("playlist") => self.playlist_now.get().map(|id| format!("playlist:{id}")).unwrap_or_default(),
            Some("server") => self.shown_server().map(|a| format!("server:{a}")).unwrap_or_default(),
            Some("group") => self.group_back.borrow().clone(),
            Some(p) => p.to_string(),
            None => String::new(),
        };
        let mut child = self.nav.first_child();
        while let Some(c) = child {
            if let Some(row) = c.downcast_ref::<gtk::ListBoxRow>() {
                if row.widget_name() == page.as_str() && !row.is_selected() {
                    self.nav.select_row(Some(row));
                }
            }
            child = c.next_sibling();
        }
    }

    /// Songs can be split: on a split server, or on this computer.
    fn can_split(&self) -> bool {
        let s = self.settings.borrow();
        s.split.is_some() || s.local_split.is_some()
    }

    /// Find Lyrics Again (the song's menu, Show Lyrics): see relook_lyrics.
    fn find_lyrics_again(self: &Rc<Self>, t: &Track) {
        let server = Aligner::of(&self.settings.borrow());
        let timed = server.is_some() && split::has_parts(t);
        self.toast(&if timed { format!("Finding “{}”'s lyrics again, then timing every word (a few minutes)…", t.title) } else { format!("Finding “{}”'s lyrics again…", t.title) });
        let (song, me) = (t.clone(), Rc::downgrade(self));
        let key = t.key();
        bg(move || relook_lyrics(&song, server.as_ref()), move |done| {
            let Some(me) = me.upgrade() else { return };
            match done {
                Ok(timed) => me.toast(if timed { "New lyrics, every word timed to the singing" } else { "New lyrics found" }),
                Err(e) => me.toast(&e),
            }
            if me.current.borrow().as_ref().is_some_and(|c| c.key() == key) {
                me.refresh_lyrics();
            }
            me.refresh_split_marks();
        });
    }

    // ------------------------------------------------------------------ the lyrics editor

    /// Create or fix a song's lyrics: each line, when it starts, who sings it.
    fn edit_lyrics(self: &Rc<Self>, t: &Track) {
        let me = Rc::downgrade(self);
        let (song, shown) = (t.clone(), t.clone());
        bg(move || lyrics_for(&song), move |l| {
            if let Some(me) = me.upgrade() {
                me.lyrics_editor(&shown, l);
            }
        });
    }

    fn lyrics_editor(self: &Rc<Self>, t: &Track, l: Lyrics) {
        struct Row {
            row: gtk::ListBoxRow,
            at: Rc<Cell<Option<f64>>>,
            text: gtk::Entry,
            singer: gtk::DropDown,
        }
        let start: Vec<(Option<f64>, String, Option<String>)> = match l {
            Lyrics::Timed { lines, .. } => lines.into_iter().map(|x| (Some(x.start), x.text, x.singer)).collect(),
            Lyrics::Plain(p) => p.lines().map(str::trim).filter(|x| !x.is_empty() && !(x.starts_with('[') && x.ends_with(']'))).map(|x| (None, x.to_string(), None)).collect(),
            Lyrics::None => vec![],
        };
        // who can sing a line: the song's artists, and any added here
        let names = gtk::StringList::new(&["Nobody set"]);
        for a in t.artists() {
            names.append(&a);
        }
        for (_, _, s) in &start {
            if let Some(s) = s {
                if !(0..names.n_items()).any(|i| names.string(i).is_some_and(|x| x == s.as_str())) {
                    names.append(s);
                }
            }
        }
        let rows: Rc<RefCell<Vec<Row>>> = Rc::default();
        let list = gtk::ListBox::builder().selection_mode(gtk::SelectionMode::Single).css_classes(["boxed-list"]).build();
        let clock_of = |at: Option<f64>| at.map(|a| format!("{}:{:05.2}", (a / 60.0) as u32, a % 60.0)).unwrap_or_else(|| "–:––.––".into());
        let me = Rc::downgrade(self);
        let key = t.key();
        // the song's position now (only when it's the one playing)
        let now_at = {
            let me = me.clone();
            let key = key.clone();
            move || -> Option<f64> {
                let me = me.upgrade()?;
                if me.current.borrow().as_ref().map(|c| c.key()) != Some(key.clone()) {
                    return None;
                }
                let p = me.player.borrow();
                p.as_ref().and_then(|p| p.position())
            }
        };
        let add_row = {
            let (rows, list, names) = (rows.clone(), list.clone(), names.clone());
            let now_at = now_at.clone();
            Rc::new(move |at: Option<f64>, text: &str, singer: Option<&str>, after: Option<usize>| {
                let line = gtk::Box::builder().spacing(8).margin_top(4).margin_bottom(4).margin_start(8).margin_end(8).build();
                let time = gtk::Button::builder().label(clock_of(at)).tooltip_text("When the line starts. Click while the song plays to set it to now.").css_classes(["flat", "numeric"]).width_request(92).build();
                let entry = gtk::Entry::builder().text(text).hexpand(true).placeholder_text("The words of this line").build();
                let who = gtk::DropDown::builder().model(&names).tooltip_text("Who sings this line").build();
                let pick = singer.and_then(|s| (0..names.n_items()).find(|&i| names.string(i).is_some_and(|x| x == s))).unwrap_or(0);
                who.set_selected(pick);
                let drop = gtk::Button::builder().icon_name("edit-delete-symbolic").tooltip_text("Remove this line").css_classes(["flat", "circular"]).build();
                line.append(&time);
                line.append(&entry);
                line.append(&who);
                line.append(&drop);
                let row = gtk::ListBoxRow::builder().child(&line).build();
                let cell = Rc::new(Cell::new(at));
                {
                    let (cell, now_at) = (cell.clone(), now_at.clone());
                    time.connect_clicked(move |b| {
                        if let Some(p) = now_at() {
                            cell.set(Some(p));
                            b.set_label(&clock_of(Some(p)));
                        }
                    });
                }
                {
                    let (rows, list, row) = (rows.clone(), list.clone(), row.clone());
                    drop.connect_clicked(move |_| {
                        list.remove(&row);
                        rows.borrow_mut().retain(|r| r.row != row);
                    });
                }
                let place = after.map(|i| i + 1).unwrap_or(rows.borrow().len());
                list.insert(&row, place as i32);
                rows.borrow_mut().insert(place.min(rows.borrow().len()), Row { row, at: cell, text: entry, singer: who });
            })
        };
        for (at, text, singer) in &start {
            add_row(*at, text, singer.as_deref(), None);
        }
        // the tools
        let paste = gtk::Button::builder().label("Paste Lyrics").tooltip_text("Lines from the clipboard (one per line), in place of these").build();
        let add = gtk::Button::builder().label("Add Line").build();
        let singer_name = gtk::Entry::builder().placeholder_text("Add a singer").width_chars(14).build();
        let tap = gtk::ToggleButton::builder().label("Tap Timing").tooltip_text("Play the song and press Enter as each line starts: it gets the time, and the next line is chosen").build();
        let tools = gtk::Box::builder().spacing(8).margin_top(12).margin_bottom(8).margin_start(12).margin_end(12).build();
        for w in [paste.upcast_ref::<gtk::Widget>(), add.upcast_ref(), singer_name.upcast_ref(), tap.upcast_ref()] {
            tools.append(w);
        }
        let hint = gtk::Label::builder()
            .label("Each line: when it starts, its words, who sings it. Saved as yours: never replaced by a lookup. Once the song is split, every word gets its own time.")
            .wrap(true).wrap_mode(gtk::pango::WrapMode::WordChar)
            .xalign(0.0)
            .css_classes(["dim-label", "caption"])
            .margin_start(12)
            .margin_end(12)
            .margin_bottom(8)
            .build();
        {
            let (rows, add_row) = (rows.clone(), add_row.clone());
            let window = self.window.clone();
            paste.connect_clicked(move |_| {
                let (rows, add_row) = (rows.clone(), add_row.clone());
                window.clipboard().read_text_async(gio::Cancellable::NONE, move |text| {
                    let Ok(Some(text)) = text else { return };
                    for r in rows.borrow_mut().drain(..) {
                        if let Some(list) = r.row.parent().and_downcast::<gtk::ListBox>() {
                            list.remove(&r.row);
                        }
                    }
                    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
                        add_row(None, line, None, None);
                    }
                });
            });
        }
        {
            let (rows, list, add_row) = (rows.clone(), list.clone(), add_row.clone());
            add.connect_clicked(move |_| {
                let after = list.selected_row().and_then(|s| rows.borrow().iter().position(|r| r.row == s));
                add_row(None, "", None, after);
            });
        }
        {
            let names = names.clone();
            singer_name.connect_activate(move |e| {
                let name = e.text().trim().to_string();
                if !name.is_empty() {
                    names.append(&name);
                    e.set_text("");
                }
            });
        }
        // tap timing: Enter stamps the chosen line, then the next is chosen
        let keys = gtk::EventControllerKey::new();
        {
            let (rows, list, tap) = (rows.clone(), list.clone(), tap.clone());
            let now_at = now_at.clone();
            keys.connect_key_pressed(move |_, key, _, _| {
                if !tap.is_active() || !matches!(key, gtk::gdk::Key::Return | gtk::gdk::Key::KP_Enter) {
                    return glib::Propagation::Proceed;
                }
                let Some(p) = now_at() else { return glib::Propagation::Proceed };
                let rows = rows.borrow();
                let i = list.selected_row().and_then(|s| rows.iter().position(|r| r.row == s)).unwrap_or(0);
                if let Some(r) = rows.get(i) {
                    r.at.set(Some(p));
                    if let Some(b) = r.row.child().and_then(|c| c.first_child()).and_downcast::<gtk::Button>() {
                        b.set_label(&clock_of(Some(p)));
                    }
                }
                if let Some(next) = rows.get(i + 1) {
                    list.select_row(Some(&next.row));
                    next.row.grab_focus();
                }
                glib::Propagation::Stop
            });
        }
        if let Some(first) = rows.borrow().first() {
            list.select_row(Some(&first.row));
        }
        let save = gtk::Button::builder().label("Save").css_classes(["suggested-action"]).build();
        let header = adw::HeaderBar::new();
        header.pack_end(&save);
        let body = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
        body.append(&tools);
        body.append(&hint);
        let scroll = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&adw::Clamp::builder().maximum_size(900).child(&list).margin_start(12).margin_end(12).margin_bottom(12).build()).build();
        body.append(&scroll);
        let view = adw::ToolbarView::new();
        view.add_top_bar(&header);
        view.set_content(Some(&body));
        let dialog = adw::Dialog::builder().title(format!("Lyrics: {}", t.title)).content_width(820).content_height(760).child(&view).build();
        dialog.add_controller(keys);
        {
            let (rows, names, song, dialog_weak) = (rows.clone(), names.clone(), t.clone(), dialog.downgrade());
            save.connect_clicked(move |_| {
                let Some(me) = me.upgrade() else { return };
                let rows = rows.borrow();
                let picked: Vec<(Option<f64>, String, Option<String>)> = rows
                    .iter()
                    .map(|r| {
                        let who = (r.singer.selected() > 0).then(|| names.string(r.singer.selected()).map(|s| s.to_string())).flatten();
                        (r.at.get(), r.text.text().trim().to_string(), who)
                    })
                    .filter(|(_, text, _)| !text.is_empty())
                    .collect();
                let lyrics = if picked.iter().all(|(at, _, _)| at.is_none()) {
                    Lyrics::Plain(picked.iter().map(|x| x.1.clone()).collect::<Vec<_>>().join("\n"))
                } else {
                    // lines without a time: between their neighbours
                    let mut times: Vec<Option<f64>> = picked.iter().map(|x| x.0).collect();
                    for i in 0..times.len() {
                        if times[i].is_none() {
                            let before = times[..i].iter().rev().flatten().next().copied().unwrap_or(0.0);
                            let after = times[i + 1..].iter().flatten().next().copied().unwrap_or(before + 4.0);
                            times[i] = Some(before + (after - before) / 2.0);
                        }
                    }
                    let lines = picked
                        .iter()
                        .enumerate()
                        .map(|(i, (_, text, who))| {
                            let start = times[i].unwrap_or(0.0);
                            let end = times.get(i + 1).copied().flatten().unwrap_or(start + 5.0).max(start + 0.3);
                            lyrics::Line { start, end, text: text.clone(), words: lyrics::spread(text, start, end), singer: who.clone() }
                        })
                        .collect();
                    Lyrics::Timed { lines, exact: false }
                };
                let Some((_, text)) = lyrics::to_file(&lyrics, Some(&song), &[lyrics::HAND_NOTE]) else { return };
                let path = lyrics_file(&song);
                if let Some(f) = song.file.as_deref() {
                    // the old .lrc, if any, goes to the Trash: the Lyricsfile is the one now
                    let lrc = library::beside(f, "lrc");
                    if lrc.exists() {
                        let _ = gio::File::for_path(&lrc).trash(gio::Cancellable::NONE);
                    }
                }
                if fs::write(&path, text).is_err() {
                    me.toast("Couldn't save the lyrics");
                    return;
                }
                crate::db::set_lyrics_checked(&song.key());
                me.toast("Lyrics saved");
                if me.current.borrow().as_ref().is_some_and(|c| c.key() == song.key()) {
                    me.refresh_lyrics();
                }
                me.refresh_split_marks();
                if let Some(d) = dialog_weak.upgrade() {
                    d.close();
                }
                // split already: every word timed (on the split server or here), singers kept
                let aligner = Aligner::of(&me.settings.borrow());
                if let (Some(aligner), true) = (aligner, split::has_parts(&song)) {
                    let (s2, back) = (song.clone(), Rc::downgrade(&me));
                    bg(move || time_words(&s2, &aligner), move |done| {
                        let Some(me) = back.upgrade() else { return };
                        match done {
                            Ok(()) => {
                                me.toast("Every word of your lyrics is timed now");
                                me.refresh_lyrics();
                                me.refresh_split_marks();
                            }
                            Err(e) => debug(&format!("word times: {e}")),
                        }
                    });
                }
            });
        }
        dialog.present(Some(&self.window));
    }

    // ------------------------------------------------------------------ storage

    /// Where your songs are kept and how much room everything takes; change the folder.
    fn storage_dialog(self: &Rc<Self>) {
        let dialog = adw::PreferencesDialog::builder().title("Storage").content_width(560).build();
        let page = adw::PreferencesPage::new();
        let place = adw::PreferencesGroup::builder().title("Where your songs are kept").build();
        let dir = library::music_dir();
        let folder = adw::ActionRow::builder().title("Folder").subtitle(dir.display().to_string()).subtitle_selectable(true).build();
        let change = gtk::Button::builder().label("Change…").valign(gtk::Align::Center).build();
        folder.add_suffix(&change);
        place.add(&folder);
        let used = adw::PreferencesGroup::builder().title("Space used").description("Counting…").build();
        page.add(&place);
        page.add(&used);
        dialog.add(&page);
        let (me, d, weak_dialog) = (Rc::downgrade(self), dir.clone(), dialog.downgrade());
        change.connect_clicked(move |_| {
            let Some(me) = me.upgrade() else { return };
            let pick = gtk::FileDialog::builder().title("Keep your songs in…").modal(true).build();
            if let Some(parent) = d.parent() {
                pick.set_initial_folder(Some(&gio::File::for_path(parent)));
            }
            let (weak, from, dlg) = (Rc::downgrade(&me), d.clone(), weak_dialog.clone());
            pick.select_folder(Some(&me.window), gio::Cancellable::NONE, move |picked| {
                let (Some(me), Ok(folder)) = (weak.upgrade(), picked) else { return };
                let Some(to) = folder.path() else { return };
                if let Some(dlg) = dlg.upgrade() {
                    dlg.close();
                }
                me.confirm_move(from.clone(), to);
            });
        });
        let weak = used.downgrade();
        bg(move || storage::usage(&dir), move |u| {
            let Some(used) = weak.upgrade() else { return };
            let total = u.songs.bytes + u.videos.bytes + u.parts.bytes + u.extras.bytes;
            used.set_description(Some(&format!("{} in your songs' folder", storage::human(total))));
            let row = |title: &str, c: storage::Count, what: &str| {
                let r = adw::ActionRow::builder().title(title).subtitle(format!("{} {what}", c.files)).build();
                r.add_suffix(&gtk::Label::builder().label(storage::human(c.bytes)).css_classes(["dim-label"]).build());
                r
            };
            used.add(&row("Songs", u.songs, "files"));
            used.add(&row("Music videos", u.videos, "files"));
            used.add(&row("Split parts", u.parts, "files (drums, bass, other, vocals; no vocals)"));
            used.add(&row("Lyrics and covers", u.extras, "files"));
            for (title, sub, bytes) in [("App data", "the database and cache", u.app), ("This computer's splitter", "Python and Demucs", u.splitter), ("Free on this drive", "", u.free)] {
                let r = adw::ActionRow::builder().title(title).build();
                if !sub.is_empty() {
                    r.set_subtitle(sub);
                }
                r.add_suffix(&gtk::Label::builder().label(storage::human(bytes)).css_classes(["dim-label"]).build());
                used.add(&r);
            }
        });
        dialog.present(Some(&self.window));
    }

    /// Asks, then moves the library to `to` and points everything at it.
    fn confirm_move(self: &Rc<Self>, from: std::path::PathBuf, to: std::path::PathBuf) {
        let to = if to.file_name().is_some_and(|n| n == "Radiance Music") { to } else { to.join("Radiance Music") };
        let ask = adw::AlertDialog::new(
            Some("Move your songs?"),
            Some(&format!("Everything in\n{}\ngoes to\n{}\n\nOn the same drive it's instant; to another drive each file is copied, checked, then taken from the old place.", from.display(), to.display())),
        );
        ask.add_response("cancel", "Cancel");
        ask.add_response("move", "Move");
        ask.set_response_appearance("move", adw::ResponseAppearance::Suggested);
        ask.set_close_response("cancel");
        let weak = Rc::downgrade(self);
        ask.connect_response(None, move |_, response| {
            if response != "move" {
                return;
            }
            let Some(me) = weak.upgrade() else { return };
            me.toast("Moving your songs…");
            let (a, b) = (from.clone(), to.clone());
            let back = Rc::downgrade(&me);
            bg(
                move || {
                    let first = storage::move_library(&a, &b, |_, _| {});
                    if first.is_ok() {
                        library::set_music_dir(Some(b.clone()));
                        crate::db::relocate(&a, &b);
                        // anything written meanwhile (a split finishing) follows
                        let _ = storage::move_library(&a, &b, |_, _| {});
                    }
                    first
                },
                {
                    let (to, from) = (to.clone(), from.clone());
                    move |result| {
                    let Some(me) = back.upgrade() else { return };
                    match result {
                        Ok((moved, kept)) => {
                            me.settings.borrow_mut().music_dir = Some(to.display().to_string());
                            settings::save(&me.settings.borrow());
                            me.liked.replace(library::load());
                            me.render_liked();
                            me.toast(&if kept > 0 {
                                format!("Moved {moved} files. {kept} already there were left as they were.")
                            } else {
                                format!("Moved {moved} files to {}", to.display())
                            });
                        }
                        Err(e) => {
                            library::set_music_dir(Some(from.clone()));
                            me.toast(&format!("Couldn't move your songs: {e}"));
                        }
                    }
                }},
            );
        });
        ask.present(Some(&self.window));
    }

    /// Above the art: opens the side panel on its Splits tab (the split queue).
    fn connect_split_queue_button(self: &Rc<Self>) {
        let me = Rc::downgrade(self);
        self.split_queue_btn.connect_clicked(move |_| {
            if let Some(me) = me.upgrade() {
                me.side_tabs.set_active_name(Some("splits"));
                me.queue_button.set_active(true);
            }
        });
    }

    /// The Splits tab: the song being split (with how far), then those waiting, in order.
    fn render_split_queue(self: &Rc<Self>) {
        crate::watch::doing("listing the split queue");
        while let Some(c) = self.split_list.first_child() {
            self.split_list.remove(&c);
        }
        self.split_now_row.replace(None);
        let row = |t: &Track, sub: &str| {
            let r = adw::ActionRow::builder().use_markup(false).title(&t.title).subtitle(sub).title_lines(1).subtitle_lines(1).build();
            let img = gtk::Image::builder().icon_name("audio-x-generic-symbolic").pixel_size(40).overflow(gtk::Overflow::Hidden).css_classes(["art"]).build();
            r.add_prefix(&img);
            let weak = img.downgrade();
            self.set_art(&art_source(t), THUMB, move |tex| {
                if let Some(img) = weak.upgrade() {
                    img.set_paintable(Some(tex));
                }
            });
            r
        };
        let now = self.splitting.borrow().clone();
        if let Some(t) = &now {
            let r = row(t, "Splitting now");
            self.split_list.append(&r);
            self.split_now_row.replace(Some(r));
            self.show_split_progress(); // its status and progress, right away
        }
        let waiting = self.split_queue.borrow().clone();
        for (i, t) in waiting.iter().enumerate() {
            self.split_list.append(&row(t, &format!("Number {} in line · {}", i + 1, t.credit())));
        }
        let away = self.away_rows();
        for (t, where_) in &away {
            self.split_list.append(&row(t, where_));
        }
        if now.is_none() && waiting.is_empty() && away.is_empty() {
            let r = adw::ActionRow::builder().title("Nothing to split").subtitle("Liked songs are split one after another, by themselves.").build();
            self.split_list.append(&r);
        }
    }

    /// Click a lyric line: play from there.
    fn seek_on_click(self: &Rc<Self>, lens: &Lens) {
        let click = gtk::GestureClick::new();
        let me = Rc::downgrade(self);
        let weak = lens.downgrade();
        click.connect_released(move |_, _, _, y| {
            let (Some(me), Some(lens)) = (me.upgrade(), weak.upgrade()) else { return };
            let at = lens.index_at(y).and_then(|i| me.lyric_lines.borrow().get(i).map(|l| l.line.start));
            let Some(at) = at else { return };
            me.cued.replace(None);
            let player = me.player.borrow();
            if let Some(p) = player.as_ref() {
                p.seek(at);
            }
        });
        lens.add_controller(click);
        lens.set_cursor_from_name(Some("pointer"));
    }

    /// Thirty times a second while lyrics are on screen: every line being sung
    /// lit, its words filling in as they're sung, the first of them in the middle.
    fn sing(&self) {
        let started = std::time::Instant::now();
        self.sing_now();
        if std::env::var_os("RADIANCE_PROFILE").is_some() {
            let spent = self.sing_ms.get() + started.elapsed().as_secs_f64() * 1000.0;
            self.sing_ms.set(if self.sing_calls.get() >= 30 { eprintln!("[profile] lyrics clock: {spent:.0} ms per second"); self.sing_calls.set(0); 0.0 } else { self.sing_calls.set(self.sing_calls.get() + 1); spent });
        }
    }

    fn sing_now(&self) {
        crate::watch::doing("lighting the words");
        let lines = self.lyric_lines.borrow();
        let (big, side) = (self.lyrics_scroll.is_mapped(), self.side_lyrics_scroll.is_mapped());
        if lines.is_empty() || !(big || side) {
            return;
        }
        // where the song is: the last reading (5 a second), counted on
        let Some((read, when)) = self.song_at.get() else { return };
        if when.elapsed() > Duration::from_millis(600) || !self.player.borrow().as_ref().is_some_and(|p| p.playing()) {
            return;
        }
        let at = read + when.elapsed().as_secs_f64();
        // lines that have started and not ended; the latest one stays lit until the next starts
        let last = lines.iter().rposition(|v| v.line.start <= at + LINE_LEAD);
        let lit: Vec<usize> = match last {
            Some(l) => (0..=l).filter(|&i| i == l || at + LINE_LEAD < lines[i].line.end).collect(),
            None => vec![],
        };
        let mut now = self.lyric_now.borrow_mut();
        for &i in now.iter().filter(|i| !lit.contains(i)) {
            if let Some(v) = lines.get(i) {
                for label in [&v.big, &v.side] {
                    label.remove_css_class("now");
                    label.set_markup(&v.resting_markup());
                }
                v.sung.set(None);
            }
        }
        for &i in &lit {
            let v = &lines[i];
            let k = v.sung_chars(at);
            if v.sung.get() != Some(k) {
                let markup = format!("{}{}", v.head_markup(), karaoke(&v.line.text, k));
                for label in [&v.big, &v.side] {
                    label.add_css_class("now");
                    label.set_markup(&markup);
                }
                v.sung.set(Some(k));
            }
        }
        *now = lit;
        let lead = now.first().copied();
        if big && self.lyrics_check.get().is_none_or(|t| t.elapsed() > Duration::from_secs(1)) {
            self.lyrics_check.set(Some(Instant::now()));
            self.check_lyrics_shown(lead.and_then(|i| lines.get(i)));
        }
        if lead != self.lyric_lead.get() {
            self.lyric_lead.set(lead);
            self.lyrics_list.set_big(lead); // the line being sung, drawn big
            if let Some(v) = lead.and_then(|i| lines.get(i)) {
                if big {
                    Self::glide_to(&self.lyrics_scroll, &v.big);
                }
                if side {
                    Self::glide_to(&self.side_lyrics_scroll, &v.side);
                }
            }
        }
    }

    /// Now Playing's lyrics stay seen: faded out for good (an animation lost),
    /// or the line being sung out of sight for 2 s, they're put back (and it's
    /// written in the app log, to find out why).
    fn check_lyrics_shown(&self, lead: Option<&LyricView>) {
        let song = || self.current.borrow().as_ref().map(|t| t.title.clone()).unwrap_or_default();
        let op = self.lyrics_scroll.opacity();
        if op < 0.99 && self.lyrics_faded.get().is_none_or(|t| t.elapsed() > Duration::from_secs(2)) {
            crate::log::note(&format!("fixed: Now Playing's lyrics of “{}” were invisible (opacity {op:.2}); shown again", song()));
            self.lyrics_scroll.set_opacity(1.0);
        }
        let Some(v) = lead else {
            self.lyrics_lost.set(0);
            return;
        };
        let h = self.lyrics_scroll.height() as f32;
        let seen = v.big.compute_bounds(&self.lyrics_scroll).is_some_and(|b| b.y() + b.height() > 0.0 && b.y() < h);
        if seen {
            self.lyrics_lost.set(0);
            return;
        }
        self.lyrics_lost.set(self.lyrics_lost.get() + 1);
        if self.lyrics_lost.get() >= 2 {
            let adj = self.lyrics_scroll.vadjustment();
            let at = v.big.compute_bounds(&self.lyrics_scroll).map(|b| format!("{:.0}", b.y())).unwrap_or("nowhere".into());
            crate::log::note(&format!(
                "fixed: the line being sung in “{}” was out of sight in Now Playing (at {at} of {h:.0}; scrolled to {:.0} of {:.0}, {} wide); scrolled back",
                song(),
                adj.value(),
                adj.upper(),
                self.lyrics_scroll.width()
            ));
            self.lyrics_lost.set(0);
            Self::glide_to(&self.lyrics_scroll, &v.big);
        }
    }

    /// The backdrop's beat for the song playing: worked out once per song, a
    /// few seconds in (so a stream's start isn't slowed by it).
    fn load_pulse(self: &Rc<Self>, at: f64) {
        let Some(key) = self.current.borrow().as_ref().map(|t| t.key()) else { return };
        let uri = self.current_uri.borrow().clone();
        if *self.pulse_for.borrow() == key || uri.is_empty() || at < 3.0 {
            return;
        }
        self.pulse_for.replace(key.clone());
        self.backdrop.set_pulse(None);
        let me = Rc::downgrade(self);
        bg(move || beat::pulse(&uri), move |p| {
            let Some(me) = me.upgrade() else { return };
            if *me.pulse_for.borrow() == key {
                me.backdrop.set_pulse(p.map(Rc::new));
            }
        });
    }
}

/// Every part's volume for the player, at full unless its slider says otherwise. Every part,
/// always: Reset empties the sliders' record, and a part left out kept its last volume in the
/// player while its slider showed full (owner, 2026-10-31: "vocals lang natugtog": after Reset
/// and Vocals down, only the vocals played until Drums, Bass and Other were each moved).
/// Four-part songs: Piano and Guitar are inside Other, so they follow it.
fn part_volumes(sliders: &HashMap<String, f64>, six: bool) -> HashMap<String, f64> {
    let mut g: HashMap<String, f64> = split::SIX.iter().map(|p| (p.to_string(), sliders.get(*p).copied().unwrap_or(1.0))).collect();
    if !six {
        let other = g["other"];
        g.insert("piano".into(), other);
        g.insert("guitar".into(), other);
    }
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_part_gets_a_volume() {
        // after Reset (nothing set) and Vocals down: drums, bass and other go at full, said
        let after_reset: HashMap<String, f64> = [("vocals".to_string(), 0.3)].into();
        let g = part_volumes(&after_reset, false);
        for p in ["drums", "bass", "other", "piano", "guitar"] {
            assert_eq!(g[p], 1.0, "{p}");
        }
        assert_eq!(g["vocals"], 0.3);
        // four parts: piano and guitar follow other
        let other_down: HashMap<String, f64> = [("other".to_string(), 0.4), ("piano".to_string(), 0.9)].into();
        assert_eq!(part_volumes(&other_down, false)["piano"], 0.4);
        assert_eq!(part_volumes(&other_down, true)["piano"], 0.9);
    }

    #[test]
    fn words_found_in_their_line() {
        let w = |t: &str| lyrics::Word { start: 0.0, end: 0.0, text: t.into() };
        assert_eq!(word_spans("Stay until the morning", &[w("Stay "), w("until "), w("the "), w("morning")]), vec![(0, 4), (5, 10), (11, 14), (15, 22)]);
        // a word the text doesn't have follows on from the last one
        assert_eq!(word_spans("Oh, là là", &[w("Oh,"), w("xyz"), w("là")]), vec![(0, 3), (3, 6), (7, 9)]);
    }

    #[test]
    fn karaoke_lights_the_sung_part() {
        assert_eq!(karaoke("Tom & Jerry", 5), "Tom &amp;<span fgalpha=\"40%\"> Jerry</span>");
        assert_eq!(karaoke("là là", 2), "là<span fgalpha=\"40%\"> là</span>");
        assert_eq!(karaoke("done", 9), "done");
    }
}

#[cfg(test)]
mod notes_tests {
    use super::release_notes;

    #[test]
    fn whats_new_comes_from_the_changelog() {
        let log = "# Changelog\n\n## v0.2.0 — today\n\n- **Bold** & new: one\n  two\n- three\n\n## v0.1.0 — before\n\n- old\n";
        assert_eq!(release_notes(log, 1), "<p>v0.2.0 — today</p><ul><li><em>Bold</em> &amp; new: one two</li><li>three</li></ul>");
        assert!(release_notes(log, 2).contains("<p>v0.1.0 — before</p><ul><li>old</li></ul>"));
    }
}
