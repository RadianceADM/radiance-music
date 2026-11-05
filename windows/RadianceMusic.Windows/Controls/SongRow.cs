using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using RadianceMusic.Native;

namespace RadianceMusic.Controls;

/// <summary>
/// A song in a list (Search): its cover, title, names, quality badge and length, as the GTK app's
/// row (ui.rs track_row, quality_badge's words). One line each, "…" when long. A button: Click plays.
/// </summary>
internal sealed class SongRow : Button
{
    private readonly Image art = new() { Stretch = Stretch.UniformToFill };

    public SongRow(MusicEngine engine, SongView song)
    {
        HorizontalAlignment = HorizontalAlignment.Stretch;
        HorizontalContentAlignment = HorizontalAlignment.Stretch;
        Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent);
        BorderThickness = new Thickness(0);
        CornerRadius = new CornerRadius(10);
        Padding = new Thickness(8, 6, 12, 6);
        var grid = new Grid { ColumnSpacing = 12 };
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(48) });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        var cover = new Border { Width = 48, Height = 48, CornerRadius = new CornerRadius(6), Background = (Brush)Application.Current.Resources["CardBrush"], Child = art };
        grid.Children.Add(cover);
        var words = new StackPanel { VerticalAlignment = VerticalAlignment.Center };
        words.Children.Add(new TextBlock { Text = song.Title, Style = (Style)Application.Current.Resources["CardTitle"] });
        words.Children.Add(new TextBlock { Text = song.Byline, Style = (Style)Application.Current.Resources["CardSub"] });
        Grid.SetColumn(words, 1);
        grid.Children.Add(words);
        var badge = Badge(song);
        Grid.SetColumn(badge, 2);
        grid.Children.Add(badge);
        var length = new TextBlock { Text = song.Length, VerticalAlignment = VerticalAlignment.Center, Style = (Style)Application.Current.Resources["Note"] };
        Grid.SetColumn(length, 3);
        grid.Children.Add(length);
        Content = grid;
        _ = LoadArt(engine, song.Art);
    }

    private async Task LoadArt(MusicEngine engine, string source) => art.Source = await ArtCache.LoadAsync(engine, source, 96);

    /// <summary>The GTK app's badge: a music video, lossless (green) or compressed (a cloud), with its words.</summary>
    private static FrameworkElement Badge(SongView s)
    {
        var (glyph, title, body, lossless) = s.Video
            ? ("\uE714", "Music video", "From YouTube; plays with its picture in Now Playing (up to 720p).", false)
            : s.Lossless
                ? ("\uE8D6", "Lossless · FLAC", "The full recording from the Internet Archive, nothing thrown away. Played first.", true)
                : ("\uE753", "Compressed · Opus", "Streams from YouTube Music, about 130–160 kbps. No lossless copy of this song was found.", false);
        var icon = new FontIcon { Glyph = glyph, FontSize = 15, VerticalAlignment = VerticalAlignment.Center, Margin = new Thickness(0, 0, 4, 0) };
        if (lossless) icon.Foreground = (Brush)Application.Current.Resources["LosslessFgBrush"];
        else icon.Opacity = 0.6;
        ToolTipService.SetToolTip(icon, title + "\n" + body);
        return icon;
    }
}
