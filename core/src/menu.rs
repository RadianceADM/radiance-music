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
pub fn song_menu(t: &Track, place: Place, liked: bool, saved: bool, playlists: &[(i64, String)]) -> Vec<Vec<Item>> {
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
    let mine = vec![if liked { item("Remove from Liked", Action::Unlike) } else { item("Like", Action::Like) }, item("Song Radio", Action::Radio)];
    let mut menu = vec![play, lists, mine, go_items(t)];
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
        let row = song_menu(&song("Ann", "Blue"), Place::Row, false, false, &lists);
        assert_eq!(labels(&row), vec![
            vec!["Play", "Play Next", "Add to Queue"],
            vec!["Road Trip", "New Playlist…"],
            vec!["Like", "Song Radio"],
            vec!["Go to Artist", "Go to Album"],
        ]);
        assert_eq!(row[1][0].action, Action::AddToPlaylist(3));
        assert_eq!(row[3][1].action, Action::GoToAlbum("Blue Ann".into()));
        let queued = song_menu(&song("Ann & Bo", ""), Place::Queue, true, true, &[]);
        assert_eq!(labels(&queued), vec![
            vec!["Remove from Queue", "Play", "Play Next", "Add to Queue"],
            vec!["New Playlist…"],
            vec!["Remove from Liked", "Song Radio"],
            vec!["Go to Ann", "Go to Bo"],
            vec!["Show in Files"],
        ]);
        assert_eq!(queued[3][1].action, Action::GoToArtist("Bo".into()));
        let listed = song_menu(&song("Ann", ""), Place::Playlist(3), false, false, &lists);
        assert_eq!(listed[1][0].label, "Remove from This Playlist");
        assert_eq!(listed[1][0].action, Action::RemoveFromPlaylist(3));
        assert_eq!(ADD_TO_PLAYLIST, "Add to Playlist");
    }

    #[test]
    fn at_most_eight_artists_to_go_to() {
        let many = (1..=10).map(|i| format!("A{i}")).collect::<Vec<_>>().join(", ");
        let go = go_items(&song(&many, ""));
        assert_eq!(go.len(), 8);
    }
}
