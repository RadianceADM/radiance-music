using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using RadianceMusic.Native;
using Ffi = RadianceMusic.Native.Native;

namespace RadianceMusic;

public partial class App : Application
{
    public static EngineHub Hub { get; private set; } = null!;
    public static MainWindow? Window { get; private set; }

    public App()
    {
        InitializeComponent();
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        var ui = DispatcherQueue.GetForCurrentThread();
        // one copy at a time, today's app or this one: the other comes forward (it shares the data)
        if (Ffi.AskTheOther(null))
        {
            if (Program.Dev.Shot == null)
                Win32.Message("Radiance Music is already open. Close it to use Radiance Music (beta).", "Radiance Music (beta)");
            Exit();
            return;
        }
        Ffi.ListenForOthers(new Others(ui));
        Hub = EngineHub.Start(ui);
        Window = new MainWindow(Hub);
        Window.Activate();
        if (Program.Dev.Shot != null) _ = Shot.TakeAsync(Window, Program.Dev);
        if (Program.Dev.PlayTest) Hub.Engine.PlayFrom(ListKind.Recent, 0);
    }

    /// <summary>Another copy asked: this window comes forward (on the UI thread).</summary>
    private sealed class Others(DispatcherQueue ui) : ShowListener
    {
        public void Show(string? link) => ui.TryEnqueue(() => Window?.BringToFront());
    }
}
