using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Navigation;
using RadianceMusic.Controls;
using RadianceMusic.Native;

namespace RadianceMusic.Pages;

/// <summary>
/// Downloading (owner, 2026-11-07: "pede ba may downloading den sa sidebar", "for download not yet saved but liked"):
/// the liked songs not saved on this computer yet, the ones saving now first, each as Liked Songs shows it
/// ("Saving…", "Not saved yet"), with Save Them Now (Liked Songs' Fix missing) while any are missing. A click plays it.
/// </summary>
public sealed partial class DownloadingPage : Page
{
    private EngineHub? hub;
    private readonly StackPanel body = new() { Spacing = 8 };
    private string shown = "-";

    /// <summary>On screen now: a kept page (NavigationCacheMode) that isn't is drawn when it comes back.</summary>
    private bool onScreen, stale;

    public DownloadingPage()
    {
        InitializeComponent();
        NavigationCacheMode = NavigationCacheMode.Required;
        Content = PageKit.Column(body);
    }

    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        onScreen = true;
        if (hub != null)
        {
            if (stale) Render();
            return;
        }
        hub = e.Parameter as EngineHub;
        if (hub == null) return;
        hub.Change += kind =>
        {
            if (kind == ChangeKind.Liked) Render();
        };
        Render();
    }

    protected override void OnNavigatedFrom(NavigationEventArgs e) => onScreen = false;

    private void Render()
    {
        if (hub == null) return;
        stale = !onScreen;
        if (stale) return;
        var d = hub.Engine.Downloading();
        var key = d.Note + d.Fix + d.Fixing + string.Concat(d.Songs.Select(s => s.Key + s.Subtitle));
        if (key == shown) return;
        shown = key;
        body.Children.Clear();
        body.Children.Add(PageKit.Title("Downloading"));
        body.Children.Add(PageKit.Note(d.Note));
        if (d.Fix is { } fix)
        {
            // Save Them Now: Liked Songs' Fix missing (the songs and their music videos), busy while it saves
            var save = PageKit.IconButton("", fix, () => hub.Engine.FixMissing(), accent: !d.Fixing);
            save.IsEnabled = !d.Fixing;
            save.HorizontalAlignment = Microsoft.UI.Xaml.HorizontalAlignment.Left;
            body.Children.Add(save);
        }
        if (d.Songs.Length == 0) return;
        var list = PageKit.SongList();
        foreach (var song in d.Songs) list.Items.Add(new SongRow(hub.Engine, song, heart: false));
        list.ItemClick += (_, e) =>
        {
            if (e.ClickedItem is SongRow r) hub.Engine.PlayFrom(ListKind.Liked, 0, r.Key);
        };
        body.Children.Add(list);
    }
}
