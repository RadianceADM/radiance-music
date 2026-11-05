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
    private string page = "home";

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
        hub.Change += OnChange;
        BarHost.Content = new Controls.PlayerBar(hub);
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

    /// <summary>A page by name: "home", "search", "playing".</summary>
    public void Show(string name)
    {
        page = name;
        var type = name switch
        {
            "search" => typeof(SearchPage),
            "playing" => typeof(NowPlayingPage),
            _ => typeof(HomePage),
        };
        if (Pages.CurrentSourcePageType != type) Pages.Navigate(type, hub, new Microsoft.UI.Xaml.Media.Animation.EntranceNavigationTransitionInfo());
        foreach (var item in Nav.MenuItems.OfType<NavigationViewItem>())
            if ((string)item.Tag == name && Nav.SelectedItem != item) Nav.SelectedItem = item;
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

    public void BringToFront()
    {
        AppWindow.Show();
        Win32.SetForegroundWindow(WinRT.Interop.WindowNative.GetWindowHandle(this));
    }

    private void OnNav(NavigationView sender, NavigationViewSelectionChangedEventArgs e)
    {
        if (e.SelectedItem is NavigationViewItem { Tag: string name } && name != page) Show(name);
    }

    private void OnChange(ChangeKind what)
    {
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
