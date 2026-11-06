using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using Microsoft.UI.Xaml.Navigation;
using RadianceMusic.Native;

namespace RadianceMusic.Pages;

/// <summary>
/// Albums, Artists or Genres, as the GTK app's grid pages: a tile for each (A to Z); an album's
/// square art and who's on it, an artist's round art and "N songs", a genre's colours (its first
/// song's wash) under its name. A click opens its songs.
/// </summary>
internal sealed class LibraryPage : Page
{
    private EngineHub? hub;
    private GroupKind kind;
    private readonly TextBlock title = PageKit.Title("");
    private readonly ItemsRepeater tiles = new();
    private string shown = "-";

    public LibraryPage()
    {
        NavigationCacheMode = NavigationCacheMode.Disabled;
        tiles.Layout = new UniformGridLayout { MinItemWidth = 180, MinColumnSpacing = 12, MinRowSpacing = 16, ItemsStretch = UniformGridLayoutItemsStretch.Fill };
        var body = new StackPanel { Padding = new Thickness(36, 20, 36, 36), Spacing = 16, MaxWidth = 1600 };
        body.Children.Add(title);
        body.Children.Add(tiles);
        Content = new ScrollViewer { Content = body };
    }

    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        if (e.Parameter is not PageArgs { What: GroupKind k } a) return;
        hub = a.Hub;
        kind = k;
        title.Text = KindName(k);
        hub.Change += OnChange;
        Render();
    }

    protected override void OnNavigatedFrom(NavigationEventArgs e)
    {
        if (hub != null) hub.Change -= OnChange;
    }

    private void OnChange(ChangeKind what)
    {
        if (what == ChangeKind.Library) Render();
    }

    public static string KindName(GroupKind k) => k switch { GroupKind.Albums => "Albums", GroupKind.Artists => "Artists", _ => "Genres" };

    private void Render()
    {
        if (hub == null) return;
        var list = hub.Engine.Library(kind);
        var key = string.Join("|", list.Select(t => t.Name + t.Sub));
        if (key == shown) return;
        shown = key;
        tiles.ItemsSource = list.Select(Tile).ToList();
    }

    private Button Tile(TileView t)
    {
        var genre = kind == GroupKind.Genres;
        var art = new Border
        {
            Height = genre ? 120 : 180,
            CornerRadius = new CornerRadius(t.Round ? 90 : 8),
            Width = t.Round ? 180 : double.NaN,
            HorizontalAlignment = t.Round ? HorizontalAlignment.Center : HorizontalAlignment.Stretch,
            Background = (Brush)Application.Current.Resources["CardBackgroundFillColorDefaultBrush"],
        };
        var content = new StackPanel { Spacing = 4 };
        if (genre)
        {
            var face = new Grid { Height = 120, CornerRadius = new CornerRadius(8) };
            face.Children.Add(art);
            face.Children.Add(new Border { Background = new SolidColorBrush(Windows.UI.Color.FromArgb(0x66, 0, 0, 0)), CornerRadius = new CornerRadius(8) });
            face.Children.Add(new TextBlock { Text = t.Name, Style = (Style)Application.Current.Resources["SubtitleTextBlockStyle"], Foreground = new SolidColorBrush(Microsoft.UI.Colors.White), Margin = new Thickness(14), VerticalAlignment = VerticalAlignment.Bottom, TextTrimming = TextTrimming.CharacterEllipsis });
            content.Children.Add(face);
            _ = Wash(art, t.Art);
        }
        else
        {
            content.Children.Add(art);
            content.Children.Add(new TextBlock { Text = t.Name, Style = (Style)Application.Current.Resources["BodyStrongTextBlockStyle"], TextTrimming = TextTrimming.CharacterEllipsis, HorizontalAlignment = t.Round ? HorizontalAlignment.Center : HorizontalAlignment.Left });
            content.Children.Add(new TextBlock { Text = t.Sub, Style = (Style)Application.Current.Resources["CaptionTextBlockStyle"], Foreground = (Brush)Application.Current.Resources["TextFillColorSecondaryBrush"], TextTrimming = TextTrimming.CharacterEllipsis, HorizontalAlignment = t.Round ? HorizontalAlignment.Center : HorizontalAlignment.Left });
            _ = Art(art, t.Art);
        }
        var b = new Button { Content = content, Padding = new Thickness(8), HorizontalAlignment = HorizontalAlignment.Stretch, HorizontalContentAlignment = HorizontalAlignment.Stretch, Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent), BorderThickness = new Thickness(0) };
        var name = t.Name;
        b.Click += (_, _) => App.Window?.Show("group", (kind, name));
        return b;
    }

    private async Task Art(Border to, string source)
    {
        if (hub == null) return;
        if (await ArtCache.LoadAsync(hub.Engine, source, 360) is { } image) to.Background = new ImageBrush { ImageSource = image, Stretch = Stretch.UniformToFill };
    }

    /// <summary>A genre: its first song's colours (the backdrop's wash, as the GTK app's genre_tile).</summary>
    private async Task Wash(Border to, string source)
    {
        if (hub == null) return;
        var files = await Task.Run(() => hub.Engine.Backdrop(source));
        if (files != null && File.Exists(files.Wash)) to.Background = new ImageBrush { ImageSource = new BitmapImage(new Uri(files.Wash)), Stretch = Stretch.UniformToFill };
    }
}
