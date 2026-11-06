//! History by day (the History page of both apps): each play under "Today", "Yesterday", its
//! weekday within a week, else its date, in this computer's time zone.

use radiance_core::library::{day_label, day_number, HISTORY_ROWS};
use radiance_core::track::Track;

/// A day of History, and its plays.
#[derive(Clone, Debug)]
pub struct Day {
    pub label: String,
    pub plays: Vec<Play>,
}

/// A play: where it sits in the whole history (what plays on from it), the song, and when
/// ("8:05 PM"; empty when its time isn't known).
#[derive(Clone, Debug)]
pub struct Play {
    pub index: usize,
    pub track: Track,
    pub time: String,
}

/// `plays` (newest first, with when, unix seconds; 0 = not known) by day, as of `now`; at most
/// HISTORY_ROWS of them. A play whose time isn't known goes under "Earlier".
pub fn days(plays: &[(Track, i64)], now: i64) -> Vec<Day> {
    let today = glib::DateTime::from_unix_local(now).ok();
    let label_for = |d: &glib::DateTime| -> String {
        let fmt = |f: &str| d.format(f).map(|s| s.to_string()).unwrap_or_default();
        let Some(t) = &today else { return fmt("%A, %B %-d") };
        let days = day_number(t.year(), t.month() as u32, t.day_of_month() as u32) - day_number(d.year(), d.month() as u32, d.day_of_month() as u32);
        day_label(days, t.difference(d).as_hours(), &fmt("%A"), &fmt("%A, %B %-d, %Y"))
    };
    let mut out: Vec<Day> = Vec::new();
    for (i, (t, at)) in plays.iter().enumerate().take(HISTORY_ROWS) {
        let when = (*at > 0).then(|| glib::DateTime::from_unix_local(*at).ok()).flatten();
        let label = when.as_ref().map(&label_for).unwrap_or_else(|| "Earlier".into());
        let time = when.as_ref().and_then(|d| d.format("%-I:%M %p").ok()).map(|s| s.to_string()).unwrap_or_default();
        let play = Play { index: i, track: t.clone(), time };
        match out.last_mut() {
            Some(day) if day.label == label => day.plays.push(play),
            _ => out.push(Day { label, plays: vec![play] }),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use radiance_core::track::{Source, Track};

    fn song(id: &str) -> Track {
        Track { source: Source::YouTube, id: id.into(), title: id.into(), artist: String::new(), album: String::new(), seconds: 1, art: String::new(), format: String::new(), file: None, video: false, genres: None }
    }

    /// Today at noon here (so a day back and a day forward stay on their own days).
    fn noon() -> i64 {
        let n = glib::DateTime::now_local().unwrap();
        glib::DateTime::from_local(n.year(), n.month(), n.day_of_month(), 12, 0, 0.0).unwrap().to_unix()
    }

    #[test]
    fn plays_are_grouped_by_local_day() {
        let now = noon();
        let plays = vec![(song("a"), now), (song("b"), now - 3600), (song("c"), now - 26 * 3600), (song("d"), now - 10 * 86400)];
        let days = days(&plays, now);
        let labels: Vec<&str> = days.iter().map(|d| d.label.as_str()).collect();
        assert_eq!(labels[..2], ["Today", "Yesterday"]);
        assert!(labels[2].contains(", 20"), "ten days back: its date ({})", labels[2]);
        assert_eq!(days[0].plays.iter().map(|p| p.index).collect::<Vec<_>>(), [0, 1], "each play keeps its place in the whole history");
        assert_eq!(days[0].plays[0].time, "12:00 PM");
    }

    #[test]
    fn untimed_plays_go_under_earlier() {
        let now = noon();
        let days = days(&[(song("a"), now), (song("b"), 0)], now);
        assert_eq!(days.last().map(|d| d.label.as_str()), Some("Earlier"));
        assert_eq!(days.last().unwrap().plays[0].time, "");
    }

    #[test]
    fn at_most_four_hundred_rows() {
        let now = noon();
        let plays: Vec<(Track, i64)> = (0..450).map(|i| (song(&format!("s{i}")), now - i)).collect();
        assert_eq!(days(&plays, now).iter().map(|d| d.plays.len()).sum::<usize>(), 400);
    }
}
