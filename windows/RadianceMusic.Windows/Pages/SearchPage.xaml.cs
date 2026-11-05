using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Navigation;
using RadianceMusic.Controls;
using RadianceMusic.Native;
using Ffi = RadianceMusic.Native.Native;

namespace RadianceMusic.Pages;

public sealed partial class SearchPage : Page
{
    private EngineHub? hub;
    private Debounce? debounce;
    private string shown = "\u0000";

    public SearchPage()
    {
        InitializeComponent();
    }

    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        if (hub != null || e.Parameter is not EngineHub h) return;
        hub = h;
        debounce = new Debounce(Debounce.SearchWait, q => hub.Engine.Search(q), new UiDelay());
        hub.Change += kind =>
        {
            if (kind is ChangeKind.Results or ChangeKind.Online) Render();
        };
        Tiles.ItemsSource = Ffi.BrowseTiles().Select((t, i) => Tile(t, i)).ToList();
        Render();
    }

    /// <summary>`q` typed in and searched (Home's tiles, screenshots).</summary>
    public void Search(string q)
    {
        Box.Text = q;
        hub?.Engine.Search(q);
    }

    public void FocusBox() => Box.Focus(FocusState.Programmatic);

    private Button Tile(BrowseTile t, int i)
    {
        var b = new Button
        {
            Background = (Brush)Application.Current.Resources[$"b{i % 6}"],
            CornerRadius = new CornerRadius(16),
            BorderThickness = new Thickness(0),
            HorizontalAlignment = HorizontalAlignment.Stretch,
            VerticalAlignment = VerticalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Left,
            VerticalContentAlignment = VerticalAlignment.Bottom,
            Padding = new Thickness(16, 14, 16, 14),
            Content = new TextBlock { Text = t.Label, FontSize = 17, FontWeight = Microsoft.UI.Text.FontWeights.ExtraBold, Foreground = new SolidColorBrush(Microsoft.UI.Colors.White) },
        };
        b.Click += (_, _) => Search(t.Query);
        return b;
    }

    private void OnTyped(AutoSuggestBox sender, AutoSuggestBoxTextChangedEventArgs e)
    {
        if (e.Reason == AutoSuggestionBoxTextChangeReason.UserInput) debounce?.Poke(sender.Text);
    }

    private void OnSubmit(AutoSuggestBox sender, AutoSuggestBoxQuerySubmittedEventArgs e) => hub?.Engine.Search(sender.Text);

    private void Render()
    {
        if (hub == null) return;
        var r = hub.Engine.Results();
        var typed = r.Query.Trim().Length >= 2;
        BrowseArea.Visibility = typed ? Visibility.Collapsed : Visibility.Visible;
        ResultsArea.Visibility = typed ? Visibility.Visible : Visibility.Collapsed;
        if (!typed) return;
        MineSection.Visibility = r.Mine.Count > 0 ? Visibility.Visible : Visibility.Collapsed;
        MineNote.Text = r.MineNote;
        LosslessNote.Text = r.LosslessNote;
        SongsNote.Text = r.SongsNote;
        LosslessSpin.IsActive = r.Lossless == null;
        SongsSpin.IsActive = r.Songs == null;
        var key = r.Query + "|" + r.Mine.Count + "|" + (r.Lossless?.Count ?? -1) + "|" + (r.Songs?.Count ?? -1);
        if (key == shown) return;
        shown = key;
        Fill(MineRows, r.Mine, ListKind.Mine);
        Fill(LosslessRows, r.Lossless ?? new List<SongView>(), ListKind.Lossless);
        Fill(SongsRows, r.Songs ?? new List<SongView>(), ListKind.Songs);
    }

    private void Fill(StackPanel rows, List<SongView> songs, ListKind list)
    {
        rows.Children.Clear();
        for (var i = 0; i < songs.Count; i++)
        {
            var index = (uint)i;
            var row = new SongRow(hub!.Engine, songs[i]);
            row.Click += (_, _) => hub!.Engine.PlayFrom(list, index);
            rows.Children.Add(row);
        }
    }
}
