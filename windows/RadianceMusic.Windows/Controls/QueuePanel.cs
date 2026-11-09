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
    /// <summary>The split queue (stage 3), as the Linux app's Splits tab: the song being split, the ones waiting, Pause.</summary>
    private static readonly BetaWords W = Ffi.BetaWords();
    private readonly SelectorBarItem splitsTab = new() { Text = W.SplitsTab, Icon = new FontIcon { Glyph = "\uE8C6" } };
    private readonly ScrollViewer splitsScroll = new() { Padding = new Thickness(16, 8, 16, 16) };
    private readonly StackPanel splitsView = new() { Spacing = 6 };
    private string splitsShown = "";
    private readonly StackPanel queueView = new() { Spacing = 6, Padding = new Thickness(12, 8, 12, 24) };
    private readonly ScrollViewer queueScroll = new();
    private readonly LyricsColumn lyrics;
    private string shown = "-";
    private object? dragged;
    private int dragFrom = -1;
    /// <summary>The queue's list as drawn now (made again on each change).</summary>
    private ListView? nextList;

    /// <summary>What a drag ends in, for the screenshots (--act queue-move): row `from` dropped at `to`.</summary>
    internal bool Drop(int from, int to)
    {
        if (nextList is not { } l || from < 0 || from >= l.Items.Count || l.Items[from] is not SongRow r) return false;
        if (QueueDrop.Map(first, from, to) is not { } m) return false;
        hub.Engine.MoveInQueue(m.From, m.To, r.Key);
        return true;
    }

    /// <summary>Row `i`'s menu (--act queue-menu:N).</summary>
    internal bool OpenMenu(int i) => nextList is { } l && i >= 0 && i < l.Items.Count && l.Items[i] is SongRow r && SongMenu.Open(r);

    /// <summary>The queue's songs, as drawn (the screenshots' log).</summary>
    /// <summary>Where Up next's row <paramref name="i"/> is in the window (its handle, at the left; its top and
    /// height): a real drag's check in a developer run.</summary>
    internal string Where(int i)
    {
        if (nextList?.ContainerFromIndex(i) is not FrameworkElement row) return "";
        var at = row.TransformToVisual(null).TransformPoint(new Windows.Foundation.Point(0, 0));
        return $"{at.X + 20:0},{at.Y + row.ActualHeight / 2:0} top {at.Y:0} height {row.ActualHeight:0}";
    }

    internal string Titles() => nextList == null ? "" : string.Join(" | ", nextList.Items.OfType<SongRow>().Select(r => r.Title));
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
        tabs.Items.Add(splitsTab);
        tabs.SelectedItem = queueTab;
        tabs.SelectionChanged += (_, _) => ShowTab();
        Children.Add(tabs);
        queueScroll.Content = queueView;
        SetRow(queueScroll, 1);
        Children.Add(queueScroll);
        SetRow(lyrics, 1);
        Children.Add(lyrics);
        splitsScroll.Content = splitsView;
        SetRow(splitsScroll, 1);
        Children.Add(splitsScroll);
        hub.Change += kind =>
        {
            if (kind is ChangeKind.Queue or ChangeKind.Now or ChangeKind.Liked or ChangeKind.History) Render();
            if (kind is ChangeKind.Splits) RenderSplits();
        };
        ShowTab();
        Render();
        RenderSplits();
    }

    /// <summary>"queue" or "lyrics".</summary>
    public string Tab
    {
        get => tabs.SelectedItem == lyricsTab ? "lyrics" : tabs.SelectedItem == splitsTab ? "splits" : "queue";
        set => tabs.SelectedItem = value == "lyrics" && tabs.Items.Contains(lyricsTab) ? lyricsTab : value == "splits" ? splitsTab : queueTab;
    }

    /// <summary>The panel just opened: its tabs laid out again. Over the page (a narrow window) the
    /// SelectorBar had laid them out while the closed pane was 0 px wide and drew only the first.</summary>
    public void Opened()
    {
        var tab = Tab;
        var has = HasLyrics;
        tabs.Items.Clear();
        tabs.Items.Add(queueTab);
        if (has) tabs.Items.Add(lyricsTab);
        tabs.Items.Add(splitsTab);
        Tab = tab;
        ShowTab();
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
            if (value) tabs.Items.Insert(1, lyricsTab); // (before Splits)
            else tabs.Items.Remove(lyricsTab);
            Tab = tab;
            ShowTab();
        }
    }

    private void ShowTab()
    {
        var onLyrics = tabs.SelectedItem == lyricsTab;
        var onSplits = tabs.SelectedItem == splitsTab;
        queueScroll.Visibility = onLyrics || onSplits ? Visibility.Collapsed : Visibility.Visible;
        lyrics.Visibility = onLyrics ? Visibility.Visible : Visibility.Collapsed;
        splitsScroll.Visibility = onSplits ? Visibility.Visible : Visibility.Collapsed;
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
        nextList = next;
        next.CanReorderItems = true;
        next.CanDragItems = true;
        next.AllowDrop = true;
        for (var i = 0; i < rows.Count; i++)
        {
            var song = rows[i];
            var index = first + (uint)i;
            var row = new SongRow(hub.Engine, song, heart: false, handle: true);
            ToolTipService.SetToolTip(row, "Drag to move it in the queue");
            SongMenu.Attach(row, hub, song.Key, index, new MenuPlace.Queue(), () => hub.Engine.PlayFrom(ListKind.Queue, index, song.Key));
            next.Items.Add(row);
        }
        next.ItemClick += (_, e) =>
        {
            if (e.ClickedItem is not SongRow r) return;
            App.Trace($"queue: clicked {r.Title}");
            hub.Engine.PlayFrom(ListKind.Queue, first + (uint)next.Items.IndexOf(r), r.Key);
        };
        next.DragItemsStarting += (_, e) =>
        {
            dragged = e.Items.FirstOrDefault();
            dragFrom = dragged == null ? -1 : next.Items.IndexOf(dragged);
            App.Trace($"queue: drag from row {dragFrom}");
        };
        next.DragItemsCompleted += (_, e) =>
        {
            var to = dragged is SongRow d ? next.Items.IndexOf(d) : -1;
            App.Trace($"queue: drag done ({e.DropResult}), row {dragFrom} now row {to}");
            if (dragged is SongRow r && QueueDrop.Map(first, dragFrom, to) is { } m) hub.Engine.MoveInQueue(m.From, m.To, r.Key);
            dragged = null;
        };
        queueView.Children.Add(next);
    }

    private static ListView List() => new() { SelectionMode = ListViewSelectionMode.None, IsItemClickEnabled = true };

    /// <summary>The Splits tab: the song being split (its bar and what it's at), the fast lane's, the ones waiting
    /// ("Number 2 in line · …"), the note, and Pause Splitting.</summary>
    private void RenderSplits()
    {
        var s = hub.Engine.Splits();
        var key = $"{s.Now?.Song.Key}|{s.Now?.Phase}|{s.TwoNow?.Key}|{string.Join(",", s.Waiting.Select(x => x.Key))}|{string.Join(",", s.TwoWaiting.Select(x => x.Key))}|{s.Paused}|{string.Join(",", s.Away.Select(x => x.Key + x.Subtitle))}";
        if (key == splitsShown) return;
        splitsShown = key;
        splitsView.Children.Clear();
        splitsView.Children.Add(Heading(W.SplittingNow));
        if (s.Now is { } now)
        {
            splitsView.Children.Add(new TextBlock { Text = now.Song.Title, Style = (Style)Application.Current.Resources["BodyTextBlockStyle"], TextTrimming = TextTrimming.CharacterEllipsis });
            splitsView.Children.Add(new ProgressBar { Minimum = 0, Maximum = 1, Value = now.Done, IsIndeterminate = now.Done <= 0, CornerRadius = new CornerRadius(4) });
            splitsView.Children.Add(Small(now.Phase));
        }
        if (s.TwoNow is { } two)
        {
            splitsView.Children.Add(new TextBlock { Text = two.Title, Style = (Style)Application.Current.Resources["BodyTextBlockStyle"], TextTrimming = TextTrimming.CharacterEllipsis });
            splitsView.Children.Add(new ProgressBar { IsIndeterminate = true, CornerRadius = new CornerRadius(4) });
        }
        if (s.Now == null && s.TwoNow == null) splitsView.Children.Add(Small(W.NothingToSplit));
        var waiting = s.Waiting.Concat(s.TwoWaiting).ToList();
        for (var i = 0; i < waiting.Count; i++)
        {
            splitsView.Children.Add(new TextBlock { Text = waiting[i].Title, Style = (Style)Application.Current.Resources["BodyTextBlockStyle"], TextTrimming = TextTrimming.CharacterEllipsis, Margin = new Thickness(0, i == 0 ? 10 : 2, 0, 0) });
            splitsView.Children.Add(Small(Ffi.InLine((uint)(i + 1), waiting[i].Byline)));
        }
        // your songs in a server's split queue (beta.11): "On NAME · number 2 in line"
        for (var i = 0; i < s.Away.Length; i++)
        {
            splitsView.Children.Add(new TextBlock { Text = s.Away[i].Title, Style = (Style)Application.Current.Resources["BodyTextBlockStyle"], TextTrimming = TextTrimming.CharacterEllipsis, Margin = new Thickness(0, i == 0 ? 10 : 2, 0, 0) });
            splitsView.Children.Add(Small(s.Away[i].Subtitle));
        }
        splitsView.Children.Add(new TextBlock { Text = W.SplitsNote, Style = (Style)Application.Current.Resources["CaptionTextBlockStyle"], Foreground = (Brush)Application.Current.Resources["TextFillColorTertiaryBrush"], TextWrapping = TextWrapping.Wrap, Margin = new Thickness(0, 12, 0, 0) });
        var pause = new ToggleSwitch { Header = W.PauseSplitting, IsOn = s.Paused, Margin = new Thickness(0, 8, 0, 0) };
        pause.Toggled += (_, _) =>
        {
            hub.Engine.SetSplitPaused(pause.IsOn);
        };
        splitsView.Children.Add(pause);
    }

    private static TextBlock Heading(string text) => new() { Text = text, Style = (Style)Application.Current.Resources["BodyStrongTextBlockStyle"], Margin = new Thickness(0, 10, 0, 0) };

    private static TextBlock Small(string text) => new() { Text = text, Style = (Style)Application.Current.Resources["CaptionTextBlockStyle"], Foreground = (Brush)Application.Current.Resources["TextFillColorSecondaryBrush"], TextWrapping = TextWrapping.Wrap };
}
