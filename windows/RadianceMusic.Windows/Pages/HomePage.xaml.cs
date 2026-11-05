using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Navigation;
using RadianceMusic.Controls;
using RadianceMusic.Native;

namespace RadianceMusic.Pages;

public sealed partial class HomePage : Page
{
    /// <summary>Cards in a row (core's words::ROW_CARDS), and their size (home.rs CARD, PHONE_CARD).</summary>
    private const int RowCards = 20;
    private const double Card = 180, PhoneCard = 136, Narrow = 860;
    private EngineHub? hub;
    private string shown = "";
    private string washFor = "";

    public HomePage()
    {
        InitializeComponent();
        SizeChanged += (_, e) =>
        {
            var phone = e.NewSize.Width < Narrow;
            Fan.Visibility = phone ? Visibility.Collapsed : Visibility.Visible;
            HeroContent.Padding = phone ? new Thickness(20, 28, 20, 28) : new Thickness(40, 34, 40, 34);
            if (phone != (cardSize == PhoneCard))
            {
                cardSize = phone ? PhoneCard : Card;
                shown = "";
                Render();
            }
        };
    }

    private double cardSize = Card;

    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        if (hub != null || e.Parameter is not EngineHub h) return;
        hub = h;
        hub.Change += kind =>
        {
            if (kind == ChangeKind.Home) Render();
        };
        Render();
    }

    private void Render()
    {
        if (hub == null) return;
        var home = hub.Engine.Home();
        Greeting.Text = home.Greeting;
        MixNote.Text = home.MixNote;
        var key = string.Join("|", home.Mix.Select(s => s.Key)) + "#" + string.Join("|", home.Recent.Take(RowCards).Select(s => s.Key)) + "#" + string.Join("|", home.Liked.Take(RowCards).Select(s => s.Key));
        if (key == shown) return;
        shown = key;
        HeroButtons.Children.Clear();
        if (home.Mix.Length > 0)
        {
            HeroButtons.Children.Add(Pill("\uE768", "Play your mix", true, () => hub.Engine.PlayFrom(ListKind.Mix, 0)));
            HeroButtons.Children.Add(Pill("\uE8B1", "Shuffle", false, () => hub.Engine.ShuffleFrom(ListKind.Mix)));
        }
        else
        {
            HeroButtons.Children.Add(Pill("\uE721", "Search for music", true, () => App.Window?.FocusSearch()));
        }
        var covers = (home.Mix.Length > 0 ? home.Mix : home.Liked).Take(4).ToList();
        _ = Fill(covers);
        Rows.Children.Clear();
        AddRow("Made for you", home.Mix, ListKind.Mix, Refresh());
        AddRow("Recently played", home.Recent, ListKind.Recent, null);
        AddRow("Your liked songs", home.Liked, ListKind.Liked, null);
    }

    /// <summary>The hero's wash (the first cover blurred) and its covers fanned like records.</summary>
    private async Task Fill(List<SongView> covers)
    {
        Fan.Children.Clear();
        if (covers.Count == 0 || hub == null)
        {
            Wash.Background = null;
            return;
        }
        if (washFor != covers[0].Art)
        {
            washFor = covers[0].Art;
            var source = covers[0].Art;
            var files = await Task.Run(() => hub.Engine.Backdrop(source));
            if (files != null && washFor == source)
                Wash.Background = new ImageBrush { ImageSource = new Microsoft.UI.Xaml.Media.Imaging.BitmapImage(new Uri(files.Wash)), Stretch = Stretch.UniformToFill };
        }
        double[] turns = { -12, -4, 4, 12 };
        for (var i = covers.Count - 1; i >= 0; i--)
        {
            var image = await ArtCache.LoadAsync(hub.Engine, covers[i].Art, 280);
            var b = new Border
            {
                Width = 140, Height = 140, CornerRadius = new CornerRadius(12),
                Background = image != null ? new ImageBrush { ImageSource = image, Stretch = Stretch.UniformToFill } : (Brush)Application.Current.Resources["CardBrush"],
                RenderTransformOrigin = new Windows.Foundation.Point(0.5, 1),
                RenderTransform = new CompositeTransform { Rotation = turns[(i + (4 - covers.Count)) % 4], TranslateX = (i - (covers.Count - 1) / 2.0) * 44 },
                HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Center,
            };
            Fan.Children.Add(b);
        }
    }

    private Button Refresh()
    {
        var b = new Button { Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent), BorderThickness = new Thickness(0), CornerRadius = new CornerRadius(999), Content = new FontIcon { Glyph = "\uE72C", FontSize = 14 } };
        ToolTipService.SetToolTip(b, "New picks");
        b.Click += (_, _) => hub?.Engine.RefreshMix();
        return b;
    }

    /// <summary>A row of cards (none: no row), as home.rs carousel.</summary>
    private void AddRow(string title, SongView[] songs, ListKind list, FrameworkElement? extra)
    {
        if (songs.Length == 0 || hub == null) return;
        var head = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        head.Children.Add(new TextBlock { Text = title, Style = (Style)Application.Current.Resources["SectionTitle"], VerticalAlignment = VerticalAlignment.Center });
        if (extra != null) head.Children.Add(extra);
        var cards = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 6 };
        for (var i = 0; i < Math.Min(songs.Length, RowCards); i++)
        {
            var index = (uint)i;
            var card = new SongCard(hub.Engine, songs[i], cardSize);
            card.Click += (_, _) => hub.Engine.PlayFrom(list, index);
            cards.Children.Add(card);
        }
        var scroll = new ScrollViewer { HorizontalScrollBarVisibility = ScrollBarVisibility.Auto, VerticalScrollBarVisibility = ScrollBarVisibility.Disabled, HorizontalScrollMode = ScrollMode.Enabled, VerticalScrollMode = ScrollMode.Disabled, Content = cards };
        var section = new StackPanel { Spacing = 8 };
        section.Children.Add(head);
        section.Children.Add(scroll);
        Rows.Children.Add(section);
    }

    /// <summary>A pill button with its icon (accent: the main one).</summary>
    private static Button Pill(string glyph, string text, bool accent, Action click)
    {
        var content = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        content.Children.Add(new FontIcon { Glyph = glyph, FontSize = 15 });
        content.Children.Add(new TextBlock { Text = text, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold });
        var b = new Button { Content = content, CornerRadius = new CornerRadius(999), Padding = new Thickness(18, 9, 20, 9) };
        if (accent) b.Style = (Style)Application.Current.Resources["AccentButtonStyle"];
        b.Click += (_, _) => click();
        return b;
    }
}
