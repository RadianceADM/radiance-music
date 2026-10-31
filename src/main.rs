//! Radiance Music: search any song and play it. Lossless recordings from the free
//! libraries (Internet Archive) come first; YouTube Music is the fallback. A liked
//! song is saved (with its synced lyrics and album art), and the EQ is your
//! Speaker EQ.
//!
//! SPDX-License-Identifier: GPL-3.0-or-later

// Windows: a windowed program (no console window beside it)
#![cfg_attr(windows, windows_subsystem = "windows")]

mod align;
mod archive;
mod art;
mod beat;
mod db;
mod effects;
mod genre;
mod library;
mod log;
mod limiter;
mod lyrics;
mod mix;
#[cfg(unix)]
mod mpris;
// Windows' own media controls, with the same face as Linux's MPRIS
#[cfg(windows)]
#[path = "smtc.rs"]
mod mpris;
mod net;
mod player;
mod relay;
mod session;
mod settings;
mod setup;
mod share;
mod split;
mod storage;
mod track;
mod ui;
mod watch;
mod youtube;

use std::fs;

use adw::prelude::*;
use gtk::{gdk, glib};

pub const APP_ID: &str = "io.github.radianceadm.Music";

const ICONS: [(&str, &str); 5] = [
    ("lossless-symbolic", include_str!("../data/icons/lossless-symbolic.svg")),
    (APP_ID, include_str!("../data/icons/io.github.radianceadm.Music.svg")),
    ("heart-filled-symbolic", include_str!("../data/icons/heart-filled-symbolic.svg")),
    ("heart-outline-symbolic", include_str!("../data/icons/heart-outline-symbolic.svg")),
    ("equalizer-symbolic", include_str!("../data/icons/equalizer-symbolic.svg")),
];

/// The app's own icons (its logo, hearts, EQ), which the system theme doesn't have.
/// The window as it is now, into a PNG (screenshot checks).
fn shoot(w: &adw::ApplicationWindow, path: &str) {
    let (width, height) = (w.width() as f64, w.height() as f64);
    let paintable = gtk::WidgetPaintable::new(Some(w));
    let snap = gtk::Snapshot::new();
    gdk::prelude::PaintableExt::snapshot(&paintable, snap.upcast_ref::<gdk::Snapshot>(), width, height);
    if let (Some(node), Some(renderer)) = (snap.to_node(), w.native().and_then(|n| n.renderer())) {
        let viewport = gtk::graphene::Rect::new(0.0, 0.0, width as f32, height as f32);
        if let Err(e) = renderer.render_texture(&node, Some(&viewport)).save_to_png(path) {
            eprintln!("screenshot: {e}");
        }
    }
}

fn install_icons() {
    let dir = glib::user_data_dir().join("radiance-music/icons");
    let _ = fs::create_dir_all(&dir);
    for (name, svg) in ICONS {
        let path = dir.join(format!("{name}.svg"));
        if fs::read_to_string(&path).ok().as_deref() != Some(svg) {
            let _ = fs::write(&path, svg);
        }
    }
    if let Some(display) = gdk::Display::default() {
        gtk::IconTheme::for_display(&display).add_search_path(&dir);
    }
}

/// `radiance-music --probe QUERY`: searches both sources, then decodes the first
/// lossless and the first YouTube result into nothing (no sound), seeking once,
/// and looks up their lyrics. Checks everything but the speakers.
fn probe(query: &str) -> glib::ExitCode {
    use gst::prelude::*;
    use std::time::{Duration, Instant};
    let found = archive::search(query);
    for t in &found {
        println!("lossless | {} | {} | {} | {} | {}", t.title, t.artist, t.album, t.length(), t.format);
    }
    let yt = youtube::search(query);
    for t in yt.iter().take(5) {
        println!("youtube  | {} | {} | {} | {}", t.title, t.artist, t.album, t.length());
    }
    let relay = relay::Relay::start().expect("relay");
    if gst::init().is_err() {
        return glib::ExitCode::FAILURE;
    }
    let mut tries = vec![];
    if let Some(t) = found.first() {
        // as the app plays it: on Windows through the relay (see relay::add_web)
        tries.push((if cfg!(windows) { relay.add_web(&t.id) } else { t.id.clone() }, t.clone()));
    }
    if let Some(t) = yt.first() {
        let at = Instant::now();
        match youtube::stream(&t.id) {
            Ok(s) => {
                println!("stream: {} kbps, {} bytes, {} ({:.1}s)", s.kbps, s.size, s.mime, at.elapsed().as_secs_f64());
                tries.push((relay.add(s.url, s.size, s.mime), t.clone()));
            }
            Err(e) => println!("stream error: {e}"),
        }
    }
    for (uri, t) in tries {
        let playbin = gst::ElementFactory::make("playbin3").build().unwrap();
        let sink = gst::ElementFactory::make("fakesink").property("sync", true).build().unwrap();
        playbin.set_property("audio-sink", &sink);
        playbin.set_property("uri", &uri);
        let bus = playbin.bus().unwrap();
        let wait = |what: &str, at: Instant| {
            let msg = bus.timed_pop_filtered(gst::ClockTime::from_seconds(30), &[gst::MessageType::AsyncDone, gst::MessageType::Error]);
            match msg.as_ref().map(|m| m.view()) {
                Some(gst::MessageView::AsyncDone(_)) => println!("  {what}: ready in {:.1}s", at.elapsed().as_secs_f64()),
                Some(gst::MessageView::Error(e)) => println!("  {what}: ERROR {}", e.error()),
                _ => println!("  {what}: timed out"),
            }
        };
        println!("{}: {}", t.title, match (t.source, uri.starts_with("http://127.0.0.1")) {
            (track::Source::Archive, true) => "Internet Archive via the relay",
            (track::Source::Archive, false) => "Internet Archive",
            _ => "YouTube via the relay",
        });
        let at = Instant::now();
        let _ = playbin.set_state(gst::State::Playing);
        wait("start", at);
        std::thread::sleep(Duration::from_secs(2));
        let caps = sink.static_pad("sink").and_then(|p| p.current_caps()).map(|c| c.to_string()).unwrap_or_default();
        println!("  output: {caps}");
        let at = Instant::now();
        let _ = playbin.seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE, gst::ClockTime::from_seconds(60));
        wait("seek to 1:00", at);
        std::thread::sleep(Duration::from_secs(2));
        let pos = playbin.query_position::<gst::ClockTime>().map(|p| p.seconds()).unwrap_or(0);
        println!("  position after seek + 2s: {pos}s");
        let _ = playbin.set_state(gst::State::Null);
        match lyrics::find(&t) {
            lyrics::Lyrics::Timed { lines, exact } => println!(
                "  lyrics: {} timed lines ({}), first: {:?}",
                lines.len(),
                if exact { "each word timed" } else { "words spread" },
                lines.iter().find(|x| !x.text.is_empty()).map(|x| &x.text)
            ),
            lyrics::Lyrics::Plain(p) => println!("  lyrics: plain, {} chars", p.len()),
            lyrics::Lyrics::None => println!("  lyrics: none"),
        }
    }
    glib::ExitCode::SUCCESS
}

/// `radiance-music --probe-mix QUERY`: the first two YouTube Music results for
/// QUERY, analysed (tempo, beats), planned, and mixed for real on the two decks,
/// into nothing (no sound). Prints what happens.
/// Silent: plays `a`, then `b` over it, then stops, timing how long each call
/// holds the window's thread (a stop must not wait on the sound server).
fn probe_decks(a: &str, b: &str) -> glib::ExitCode {
    use std::rc::Rc;
    use std::time::Instant;
    std::env::set_var("RADIANCE_SILENT", "1");
    let main = glib::MainLoop::new(None, false);
    let player = Rc::new(player::Player::new(|_| {}).expect("player"));
    let uri = |f: &str| glib::filename_to_uri(f, None).map(|u| u.to_string()).unwrap_or_default();
    let (ua, ub) = (uri(a), uri(b));
    let t = Instant::now();
    player.load(&ua);
    println!("load a: {:.1} ms", t.elapsed().as_secs_f64() * 1000.0);
    let step = Rc::new(std::cell::Cell::new(0u32));
    let (p, m) = (player.clone(), main.clone());
    glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
        step.set(step.get() + 1);
        println!("  {:.1}s: at {:.2}s, playing {}", step.get() as f64 / 2.0, p.position().unwrap_or(0.0), p.playing());
        match step.get() {
            4 => {
                let t = Instant::now();
                p.load(&ub);
                println!("load b over a: {:.1} ms", t.elapsed().as_secs_f64() * 1000.0);
            }
            8 => {
                let t = Instant::now();
                p.stop();
                println!("stop: {:.1} ms", t.elapsed().as_secs_f64() * 1000.0);
            }
            10 => {
                m.quit();
                return glib::ControlFlow::Break;
            }
            _ => {}
        }
        glib::ControlFlow::Continue
    });
    main.run();
    glib::ExitCode::SUCCESS
}

fn probe_mix(query: &str) -> glib::ExitCode {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::Instant;
    std::env::set_var("RADIANCE_SILENT", "1");
    let found = youtube::search(query);
    let (Some(a), Some(b)) = (found.first().cloned(), found.get(1).cloned()) else {
        println!("need two results");
        return glib::ExitCode::FAILURE;
    };
    let relay = relay::Relay::start().expect("relay");
    let uri = |t: &track::Track| youtube::stream(&t.id).map(|s| relay.add(s.url, s.size, s.mime));
    let (Ok(ua), Ok(ub)) = (uri(&a), uri(&b)) else {
        println!("no streams");
        return glib::ExitCode::FAILURE;
    };
    let a_len = a.seconds as f64;
    let t0 = Instant::now();
    let ba = beat::analyse(&ua, (a_len - 45.0).max(0.0), 45.0);
    let bb = beat::analyse(&ub, 0.0, 40.0);
    println!("analysed both in {:.1}s", t0.elapsed().as_secs_f64());
    println!("A {} – {} ({a_len}s): {ba:?}", a.artist, a.title);
    println!("B {} – {}: {bb:?}", b.artist, b.title);
    let plan = beat::plan(ba, bb, a_len, 6.0);
    println!("plan: {plan:?}");
    let main = glib::MainLoop::new(None, false);
    let player = Rc::new(player::Player::new(|_| {}).expect("player"));
    player.load(&ua);
    let step = Rc::new(Cell::new(0u32));
    let started = Rc::new(Cell::new(None::<Instant>));
    let (p, m) = (player.clone(), main.clone());
    glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
        let pos = p.position().unwrap_or(0.0);
        match step.get() {
            0 if p.duration().is_some() => {
                p.seek(plan.at - 4.0);
                step.set(1);
            }
            1 if pos >= plan.at - 3.9 && pos < plan.at => {
                p.cue(&ub, plan.from, plan.rate, plan.beat_matched);
                step.set(2);
            }
            2 if p.cued() && pos >= plan.at => {
                println!("mix starts: A at {pos:.2}s (planned {:.2}s)", plan.at);
                p.mix(plan.seconds, plan.beat_matched);
                started.set(Some(Instant::now()));
                step.set(3);
            }
            3 => {
                let t = started.get().map(|s| s.elapsed().as_secs_f64()).unwrap_or(0.0);
                if (t * 10.0).round() as u64 % 10 == 0 {
                    println!("  {t:4.1}s into the mix: B at {pos:.2}s, mixing {}", p.mixing());
                }
                if t > plan.seconds + 1.5 {
                    p.stop();
                    m.quit();
                    return glib::ControlFlow::Break;
                }
            }
            _ => {}
        }
        glib::ControlFlow::Continue
    });
    main.run();
    glib::ExitCode::SUCCESS
}

/// `radiance-music --probe-split QUERY`: splits the first YouTube Music result
/// on the split server (settings.json), into the cache. No sound.
fn probe_split(query: &str) -> glib::ExitCode {
    let Some(server) = settings::load().split else {
        println!("no split server in settings.json");
        return glib::ExitCode::FAILURE;
    };
    let Some(t) = youtube::search(query).into_iter().next() else { return glib::ExitCode::FAILURE };
    let relay = relay::Relay::start().expect("relay");
    let s = youtube::stream(&t.id).expect("stream");
    let uri = relay.add(s.url, s.size, s.mime);
    let at = std::time::Instant::now();
    println!("splitting {} – {} on {}…", t.artist, t.title, server.host);
    let local = settings::load().local_split;
    let report: split::Progress = std::sync::Arc::new(|p| eprintln!("progress {:.0}%", p * 100.0));
    match split::split(&t, &uri, Some(&server), local.as_ref(), report) {
        Ok(()) => {
            for v in [split::Voice::NoVocals, split::Voice::VocalsOnly] {
                let p = split::ready(&t, v);
                let size = p.as_ref().and_then(|p| std::fs::metadata(p).ok()).map(|m| m.len()).unwrap_or(0);
                println!("{}: {:?} ({size} bytes)", v.label(), p);
            }
            println!("done in {:.0}s", at.elapsed().as_secs_f64());
            glib::ExitCode::SUCCESS
        }
        Err(e) => {
            println!("failed: {e}");
            glib::ExitCode::FAILURE
        }
    }
}

/// `radiance-music --probe-stems DIR`: plays DIR's drums/bass/other/vocals
/// parts through the mixer, into nothing (no sound): starts, changes a part's
/// volume, seeks, and reports.
/// `--probe-align DIR...`: how close word timing from the vocals lands to
/// hand-marked word starts (JamendoLyrics folders: words.csv, lines.csv,
/// split/vocals.ogg or .flac; the first 88 s). Lines get their own start, or one made
/// sloppy like a typical .lrc (0.5 s early to 0.1 s late); each line ends where
/// the next starts. Compared: words spread by length (before) and from the vocals.
fn probe_align(dirs: &[String]) -> glib::ExitCode {
    fn stats(errors: &mut [f64]) -> String {
        if errors.is_empty() {
            return "-".into();
        }
        errors.sort_by(f64::total_cmp);
        let mean = errors.iter().sum::<f64>() / errors.len() as f64;
        let within = errors.iter().filter(|e| **e <= 0.3).count() as f64 / errors.len() as f64 * 100.0;
        format!("mean {mean:.2} s, median {:.2} s, {within:.0}% within 0.3 s", errors[errors.len() / 2])
    }
    let mut totals = [vec![], vec![], vec![], vec![]];
    for (si, dir) in dirs.iter().enumerate() {
        let d = std::path::Path::new(dir);
        let truth: Vec<f64> = fs::read_to_string(d.join("words.csv")).unwrap_or_default().lines().skip(1).filter_map(|l| l.split(',').next()?.parse().ok()).collect();
        let rows: Vec<(f64, String)> = fs::read_to_string(d.join("lines.csv"))
            .unwrap_or_default()
            .lines()
            .skip(1)
            .filter_map(|l| {
                let mut p = l.splitn(3, ',');
                let start: f64 = p.next()?.parse().ok()?;
                let end: f64 = p.next()?.parse().ok()?;
                let text = p.next()?.trim_matches('"').to_string();
                (end < 88.0).then_some((start, text))
            })
            .collect();
        let ogg = d.join("split/vocals.ogg");
        let vocals = gtk::gio::File::for_path(if ogg.exists() { ogg } else { d.join("split/vocals.flac") }).uri().to_string();
        for (case, sloppy) in [(0, false), (1, true)] {
            // a fixed "random" offset per line (the same every run)
            let off = |i: usize| if sloppy { -0.5 + 0.6 * (((si * 7919 + i * 104729) % 1000) as f64 / 1000.0) } else { 0.0 };
            let starts: Vec<f64> = rows.iter().enumerate().map(|(i, r)| (r.0 + off(i)).max(0.0)).collect();
            let mut lines: Vec<lyrics::Line> = rows
                .iter()
                .enumerate()
                .map(|(i, (_, text))| {
                    let (start, end) = (starts[i], starts.get(i + 1).copied().unwrap_or(starts[i] + 4.0));
                    lyrics::Line { start, end, text: text.clone(), words: lyrics::spread(text, start, end), singer: None }
                })
                .collect();
            let n: usize = lines.iter().map(|l| l.words.len()).sum();
            if n > truth.len() {
                println!("{dir}: {n} words in the lines but {} marked; skipped", truth.len());
                break;
            }
            let errors = |lines: &[lyrics::Line]| -> Vec<f64> { lines.iter().flat_map(|l| l.words.iter()).zip(&truth).map(|(w, t)| (w.start - t).abs()).collect() };
            let mut before = errors(&lines);
            let ok = align::words_from_vocals(&mut lines, &vocals);
            let mut after = errors(&lines);
            totals[case * 2].extend(before.iter().copied());
            totals[case * 2 + 1].extend(after.iter().copied());
            let name = d.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            println!("{name} ({n} words, {}): spread: {} | vocals{}: {}", if sloppy { "lrc-like starts" } else { "exact starts" }, stats(&mut before), if ok { "" } else { " (not used)" }, stats(&mut after));
        }
    }
    println!("ALL, exact line starts: spread: {} | vocals: {}", stats(&mut totals[0]), stats(&mut totals[1]));
    println!("ALL, lrc-like starts:   spread: {} | vocals: {}", stats(&mut totals[2]), stats(&mut totals[3]));
    glib::ExitCode::SUCCESS
}

/// Do a song's parts line up with its own file in the player? The file plus
/// its parts turned upside down, mixed as the player mixes them: what's left
/// over (dB under the file alone) is how far apart they are; in step, the
/// parts cancel the file out almost entirely.
fn probe_null(dir: &str) -> glib::ExitCode {
    use gst::prelude::*;
    use std::sync::{Arc, Mutex};
    let uri = |name: &str| ["opus", "flac", "ogg"].iter().map(|e| std::path::Path::new(dir).join(format!("{name}.{e}"))).find(|f| f.exists()).and_then(|f| glib::filename_to_uri(&f, None).ok()).map(|u| u.to_string());
    let Some(whole) = uri("whole") else {
        println!("no whole.opus / whole.flac in {dir}");
        return glib::ExitCode::FAILURE;
    };
    let run = |flip: bool| -> Option<f64> {
        let mut desc = format!("audiomixer name=mix ! audioconvert ! audio/x-raw,format=F32LE,rate=48000,channels=2 ! identity name=meter ! fakesink sync=false uridecodebin caps=audio/x-raw uri=\"{whole}\" ! audioconvert ! audioresample ! audio/x-raw,format=F32LE ! mix.");
        if flip {
            for p in split::PARTS {
                let u = uri(p)?;
                desc += &format!(" uridecodebin caps=audio/x-raw uri=\"{u}\" ! audioconvert ! audioresample ! audio/x-raw,format=F32LE ! audioamplify amplification=-1 ! mix.");
            }
        }
        let pipe = gst::parse::launch(&desc).ok()?.downcast::<gst::Pipeline>().ok()?;
        let sum = Arc::new(Mutex::new((0.0f64, 0usize)));
        let s2 = sum.clone();
        let meter = pipe.by_name("meter")?;
        meter.static_pad("src")?.add_probe(gst::PadProbeType::BUFFER, move |_, info| {
            if let Some(gst::PadProbeData::Buffer(b)) = info.data.as_ref() {
                // only 60 s to 80 s in
                let t = b.pts().map(|p| p.seconds_f64()).unwrap_or(0.0);
                if (60.0..80.0).contains(&t) {
                    if let Ok(map) = b.map_readable() {
                        let mut g = s2.lock().unwrap();
                        for c in map.as_slice().chunks_exact(4) {
                            let x = f32::from_le_bytes([c[0], c[1], c[2], c[3]]) as f64;
                            g.0 += x * x;
                            g.1 += 1;
                        }
                    }
                }
            }
            gst::PadProbeReturn::Ok
        });
        pipe.set_state(gst::State::Playing).ok()?;
        let bus = pipe.bus()?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        while std::time::Instant::now() < deadline {
            let got = sum.lock().unwrap().1;
            if got > 0 && bus.timed_pop_filtered(gst::ClockTime::from_mseconds(100), &[gst::MessageType::Eos, gst::MessageType::Error]).is_some() {
                break;
            }
            // stop once past 80 s
            if pipe.query_position::<gst::ClockTime>().is_some_and(|p| p.seconds_f64() > 81.0) {
                break;
            }
        }
        let _ = pipe.set_state(gst::State::Null);
        let (sq, n) = *sum.lock().unwrap();
        (n > 0).then(|| sq / n as f64)
    };
    gst::init().ok();
    match (run(false), run(true)) {
        (Some(alone), Some(left)) => {
            println!("the song's file minus its parts, 60-80 s in: {:.1} dB under the file alone", 10.0 * (left / alone).log10());
            glib::ExitCode::SUCCESS
        }
        _ => {
            println!("couldn't play them");
            glib::ExitCode::FAILURE
        }
    }
}

/// Crossfades with a song's parts at hand, silently: parts to parts, parts to
/// a plain file, a plain file to parts (DIR holds whole.opus and the parts).
fn probe_xfade(dir: &str) -> glib::ExitCode {
    use std::cell::Cell;
    use std::rc::Rc;
    std::env::set_var("RADIANCE_SILENT", "1");
    let file = |name: &str| ["opus", "ogg"].iter().map(|e| std::path::Path::new(dir).join(format!("{name}.{e}"))).find(|f| f.exists()).and_then(|f| glib::filename_to_uri(&f, None).ok()).map(|u| u.to_string());
    let Some(whole) = file("whole") else { return glib::ExitCode::FAILURE };
    let parts: Vec<(String, String)> = split::PARTS.iter().filter_map(|p| Some((p.to_string(), file(p)?))).collect();
    let gains: std::collections::HashMap<String, f64> = split::PARTS.iter().map(|p| (p.to_string(), 1.0)).collect();
    let main = glib::MainLoop::new(None, false);
    let events = Rc::new(Cell::new((0u32, 0u32)));
    let ev = events.clone();
    let player = Rc::new(player::Player::new(move |e| {
        let (r, n) = ev.get();
        match e {
            player::Event::Ready => ev.set((r + 1, n)),
            player::Event::Ended => ev.set((r, n + 1)),
            player::Event::Error(e) => println!("  ERROR {e}"),
        }
    }).expect("player"));
    if let Err(e) = player.load_parts(Some(&whole), &parts, 100.0, &gains, Some(48_000), false) {
        println!("FAILED: {e}");
        return glib::ExitCode::FAILURE;
    }
    let t = Rc::new(Cell::new(0u32));
    let (p, m, w, pa, g) = (player.clone(), main.clone(), whole.clone(), parts.clone(), gains.clone());
    glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
        t.set(t.get() + 1);
        let pos = p.position().unwrap_or(-1.0);
        let what = if p.is_stems() { if p.hears_parts() { "parts" } else { "its file among its parts" } } else { "a plain file (deck)" };
        println!("  {:>4.1}s: at {pos:6.2}s, {what}, cued {}, mixing {}, events (ready, ended) {:?}", t.get() as f64 / 2.0, p.cued(), p.mixing(), events.get());
        match t.get() {
            2 => p.cue_parts(&w, &pa, 30.0, &g, Some(48_000)),
            5 => p.mix(3.0, false), // parts -> parts: now at about 30 s
            14 => p.cue(&w, 50.0, 1.0, false),
            17 => p.mix(3.0, false), // parts -> deck: about 50 s
            26 => p.cue_parts(&w, &pa, 70.0, &g, Some(48_000)),
            29 => p.mix(3.0, false), // deck -> parts: about 70 s
            30 => {
                p.set_part("vocals", 0.0);
                p.set_whole(false);
            }
            38 => {
                p.stop();
                m.quit();
                return glib::ControlFlow::Break;
            }
            _ => {}
        }
        glib::ControlFlow::Continue
    });
    main.run();
    glib::ExitCode::SUCCESS
}

/// Splits FILE on this computer alone (as Splitting… set it up), then times
/// a few words with MMS: the on-device path, end to end.
fn probe_split_here(file: &str) -> glib::ExitCode {
    let Some(local) = settings::load().local_split else {
        println!("splitting isn't set up on this computer (run --setup-here)");
        return glib::ExitCode::FAILURE;
    };
    let path = std::fs::canonicalize(file).unwrap_or_else(|_| file.into());
    let Ok(uri) = glib::filename_to_uri(&path, None) else { return glib::ExitCode::FAILURE };
    let t = track::Track {
        source: track::Source::Archive,
        id: uri.to_string(),
        title: path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
        artist: "Test".into(),
        album: String::new(),
        seconds: 240,
        art: String::new(),
        format: "FLAC".into(),
        file: Some(path.display().to_string()),
        video: false,
        genres: None,
    };
    println!("splitting {} on this computer, {} cores", path.display(), local.threads);
    let started = std::time::Instant::now();
    let last = std::sync::Arc::new(std::sync::Mutex::new(-10.0f64));
    let report: split::Progress = std::sync::Arc::new(move |p| {
        let mut l = last.lock().unwrap();
        if p - *l >= 0.1 || p >= 1.0 {
            *l = p;
            println!("  {:>3.0} %", p * 100.0);
        }
    });
    if let Err(e) = split::split(&t, &uri, None, Some(&local), report) {
        println!("FAILED: {e}");
        return glib::ExitCode::FAILURE;
    }
    println!("split in {:.0} s: parts {:?}", started.elapsed().as_secs_f64(), split::mix_parts(&t));
    let started = std::time::Instant::now();
    match split::align_here(&t, &local, &serde_json::json!({ "plain": "this is only a test\nof the word timing" })) {
        Ok(y) => println!("words timed in {:.0} s: {} lines of Lyricsfile", started.elapsed().as_secs_f64(), y.lines().count()),
        Err(e) => {
            println!("word timing FAILED: {e}");
            return glib::ExitCode::FAILURE;
        }
    }
    glib::ExitCode::SUCCESS
}

fn probe_stems(dir: &str) -> glib::ExitCode {
    use std::cell::Cell;
    use std::rc::Rc;
    std::env::set_var("RADIANCE_SILENT", "1");
    let parts: Vec<(String, String)> = split::PARTS
        .iter()
        .filter_map(|p| {
            let f = ["wav", "flac", "ogg"].iter().map(|e| std::path::Path::new(dir).join(format!("{p}.{e}"))).find(|f| f.exists())?;
            Some((p.to_string(), glib::filename_to_uri(&f, None).ok()?.to_string()))
        })
        .collect();
    // the song's own file too, when it's there (whole.opus / whole.flac): played among its parts
    let whole = ["opus", "flac"].iter().map(|e| std::path::Path::new(dir).join(format!("whole.{e}"))).find(|f| f.exists()).and_then(|f| glib::filename_to_uri(&f, None).ok()).map(|u| u.to_string());
    println!("parts: {:?}, the song's own file: {}", parts.iter().map(|p| &p.0).collect::<Vec<_>>(), whole.is_some());
    let main = glib::MainLoop::new(None, false);
    let player = Rc::new(player::Player::new(|_| {}).expect("player"));
    let gains = split::PARTS.iter().map(|p| (p.to_string(), 1.0)).collect(); // all up: the song's own file
    if let Err(e) = player.load_parts(whole.as_deref(), &parts, 0.0, &gains, Some(48_000), false) {
        println!("FAILED: {e}");
        return glib::ExitCode::FAILURE;
    }
    let t = Rc::new(Cell::new(0u32));
    let (p, m) = (player.clone(), main.clone());
    glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
        t.set(t.get() + 1);
        let pos = p.position().unwrap_or(0.0);
        let (makeup, peak, squeeze) = p.stems_level().unwrap_or((0.0, 0.0, 1.0));
        let db = |x: f64| 20.0 * x.max(1e-9).log10();
        println!(
            "  {:.1}s in: at {pos:.2}s of {:.1}s, heard: {}, made up {:+.1} dB, loudest out {peak:.3}, limiter down {:.1} dB at most",
            t.get() as f64 / 2.0,
            p.duration().unwrap_or(0.0),
            if p.hears_parts() { "the parts" } else { "the song's own file" },
            db(makeup),
            db(squeeze)
        );
        match t.get() {
            2 => p.seek(60.0),
            8 => {
                p.set_part("drums", 0.0);
                p.set_whole(false);
            }
            16 => {
                p.set_part("drums", 1.0);
                p.set_whole(true);
            }
            22 => {
                p.stop();
                m.quit();
                return glib::ControlFlow::Break;
            }
            _ => {}
        }
        glib::ControlFlow::Continue
    });
    main.run();
    glib::ExitCode::SUCCESS
}

/// `radiance-music --probe-mixer SONG OUT.wav [PART=GAIN…] [at=S] [after=S]`: plays SONG's own
/// file and its four parts (beside it, as the app keeps them) through the Mixer into OUT.wav,
/// never to the speakers: every part up for `after` seconds (the song's own file heard), then
/// the gains given, as the sliders do (set_part, then set_whole). For finding Mixer bugs.
fn probe_mixer(args: &[String]) -> glib::ExitCode {
    use std::cell::Cell;
    use std::rc::Rc;
    let (Some(song), Some(out)) = (args.first(), args.get(1)) else {
        println!("--probe-mixer SONG OUT.wav [drums=0 …] [at=15] [after=4]");
        return glib::ExitCode::FAILURE;
    };
    std::env::set_var("RADIANCE_RECORD", out);
    let base = std::path::Path::new(song).with_extension("");
    let parts: Vec<(String, String)> = split::PARTS
        .iter()
        .filter_map(|p| {
            let f = ["flac", "ogg", "m4a"].iter().map(|e| std::path::PathBuf::from(format!("{}.{p}.{e}", base.display()))).find(|f| f.exists())?;
            Some((p.to_string(), glib::filename_to_uri(&f, None).ok()?.to_string()))
        })
        .collect();
    let num = |k: &str, d: f64| args.iter().find_map(|a| a.strip_prefix(&format!("{k}=")).and_then(|v| v.parse::<f64>().ok())).unwrap_or(d);
    let (at, after, length) = (num("at", 15.0), num("after", 4.0), num("for", 10.0));
    let wanted: Vec<(String, f64)> = split::PARTS.iter().map(|p| (p.to_string(), num(p, 1.0))).collect();
    // next=SONG2: a crossfade into SONG2 (its parts at hand too) `xfade` seconds in, as the app does between songs
    let next = args.iter().find_map(|a| a.strip_prefix("next=")).map(String::from);
    let xfade = num("xfade", 6.0);
    let cue_vocals = args.iter().any(|a| a.starts_with("cue_vocals=")).then(|| num("cue_vocals", 1.0));
    let whole = glib::filename_to_uri(song, None).ok().map(|u| u.to_string());
    println!("parts {:?}; from {at} s; all up for {after} s, then {wanted:?}; {length} s in all", parts.iter().map(|p| &p.0).collect::<Vec<_>>());
    let main = glib::MainLoop::new(None, false);
    let player = Rc::new(player::Player::new(|_| {}).expect("player"));
    let all_up = split::PARTS.iter().map(|p| (p.to_string(), 1.0)).collect();
    let start_gains: std::collections::HashMap<String, f64> = if after > 0.0 { all_up } else { wanted.iter().cloned().collect() };
    if let Err(e) = player.load_parts(whole.as_deref(), &parts, at, &start_gains, Some(48_000), false) {
        println!("FAILED: {e}");
        return glib::ExitCode::FAILURE;
    }
    let ticks = Rc::new(Cell::new(0u32));
    let (p, m) = (player.clone(), main.clone());
    glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
        ticks.set(ticks.get() + 1);
        let t = ticks.get() as f64 / 10.0;
        if after > 0.0 && (t - after).abs() < 0.05 {
            for (name, g) in &wanted {
                p.set_part(name, *g);
            }
            p.set_whole(wanted.iter().all(|(_, g)| *g >= 0.999));
            println!("  {t:.1} s: sliders moved; the parts are heard: {}", p.hears_parts());
        }
        if (t * 10.0).round() as u32 % 10 == 0 {
            let (makeup, peak, squeeze) = p.stems_level().unwrap_or((0.0, 0.0, 1.0));
            println!("  {t:.0} s: at {:.2} s, made up {:+.1} dB, loudest {peak:.3}, limiter {:.1} dB", p.position().unwrap_or(0.0), 20.0 * makeup.max(1e-9).log10(), 20.0 * squeeze.max(1e-9).log10());
        }
        if let Some(n) = &next {
            if (t - 1.0).abs() < 0.05 {
                let base = std::path::Path::new(n).with_extension("");
                let parts: Vec<(String, String)> = split::PARTS
                    .iter()
                    .filter_map(|q| {
                        let f = ["flac", "ogg", "m4a"].iter().map(|e| std::path::PathBuf::from(format!("{}.{q}.{e}", base.display()))).find(|f| f.exists())?;
                        Some((q.to_string(), glib::filename_to_uri(&f, None).ok()?.to_string()))
                    })
                    .collect();
                let mut gains: std::collections::HashMap<String, f64> = wanted.iter().cloned().collect();
                // cue_vocals=G: the next song cued while Vocals was at G (then the slider moved on)
                if let Some(g) = cue_vocals {
                    gains.insert("vocals".into(), g);
                }
                let uri = glib::filename_to_uri(n, None).unwrap().to_string();
                p.cue_parts(&uri, &parts, 0.0, &gains, Some(48_000));
                println!("  {t:.1} s: cued {n} with its parts");
            }
            if (t - xfade).abs() < 0.05 {
                println!("  {t:.1} s: cued and ready: {}; crossfading", p.cued());
                p.retune_cued(&wanted.iter().cloned().collect()); // as the app does, just before
                p.mix(6.0, false);
            }
        }
        if t >= length {
            p.stop();
            m.quit();
            return glib::ControlFlow::Break;
        }
        glib::ControlFlow::Continue
    });
    main.run();
    glib::ExitCode::SUCCESS
}

/// Windows: the app's own folder holds GStreamer's plugins, GTK's settings and its icons
/// (bin\ beside lib\ and share\); say so before anything starts looking.
#[cfg(windows)]
fn windows_paths() {
    let Some(root) = std::env::current_exe().ok().and_then(|e| e.parent()?.parent().map(|p| p.to_path_buf())) else { return };
    let set = |k: &str, v: std::path::PathBuf| {
        if std::env::var_os(k).is_none() {
            std::env::set_var(k, v);
        }
    };
    set("GST_PLUGIN_SYSTEM_PATH_1_0", root.join("lib").join("gstreamer-1.0"));
    set("GST_PLUGIN_SCANNER_1_0", root.join("libexec").join("gstreamer-1.0").join("gst-plugin-scanner.exe"));
    set("GSETTINGS_SCHEMA_DIR", root.join("share").join("glib-2.0").join("schemas"));
    set("XDG_DATA_DIRS", root.join("share"));
    set("GIO_MODULE_DIR", root.join("lib").join("gio").join("modules")); // TLS for https
    // this GnuTLS refuses glib-networking's default "%COMPAT" priority string
    if std::env::var_os("G_TLS_GNUTLS_PRIORITY").is_none() {
        std::env::set_var("G_TLS_GNUTLS_PRIORITY", "NORMAL");
    }
    // without OpenGL (no graphics driver: a virtual machine, Remote Desktop) GTK tried EGL,
    // failed every frame and never showed the window; without EGL it draws in software
    // there, and a PC with a graphics card still draws with OpenGL (WGL)
    if std::env::var_os("GDK_DISABLE").is_none() {
        std::env::set_var("GDK_DISABLE", "egl");
    }
}

fn main() -> glib::ExitCode {
    #[cfg(windows)]
    windows_paths();
    let args: Vec<String> = std::env::args().collect();
    // which version this is (the Linux download's installer also asks it, to see the app's libraries load)
    if args.get(1).map(String::as_str) == Some("--version") {
        println!("Radiance Music {}", env!("CARGO_PKG_VERSION"));
        return glib::ExitCode::SUCCESS;
    }
    if args.get(1).map(String::as_str) == Some("--probe-video") {
        // a liked song's music video, saved as the app does (720p, at most 1.5 MB/s)
        let want = args.get(2).cloned().unwrap_or_default().to_lowercase();
        let Some(t) = library::load().into_iter().find(|t| t.title.to_lowercase().contains(&want) && t.file.is_some()) else {
            println!("no saved liked song with {want:?} in its title");
            return glib::ExitCode::FAILURE;
        };
        let started = std::time::Instant::now();
        match ui::keep_video(&t) {
            Ok(Some(path)) => println!("{}: saved {path} ({} MB) in {:.0} s", t.title, std::fs::metadata(&path).map(|m| m.len() / 1_000_000).unwrap_or(0), started.elapsed().as_secs_f64()),
            Ok(None) => println!("{}: no official video found", t.title),
            Err(e) => println!("{}: {e}", t.title),
        }
        return glib::ExitCode::SUCCESS;
    }
    if args.get(1).map(String::as_str) == Some("--probe-pulse") {
        // the backdrop's beat for a file: how many hits, how strong
        let uri = gtk::gio::File::for_commandline_arg(args.get(2).map(String::as_str).unwrap_or_default()).uri();
        let started = std::time::Instant::now();
        match beat::pulse(&uri) {
            Some(p) => {
                let secs = p.level.len() as f64 / p.fps;
                let hits = p.level.windows(2).filter(|w| w[1] > w[0] + 0.2 && w[1] > 0.5).count();
                let mean = p.level.iter().sum::<f32>() / p.level.len().max(1) as f32;
                println!("{secs:.0} s analysed in {:.1} s: {hits} beats ({:.1} a second), mean level {mean:.2}", started.elapsed().as_secs_f64(), hits as f64 / secs.max(1.0));
            }
            None => println!("no beat (the song couldn't be read)"),
        }
        return glib::ExitCode::SUCCESS;
    }
    if args.get(1).map(String::as_str) == Some("--setup-here") {
        // Splitting… → Set Up, from the command line (shows each step)
        let check = setup::check();
        println!("this computer: {} cores, {:.1} GB memory, {:.1} GB free, {}", check.cores, check.ram_gb, check.free_gb, check.python.as_deref().unwrap_or("no Python 3 with venv"));
        let started = std::time::Instant::now();
        return match setup::install(setup::half_the_cores(), &|what, done| println!("  {:>3.0} % {what}", done * 100.0)) {
            Ok(l) => {
                println!("set up in {:.0} s: {} ({} cores)", started.elapsed().as_secs_f64(), l.python, l.threads);
                glib::ExitCode::SUCCESS
            }
            Err(e) => {
                println!("FAILED: {e}");
                glib::ExitCode::FAILURE
            }
        };
    }
    if args.get(1).map(String::as_str) == Some("--split-here") && args.len() > 2 {
        // a song file split on this computer alone, then a few words timed (MMS)
        return probe_split_here(&args[2]);
    }
    if args.get(1).map(String::as_str) == Some("--find-lyrics-again") {
        // Find Lyrics Again (as the song's menu does) for the liked song whose title is TEXT
        let want = args.get(2).cloned().unwrap_or_default().to_lowercase();
        let Some(t) = library::load().into_iter().find(|t| t.title.to_lowercase() == want) else {
            println!("no liked song titled {want:?}");
            return glib::ExitCode::FAILURE;
        };
        let started = std::time::Instant::now();
        match ui::relook_lyrics(&t, ui::Aligner::of(&settings::load()).as_ref()) {
            Ok(timed) => println!("{} by {}: new lyrics{} in {:.0} s", t.title, t.artist, if timed { ", every word timed" } else { "" }, started.elapsed().as_secs_f64()),
            Err(e) => {
                println!("{}: {e}", t.title);
                return glib::ExitCode::FAILURE;
            }
        }
        return glib::ExitCode::SUCCESS;
    }
    if args.get(1).map(String::as_str) == Some("--probe-host") {
        // the server's library copy of one liked, split song whose title has TEXT
        let want = args.get(2).cloned().unwrap_or_default().to_lowercase();
        let Some(server) = settings::load().split else { return glib::ExitCode::FAILURE };
        let Some(t) = library::load().into_iter().find(|t| t.title.to_lowercase().contains(&want) && split::has_parts(t)) else {
            println!("no liked, split song with {want:?} in its title");
            return glib::ExitCode::FAILURE;
        };
        let started = std::time::Instant::now();
        ui::keep_on_server(&t, &server, &|| println!("timing the words…"));
        println!("{}: kept in {:.1} s, hosted now: {}", t.title, started.elapsed().as_secs_f64(), split::hosted().contains(&t.key()));
        return glib::ExitCode::SUCCESS;
    }
    if args.get(1).map(String::as_str) == Some("--probe-align") {
        return probe_align(&args[2..]);
    }
    if args.get(1).map(String::as_str) == Some("--probe-effects") {
        // read only: the device playing and its effects, as the Mixer shows them (from Speaker EQ)
        match effects::effects() {
            Some(p) => println!(
                "{}: low cut {} Hz, high cut {} Hz, reverb {} %, its EQ chain {}",
                p.name,
                p.sound.low_cut,
                p.sound.high_cut,
                p.sound.reverb,
                if p.running { "running" } else { "not running" }
            ),
            None if effects::available() => println!("the device playing isn't one Speaker EQ filters"),
            None => println!("Speaker EQ isn't installed (no speaker-eq command)"),
        }
        return glib::ExitCode::SUCCESS;
    }
    if args.get(1).map(String::as_str) == Some("--probe-xfade") {
        return probe_xfade(&args[2]);
    }
    if args.get(1).map(String::as_str) == Some("--probe-null") {
        return probe_null(&args[2]);
    }
    if args.get(1).map(String::as_str) == Some("--probe-mixer") {
        return probe_mixer(&args[2..]);
    }
    if args.get(1).map(String::as_str) == Some("--probe-stems") {
        return probe_stems(&args[2]);
    }
    if args.get(1).map(String::as_str) == Some("--probe-split") {
        return probe_split(&args[2..].join(" "));
    }
    if args.get(1).map(String::as_str) == Some("--probe-decks") && args.len() > 3 {
        return probe_decks(&args[2], &args[3]);
    }
    if args.get(1).map(String::as_str) == Some("--probe-mix") {
        return probe_mix(&args[2..].join(" "));
    }
    if args.get(1).map(String::as_str) == Some("--probe") {
        return probe(&args[2..].join(" "));
    }
    // RADIANCE_SHOT=file.png [RADIANCE_SHOT_PAGE=home]: a second copy draws its
    // window to a PNG after a few seconds (RADIANCE_SHOT_WAIT, default 8) and
    // quits (design checks). RADIANCE_SHOT_PLAY=1 plays the last song on, with
    // no sound at all; it never saves where you are and never splits.
    let shot = std::env::var("RADIANCE_SHOT").ok();
    let shot_play = shot.is_some() && std::env::var_os("RADIANCE_SHOT_PLAY").is_some();
    if shot_play {
        std::env::set_var("RADIANCE_SILENT", "1");
    }
    let wait = std::env::var("RADIANCE_SHOT_WAIT").ok().and_then(|s| s.parse().ok()).unwrap_or(8u64).min(18);
    let mut builder = adw::Application::builder().application_id(APP_ID);
    if shot.is_some() {
        builder = builder.flags(gtk::gio::ApplicationFlags::NON_UNIQUE);
    }
    let app = builder.build();
    // always dark (owner: "either stone or true black lang"), whatever the system's light or dark
    // setting is (a light desktop, or Windows, would otherwise get libadwaita's light look)
    app.connect_startup(|_| adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark));
    app.connect_activate(move |app| {
        if let Some(w) = app.active_window() {
            w.present();
            return;
        }
        install_icons();
        youtube::use_sign_in_from(settings::load().youtube_sign_in);
        gtk::Window::set_default_icon_name(APP_ID);
        let ui = ui::Ui::build(app);
        watch::start(&ui.window);
        ui.window.present();
        if let Some(path) = shot.clone() {
            // RADIANCE_SHOT_SIZE=WxH: the window that size (responsive checks)
            if let Some((w, h)) = std::env::var("RADIANCE_SHOT_SIZE").ok().and_then(|s| s.split_once('x').map(|(w, h)| (w.parse().ok(), h.parse().ok()))).and_then(|(w, h)| Some((w?, h?))) {
                ui.window.set_default_size(w, h);
            }
            // RADIANCE_SHOT_MAXIMIZE=1: maximized, as a real window often is
            if std::env::var_os("RADIANCE_SHOT_MAXIMIZE").is_some() {
                ui.window.maximize();
            }
            // it shows on screen for a few seconds: clicks there must do nothing
            if let Some(content) = ui.window.child() {
                content.set_can_target(false);
            }
            if let Ok(page) = std::env::var("RADIANCE_SHOT_PAGE") {
                ui.show_page(&page);
            }
            if let Ok(q) = std::env::var("RADIANCE_SHOT_SEARCH") {
                ui.search_for(&q);
            }
            // RADIANCE_SHOT_MIXER=1: the Mixer opened
            if std::env::var_os("RADIANCE_SHOT_MIXER").is_some() {
                ui.shot_mixer();
            }
            // RADIANCE_SHOT_SPLITTING=1: the Splitting window open
            if std::env::var_os("RADIANCE_SHOT_SPLITTING").is_some() {
                ui.shot_splitting();
            }
            // RADIANCE_SHOT_FIND=text: Liked Songs searched for it
            if let Ok(q) = std::env::var("RADIANCE_SHOT_FIND") {
                ui.find_liked(&q);
            }
            if shot_play {
                ui.shot_play();
            }
            // RADIANCE_SHOT_PANEL=queue|lyrics|splits: the side panel open on that tab
            if let Ok(tab) = std::env::var("RADIANCE_SHOT_PANEL") {
                ui.shot_panel(&tab);
            }
            if std::env::var_os("RADIANCE_PROFILE").is_some() {
                profile_frames(&ui.window);
            }
            // RADIANCE_TEST_STALL=1: the window thread blocks 6 s (checks the freeze recorder)
            if std::env::var_os("RADIANCE_TEST_STALL").is_some() {
                glib::timeout_add_local_once(std::time::Duration::from_secs(2), || {
                    watch::doing("a test stall");
                    std::thread::sleep(std::time::Duration::from_secs(6));
                });
            }
            // RADIANCE_SHOT_SPLASH=1: the start's splash too, caught as it plays (PATH-<ms>.png)
            if std::env::var_os("RADIANCE_SHOT_SPLASH").is_some() {
                // RADIANCE_SHOT_AT=ms,ms,…: when to catch it
                let at: Vec<u64> = std::env::var("RADIANCE_SHOT_AT").ok().map(|v| v.split(',').filter_map(|x| x.trim().parse().ok()).collect()).unwrap_or_else(|| vec![300, 1200, 2000, 2800, 3500, 4100, 4500, 5000, 5600, 5850, 6100, 6400]);
                for ms in at {
                    let (w, p) = (ui.window.clone(), path.replace(".png", &format!("-{ms}.png")));
                    glib::timeout_add_local_once(std::time::Duration::from_millis(ms), move || shoot(&w, &p));
                }
            }
            let (w, a) = (ui.window.clone(), app.clone());
            glib::timeout_add_local_once(std::time::Duration::from_secs(wait), move || {
                // while something moves, a frame can be caught half-made: try again shortly
                let tries = std::cell::Cell::new(0);
                glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
                    let (width, height) = (w.width() as f64, w.height() as f64);
                    let paintable = gtk::WidgetPaintable::new(Some(&w));
                    let snap = gtk::Snapshot::new();
                    gdk::prelude::PaintableExt::snapshot(&paintable, snap.upcast_ref::<gdk::Snapshot>(), width, height);
                        // RADIANCE_SHOT_WIDTHS=1: what asks for the most width (responsive checks)
                    if let (Some(at_least), 0) = (std::env::var("RADIANCE_SHOT_WIDTHS").ok().map(|v| v.parse::<i32>().unwrap_or(250)), tries.get()) {
                        // RADIANCE_SHOT_WIDTHS=N: everything asking for N px or more (default 250), however deep
                        let walk = |x: &gtk::Widget, depth: usize| walk_widths(x, depth, at_least);
                        fn walk_widths(x: &gtk::Widget, depth: usize, at_least: i32) {
                            let (min, _, _, _) = x.measure(gtk::Orientation::Horizontal, -1);
                            if min >= at_least && x.is_visible() {
                                eprintln!("{}{} {:?} min {min} has {}", "  ".repeat(depth), x.type_().name(), x.css_classes().iter().map(|c| c.to_string()).collect::<Vec<_>>(), x.width());
                            }
                            let mut c = x.first_child();
                            while let Some(ch) = c {
                                if depth < 40 && min >= at_least {
                                    walk_widths(&ch, depth + 1, at_least);
                                }
                                c = ch.next_sibling();
                            }
                        }
                        walk(w.upcast_ref(), 0);
                    }
                    match (snap.to_node(), w.native().and_then(|n| n.renderer())) {
                        (Some(node), Some(renderer)) => {
                            let viewport = gtk::graphene::Rect::new(0.0, 0.0, width as f32, height as f32);
                            if let Err(e) = renderer.render_texture(&node, Some(&viewport)).save_to_png(&path) {
                                eprintln!("screenshot: {e}");
                            }
                        }
                        _ if tries.get() < 60 => {
                            tries.set(tries.get() + 1);
                            return glib::ControlFlow::Continue;
                        }
                        _ => eprintln!("screenshot: nothing to draw"),
                    }
                    a.quit();
                    glib::ControlFlow::Break
                });
            });
        }
    });
    app.run_with_args(&args[..1])
}

/// RADIANCE_PROFILE=1: once a second, how long the window's frames took in
/// each phase (animations, layout, drawing) and how many there were (stderr).
fn profile_frames(w: &adw::ApplicationWindow) {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Instant;
    let w = w.clone();
    glib::timeout_add_local_once(std::time::Duration::from_millis(500), move || {
        let Some(clock) = w.frame_clock() else { return };
        #[derive(Default)]
        struct P { mark: Option<Instant>, update: f64, layout: f64, paint: f64, frames: u32, since: Option<Instant> }
        let p = Rc::new(RefCell::new(P::default()));
        let phase = |p: &Rc<RefCell<P>>, add: fn(&mut P, f64)| {
            let mut p = p.borrow_mut();
            if let Some(m) = p.mark {
                let d = m.elapsed().as_secs_f64() * 1000.0;
                add(&mut p, d);
            }
            p.mark = Some(Instant::now());
        };
        let q = p.clone();
        clock.connect_update(move |_| { q.borrow_mut().mark = Some(Instant::now()); });
        let q = p.clone();
        clock.connect_layout(move |_| phase(&q, |p, d| p.update += d));
        let q = p.clone();
        clock.connect_paint(move |_| phase(&q, |p, d| p.layout += d));
        let q = p.clone();
        clock.connect_after_paint(move |_| {
            phase(&q, |p, d| p.paint += d);
            let mut p = q.borrow_mut();
            p.mark = None;
            p.frames += 1;
            let since = *p.since.get_or_insert_with(Instant::now);
            if since.elapsed().as_secs_f64() >= 1.0 {
                eprintln!("[profile] {} frames: animations {:.0} ms, layout {:.0} ms, drawing {:.0} ms (per second)", p.frames, p.update, p.layout, p.paint);
                *p = P::default();
            }
        });
    });
}
