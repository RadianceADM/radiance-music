using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using RadianceMusic.Native;

namespace RadianceMusic.Controls;

/// <summary>A song as a card on Home, as Windows' Media Player shows albums: its cover (rounded),
/// title and names under it, one line each.</summary>
internal sealed class SongCard : Button
{
    private readonly Border cover;

    public SongCard(MusicEngine engine, SongView song, double size)
    {
        Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent);
        BorderThickness = new Thickness(0);
        Padding = new Thickness(8);
        Width = size + 16;
        HorizontalContentAlignment = HorizontalAlignment.Left;
        var column = new StackPanel { Spacing = 8 };
        cover = new Border
        {
            Width = size, Height = size, CornerRadius = new CornerRadius(8),
            Background = (Brush)Application.Current.Resources["CardBackgroundFillColorDefaultBrush"],
            BorderBrush = (Brush)Application.Current.Resources["CardStrokeColorDefaultBrush"],
            BorderThickness = new Thickness(1),
            Child = new FontIcon { Glyph = "\uE8D6", FontSize = size / 4, Opacity = 0.4 },
        };
        column.Children.Add(cover);
        var words = new StackPanel();
        words.Children.Add(new TextBlock { Text = song.Title, Style = (Style)Application.Current.Resources["BodyStrongTextBlockStyle"], TextTrimming = TextTrimming.CharacterEllipsis, TextWrapping = TextWrapping.NoWrap });
        words.Children.Add(new TextBlock { Text = song.Byline, Style = (Style)Application.Current.Resources["CaptionTextBlockStyle"], Foreground = (Brush)Application.Current.Resources["TextFillColorSecondaryBrush"], TextTrimming = TextTrimming.CharacterEllipsis, TextWrapping = TextWrapping.NoWrap });
        column.Children.Add(words);
        Content = column;
        ToolTipService.SetToolTip(this, song.Title + "\n" + song.Byline);
        _ = LoadArt(engine, song.Art, (uint)(size * 2));
    }

    private async Task LoadArt(MusicEngine engine, string source, uint px)
    {
        var image = await ArtCache.LoadAsync(engine, source, px);
        if (image == null) return;
        cover.Child = null;
        cover.Background = new ImageBrush { ImageSource = image, Stretch = Stretch.UniformToFill };
    }
}
