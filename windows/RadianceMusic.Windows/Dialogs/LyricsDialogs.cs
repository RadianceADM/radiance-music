using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Media;
using RadianceMusic.Native;
using Ffi = RadianceMusic.Native.Native;

namespace RadianceMusic.Dialogs;

/// <summary>
/// The song menu's lyrics (stage 3), as the Linux app's: Show Lyrics (their words, selectable) and the lyrics
/// editor (ui.rs lyrics_editor): each line's start (a click while it plays sets it to now), its words and who
/// sings it, Remove; Add Line, Paste Lyrics, Tap Timing (Enter stamps the chosen line), Add a singer; Save keeps
/// them as yours (never replaced by a lookup). Looked up off the UI thread; soft corners.
/// </summary>
internal static class LyricsDialogs
{
    private static readonly BetaWords W = Ffi.BetaWords();

    public static async Task Show(EngineHub hub, XamlRoot root, string key)
    {
        var text = await Task.Run(() => hub.Engine.ShowLyrics(key));
        var words = new TextBlock { Text = text.Length > 0 ? text : "No lyrics found for this song.", TextWrapping = TextWrapping.Wrap, IsTextSelectionEnabled = true, LineHeight = 22 };
        var dialog = new ContentDialog
        {
            XamlRoot = root,
            Title = W.ShowLyrics,
            Content = new ScrollViewer { Content = words, MaxHeight = 480, Padding = new Thickness(0, 0, 12, 0) },
            CloseButtonText = "Close",
            CornerRadius = new CornerRadius(16),
        };
        await dialog.ShowAsync();
    }

    private sealed class Row
    {
        public double? At;
        public required Grid Box;
        public required Button Time;
        public required TextBox Text;
        public required ComboBox Singer;
    }

    public static async Task Edit(EngineHub hub, XamlRoot root, string key)
    {
        var view = await Task.Run(() => hub.Engine.LyricsText(key));
        if (view == null) return;
        var names = new List<string> { W.NobodySet };
        names.AddRange(view.Names);
        var rows = new List<Row>();
        var list = new StackPanel { Spacing = 6 };
        var chosen = 0;
        var tap = new ToggleButton { Content = Labelled("", W.TapTiming) };
        ToolTipService.SetToolTip(tap, W.TapTip);

        string Clock(double? at) => at is { } a ? TimeSpan.FromSeconds(a).ToString(a >= 3600 ? @"h\:mm\:ss\.f" : @"m\:ss\.f") : "—";
        double PlayingAt() => hub.Engine.Now().At;
        void Choose(int i)
        {
            chosen = i;
            for (var k = 0; k < rows.Count; k++)
                rows[k].Box.Background = k == i && tap.IsChecked == true ? (Brush)Application.Current.Resources["SubtleFillColorSecondaryBrush"] : null;
        }
        void AddRow(double? at, string text, string? singer, int index = -1)
        {
            var box = new Grid { ColumnSpacing = 8, CornerRadius = new CornerRadius(8), Padding = new Thickness(4) };
            box.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(72) });
            box.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            box.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(150) });
            box.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
            var time = new Button { Content = Clock(at), HorizontalAlignment = HorizontalAlignment.Stretch, CornerRadius = new CornerRadius(8) };
            ToolTipService.SetToolTip(time, W.StartTip);
            var words = new TextBox { Text = text, PlaceholderText = W.LinePlaceholder, CornerRadius = new CornerRadius(8) };
            var who = new ComboBox { ItemsSource = names, SelectedIndex = singer != null && names.Contains(singer) ? names.IndexOf(singer) : 0, HorizontalAlignment = HorizontalAlignment.Stretch, CornerRadius = new CornerRadius(8) };
            ToolTipService.SetToolTip(who, W.SingerTip);
            var remove = new Button { Content = new FontIcon { Glyph = "", FontSize = 13 }, CornerRadius = new CornerRadius(8) };
            ToolTipService.SetToolTip(remove, W.RemoveLineTip);
            Grid.SetColumn(words, 1);
            Grid.SetColumn(who, 2);
            Grid.SetColumn(remove, 3);
            box.Children.Add(time);
            box.Children.Add(words);
            box.Children.Add(who);
            box.Children.Add(remove);
            var row = new Row { At = at, Box = box, Time = time, Text = words, Singer = who };
            time.Click += (_, _) =>
            {
                row.At = PlayingAt();
                time.Content = Clock(row.At);
            };
            remove.Click += (_, _) =>
            {
                rows.Remove(row);
                list.Children.Remove(box);
            };
            if (index < 0 || index > rows.Count)
            {
                rows.Add(row);
                list.Children.Add(box);
            }
            else
            {
                rows.Insert(index, row);
                list.Children.Insert(index, box);
            }
        }
        foreach (var r in view.Rows) AddRow(r.At, r.Text, r.Singer);
        if (rows.Count == 0) AddRow(null, "", null);

        var add = new Button { Content = Labelled("", W.AddLine) };
        add.Click += (_, _) => AddRow(null, "", null);
        var paste = new Button { Content = Labelled("", W.PasteLyrics) };
        ToolTipService.SetToolTip(paste, W.PasteTip);
        paste.Click += async (_, _) =>
        {
            var clip = Windows.ApplicationModel.DataTransfer.Clipboard.GetContent();
            if (!clip.Contains(Windows.ApplicationModel.DataTransfer.StandardDataFormats.Text)) return;
            var text = await clip.GetTextAsync();
            rows.Clear();
            list.Children.Clear();
            foreach (var line in text.Split('\n').Select(l => l.Trim()).Where(l => l.Length > 0)) AddRow(null, line, null);
        };
        tap.Checked += (_, _) => Choose(0);
        tap.Unchecked += (_, _) => Choose(-1);
        var singer = new TextBox { PlaceholderText = W.AddSinger, Width = 160, CornerRadius = new CornerRadius(8) };
        singer.KeyDown += (_, e) =>
        {
            if (e.Key != Windows.System.VirtualKey.Enter) return;
            var name = singer.Text.Trim();
            if (name.Length == 0 || names.Contains(name)) return;
            names.Add(name);
            foreach (var r in rows)
            {
                var keep = r.Singer.SelectedIndex;
                r.Singer.ItemsSource = null;
                r.Singer.ItemsSource = names;
                r.Singer.SelectedIndex = keep;
            }
            singer.Text = "";
            e.Handled = true;
        };
        var tools = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        foreach (var t in new FrameworkElement[] { add, paste, tap, singer }) tools.Children.Add(t);

        var body = new StackPanel { Spacing = 12, MinWidth = 640 };
        body.Children.Add(new TextBlock { Text = W.EditorNote, TextWrapping = TextWrapping.Wrap, Style = (Style)Application.Current.Resources["CaptionTextBlockStyle"] });
        body.Children.Add(new ScrollViewer { Content = list, MaxHeight = 420, Padding = new Thickness(0, 0, 12, 0) });
        body.Children.Add(tools);
        var dialog = new ContentDialog
        {
            XamlRoot = root,
            Title = W.EditLyrics.TrimEnd('…'),
            Content = body,
            PrimaryButtonText = W.Save,
            CloseButtonText = "Cancel",
            // (no default button: Enter in a line's box would save the half-edited lyrics as yours; final review I6)
            DefaultButton = ContentDialogButton.None,
            CornerRadius = new CornerRadius(16),
        };
        dialog.Resources["ContentDialogMaxWidth"] = 900.0;
        // Tap Timing: Enter stamps the chosen line with the playing time, then the next is chosen
        dialog.PreviewKeyDown += (_, e) =>
        {
            if (tap.IsChecked != true || e.Key != Windows.System.VirtualKey.Enter || chosen < 0 || chosen >= rows.Count) return;
            var times = rows.Select(r => r.At).ToList();
            var next = TapTiming.Stamp(times, chosen, PlayingAt());
            rows[chosen].At = times[chosen];
            rows[chosen].Time.Content = Clock(rows[chosen].At);
            Choose(next < rows.Count ? next : -1);
            e.Handled = true;
        };
        if (await dialog.ShowAsync() != ContentDialogResult.Primary) return;
        var lines = rows.Select(r => new EditedLineView(r.At, r.Text.Text, r.Singer.SelectedIndex > 0 ? names[r.Singer.SelectedIndex] : null)).ToArray();
        hub.Engine.SaveLyrics(key, lines);
    }

    /// <summary>A button's leading icon and its words (every button carries an icon).</summary>
    private static StackPanel Labelled(string glyph, string text)
    {
        var p = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        p.Children.Add(new FontIcon { Glyph = glyph, FontSize = 14 });
        p.Children.Add(new TextBlock { Text = text });
        return p;
    }
}
