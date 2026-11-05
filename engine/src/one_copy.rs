//! One copy at a time on Windows (issue #45: today's GTK app and the WinUI beta alike): the copy
//! running listens on a local port written in the data folder; a second copy asks it to show its
//! window (and to open the radiancemusic:// link it was opened with), then quits.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

const ASK: &str = "radiance-music show";
const OPEN: &str = "radiance-music open ";
/// The running copy's answer.
pub const SHOWN: &str = "radiance-music shown";

fn port_file() -> std::path::PathBuf {
    glib::user_data_dir().join("radiance-music").join("running.port")
}

/// The line a second copy sends (a link: one line, no line breaks in it).
pub fn ask_line(link: Option<&str>) -> String {
    match link.map(|l| l.replace(['\r', '\n'], "")) {
        Some(l) => format!("{OPEN}{l}\n"),
        None => format!("{ASK}\n"),
    }
}

/// What the running copy heard: Some(link or None) when it's a copy asking; None: anything else.
pub fn heard(line: &str) -> Option<Option<String>> {
    let line = line.trim();
    if line == ASK {
        return Some(None);
    }
    let link = line.strip_prefix(OPEN.trim_end())?.trim();
    Some(Some(link.to_string()).filter(|l| !l.is_empty()))
}

/// Another copy runs and was asked to show its window, and to open `link` (a
/// radiancemusic:// link) when there is one (then this one quits).
pub fn ask_the_other(link: Option<&str>) -> bool {
    let Some(port) = std::fs::read_to_string(port_file()).ok().and_then(|s| s.trim().parse::<u16>().ok()) else { return false };
    let Ok(mut s) = TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(500)) else { return false };
    let _ = s.set_read_timeout(Some(Duration::from_secs(3)));
    if s.write_all(ask_line(link).as_bytes()).is_err() {
        return false;
    }
    let mut answer = String::new();
    BufReader::new(s).read_line(&mut answer).is_ok() && answer.trim() == SHOWN
}

/// This is the copy: the next ones are heard. `show` gets their link, on the listening thread
/// (the GTK app hands it to its main loop; the Windows app to its window's thread).
pub fn listen(show: impl Fn(Option<String>) + Send + Sync + 'static) {
    let Ok(listener) = TcpListener::bind("127.0.0.1:0") else { return };
    let Ok(addr) = listener.local_addr() else { return };
    let file = port_file();
    if let Some(d) = file.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(&file, addr.port().to_string());
    let _ = std::thread::Builder::new().name("one-copy".into()).spawn(move || {
        for conn in listener.incoming().flatten() {
            let _ = conn.set_read_timeout(Some(Duration::from_secs(2)));
            let mut line = String::new();
            if let Some(link) = BufReader::new(&conn).read_line(&mut line).ok().and_then(|_| heard(&line)) {
                let _ = (&conn).write_all(format!("{SHOWN}\n").as_bytes());
                show(link);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_copy_asks_and_brings_its_link() {
        assert_eq!(heard(&ask_line(None)), Some(None));
        let link = "radiancemusic://join?u=https%3A%2F%2Fmusic.example.com%2Fjoin%2FABCD-EFGH-JKMN";
        assert_eq!(heard(&ask_line(Some(link))), Some(Some(link.to_string())));
        // (an older copy's ask still works; a link can't break the line)
        assert_eq!(heard("radiance-music show\r\n"), Some(None));
        assert_eq!(ask_line(Some("a\nb")), "radiance-music open ab\n");
        assert_eq!(heard("GET / HTTP/1.1"), None);
        assert_eq!(heard("radiance-music open "), Some(None));
    }
}
