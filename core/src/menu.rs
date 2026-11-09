//! A song's menu (right-click, long-press, the Menu key), as the Linux app builds it: its stage-2
//! items for the Windows beta, in the same order and words. The Linux app's own menu adds its
//! splitting, server, lyrics, info and link items around these (ported in stages 3–4).

use crate::track::Track;

/// Where a song's menu was opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// Any list of songs (Search, Home, Liked Songs, Albums…).
    Row,
    /// The queue.
    Queue,
    /// A playlist (its id).
    Playlist(i64),
    /// History.
    History,
}

/// What a menu item does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Plays it from where the menu was opened (the app knows its list).
    Play,
    PlayNext,
    AddToQueue,
    RemoveFromQueue,
    RemoveFromPlaylist(i64),
    AddToPlaylist(i64),
    /// A new playlist with this song in it (the app asks for its name).
    NewPlaylistWith,
    Like,
    Unlike,
    Radio,
    /// Searches for the artist.
    GoToArtist(String),
    /// Searches for the album (its words: "album artist").
    GoToAlbum(String),
    ShowInFiles,
    /// Split in two (Music + Vocals, the fast split) or in four parts (stage 3).
    SplitTwo,
    SplitFour,
    /// The lyrics: shown as text, the editor, looked up again (the app opens the first two).
    ShowLyrics,
    EditLyrics,
    FindLyricsAgain,
    /// The song's page on the web to the clipboard (the app does it: stage 4b, the Linux menu's Copy Link).
    CopyLink,
    /// One of your saved songs sent to a joined server (its address): the Linux menu's "Add to NAME" (beta.11).
    AddToServer(String),
    /// A server's song split on its server, for everyone there: "Split on NAME" (beta.11).
    SplitOnServer,
}

/// What splitting can do for a song now, for its menu (stage 3): online, how many parts it has
/// (2: Music and Vocals; 4: split in four), whether each kind of split can run here, and whether the
/// song is being split (or waits) in each.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SplitAbility {
    pub online: bool,
    pub parts: usize,
    pub can_two: bool,
    pub can_four: bool,
    pub busy_two: bool,
    pub busy_four: bool,
}

impl SplitAbility {
    /// Nothing can split here.
    pub const NONE: SplitAbility = SplitAbility { online: true, parts: 0, can_two: false, can_four: false, busy_two: false, busy_four: false };
}

/// The song's split items, as the Linux menu's: none for a video, or offline with no file here; the fast
/// split while it has no halves; four parts while it has fewer ("Split in Four Parts" once in two).
pub fn split_items(t: &Track, s: &SplitAbility) -> Vec<Item> {
    if t.video || !(s.online || t.file.is_some()) {
        return Vec::new();
    }
    let halves = s.parts > 0;
    let mut v = Vec::new();
    if !halves && s.can_two && !s.busy_two {
        v.push(item(crate::words::SPLIT_TWO, Action::SplitTwo));
    }
    if s.can_four && s.parts < 4 && !s.busy_four {
        v.push(item(if halves { crate::words::SPLIT_FOUR_AFTER_TWO } else { crate::words::SPLIT_FOUR }, Action::SplitFour));
    }
    v
}

/// The lyrics section: Show Lyrics, Edit Lyrics…, Find Lyrics Again.
pub fn lyrics_items() -> Vec<Item> {
    vec![
        item(crate::words::SHOW_LYRICS, Action::ShowLyrics),
        item(crate::words::EDIT_LYRICS, Action::EditLyrics),
        item(crate::words::FIND_LYRICS_AGAIN, Action::FindLyricsAgain),
        // the Linux menu's "more" section ends with Copy Link (stage 4b)
        item(crate::share_words::COPY_LINK, Action::CopyLink),
    ]
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub label: String,
    pub action: Action,
}

/// The submenu that the AddToPlaylist and NewPlaylistWith items go in.
pub const ADD_TO_PLAYLIST: &str = "Add to Playlist";

fn item(label: &str, action: Action) -> Item {
    Item { label: label.to_string(), action }
}

/// The song's menu, a section at a time: playing it; playlists; liking it and its radio; going
/// to its artists and album; its file (`saved`: its file is here). `playlists`: (id, name).
pub fn song_menu(t: &Track, place: Place, liked: bool, saved: bool, playlists: &[(i64, String)], split: &SplitAbility) -> Vec<Vec<Item>> {
    let mut play = vec![];
    if place == Place::Queue {
        play.push(item("Remove from Queue", Action::RemoveFromQueue));
    }
    play.extend([item("Play", Action::Play), item("Play Next", Action::PlayNext), item("Add to Queue", Action::AddToQueue)]);
    let mut lists = vec![];
    if let Place::Playlist(id) = place {
        lists.push(item("Remove from This Playlist", Action::RemoveFromPlaylist(id)));
    }
    lists.extend(playlists.iter().map(|(id, name)| item(name, Action::AddToPlaylist(*id))));
    lists.push(item("New Playlist…", Action::NewPlaylistWith));
    let mut mine = vec![if liked { item("Remove from Liked", Action::Unlike) } else { item("Like", Action::Like) }, item("Song Radio", Action::Radio)];
    mine.extend(split_items(t, split));
    let mut menu = vec![play, lists, mine, go_items(t), lyrics_items()];
    if saved {
        menu.push(vec![item("Show in Files", Action::ShowInFiles)]);
    }
    menu.retain(|s| !s.is_empty());
    menu
}

/// Going to the song's artists (one: "Go to Artist"; several: "Go to {each}", at most eight) and
/// its album ("Go to Album", searching for "album artist").
pub fn go_items(t: &Track) -> Vec<Item> {
    let artists = t.artists();
    let mut go: Vec<Item> = if artists.len() == 1 {
        vec![item("Go to Artist", Action::GoToArtist(artists[0].clone()))]
    } else {
        artists.iter().take(8).map(|a| item(&format!("Go to {a}"), Action::GoToArtist(a.clone()))).collect()
    };
    if !t.album.is_empty() {
        go.push(item("Go to Album", Action::GoToAlbum(format!("{} {}", t.album, t.artist).trim().to_string())));
    }
    go
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::{Source, Track};

    fn song(artist: &str, album: &str) -> Track {
        Track { source: Source::YouTube, id: "m1".into(), title: "Menu Song".into(), artist: artist.into(), album: album.into(), seconds: 200, art: String::new(), format: String::new(), file: None, video: false, genres: None }
    }

    fn labels(m: &[Vec<Item>]) -> Vec<Vec<String>> {
        m.iter().map(|s| s.iter().map(|i| i.label.clone()).collect()).collect()
    }

    #[test]
    fn the_menu_has_gtk_s_items_in_gtk_s_order() {
        let lists = vec![(3, "Road Trip".to_string())];
        let row = song_menu(&song("Ann", "Blue"), Place::Row, false, false, &lists, &SplitAbility::NONE);
        assert_eq!(labels(&row), vec![
            vec!["Play", "Play Next", "Add to Queue"],
            vec!["Road Trip", "New Playlist…"],
            vec!["Like", "Song Radio"],
            vec!["Go to Artist", "Go to Album"],
            vec!["Show Lyrics", "Edit Lyrics…", "Find Lyrics Again", "Copy Link"],
        ]);
        assert_eq!(row[1][0].action, Action::AddToPlaylist(3));
        assert_eq!(row[3][1].action, Action::GoToAlbum("Blue Ann".into()));
        let can = SplitAbility { can_two: true, can_four: true, ..SplitAbility::NONE };
        let queued = song_menu(&song("Ann & Bo", ""), Place::Queue, true, true, &[], &can);
        assert_eq!(labels(&queued), vec![
            vec!["Remove from Queue", "Play", "Play Next", "Add to Queue"],
            vec!["New Playlist…"],
            vec!["Remove from Liked", "Song Radio", "Split: Music + Vocals (Fast)", "Split: Four Parts"],
            vec!["Go to Ann", "Go to Bo"],
            vec!["Show Lyrics", "Edit Lyrics…", "Find Lyrics Again", "Copy Link"],
            vec!["Show in Files"],
        ]);
        assert_eq!(queued[3][1].action, Action::GoToArtist("Bo".into()));
        let listed = song_menu(&song("Ann", ""), Place::Playlist(3), false, false, &lists, &SplitAbility::NONE);
        assert_eq!(listed[1][0].label, "Remove from This Playlist");
        assert_eq!(listed[1][0].action, Action::RemoveFromPlaylist(3));
        assert_eq!(ADD_TO_PLAYLIST, "Add to Playlist");
    }

    #[test]
    fn split_items_follow_the_linux_menu() {
        let t = song("Ann", "");
        let all = SplitAbility { online: true, parts: 0, can_two: true, can_four: true, busy_two: false, busy_four: false };
        let l = |s: &SplitAbility, t: &Track| split_items(t, s).iter().map(|i| i.label.clone()).collect::<Vec<_>>();
        assert_eq!(l(&all, &t), ["Split: Music + Vocals (Fast)", "Split: Four Parts"]);
        assert_eq!(split_items(&t, &all)[0].action, Action::SplitTwo);
        assert_eq!(split_items(&t, &all)[1].action, Action::SplitFour);
        let mut video = t.clone();
        video.video = true;
        assert!(l(&all, &video).is_empty(), "a video");
        assert!(l(&SplitAbility { online: false, ..all }, &t).is_empty(), "offline and not saved");
        let mut saved = t.clone();
        saved.file = Some("/m/a.opus".into());
        assert_eq!(l(&SplitAbility { online: false, ..all }, &saved).len(), 2, "offline but saved");
        assert_eq!(l(&SplitAbility { parts: 2, ..all }, &t), ["Split in Four Parts"], "with halves");
        assert!(l(&SplitAbility { parts: 4, ..all }, &t).is_empty(), "in four already");
        assert_eq!(l(&SplitAbility { busy_two: true, ..all }, &t), ["Split: Four Parts"]);
        assert_eq!(l(&SplitAbility { busy_four: true, ..all }, &t), ["Split: Music + Vocals (Fast)"]);
        assert_eq!(l(&SplitAbility { can_four: false, ..all }, &t), ["Split: Music + Vocals (Fast)"]);
        assert_eq!(lyrics_items().iter().map(|i| i.action.clone()).collect::<Vec<_>>(), [Action::ShowLyrics, Action::EditLyrics, Action::FindLyricsAgain, Action::CopyLink]);
    }

    #[test]
    fn at_most_eight_artists_to_go_to() {
        let many = (1..=10).map(|i| format!("A{i}")).collect::<Vec<_>>().join(", ");
        let go = go_items(&song(&many, ""));
        assert_eq!(go.len(), 8);
    }
}
