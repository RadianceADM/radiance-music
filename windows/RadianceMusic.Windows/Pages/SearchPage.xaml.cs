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
    private string shown = "-";

    public SearchPage()
    {
        InitializeComponent();
    }

    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        if (hub != null || e.Parameter is not EngineHub h) return;
        hub = h;
        debounce = new Debounce(Debounce.SearchWait, q =>
        {
            hub.Engine.Search(q);
            hub.Engine.Suggest(q); // (the same 300 ms: suggestions with the results)
        }, new UiDelay());
        hub.Change += kind =>
        {
            if (kind is ChangeKind.Results or ChangeKind.Online or ChangeKind.Liked) Render();
            if (kind == ChangeKind.Suggestions) ShowSuggestions();
        };
        // the box's own list, Windows' way (the Linux app's chips): your artists before you type
        Box.UpdateTextOnSelect = false;
        Box.GotFocus += (_, _) =>
        {
            if (Box.Text.Trim().Length == 0) hub.Engine.Suggest("");
        };
        Tiles.ItemsSource = Ffi.BrowseTiles().Select(Tile).ToList();
        Render();
    }

    /// <summary>`q` typed in and searched (a browse pick, screenshots).</summary>
    public void Search(string q)
    {
        Box.Text = q;
        hub?.Engine.Search(q);
    }

    public void FocusBox() => Box.Focus(FocusState.Programmatic);

    /// <summary>`q` in the box and its suggestions asked for, as typing it would (screenshots).</summary>
    public void Typed(string q)
    {
        Box.Text = q;
        FocusBox();
        hub?.Engine.Suggest(q);
    }

    /// <summary>A suggestion from your liked songs is marked so (a heart before it).</summary>
    private const string Mine = "\u2665  ";

    private void ShowSuggestions()
    {
        if (hub == null) return;
        var items = hub.Engine.Suggestions().Select(c => c.Mine ? Mine + c.Text : c.Text).ToList();
        Box.ItemsSource = items;
        // (the box opens its list by itself only as you type: your songs on an empty box's focus open it too)
        // (a hidden screenshot's window never takes the keyboard: its list shows for the shot all the same)
        Box.IsSuggestionListOpen = items.Count > 0 && (Box.FocusState != FocusState.Unfocused || Program.Dev.Hidden);
        App.Trace($"suggestions: {items.Count}, focus {Box.FocusState}, list open {Box.IsSuggestionListOpen}");
    }

    private Button Tile(BrowseTile t)
    {
        var content = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 12 };
        content.Children.Add(new FontIcon { Glyph = "\uE8D6", FontSize = 16, Foreground = (Brush)Application.Current.Resources["AccentTextFillColorPrimaryBrush"] });
        content.Children.Add(new TextBlock { Text = t.Label, Style = (Style)Application.Current.Resources["BodyStrongTextBlockStyle"], VerticalAlignment = VerticalAlignment.Center });
        var b = new Button
        {
            Content = content,
            HorizontalAlignment = HorizontalAlignment.Stretch,
            VerticalAlignment = VerticalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Left,
            Padding = new Thickness(16, 12, 16, 12),
        };
        b.Click += (_, _) => Search(t.Query);
        return b;
    }

    private void OnTyped(AutoSuggestBox sender, AutoSuggestBoxTextChangedEventArgs e)
    {
        if (e.Reason == AutoSuggestionBoxTextChangeReason.UserInput) debounce?.Poke(sender.Text);
    }

    /// <summary>Enter, the search button, or a suggestion clicked or picked with the arrow keys and Enter
    /// (SuggestionChosen fires on every arrow key's highlight: not searched there).</summary>
    private void OnSubmit(AutoSuggestBox sender, AutoSuggestBoxQuerySubmittedEventArgs e)
    {
        if (e.ChosenSuggestion is string chosen) Search(chosen.StartsWith(Mine) ? chosen[Mine.Length..] : chosen);
        else hub?.Engine.Search(sender.Text);
    }

    private void OnPlay(object sender, ItemClickEventArgs e)
    {
        if (hub == null || sender is not ListView list || e.ClickedItem is not SongRow row) return;
        var kind = (string)list.Tag switch { "Mine" => ListKind.Mine, "Lossless" => ListKind.Lossless, _ => ListKind.Songs };
        hub.Engine.PlayFrom(kind, (uint)list.Items.IndexOf(row), row.Key);
    }

    private void Render()
    {
        if (hub == null) return;
        var r = hub.Engine.Results();
        var typed = r.Query.Trim().Length >= 2;
        BrowseArea.Visibility = typed ? Visibility.Collapsed : Visibility.Visible;
        ResultsArea.Visibility = typed ? Visibility.Visible : Visibility.Collapsed;
        if (!typed) return;
        MineSection.Visibility = r.Mine.Length > 0 ? Visibility.Visible : Visibility.Collapsed;
        MineNote.Text = r.MineNote;
        LosslessNote.Text = r.LosslessNote;
        SongsNote.Text = r.SongsNote;
        LosslessSpin.IsActive = r.Lossless == null;
        SongsSpin.IsActive = r.Songs == null;
        // (the hearts too: a like from here, or from anywhere, fills or empties its row's)
        var liked = string.Concat(r.Mine.Concat(r.Lossless ?? Array.Empty<SongView>()).Concat(r.Songs ?? Array.Empty<SongView>()).Select(s => s.Liked ? '1' : '0'));
        var key = r.Query + "|" + r.Mine.Length + "|" + (r.Lossless?.Length ?? -1) + "|" + (r.Songs?.Length ?? -1) + "|" + liked;
        if (key == shown) return;
        shown = key;
        Fill(MineList, r.Mine);
        Fill(LosslessList, r.Lossless ?? Array.Empty<SongView>());
        Fill(SongsList, r.Songs ?? Array.Empty<SongView>());
    }

    private void Fill(ListView list, SongView[] songs)
    {
        list.Items.Clear();
        foreach (var song in songs) list.Items.Add(new SongRow(hub!.Engine, song));
    }
}
