using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using RadianceMusic.Native;
using Ffi = RadianceMusic.Native.Native;

namespace RadianceMusic;

public partial class App : Application
{
    internal static EngineHub Hub { get; private set; } = null!;
    public static MainWindow? Window { get; private set; }
    private static Tray? tray;
    private static MediaKeys? keys;
    private static bool quitting;
    /// <summary>The sunrise holds for the first start's download; let go (ready, or "Open Radiance Music Now").</summary>
    private static bool holding, letGo;

    public App()
    {
        InitializeComponent();
        UnhandledException += (_, e) => Crash.Note(e.Exception);
        DebugSettings.XamlResourceReferenceFailed += (_, e) => Crash.Note("XAML resource: " + e.Message);
        DebugSettings.BindingFailed += (_, e) => Crash.Note("XAML binding: " + e.Message);
        AppDomain.CurrentDomain.UnhandledException += (_, e) => Crash.Note(e.ExceptionObject);
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        try
        {
            Launch();
        }
        catch (Exception e)
        {
            Crash.Note(e);
            throw;
        }
    }

    private void Launch()
    {
        var ui = DispatcherQueue.GetForCurrentThread();
        Trace("start");
        // the music videos' spike: only the video, in a window of its own (no engine, no data)
        if (Program.Dev.VideoProbe is { } video)
        {
            var probe = new VideoProbeWindow(video);
            probe.Activate();
            if (Program.Dev.Shot != null) _ = probe.ShootAsync(Program.Dev);
            return;
        }
        // one copy at a time, today's app or this one: the other comes forward (it shares the data)
        if (Ffi.AskTheOther(null))
        {
            if (Program.Dev.Shot == null)
                Win32.Message("Radiance Music is already open. Close it to use Radiance Music (beta).", "Radiance Music (beta)");
            Trace("another copy runs: it comes forward, this one quits");
            Exit();
            return;
        }
        Ffi.ListenForOthers(new Others(ui));
        // --setup-act: the first start's download acted out (screenshot checks of the sunrise's hold)
        if (Program.Dev.SetupAct) Environment.SetEnvironmentVariable("RADIANCE_SETUP_TEST", "act");
        Hub = EngineHub.Start(ui);
        Trace("engine started");
        Window = new MainWindow(Hub);
        // the sunrise while it gets ready, as the Linux app's start (#55); not in screenshots unless asked
        if (Program.Dev.Shot == null || Program.Dev.SplashAt.Count > 0) Window.CoverWithSplash(Program.Dev.SplashAt.Count > 0);
        if (Program.Dev.Hidden)
        {
            // a screenshot run: behind everything, no focus, not in the taskbar (the owner's PC stays his)
            Window.AppWindow.IsShownInSwitchers = false;
            Window.AppWindow.Show(false);
            Win32.Behind(WinRT.Interop.WindowNative.GetWindowHandle(Window));
        }
        else
        {
            Window.Activate();
        }
        Trace("window shown");
        var icon = Path.Combine(AppContext.BaseDirectory, "radiance-music.ico");
        tray = new Tray(Hub, icon);
        keys = new MediaKeys(Hub, WinRT.Interop.WindowNative.GetWindowHandle(Window));
        Trace($"tray icon added: {tray.Added}; media keys: on");
        // ready: the engine runs on its own thread and the pages are built (the sun settles once it has shown)
        Window.Splash?.Ready();
        // splitting getting ready at the first start: the sunrise holds, saying how far along (stage 3)
        if (Window.Splash != null)
        {
            ShowSetup();
            Hub.Change += kind =>
            {
                if (kind == ChangeKind.Setup) ShowSetup();
            };
        }
        if (Program.Dev.Shot != null) _ = Shot.TakeAsync(Window, Program.Dev);
        // --page without a screenshot (measure-cpu.ps1): that page at once (it only reached shots before)
        else if (Program.Dev.Page != "home") Window.Show(Program.Dev.Page);
        Later(Program.Dev.MinimizeAt, () => Win32.ShowWindow(WinRT.Interop.WindowNative.GetWindowHandle(Window), Program.Dev.Hidden ? 7 /* SW_SHOWMINNOACTIVE */ : 6 /* SW_MINIMIZE */), "minimized itself");
        Later(Program.Dev.TrayAt, () => Window.AppWindow.Hide(), "hid itself to the tray"); // (as closing while it plays)
        if (Program.Dev.PlayTest && Hub.Engine.Home().Recent is { Length: > 0 } recent)
        {
            var i = Program.Dev.PlayTitle is { } title ? Math.Max(0, Array.FindIndex(recent, s => s.Title.Contains(title, StringComparison.OrdinalIgnoreCase))) : 0;
            Hub.Engine.PlayFrom(ListKind.Recent, (uint)i, recent[i].Key);
        }
    }

    /// <summary>
    /// The first start's download on the sunrise, as the Linux app's get_splitter_ready: held while it's
    /// getting ready (what and how far along under the sun); offline or failed, it says why and offers Try
    /// Again or Open Radiance Music Now (splitting then gets ready in the background); let go once it's ready.
    /// </summary>
    private static void ShowSetup()
    {
        if (letGo || Window?.Splash is not { } splash || splash.Finished) return;
        var setup = Hub.Engine.Setup();
        switch (setup.Kind)
        {
            case SetupKind.Getting or SetupKind.Offline or SetupKind.Failed:
                if (!holding)
                {
                    if (splash.Settling) return; // (too late: the Splits tab and Splitting… say how it goes)
                    splash.Hold();
                    holding = true;
                    Trace("splash: held for getting splitting ready");
                }
                var getting = setup.Kind == SetupKind.Getting;
                splash.Progress(setup.Text, getting ? setup.Done : 0);
                if (getting) splash.ClearOffer();
                else splash.Offer(SetupButtons(splash));
                break;
            default:
                if (!holding) return;
                holding = false;
                splash.Release();
                Trace("splash: let go (splitting is ready, or not needed)");
                break;
        }
    }

    /// <summary>Under the sunrise when getting splitting ready failed: try again now, or open the app and let
    /// it get ready in the background (as soon as it can). Pills, the second the suggested one.</summary>
    private static StackPanel SetupButtons(Controls.Splash splash)
    {
        var w = Ffi.BetaWords();
        var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 12 };
        var again = new Button { Content = Labelled("\uE72C", w.TryAgain), CornerRadius = new CornerRadius(18), Padding = new Thickness(18, 8, 18, 8) };
        again.Click += (_, _) =>
        {
            splash.ClearOffer();
            Hub.Engine.RetrySetup();
        };
        var now = new Button
        {
            Content = Labelled("\uE768", w.OpenNow),
            Style = (Style)Current.Resources["AccentButtonStyle"],
            CornerRadius = new CornerRadius(18),
            Padding = new Thickness(18, 8, 18, 8),
        };
        ToolTipService.SetToolTip(now, w.OpenNowNote);
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(now, w.OpenNow);
        now.Click += (_, _) =>
        {
            letGo = true; // (the next try has no sunrise to report to)
            holding = false;
            splash.Release();
            Trace("splash: opened now; splitting gets ready in the background");
        };
        row.Children.Add(again);
        row.Children.Add(now);
        return row;
    }

    private static StackPanel Labelled(string glyph, string text)
    {
        var p = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        p.Children.Add(new FontIcon { Glyph = glyph, FontSize = 14 });
        p.Children.Add(new TextBlock { Text = text });
        return p;
    }

    private static readonly List<DispatcherQueueTimer> devTimers = new();

    /// <summary>A developer run's step `seconds` after the start (0: none).</summary>
    private static void Later(double seconds, Action step, string what)
    {
        if (seconds <= 0) return;
        var timer = DispatcherQueue.GetForCurrentThread().CreateTimer();
        devTimers.Add(timer); // (kept: a timer nothing holds is collected and never fires)
        timer.Interval = TimeSpan.FromSeconds(seconds);
        timer.IsRepeating = false;
        timer.Tick += (_, _) =>
        {
            step();
            Trace($"dev: {what}");
        };
        timer.Start();
    }

    /// <summary>Developer runs (--shot, --play-test): each step of the start in the crash note.</summary>
    internal static void Trace(string step)
    {
        if (Program.Dev.Shot != null || Program.Dev.PlayTest) Crash.Note($"[{Environment.ProcessId}] {step}");
    }

    /// <summary>Quits: where you are saved, the tray icon gone, the window closed.</summary>
    public static void Quit()
    {
        if (quitting) return;
        quitting = true;
        tray?.Dispose();
        Hub.Engine.Quit();
        Current.Exit();
    }

    public static bool Quitting => quitting;

    /// <summary>Another copy asked: this window comes forward (on the UI thread).</summary>
    private sealed class Others(DispatcherQueue ui) : ShowListener
    {
        public void Show(string? link) => ui.TryEnqueue(() =>
        {
            Trace("another copy asked: this window comes forward");
            Window?.BringToFront();
        });
    }
}
