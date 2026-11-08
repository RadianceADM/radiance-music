using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Navigation;
using RadianceMusic.Controls;
using RadianceMusic.Native;
using Ffi = RadianceMusic.Native.Native;

namespace RadianceMusic.Pages;

/// <summary>
/// A Radiance Music Server's page, as the Linux app's render_server and filter_server (stage 4c): its name, its songs
/// and who you are there, what its owner lets you do, the problem card when it isn't answering or took this computer's
/// key away (Try Again, Servers…), Play / Shuffle / Refresh, Search NAME, and its songs (who added each). A click plays
/// from there (into its party when you're in it); liking one saves it into your own library. Its listening party's card
/// sits under the head (stage 4c-2: Controls/PartyCard).
/// </summary>
public sealed partial class ServerPage : Page
{
    private EngineHub? hub;
    private string address = "";
    private readonly StackPanel body = new() { Spacing = 12 };
    private readonly StackPanel head = new() { Spacing = 12 };
    private readonly TextBox search = new() { MaxWidth = 420, HorizontalAlignment = HorizontalAlignment.Left };
    private readonly TextBlock note = PageKit.Note();
    private readonly ListView list = PageKit.SongList();
    private readonly DispatcherTimer typing = new() { Interval = TimeSpan.FromMilliseconds(300) };
    private string shownHead = "";
    private readonly Border partySlot = new();
    private PartyCard? party;

    public ServerPage()
    {
        InitializeComponent();
        NavigationCacheMode = NavigationCacheMode.Disabled;
        body.Children.Add(head);
        body.Children.Add(partySlot);
        body.Children.Add(search);
        body.Children.Add(note);
        body.Children.Add(list);
        Content = PageKit.Column(body);
        // the search as typed, a moment after the last key (SEARCH_DEBOUNCE_MS)
        typing.Tick += (_, _) =>
        {
            typing.Stop();
            RenderSongs();
        };
        search.TextChanged += (_, _) =>
        {
            typing.Stop();
            typing.Start();
        };
        list.ItemClick += (_, e) =>
        {
            if (hub != null && e.ClickedItem is SongRow row) hub.Engine.PlayServer(address, search.Text, (uint)list.Items.IndexOf(row), row.Key);
        };
    }

    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        if (e.Parameter is not PageArgs { What: string what } a) return;
        hub = a.Hub;
        address = what;
        hub.Change += OnChange;
        party = new PartyCard(hub, address);
        partySlot.Child = party;
        Render();
        hub.Engine.ServerRefresh(address); // asked again as it opens (the Linux app's open_server)
        hub.Engine.ServerShown(address); // its party kept fresh while it's open
    }

    protected override void OnNavigatedFrom(NavigationEventArgs e)
    {
        typing.Stop();
        if (hub == null) return;
        hub.Change -= OnChange;
        hub.Engine.ServerShown("");
    }

    private void OnChange(ChangeKind what)
    {
        if (what is ChangeKind.Servers or ChangeKind.Liked) Render();
        if (what is ChangeKind.Party or ChangeKind.Servers) party?.Render();
    }

    /// <summary>The page as it is now; the head made again only when it changed (the search box keeps its text and focus).</summary>
    private void Render()
    {
        if (hub == null) return;
        var p = hub.Engine.ServerPage(address, search.Text);
        var w = Ffi.ServerWords();
        var key = $"{p.Name}|{p.Line}|{p.Permissions}|{p.Problem}|{p.SignedOut}|{p.Fetching}";
        if (key != shownHead)
        {
            shownHead = key;
            head.Children.Clear();
            head.Children.Add(PageKit.Title(p.Name));
            if (p.Line.Length > 0) head.Children.Add(PageKit.Note(p.Line));
            if (p.Permissions.Length > 0) head.Children.Add(PageKit.Note(p.Permissions));
            if (p.Problem is { } problem)
            {
                var card = new InfoBar { Title = "", Message = problem, Severity = InfoBarSeverity.Warning, IsClosable = false, IsOpen = true };
                var actions = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
                actions.Children.Add(PageKit.IconButton("", w.TryAgain, () => hub.Engine.ServerRefresh(address)));
                if (p.SignedOut) actions.Children.Add(PageKit.IconButton("", w.Menu, () => _ = Dialogs.ServersDialog.Show(hub, XamlRoot)));
                card.Content = actions;
                head.Children.Add(card);
            }
            var buttons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
            var play = PageKit.IconButton("", w.Play, () =>
            {
                var now = hub.Engine.ServerPage(address, search.Text);
                if (now.Songs.Length > 0) hub.Engine.PlayServer(address, search.Text, 0, now.Songs[0].Key);
            }, accent: true);
            var shuffle = PageKit.IconButton("", w.Shuffle, () => hub.Engine.ShuffleServer(address, search.Text));
            var refresh = PageKit.IconButton("", w.Refresh, () => hub.Engine.ServerRefresh(address));
            ToolTipService.SetToolTip(refresh, w.RefreshTip);
            play.IsEnabled = shuffle.IsEnabled = p.Songs.Length > 0;
            foreach (var b in new[] { play, shuffle, refresh }) buttons.Children.Add(b);
            head.Children.Add(buttons);
            search.PlaceholderText = Ffi.ServerSearchWords(p.Name, address)[0];
        }
        RenderSongs(p);
    }

    private void RenderSongs(ServerPageView? page = null)
    {
        if (hub == null) return;
        var p = page ?? hub.Engine.ServerPage(address, search.Text);
        note.Text = p.Note;
        note.Visibility = p.Note.Length > 0 ? Visibility.Visible : Visibility.Collapsed;
        list.Items.Clear();
        for (var i = 0; i < p.Songs.Length; i++)
        {
            var song = p.Songs[i];
            var index = (uint)i;
            var row = new SongRow(hub.Engine, song);
            SongMenu.Attach(row, hub, song.Key, index, new MenuPlace.Row(), () => hub.Engine.PlayServer(address, search.Text, index, song.Key));
            list.Items.Add(row);
        }
    }

    /// <summary>The first song's menu (the screenshots' --act menu on this page).</summary>
    internal int Count => list.Items.Count;
}
