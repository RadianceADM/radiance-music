using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Navigation;
using RadianceMusic.Controls;
using RadianceMusic.Native;

namespace RadianceMusic.Pages;

/// <summary>
/// Liked Songs, as the GTK app's: its own search ("Search your liked songs"), the words under it,
/// Play / Shuffle / Fix missing, the songs A to Z with the rail beside them.
/// </summary>
public sealed partial class LikedPage : Page
{
    private EngineHub? hub;
    private Debounce? debounce;
    private readonly AutoSuggestBox box = new() { PlaceholderText = "Search your liked songs", QueryIcon = new SymbolIcon(Symbol.Find), MaxWidth = 640, HorizontalAlignment = HorizontalAlignment.Stretch };
    private readonly TextBlock note = PageKit.Note();
    private readonly Button play, shuffle, fix;
    private readonly ListView list = PageKit.SongList();
    private readonly AzRail rail = new();
    private LikedPageView? page;
    private string shown = "-";

    public LikedPage()
    {
        InitializeComponent();
        NavigationCacheMode = NavigationCacheMode.Required;
        play = PageKit.IconButton("", "Play", () => { if (page is { Songs.Length: > 0 } p) hub?.Engine.PlayLiked(0, p.Songs[0].Key); }, accent: true);
        shuffle = PageKit.IconButton("", "Shuffle", () => hub?.Engine.ShuffleLiked());
        fix = PageKit.IconButton("", "", () => hub?.Engine.FixMissing());
        ToolTipService.SetToolTip(fix, "Saves every liked song whose file or music video isn't on this computer (with its lyrics and cover), in the background");
        var buttons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        foreach (var b in new[] { play, shuffle, fix }) buttons.Children.Add(b);
        var head = new StackPanel { Padding = new Thickness(36, 20, 36, 8), Spacing = 12 };
        head.Children.Add(PageKit.Title("Liked Songs"));
        head.Children.Add(box);
        head.Children.Add(note);
        head.Children.Add(buttons);
        var body = new Grid { Padding = new Thickness(36, 0, 12, 0), ColumnSpacing = 8 };
        body.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        body.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        body.Children.Add(list);
        // the whole rail stays reachable in a short window: it only ever shrinks to fit its column
        var fit = new Viewbox { Child = rail, Stretch = Stretch.Uniform, StretchDirection = StretchDirection.DownOnly, VerticalAlignment = VerticalAlignment.Center, Margin = new Thickness(0, 4, 0, 4) };
        Grid.SetColumn(fit, 1);
        body.Children.Add(fit);
        var root = new Grid();
        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        root.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        root.Children.Add(head);
        Grid.SetRow(body, 1);
        root.Children.Add(body);
        Content = root;
        box.TextChanged += (s, e) =>
        {
            if (e.Reason == AutoSuggestionBoxTextChangeReason.UserInput) debounce?.Poke(s.Text);
        };
        box.QuerySubmitted += (_, _) => Render();
        list.ItemClick += (_, e) =>
        {
            if (e.ClickedItem is SongRow row) hub?.Engine.PlayLiked((uint)list.Items.IndexOf(row), row.Key);
        };
        rail.Jump += JumpTo;
        list.Loaded += (_, _) =>
        {
            if (PageKit.ScrollerOf(list) is { } s) s.ViewChanged += (_, _) => LightLetter();
        };
    }

    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        if (hub != null) return;
        hub = e.Parameter switch { EngineHub h => h, PageArgs a => a.Hub, _ => null };
        if (hub == null) return;
        debounce = new Debounce(Debounce.SearchWait, _ => Render(), new UiDelay());
        hub.Change += kind =>
        {
            if (kind is ChangeKind.Liked or ChangeKind.Library) Render();
        };
        Render();
    }

    private void Render()
    {
        if (hub == null) return;
        page = hub.Engine.LikedPage(box.Text);
        note.Text = page.Note;
        play.IsEnabled = page.Songs.Length > 0;
        shuffle.IsEnabled = page.Songs.Length > 0;
        fix.Visibility = page.Fix == null ? Visibility.Collapsed : Visibility.Visible;
        PageKit.SetText(fix, page.Fix ?? "");
        fix.IsEnabled = !page.Fixing;
        rail.SetLetters(page.Letters);
        var key = string.Join("|", page.Songs.Select(s => s.Key + s.Subtitle + s.Liked + s.Plays));
        if (key == shown) return;
        shown = key;
        list.Items.Clear();
        for (var i = 0; i < page.Songs.Length; i++)
        {
            var song = page.Songs[i];
            var index = (uint)i;
            var row = new SongRow(hub.Engine, song);
            SongMenu.Attach(row, hub, song.Key, index, new MenuPlace.Row(), () => hub.Engine.PlayLiked(index, song.Key));
            list.Items.Add(row);
        }
        LightLetter();
    }

    /// <summary>The rail's letter picked: the list goes to the first song of it (or of the next letter that has one).</summary>
    /// <summary>A search typed into the page's own box (DevActs: the screenshots), as a person's.</summary>
    internal void SearchFor(string q)
    {
        box.Text = q;
        debounce?.Poke(q);
    }

    private void JumpTo(int letter)
    {
        if (page == null) return;
        for (var k = letter; k < page.Letters.Length; k++)
        {
            if (page.Letters[k].First is not { } first || first >= list.Items.Count) continue;
            list.ScrollIntoView(list.Items[(int)first], ScrollIntoViewAlignment.Leading);
            rail.Light(k);
            return;
        }
    }

    /// <summary>The letter of the songs at the top of the list (AzLight's rule, the GTK app's).</summary>
    private void LightLetter()
    {
        if (page == null) return;
        var lettered = page.Letters.Select((l, i) => (l, i)).Where(x => x.l.First != null).ToList();
        var firstShown = list.ItemsPanelRoot is ItemsStackPanel p ? p.FirstVisibleIndex : 0;
        var tops = lettered.Select(x =>
        {
            var row = (int)x.l.First!.Value;
            if (list.ContainerFromIndex(row) is UIElement c) return c.TransformToVisual(list).TransformPoint(new Windows.Foundation.Point(0, 0)).Y;
            return row <= firstShown ? -1e6 : 1e6; // not drawn: above or below what's shown
        }).ToList();
        rail.Light(AzLight.Pick(tops) is { } k ? lettered[k].i : null);
    }
}
