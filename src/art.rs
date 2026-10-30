//! Album art: fetched, cut to a square and shrunk to the size it's shown at, all
//! on a worker thread (a big image never reaches the screen at full size).

use image::imageops::FilterType;

use crate::net;

/// RGBA pixels, `size` x `size`.
pub struct Pixels {
    pub size: u32,
    pub rgba: Vec<u8>,
}

/// Web art is kept on disk (~/.cache/radiance-music/art), so it shows at once
/// next time instead of being fetched again.
fn web_bytes(url: &str) -> Option<Vec<u8>> {
    let hash = url.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3));
    let path = gtk::glib::user_cache_dir().join("radiance-music/art").join(format!("{hash:016x}"));
    if let Ok(bytes) = std::fs::read(&path) {
        return Some(bytes);
    }
    let bytes = net::get_bytes(url)?;
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&path, &bytes);
    Some(bytes)
}

pub fn fetch(source: &str, size: u32) -> Option<Pixels> {
    let bytes = if source.starts_with("http") { web_bytes(source)? } else { std::fs::read(source).ok()? };
    let img = image::load_from_memory(&bytes).ok()?;
    let (w, h) = (img.width(), img.height());
    let side = w.min(h);
    let square = img.crop_imm((w - side) / 2, (h - side) / 2, side, side);
    let small = square.resize_exact(size, size, FilterType::Triangle).to_rgba8();
    Some(Pixels { size, rgba: small.into_raw() })
}

/// The Now playing backdrop: the art shrunk, blurred and a little more
/// colourful. Tiny (64 px); the screen stretches it smoothly.
pub fn backdrop(source: &str) -> Option<Pixels> {
    let size = 64;
    let mut px = fetch(source, size)?;
    let img = image::RgbaImage::from_raw(size, size, std::mem::take(&mut px.rgba))?;
    let mut blurred = image::imageops::blur(&img, 5.0);
    for p in blurred.pixels_mut() {
        let [r, g, b, _] = p.0.map(|c| c as f32);
        let lum = 0.3 * r + 0.59 * g + 0.11 * b;
        let boost = |c: f32| ((lum + (c - lum) * 1.4) * 0.9).clamp(0.0, 255.0) as u8;
        p.0 = [boost(r), boost(g), boost(b), 255];
    }
    Some(Pixels { size, rgba: blurred.into_raw() })
}

/// The backdrop as a soft round patch: the same colours, fading out towards
/// its edge (no corners or straight edges to show when it's drawn on top).
pub fn blob(px: &Pixels) -> Pixels {
    let n = px.size as usize;
    let mut rgba = px.rgba.clone();
    for y in 0..n {
        for x in 0..n {
            let (dx, dy) = ((x as f32 + 0.5) / n as f32 - 0.5, (y as f32 + 0.5) / n as f32 - 0.5);
            let k = (1.0 - (dx * dx + dy * dy).sqrt() / 0.5).clamp(0.0, 1.0);
            rgba[(y * n + x) * 4 + 3] = (k * k * (3.0 - 2.0 * k) * 255.0) as u8;
        }
    }
    Pixels { size: px.size, rgba }
}
