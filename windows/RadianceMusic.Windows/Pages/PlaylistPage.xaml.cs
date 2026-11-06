using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Navigation;
using RadianceMusic.Controls;
using RadianceMusic.Native;

namespace RadianceMusic.Pages;

/// <summary>
/// A playlist, as the GTK app's open_playlist: its name, "N songs", Play / Shuffle / Rename /
/// Delete, its songs (their menu has Remove from This Playlist), or the words when it's empty.
/// </summary>
public sealed partial class PlaylistPage : Page
{
    private EngineHub? hub;
    private long id;
    private readonly StackPanel body = new() { Padding = new Thickness(36, 20, 36, 36), Spacing = 12, MaxWidth = 1400 };

    public PlaylistPage()
    {
        InitializeComponent();
        NavigationCacheMode = NavigationCacheMode.Disabled;
        Content = new ScrollViewer { Content = body };
    }

    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        if (e.Parameter is not PageArgs { What: long what } a) return;
        hub = a.Hub;
        id = what;
        hub.Change += OnChange;
        Render();
    }

    protected override void OnNavigatedFrom(NavigationEventArgs e)
    {
        if (hub != null) hub.Change -= OnChange;
    }

    private void OnChange(ChangeKind what)
    {
        if (what is ChangeKind.Playlists or ChangeKind.Liked) Render();
    }

    private void Render()
    {
        if (hub == null) return;
        body.Children.Clear();
        if (hub.Engine.Playlist(id) is not { } p) return;
        body.Children.Add(PageKit.Title(p.Name));
        body.Children.Add(PageKit.Note(p.Count));
        var songs = p.Songs;
        var buttons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        var play = PageKit.IconButton("", "Play", () => { if (songs.Length > 0) hub.Engine.PlayPlaylist(id, 0, songs[0].Key); }, accent: true);
        var shuffle = PageKit.IconButton("", "Shuffle", () => hub.Engine.ShufflePlaylist(id));
        play.IsEnabled = shuffle.IsEnabled = songs.Length > 0;
        var rename = PageKit.IconButton("", "Rename", () => _ = Rename(p.Name));
        ToolTipService.SetToolTip(rename, "Rename");
        var delete = PageKit.IconButton("", "Delete", () => _ = Delete(p.Name));
        ToolTipService.SetToolTip(delete, "Delete the playlist");
        foreach (var b in new[] { play, shuffle, rename, delete }) buttons.Children.Add(b);
        body.Children.Add(buttons);
        if (p.Empty is { } empty)
        {
            body.Children.Add(PageKit.Note(empty));
            return;
        }
        var list = PageKit.SongList();
        for (var i = 0; i < songs.Length; i++)
        {
            var song = songs[i];
            var index = (uint)i;
            var row = new SongRow(hub.Engine, song);
            SongMenu.Attach(row, hub, song.Key, index, new MenuPlace.Playlist(id), () => hub.Engine.PlayPlaylist(id, index, song.Key));
            list.Items.Add(row);
        }
        list.ItemClick += (_, e) =>
        {
            if (e.ClickedItem is SongRow row) hub.Engine.PlayPlaylist(id, (uint)list.Items.IndexOf(row), row.Key);
        };
        body.Children.Add(list);
    }

    private async Task Rename(string current)
    {
        if (hub == null) return;
        if (await Dialogs.NameDialog.Ask(XamlRoot, "Rename Playlist", "Rename", current) is { } name) hub.Engine.RenamePlaylist(id, name);
    }

    private async Task Delete(string name)
    {
        if (hub == null) return;
        var ask = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = $"Delete “{name}”?",
            Content = "The playlist goes; its songs stay in your library.",
            PrimaryButtonText = "Delete",
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Close,
        };
        if (await ask.ShowAsync() != ContentDialogResult.Primary) return;
        hub.Engine.DeletePlaylist(id);
        App.Window?.Show("liked");
    }
}
