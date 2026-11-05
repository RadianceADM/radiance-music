using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using RadianceMusic.Native;

namespace RadianceMusic.Controls;

/// <summary>A song as a card on Home: its cover (rounded), title and names, one line each.</summary>
internal sealed class SongCard : Button
{
    private readonly Border cover;

    public SongCard(MusicEngine engine, SongView song, double size)
    {
        Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent);
        BorderThickness = new Thickness(0);
        CornerRadius = new CornerRadius(16);
        Padding = new Thickness(6);
        Width = size + 12;
        var column = new StackPanel { Spacing = 8 };
        cover = new Border { Width = size, Height = size, CornerRadius = new CornerRadius(10), Background = (Brush)Application.Current.Resources["CardBrush"] };
        column.Children.Add(cover);
        var words = new StackPanel { Spacing = 1 };
        words.Children.Add(new TextBlock { Text = song.Title, Style = (Style)Application.Current.Resources["CardTitle"] });
        words.Children.Add(new TextBlock { Text = song.Byline, Style = (Style)Application.Current.Resources["CardSub"] });
        column.Children.Add(words);
        Content = column;
        ToolTipService.SetToolTip(this, song.Title + "\n" + song.Byline);
        _ = LoadArt(engine, song.Art, (uint)(size * 2));
    }

    private async Task LoadArt(MusicEngine engine, string source, uint px)
    {
        var image = await ArtCache.LoadAsync(engine, source, px);
        if (image != null) cover.Background = new ImageBrush { ImageSource = image, Stretch = Stretch.UniformToFill };
    }
}
