using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Navigation;
using RadianceMusic.Controls;
using RadianceMusic.Native;
using Ffi = RadianceMusic.Native.Native;

namespace RadianceMusic.Pages;

/// <summary>
/// History, as the GTK app's: the plays by day ("Today", "Yesterday", the weekday, the date),
/// each with when; a click plays on through the history from that play.
/// </summary>
public sealed partial class HistoryPage : Page
{
    private EngineHub? hub;
    private readonly StackPanel body = new() { Padding = new Thickness(36, 20, 36, 36), Spacing = 8, MaxWidth = 1400 };
    private string shown = "-";

    public HistoryPage()
    {
        InitializeComponent();
        NavigationCacheMode = NavigationCacheMode.Required;
        Content = new ScrollViewer { Content = body };
    }

    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        if (hub != null) return;
        hub = e.Parameter switch { EngineHub h => h, PageArgs a => a.Hub, _ => null };
        if (hub == null) return;
        hub.Change += kind =>
        {
            if (kind is ChangeKind.History or ChangeKind.Liked) Render();
        };
        Render();
    }

    private void Render()
    {
        if (hub == null) return;
        var days = hub.Engine.History();
        var key = string.Join("|", days.Select(d => d.Label + d.Plays.Length + (d.Plays.FirstOrDefault()?.Time ?? "")));
        if (key == shown) return;
        shown = key;
        body.Children.Clear();
        body.Children.Add(PageKit.Title("History"));
        if (days.Length == 0)
        {
            body.Children.Add(PageKit.Note(Ffi.HistoryEmpty()));
            return;
        }
        foreach (var day in days)
        {
            var head = PageKit.Subtitle(day.Label);
            head.Margin = new Thickness(0, 16, 0, 0);
            body.Children.Add(head);
            var list = PageKit.SongList();
            foreach (var play in day.Plays)
            {
                var p = play;
                var row = new Grid { ColumnSpacing = 12 };
                row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
                row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
                row.Children.Add(new SongRow(hub.Engine, p.Song));
                var when = new TextBlock { Text = p.Time, VerticalAlignment = VerticalAlignment.Center, Style = (Style)Application.Current.Resources["CaptionTextBlockStyle"], Foreground = (Brush)Application.Current.Resources["TextFillColorSecondaryBrush"] };
                Grid.SetColumn(when, 1);
                row.Children.Add(when);
                row.Tag = p;
                SongMenu.Attach(row, hub, p.Song.Key, p.Index, new MenuPlace.History(), () => hub.Engine.PlayHistory(p.Index, p.Song.Key));
                list.Items.Add(row);
            }
            list.ItemClick += (_, e) =>
            {
                if (e.ClickedItem is Grid { Tag: PlayView p }) hub.Engine.PlayHistory(p.Index, p.Song.Key);
            };
            body.Children.Add(list);
        }
    }
}
