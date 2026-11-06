using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
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
        if (Program.Dev.Shot != null) _ = Shot.TakeAsync(Window, Program.Dev);
        if (Program.Dev.PlayTest && Hub.Engine.Home().Recent is { Length: > 0 } recent)
        {
            var i = Program.Dev.PlayTitle is { } title ? Math.Max(0, Array.FindIndex(recent, s => s.Title.Contains(title, StringComparison.OrdinalIgnoreCase))) : 0;
            Hub.Engine.PlayFrom(ListKind.Recent, (uint)i, recent[i].Key);
        }
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
