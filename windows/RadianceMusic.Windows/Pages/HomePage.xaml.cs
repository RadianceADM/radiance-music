using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Navigation;
using RadianceMusic.Controls;
using RadianceMusic.Native;

namespace RadianceMusic.Pages;

public sealed partial class HomePage : Page
{
    /// <summary>Cards in a row (core's words::ROW_CARDS), and their size (smaller in a narrow window).</summary>
    private const int RowCards = 20;
    private const double Card = 160, SmallCard = 128, Narrow = 640;
    private EngineHub? hub;
    private string shown = "";
    private double cardSize = Card;

    public HomePage()
    {
        InitializeComponent();
        SizeChanged += (_, e) =>
        {
            var size = e.NewSize.Width < Narrow ? SmallCard : Card;
            if (size == cardSize) return;
            cardSize = size;
            shown = "";
            Render();
        };
    }

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
            HeroButtons.Children.Add(Labeled("\uE768", "Play your mix", true, () => hub.Engine.PlayFrom(ListKind.Mix, 0)));
            HeroButtons.Children.Add(Labeled("\uE8B1", "Shuffle", false, () => hub.Engine.ShuffleFrom(ListKind.Mix)));
        }
        else
        {
            HeroButtons.Children.Add(Labeled("\uE721", "Search for music", true, () => App.Window?.FocusSearch()));
        }
        Rows.Children.Clear();
        AddRow("Made for you", home.Mix, ListKind.Mix, Refresh());
        AddRow("Recently played", home.Recent, ListKind.Recent, null);
        AddRow("Your liked songs", home.Liked, ListKind.Liked, null);
    }

    private Button Refresh()
    {
        var b = new Button { Content = new FontIcon { Glyph = "\uE72C", FontSize = 14 }, Padding = new Thickness(8), Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent), BorderThickness = new Thickness(0) };
        ToolTipService.SetToolTip(b, "New picks");
        b.Click += (_, _) => hub?.Engine.RefreshMix();
        return b;
    }

    /// <summary>A row of cards (none: no row), as home.rs's.</summary>
    private void AddRow(string title, SongView[] songs, ListKind list, FrameworkElement? extra)
    {
        if (songs.Length == 0 || hub == null) return;
        var head = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 4 };
        head.Children.Add(new TextBlock { Text = title, Style = (Style)Application.Current.Resources["SubtitleTextBlockStyle"], VerticalAlignment = VerticalAlignment.Center });
        if (extra != null) head.Children.Add(extra);
        var cards = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 4 };
        for (var i = 0; i < Math.Min(songs.Length, RowCards); i++)
        {
            var index = (uint)i;
            var card = new SongCard(hub.Engine, songs[i], cardSize);
            card.Click += (_, _) => hub.Engine.PlayFrom(list, index);
            cards.Children.Add(card);
        }
        var scroll = new ScrollViewer { HorizontalScrollBarVisibility = ScrollBarVisibility.Auto, VerticalScrollBarVisibility = ScrollBarVisibility.Disabled, HorizontalScrollMode = ScrollMode.Enabled, VerticalScrollMode = ScrollMode.Disabled, Content = cards, Padding = new Thickness(0, 0, 0, 12) };
        var section = new StackPanel { Spacing = 8 };
        section.Children.Add(head);
        section.Children.Add(scroll);
        Rows.Children.Add(section);
    }

    /// <summary>A button with its icon (accent: the main one).</summary>
    internal static Button Labeled(string glyph, string text, bool accent, Action click)
    {
        var content = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        content.Children.Add(new FontIcon { Glyph = glyph, FontSize = 16 });
        content.Children.Add(new TextBlock { Text = text });
        var b = new Button { Content = content, Padding = new Thickness(16, 8, 16, 8) };
        if (accent) b.Style = (Style)Application.Current.Resources["AccentButtonStyle"];
        b.Click += (_, _) => click();
        return b;
    }
}
