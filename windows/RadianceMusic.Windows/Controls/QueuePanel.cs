using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using RadianceMusic.Native;
using Ffi = RadianceMusic.Native.Native;

namespace RadianceMusic.Controls;

/// <summary>
/// The right-hand panel, as the GTK app's side panel: Queue | Lyrics (Splits comes with stage 3).
/// Queue: "Played before" (its last five, faint; a click plays it again now), then the queue from
/// the song playing ("Now playing · …"), the songs after it dragged to reorder, each with its menu.
/// </summary>
internal sealed class QueuePanel : Grid
{
    private readonly EngineHub hub;
    private readonly SelectorBar tabs = new();
    private readonly SelectorBarItem queueTab = new() { Text = "Queue", Icon = new FontIcon { Glyph = "" } };
    private readonly SelectorBarItem lyricsTab = new() { Text = "Lyrics", Icon = new FontIcon { Glyph = "" } };
    private readonly StackPanel queueView = new() { Spacing = 6, Padding = new Thickness(12, 8, 12, 24) };
    private readonly ScrollViewer queueScroll = new();
    private readonly LyricsColumn lyrics;
    private string shown = "-";
    private object? dragged;
    private int dragFrom = -1;
    private uint first;

    public QueuePanel(EngineHub hub)
    {
        this.hub = hub;
        lyrics = new LyricsColumn(hub, big: false);
        RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        Background = (Brush)Application.Current.Resources["LayerFillColorDefaultBrush"];
        tabs.Items.Add(queueTab);
        tabs.Items.Add(lyricsTab);
        tabs.SelectedItem = queueTab;
        tabs.SelectionChanged += (_, _) => ShowTab();
        Children.Add(tabs);
        queueScroll.Content = queueView;
        SetRow(queueScroll, 1);
        Children.Add(queueScroll);
        SetRow(lyrics, 1);
        Children.Add(lyrics);
        hub.Change += kind =>
        {
            if (kind is ChangeKind.Queue or ChangeKind.Now or ChangeKind.Liked or ChangeKind.History) Render();
        };
        ShowTab();
        Render();
    }

    /// <summary>"queue" or "lyrics".</summary>
    public string Tab
    {
        get => tabs.SelectedItem == lyricsTab ? "lyrics" : "queue";
        set => tabs.SelectedItem = value == "lyrics" ? lyricsTab : queueTab;
    }

    /// <summary>Whether the Lyrics tab is there (not on Now Playing while its lyrics column shows: SideTabs).</summary>
    public bool HasLyrics
    {
        get => tabs.Items.Contains(lyricsTab);
        set
        {
            if (value == HasLyrics) return;
            if (Program.Dev.Shot != null) Crash.Note($"side panel: Lyrics tab {(value ? "back" : "gone")}");
            var tab = SideTabs.Tab(Tab, value);
            if (value) tabs.Items.Add(lyricsTab);
            else tabs.Items.Remove(lyricsTab);
            Tab = tab;
            ShowTab();
        }
    }

    private void ShowTab()
    {
        var onLyrics = tabs.SelectedItem == lyricsTab;
        queueScroll.Visibility = onLyrics ? Visibility.Collapsed : Visibility.Visible;
        lyrics.Visibility = onLyrics ? Visibility.Visible : Visibility.Collapsed;
    }

    private void Render()
    {
        var p = hub.Engine.QueuePanel();
        var key = string.Join("|", p.Before.Select(s => s.Key)) + "#" + string.Join("|", p.Rows.Select(s => s.Key)) + "#" + p.Start + p.Now + p.Note;
        if (key == shown) return;
        shown = key;
        queueView.Children.Clear();
        if (p.Before.Length > 0)
        {
            queueView.Children.Add(Heading("Played before"));
            queueView.Children.Add(Small(Ffi.PlayedBeforeNote()));
            var before = List();
            foreach (var song in p.Before)
            {
                var row = new SongRow(hub.Engine, song, heart: false) { Opacity = 0.7 };
                var k = song.Key;
                SongMenu.Attach(row, hub, k, 0, new MenuPlace.Row(), () => hub.Engine.PlayAgain(k));
                before.Items.Add(row);
            }
            before.ItemClick += (_, e) =>
            {
                if (e.ClickedItem is SongRow r) hub.Engine.PlayAgain(r.Key);
            };
            queueView.Children.Add(before);
        }
        queueView.Children.Add(Heading("Queue"));
        queueView.Children.Add(Small(p.Note));
        var rows = p.Rows.ToList();
        first = p.Start;
        if (p.Now && rows.Count > 0)
        {
            var now = rows[0];
            var nowRow = new SongRow(hub.Engine, now with { Subtitle = "Now playing · " + now.Byline }, heart: false);
            var holder = new Border { Child = nowRow, Padding = new Thickness(12, 2, 12, 2), CornerRadius = new CornerRadius(6), Background = (Brush)Application.Current.Resources["SubtleFillColorSecondaryBrush"] };
            SongMenu.Attach(holder, hub, now.Key, p.Start, new MenuPlace.Queue(), null);
            queueView.Children.Add(holder);
            rows.RemoveAt(0);
            first = p.Start + 1;
        }
        var next = List();
        next.CanReorderItems = true;
        next.CanDragItems = true;
        next.AllowDrop = true;
        for (var i = 0; i < rows.Count; i++)
        {
            var song = rows[i];
            var index = first + (uint)i;
            var row = new SongRow(hub.Engine, song, heart: false);
            ToolTipService.SetToolTip(row, "Drag to move it in the queue");
            SongMenu.Attach(row, hub, song.Key, index, new MenuPlace.Queue(), () => hub.Engine.PlayFrom(ListKind.Queue, index, song.Key));
            next.Items.Add(row);
        }
        next.ItemClick += (_, e) =>
        {
            if (e.ClickedItem is SongRow r) hub.Engine.PlayFrom(ListKind.Queue, first + (uint)next.Items.IndexOf(r), r.Key);
        };
        next.DragItemsStarting += (_, e) =>
        {
            dragged = e.Items.FirstOrDefault();
            dragFrom = dragged == null ? -1 : next.Items.IndexOf(dragged);
        };
        next.DragItemsCompleted += (_, _) =>
        {
            if (dragged is SongRow r && QueueDrop.Map(first, dragFrom, next.Items.IndexOf(r)) is { } m) hub.Engine.MoveInQueue(m.From, m.To, r.Key);
            dragged = null;
        };
        queueView.Children.Add(next);
    }

    private static ListView List() => new() { SelectionMode = ListViewSelectionMode.None, IsItemClickEnabled = true };

    private static TextBlock Heading(string text) => new() { Text = text, Style = (Style)Application.Current.Resources["BodyStrongTextBlockStyle"], Margin = new Thickness(0, 10, 0, 0) };

    private static TextBlock Small(string text) => new() { Text = text, Style = (Style)Application.Current.Resources["CaptionTextBlockStyle"], Foreground = (Brush)Application.Current.Resources["TextFillColorSecondaryBrush"], TextWrapping = TextWrapping.Wrap };
}
