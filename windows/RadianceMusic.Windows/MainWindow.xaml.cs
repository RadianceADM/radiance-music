using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media.Imaging;
using RadianceMusic.Native;
using RadianceMusic.Pages;

namespace RadianceMusic;

public sealed partial class MainWindow : Window
{
    private readonly EngineHub hub;
    private readonly DispatcherTimer noticeTimer = new() { Interval = TimeSpan.FromSeconds(10) };
    private readonly DispatcherTimer toastTimer = new() { Interval = TimeSpan.FromSeconds(4) };
    private readonly Queue<string> toasts = new();
    private string page = "home";
    private readonly Controls.QueuePanel side;
    private object? what;

    internal MainWindow(EngineHub hub)
    {
        this.hub = hub;
        InitializeComponent();
        SystemBackdrop = new Microsoft.UI.Xaml.Media.MicaBackdrop();
        ExtendsContentIntoTitleBar = true;
        SetTitleBar(AppTitleBar);
        var icon = Path.Combine(AppContext.BaseDirectory, "radiance-music.ico");
        if (File.Exists(icon))
        {
            AppWindow.SetIcon(icon);
            TitleIcon.Source = new BitmapImage(new Uri(icon));
        }
        if (Program.Dev.Shot != null || Program.Dev.PlayTest)
            AppWindow.Resize(new Windows.Graphics.SizeInt32(Program.Dev.Width, Program.Dev.Height));
        noticeTimer.Tick += (_, _) => { noticeTimer.Stop(); Notice.IsOpen = false; };
        toastTimer.Tick += (_, _) => { toastTimer.Stop(); Toast.IsOpen = false; NextToast(); };
        Toast.Closed += (_, _) => { toastTimer.Stop(); NextToast(); };
        RenderPlaylists();
        // "New Playlist" isn't a page (it asks for the name): it acts when clicked, it's never selected
        Nav.ItemInvoked += (_, e) =>
        {
            if (e.InvokedItemContainer is NavigationViewItem { Tag: "new-playlist" }) Show("new-playlist");
        };
        hub.Change += OnChange;
        BarHost.Content = new Controls.PlayerBar(hub);
        // the side panel (Queue | Lyrics): beside the page from 1300 px, over it below (as the GTK app's)
        side = new Controls.QueuePanel(hub);
        // Now Playing's lyrics column decides the panel's Lyrics tab (as the GTK app's)
        Pages.Navigated += (_, e) =>
        {
            if (e.Content is NowPlayingPage np)
            {
                np.ShowsLyricsChanged -= SyncLyricsTab;
                np.ShowsLyricsChanged += SyncLyricsTab;
            }
            SyncLyricsTab();
        };
        SideHost.Content = side;
        Side.PaneOpened += (_, _) => side.Opened();
        Root.SizeChanged += (_, e) => Side.DisplayMode = e.NewSize.Width >= 1300 ? SplitViewDisplayMode.Inline : SplitViewDisplayMode.Overlay;
        // closing while music plays: it plays on (the tray icon brings the window back); else it quits
        AppWindow.Closing += (_, e) =>
        {
            if (!App.Quitting && hub.Engine.Now().Playing)
            {
                e.Cancel = true;
                AppWindow.Hide();
            }
            else
            {
                e.Cancel = true;
                App.Quit();
            }
        };
        Show("home");
    }

    /// <summary>A page by name: "home", "search", "playing", "liked", "history", "albums", "artists",
    /// "genres", "group" (`what`: (GroupKind, name)), "playlist:{id}", "new-playlist".</summary>
    public void Show(string name, object? subject = null)
    {
        if (name == "new-playlist")
        {
            _ = NewPlaylist();
            return;
        }
        var (type, args) = name switch
        {
            "search" => (typeof(SearchPage), (object)hub),
            "playing" => (typeof(NowPlayingPage), hub),
            "liked" => (typeof(LikedPage), hub),
            "history" => (typeof(HistoryPage), hub),
            "albums" => (typeof(LibraryPage), new PageArgs(hub, GroupKind.Albums)),
            "artists" => (typeof(LibraryPage), new PageArgs(hub, GroupKind.Artists)),
            "genres" => (typeof(LibraryPage), new PageArgs(hub, GroupKind.Genres)),
            "group" => (typeof(GroupPage), new PageArgs(hub, subject)),
            _ when name.StartsWith("playlist:") && long.TryParse(name["playlist:".Length..], out var id) => (typeof(PlaylistPage), new PageArgs(hub, id)),
            _ => (typeof(HomePage), hub),
        };
        var whatNow = args is PageArgs a ? a.What : null;
        var moved = Pages.CurrentSourcePageType != type || !Equals(what, whatNow);
        page = name;
        what = whatNow;
        if (moved) Pages.Navigate(type, args, new Microsoft.UI.Xaml.Media.Animation.EntranceNavigationTransitionInfo());
        // an album's, artist's or genre's songs: its grid's item stays the one lit
        var lit = name == "group" && subject is ValueTuple<GroupKind, string> g ? LibraryPage.KindName(g.Item1).ToLowerInvariant() : name;
        foreach (var item in Nav.MenuItems.OfType<NavigationViewItem>())
            if ((string)item.Tag == lit && !ReferenceEquals(Nav.SelectedItem, item)) Nav.SelectedItem = item;
    }

    private async Task NewPlaylist()
    {
        if (await Dialogs.NameDialog.NewPlaylist(hub, Content.XamlRoot, null) is { } id) Show("playlist:" + id);
    }

    /// <summary>The playlists under their heading, then "New Playlist" (as the GTK app's sidebar).</summary>
    private void RenderPlaylists()
    {
        foreach (var old in Nav.MenuItems.OfType<NavigationViewItem>().Where(i => i.Tag is string t && (t.StartsWith("playlist:") || t == "new-playlist")).ToList())
            Nav.MenuItems.Remove(old);
        foreach (var p in hub.Engine.Playlists())
        {
            var item = new NavigationViewItem { Content = p.Name, Tag = "playlist:" + p.Id, Icon = new FontIcon { Glyph = "\uE90B" } };
            ToolTipService.SetToolTip(item, p.Tip);
            Nav.MenuItems.Add(item);
        }
        Nav.MenuItems.Add(new NavigationViewItem { Content = "New Playlist", Tag = "new-playlist", Icon = new FontIcon { Glyph = "\uE710" }, SelectsOnInvoked = false });
        foreach (var item in Nav.MenuItems.OfType<NavigationViewItem>())
            if ((string)item.Tag == page && !ReferenceEquals(Nav.SelectedItem, item)) Nav.SelectedItem = item;
    }

    /// <summary>The engine's notes ("Liked “X”", "Added to “Y”"), one at a time, 4 s each.</summary>
    private void NextToast()
    {
        if (Toast.IsOpen || toasts.Count == 0) return;
        Toast.Message = toasts.Dequeue();
        Toast.IsOpen = true;
        toastTimer.Start();
    }

    /// <summary>Search, with `q` typed in (screenshots, Home's "Search for music").</summary>
    public void SearchFor(string q)
    {
        Show("search");
        (Pages.Content as SearchPage)?.Search(q);
    }

    /// <summary>Search, its box ready to type in (Home's "Search for music").</summary>
    public void FocusSearch()
    {
        Show("search");
        DispatcherQueue.TryEnqueue(() => (Pages.Content as SearchPage)?.FocusBox());
    }

    /// <summary>The player bar's Queue and Lyrics buttons: the side panel on that tab, or closed when it's showing it.</summary>
    public void ToggleSide(string tab)
    {
        if (tab == "lyrics" && !side.HasLyrics) return; // Now Playing shows them already (SyncLyricsTab)
        if (Side.IsPaneOpen && side.Tab == tab)
        {
            Side.IsPaneOpen = false;
            return;
        }
        side.Tab = tab;
        Side.IsPaneOpen = true;
    }

    /// <summary>For the screenshots' log: the panel's open state, mode and tabs.</summary>
    public string SideState => $"open {Side.IsPaneOpen}, {Side.DisplayMode}, tab {side.Tab}, lyrics tab {side.HasLyrics}, page {Pages.Content?.GetType().Name}";

    /// <summary>The side panel open on `tab` (not toggled: the screenshots' --side).</summary>
    public void OpenSide(string tab)
    {
        if (tab == "lyrics" && !side.HasLyrics) return;
        side.Tab = tab;
        Side.IsPaneOpen = true;
    }

    /// <summary>The side panel's Lyrics tab comes and goes with Now Playing's own lyrics column.</summary>
    private void SyncLyricsTab()
    {
        var playing = Pages.Content as NowPlayingPage;
        side.HasLyrics = SideTabs.HasLyrics(playing != null, playing?.ShowsLyrics ?? false);
    }

    public void BringToFront()
    {
        AppWindow.Show();
        Win32.SetForegroundWindow(WinRT.Interop.WindowNative.GetWindowHandle(this));
    }

    private void OnNav(NavigationView sender, NavigationViewSelectionChangedEventArgs e)
    {
        if (e.SelectedItem is NavigationViewItem { Tag: string name } && (name != page || what != null)) Show(name);
    }

    private void OnChange(ChangeKind what)
    {
        if (what == ChangeKind.Playlists) RenderPlaylists();
        if (what == ChangeKind.Toast)
        {
            while (hub.Engine.Toast() is { } t) toasts.Enqueue(t);
            NextToast();
        }
        if (what != ChangeKind.Problem) return;
        if (hub.Engine.Problem() is not { } problem) return;
        // as the GTK app's toasts: it stopped: Try Again, 10 s; it went on to the next song: 4 s, no button
        Notice.Message = problem.Text;
        Notice.ActionButton.Visibility = problem.Retry ? Visibility.Visible : Visibility.Collapsed;
        Notice.IsOpen = true;
        noticeTimer.Stop();
        noticeTimer.Interval = TimeSpan.FromSeconds(problem.Retry ? 10 : 4);
        noticeTimer.Start();
    }

    private void OnTryAgain(object sender, RoutedEventArgs e)
    {
        Notice.IsOpen = false;
        hub.Engine.Retry();
    }
}
