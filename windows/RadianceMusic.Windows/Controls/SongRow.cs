using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using RadianceMusic.Native;

namespace RadianceMusic.Controls;

/// <summary>
/// A song in a list (an item of a ListView: Windows' own hover, focus and click): its cover, title,
/// its second line (the names, or "Saving…" / "Not saved yet" / "… · streams" for a liked song),
/// quality badge (the GTK app's words), plays ("N×"), length and heart. One line each, "…" when
/// long. The queue's rows have no heart (as the GTK app's).
/// </summary>
internal sealed class SongRow : Grid
{
    private readonly Border cover;

    /// <summary>The song's key (a click plays this song, wherever it is in the list by then).</summary>
    public string Key { get; }
    /// <summary>The song's title (the screenshots' queue log).</summary>
    public string Title { get; }

    public SongRow(MusicEngine engine, SongView song, bool heart = true, bool handle = false)
    {
        Key = song.Key;
        Title = song.Title;
        ColumnSpacing = 12;
        Padding = new Thickness(0, 6, 0, 6);
        ColumnDefinitions.Add(new ColumnDefinition { Width = handle ? GridLength.Auto : new GridLength(40) });
        ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(48) });
        ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        var res = Application.Current.Resources;
        cover = new Border { Width = 40, Height = 40, CornerRadius = new CornerRadius(4), Background = (Brush)res["CardBackgroundFillColorDefaultBrush"] };
        if (handle)
        {
            // the queue's Up next: the handle it's dragged by, before the art (the Linux app's list-drag-handle)
            var grip = new FontIcon { Glyph = "\uE76F", FontSize = 14, Foreground = (Brush)res["TextFillColorTertiaryBrush"], VerticalAlignment = VerticalAlignment.Center };
            ToolTipService.SetToolTip(grip, "Drag to move it in the queue");
            var lead = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 10, VerticalAlignment = VerticalAlignment.Center };
            lead.Children.Add(grip);
            lead.Children.Add(cover);
            Children.Add(lead);
        }
        else Children.Add(cover);
        var words = new StackPanel { VerticalAlignment = VerticalAlignment.Center };
        words.Children.Add(new TextBlock { Text = song.Title, Style = (Style)res["BodyTextBlockStyle"], TextTrimming = TextTrimming.CharacterEllipsis, TextWrapping = TextWrapping.NoWrap });
        words.Children.Add(new TextBlock { Text = song.Subtitle, Style = (Style)res["CaptionTextBlockStyle"], Foreground = (Brush)res["TextFillColorSecondaryBrush"], TextTrimming = TextTrimming.CharacterEllipsis, TextWrapping = TextWrapping.NoWrap });
        SetColumn(words, 1);
        Children.Add(words);
        // its split mark (stage 3: waiting, being split, split), then its kind
        var marks = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8, VerticalAlignment = VerticalAlignment.Center };
        if (SplitMark(song.Split) is { } mark) marks.Children.Add(mark);
        marks.Children.Add(Badge(song));
        SetColumn(marks, 2);
        Children.Add(marks);
        if (song.Plays > 0)
        {
            var plays = new TextBlock { Text = song.Plays + "×", VerticalAlignment = VerticalAlignment.Center, Style = (Style)res["CaptionTextBlockStyle"], Foreground = (Brush)res["TextFillColorTertiaryBrush"] };
            ToolTipService.SetToolTip(plays, song.Counts);
            SetColumn(plays, 3);
            Children.Add(plays);
        }
        var length = new TextBlock { Text = song.Length, VerticalAlignment = VerticalAlignment.Center, HorizontalAlignment = HorizontalAlignment.Right, Style = (Style)res["CaptionTextBlockStyle"], Foreground = (Brush)res["TextFillColorSecondaryBrush"] };
        SetColumn(length, 4);
        Children.Add(length);
        if (heart)
        {
            var h = Heart(song.Liked);
            h.Click += (_, _) => engine.ToggleLike(Key);
            SetColumn(h, 5);
            Children.Add(h);
        }
        _ = LoadArt(engine, song.Art);
    }

    /// <summary>The heart: filled red when liked ("Liked. Click to remove from Liked"), else "Like".</summary>
    internal static Button Heart(bool liked)
    {
        var icon = new FontIcon { Glyph = liked ? "\uEB52" : "\uEB51", FontSize = 15 };
        if (liked) icon.Foreground = (Brush)Application.Current.Resources["SystemFillColorCriticalBrush"];
        var b = new Button
        {
            Content = icon,
            Padding = new Thickness(8),
            VerticalAlignment = VerticalAlignment.Center,
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            BorderThickness = new Thickness(0),
        };
        ToolTipService.SetToolTip(b, liked ? "Liked. Click to remove from Liked" : "Like");
        return b;
    }

    private async Task LoadArt(MusicEngine engine, string source)
    {
        var image = await ArtCache.LoadAsync(engine, source, 80);
        if (image != null) cover.Background = new ImageBrush { ImageSource = image, Stretch = Stretch.UniformToFill };
    }

    /// <summary>The GTK app's badge, with its words: a music video, lossless (green) or compressed (a cloud).</summary>
    /// <summary>A song's split mark, as the Linux app's: a ring while it's split, a clock while it waits, scissors
    /// once it's split (its words: Ffi.SplitMarkTip).</summary>
    private static FrameworkElement? SplitMark(SplitMarkView m)
    {
        FrameworkElement? e = m switch
        {
            SplitMarkView.Splitting => new ProgressRing { IsActive = true, Width = 14, Height = 14 },
            SplitMarkView.Waiting => new FontIcon { Glyph = "\uE823", FontSize = 13, Foreground = (Brush)Application.Current.Resources["TextFillColorTertiaryBrush"] },
            SplitMarkView.Two or SplitMarkView.Four => new FontIcon { Glyph = "\uE8C6", FontSize = 13, Foreground = (Brush)Application.Current.Resources["AccentTextFillColorPrimaryBrush"] },
            _ => null,
        };
        if (e != null) ToolTipService.SetToolTip(e, RadianceMusic.Native.Native.SplitMarkTip(m));
        return e;
    }

    private static FrameworkElement Badge(SongView s)
    {
        var (glyph, title, body, lossless) = s.Video
            ? ("\uE714", "Music video", "From YouTube; plays with its picture in Now Playing (up to 720p).", false)
            : s.Lossless
                ? ("\uE8D6", "Lossless · FLAC", "The full recording from the Internet Archive, nothing thrown away. Played first.", true)
                : ("\uE753", "Compressed · Opus", "Streams from YouTube Music, about 130–160 kbps. No lossless copy of this song was found.", false);
        var icon = new FontIcon { Glyph = glyph, FontSize = 14, VerticalAlignment = VerticalAlignment.Center };
        icon.Foreground = (Brush)Application.Current.Resources[lossless ? "SystemFillColorSuccessBrush" : "TextFillColorTertiaryBrush"];
        ToolTipService.SetToolTip(icon, title + "\n" + body);
        return icon;
    }
}
