//! How long a song is, read from its own file (the party moves on to the next
//! song when one ends): FLAC's STREAMINFO, an Ogg file's last page (Opus or
//! Vorbis), an MP4's movie header; else ffprobe, when it's installed.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

pub fn duration_ms(path: &Path) -> Option<i64> {
    let mut f = File::open(path).ok()?;
    let mut head = [0u8; 4096];
    let n = read_up_to(&mut f, &mut head).ok()?;
    let head = &head[..n];
    let ms = if head.starts_with(b"fLaC") || head.starts_with(b"ID3") {
        flac(&mut f, head)
    } else if head.starts_with(b"OggS") {
        ogg(&mut f, head)
    } else if head.len() >= 8 && &head[4..8] == b"ftyp" {
        mp4(&mut f)
    } else {
        None
    };
    ms.filter(|ms| *ms > 0).or_else(|| ffprobe(path))
}

fn read_up_to(f: &mut File, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match f.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}

/// FLAC: total samples ÷ sample rate, from STREAMINFO (after an ID3 tag, if any).
fn flac(f: &mut File, head: &[u8]) -> Option<i64> {
    let mut at = 0u64;
    if head.starts_with(b"ID3") && head.len() >= 10 {
        let size = head[6..10].iter().fold(0u64, |s, b| (s << 7) | (*b as u64 & 0x7f));
        at = 10 + size;
    }
    let mut buf = [0u8; 4 + 4 + 34];
    f.seek(SeekFrom::Start(at)).ok()?;
    f.read_exact(&mut buf).ok()?;
    if &buf[..4] != b"fLaC" || buf[4] & 0x7f != 0 {
        return None;
    }
    let si = &buf[8..];
    let rate = ((si[10] as u64) << 12) | ((si[11] as u64) << 4) | ((si[12] as u64) >> 4);
    let total = (((si[13] & 0x0f) as u64) << 32) | u32::from_be_bytes([si[14], si[15], si[16], si[17]]) as u64;
    (rate > 0).then(|| (total * 1000 / rate) as i64)
}

/// Ogg: the last page's granule position, less Opus' pre-skip, ÷ the rate
/// (Opus always counts at 48 kHz; Vorbis says its own).
fn ogg(f: &mut File, head: &[u8]) -> Option<i64> {
    if head.len() < 28 {
        return None;
    }
    let body = 27 + head[26] as usize;
    let packet = head.get(body..)?;
    let (rate, pre_skip) = if packet.starts_with(b"OpusHead") && packet.len() >= 12 {
        (48_000u64, u16::from_le_bytes([packet[10], packet[11]]) as u64)
    } else if packet.starts_with(b"\x01vorbis") && packet.len() >= 16 {
        (u32::from_le_bytes([packet[12], packet[13], packet[14], packet[15]]) as u64, 0)
    } else {
        return None;
    };
    let len = f.seek(SeekFrom::End(0)).ok()?;
    let back = len.min(256 * 1024);
    f.seek(SeekFrom::Start(len - back)).ok()?;
    let mut tail = vec![0u8; back as usize];
    f.read_exact(&mut tail).ok()?;
    let mut i = tail.len().checked_sub(14)?;
    loop {
        if &tail[i..i + 4] == b"OggS" {
            let g = i64::from_le_bytes(tail[i + 6..i + 14].try_into().ok()?);
            if g > 0 {
                return (rate > 0).then(|| ((g as u64).saturating_sub(pre_skip) * 1000 / rate) as i64);
            }
        }
        if i == 0 {
            return None;
        }
        i -= 1;
    }
}

/// MP4 / M4A: moov → mvhd's duration ÷ its timescale (moov may come after the audio).
fn mp4(f: &mut File) -> Option<i64> {
    let len = f.seek(SeekFrom::End(0)).ok()?;
    let moov = find_box(f, 0, len, b"moov")?;
    let mvhd = find_box(f, moov.0, moov.1, b"mvhd")?;
    f.seek(SeekFrom::Start(mvhd.0)).ok()?;
    let mut v = [0u8; 4];
    f.read_exact(&mut v).ok()?;
    let (scale, duration) = if v[0] == 1 {
        let mut b = [0u8; 28];
        f.read_exact(&mut b).ok()?;
        (u32::from_be_bytes(b[16..20].try_into().ok()?) as u64, u64::from_be_bytes(b[20..28].try_into().ok()?))
    } else {
        let mut b = [0u8; 16];
        f.read_exact(&mut b).ok()?;
        (u32::from_be_bytes(b[8..12].try_into().ok()?) as u64, u32::from_be_bytes(b[12..16].try_into().ok()?) as u64)
    };
    (scale > 0).then(|| (duration * 1000 / scale) as i64)
}

/// A box of `kind` among those from `start` to `end`: (where its contents start, where it ends).
fn find_box(f: &mut File, start: u64, end: u64, kind: &[u8; 4]) -> Option<(u64, u64)> {
    let mut at = start;
    while at + 8 <= end {
        f.seek(SeekFrom::Start(at)).ok()?;
        let mut h = [0u8; 8];
        f.read_exact(&mut h).ok()?;
        let mut size = u32::from_be_bytes(h[..4].try_into().ok()?) as u64;
        let mut header = 8;
        if size == 1 {
            let mut big = [0u8; 8];
            f.read_exact(&mut big).ok()?;
            size = u64::from_be_bytes(big);
            header = 16;
        } else if size == 0 {
            size = end - at;
        }
        if size < header {
            return None;
        }
        if &h[4..8] == kind {
            return Some((at + header, (at + size).min(end)));
        }
        at += size;
    }
    None
}

fn ffprobe(path: &Path) -> Option<i64> {
    let out = std::process::Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "format=duration", "-of", "default=noprint_wrappers=1:nokey=1"])
        .arg(path)
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    let secs: f64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
    (secs > 0.0).then(|| (secs * 1000.0) as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("rms-media-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }

    /// A FLAC header: 44.1 kHz, 441000 samples = 10 s.
    pub fn flac_bytes(samples: u64, rate: u32) -> Vec<u8> {
        let mut v = b"fLaC".to_vec();
        v.extend([0x80, 0, 0, 34]); // the last block: STREAMINFO, 34 bytes
        let mut si = [0u8; 34];
        si[10] = (rate >> 12) as u8;
        si[11] = (rate >> 4) as u8;
        si[12] = ((rate & 0xf) << 4) as u8 | (1 << 1); // 2 channels
        si[13] = (15 << 4) as u8 | ((samples >> 32) & 0xf) as u8; // 16 bits
        si[14..18].copy_from_slice(&(samples as u32).to_be_bytes());
        v.extend(si);
        v.extend(vec![0u8; 2000]);
        v
    }

    #[test]
    fn flac_length() {
        let p = write("a.flac", &flac_bytes(441_000, 44_100));
        assert_eq!(duration_ms(&p), Some(10_000));
    }

    #[test]
    fn ogg_opus_length() {
        // first page: OpusHead with a pre-skip of 312; last page at granule 312 + 48000*3
        let mut v = b"OggS".to_vec();
        v.extend([0, 2]);
        v.extend(0i64.to_le_bytes());
        v.extend([0u8; 12]);
        v.push(1); // one segment
        v.push(19);
        v.extend(b"OpusHead");
        v.push(1);
        v.push(2);
        v.extend(312u16.to_le_bytes());
        v.extend(48_000u32.to_le_bytes());
        v.extend([0, 0, 0]);
        v.extend(vec![7u8; 5000]);
        v.extend(b"OggS");
        v.extend([0, 4]);
        v.extend((312i64 + 48_000 * 3).to_le_bytes());
        v.extend([0u8; 13]);
        let p = write("a.opus", &v);
        assert_eq!(duration_ms(&p), Some(3000));
    }

    #[test]
    fn mp4_length() {
        let mut mvhd = vec![0u8; 4]; // version 0
        mvhd.extend([0u8; 8]);
        mvhd.extend(1000u32.to_be_bytes());
        mvhd.extend(185_500u32.to_be_bytes());
        mvhd.extend([0u8; 80]);
        let mut mvhd_box = ((mvhd.len() + 8) as u32).to_be_bytes().to_vec();
        mvhd_box.extend(b"mvhd");
        mvhd_box.extend(mvhd);
        let mut moov = ((mvhd_box.len() + 8) as u32).to_be_bytes().to_vec();
        moov.extend(b"moov");
        moov.extend(mvhd_box);
        let mut v = 16u32.to_be_bytes().to_vec();
        v.extend(b"ftypM4A ");
        v.extend([0u8; 4]);
        v.extend(1008u32.to_be_bytes());
        v.extend(b"mdat");
        v.extend(vec![1u8; 1000]);
        v.extend(moov);
        let p = write("a.m4a", &v);
        assert_eq!(duration_ms(&p), Some(185_500));
    }
}
