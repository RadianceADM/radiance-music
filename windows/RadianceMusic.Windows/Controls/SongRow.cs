using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using RadianceMusic.Native;

namespace RadianceMusic.Controls;

/// <summary>
/// A song in a list (an item of a ListView: Windows' own hover, focus and click): its cover, title,
/// names, quality badge (the GTK app's words) and length. One line each, "…" when long.
/// </summary>
internal sealed class SongRow : Grid
{
    private readonly Border cover;

    /// <summary>The song's key (a click plays this song, wherever it is in the list by then).</summary>
    public string Key { get; }

    public SongRow(MusicEngine engine, SongView song)
    {
        Key = song.Key;
        ColumnSpacing = 12;
        Padding = new Thickness(0, 6, 0, 6);
        ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(40) });
        ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(48) });
        var res = Application.Current.Resources;
        cover = new Border { Width = 40, Height = 40, CornerRadius = new CornerRadius(4), Background = (Brush)res["CardBackgroundFillColorDefaultBrush"] };
        Children.Add(cover);
        var words = new StackPanel { VerticalAlignment = VerticalAlignment.Center };
        words.Children.Add(new TextBlock { Text = song.Title, Style = (Style)res["BodyTextBlockStyle"], TextTrimming = TextTrimming.CharacterEllipsis, TextWrapping = TextWrapping.NoWrap });
        words.Children.Add(new TextBlock { Text = song.Byline, Style = (Style)res["CaptionTextBlockStyle"], Foreground = (Brush)res["TextFillColorSecondaryBrush"], TextTrimming = TextTrimming.CharacterEllipsis, TextWrapping = TextWrapping.NoWrap });
        SetColumn(words, 1);
        Children.Add(words);
        var badge = Badge(song);
        SetColumn(badge, 2);
        Children.Add(badge);
        var length = new TextBlock { Text = song.Length, VerticalAlignment = VerticalAlignment.Center, HorizontalAlignment = HorizontalAlignment.Right, Style = (Style)res["CaptionTextBlockStyle"], Foreground = (Brush)res["TextFillColorSecondaryBrush"] };
        SetColumn(length, 3);
        Children.Add(length);
        _ = LoadArt(engine, song.Art);
    }

    private async Task LoadArt(MusicEngine engine, string source)
    {
        var image = await ArtCache.LoadAsync(engine, source, 80);
        if (image != null) cover.Background = new ImageBrush { ImageSource = image, Stretch = Stretch.UniformToFill };
    }

    /// <summary>The GTK app's badge, with its words: a music video, lossless (green) or compressed (a cloud).</summary>
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
