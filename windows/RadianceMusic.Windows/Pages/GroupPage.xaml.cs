using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Navigation;
using RadianceMusic.Controls;
using RadianceMusic.Native;

namespace RadianceMusic.Pages;

/// <summary>
/// An album's, an artist's or a genre's liked songs, as the GTK app's open_group: back to the
/// grid (its name on the button), the art (round for an artist), the name, who / how many, Play /
/// Shuffle, the songs.
/// </summary>
public sealed partial class GroupPage : Page
{
    private EngineHub? hub;
    private GroupKind kind;
    private string name = "";
    private readonly StackPanel body = new() { Padding = new Thickness(36, 20, 36, 36), Spacing = 12, MaxWidth = 1400 };

    public GroupPage()
    {
        InitializeComponent();
        NavigationCacheMode = NavigationCacheMode.Disabled;
        Content = new ScrollViewer { Content = body };
    }

    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        if (e.Parameter is not PageArgs { What: ValueTuple<GroupKind, string> what } a) return;
        hub = a.Hub;
        (kind, name) = what;
        hub.Change += OnChange;
        Render();
    }

    protected override void OnNavigatedFrom(NavigationEventArgs e)
    {
        if (hub != null) hub.Change -= OnChange;
    }

    private void OnChange(ChangeKind what)
    {
        if (what is ChangeKind.Library or ChangeKind.Liked) Render();
    }

    private void Render()
    {
        if (hub == null) return;
        body.Children.Clear();
        var back = PageKit.IconButton("", LibraryPage.KindName(kind), () => App.Window?.Show(LibraryPage.KindName(kind).ToLowerInvariant()));
        body.Children.Add(back);
        if (hub.Engine.Group(kind, name) is not { } g)
        {
            body.Children.Add(PageKit.Note("Nothing here any more."));
            return;
        }
        var art = new Border { Width = 220, Height = 220, CornerRadius = new CornerRadius(g.Round ? 110 : 8), Background = (Brush)Application.Current.Resources["CardBackgroundFillColorDefaultBrush"], HorizontalAlignment = HorizontalAlignment.Left };
        body.Children.Add(art);
        _ = Art(art, g.Art);
        body.Children.Add(PageKit.Title(g.Title));
        body.Children.Add(PageKit.Note(g.Sub));
        var buttons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        var songs = g.Songs;
        buttons.Children.Add(PageKit.IconButton("", "Play", () => { if (songs.Length > 0) hub.Engine.PlayGroup(kind, name, 0, songs[0].Key); }, accent: true));
        buttons.Children.Add(PageKit.IconButton("", "Shuffle", () => hub.Engine.ShuffleGroup(kind, name)));
        body.Children.Add(buttons);
        var list = PageKit.SongList();
        for (var i = 0; i < songs.Length; i++)
        {
            var song = songs[i];
            var index = (uint)i;
            var row = new SongRow(hub.Engine, song);
            SongMenu.Attach(row, hub, song.Key, index, new MenuPlace.Row(), () => hub.Engine.PlayGroup(kind, name, index, song.Key));
            list.Items.Add(row);
        }
        list.ItemClick += (_, e) =>
        {
            if (e.ClickedItem is SongRow row) hub.Engine.PlayGroup(kind, name, (uint)list.Items.IndexOf(row), row.Key);
        };
        body.Children.Add(list);
    }

    private async Task Art(Border to, string source)
    {
        if (hub == null) return;
        if (await ArtCache.LoadAsync(hub.Engine, source, 440) is { } image) to.Background = new ImageBrush { ImageSource = image, Stretch = Stretch.UniformToFill };
    }
}
