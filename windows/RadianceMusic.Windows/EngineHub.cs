using Microsoft.UI.Dispatching;
using RadianceMusic.Native;

namespace RadianceMusic;

/// <summary>
/// The engine (radiance_ffi.dll) and the UI thread: the engine tells what changed on its own
/// thread; that's gathered (Coalescer) and handed to the UI thread once per batch, where pages
/// read the engine's snapshots again. Commands go straight to the engine (it posts them to its
/// own thread, so they're safe from any thread).
/// </summary>
internal sealed class EngineHub : EngineListener
{
    private readonly DispatcherQueue ui;
    private readonly Coalescer<ChangeKind> pending = new();

    public MusicEngine Engine { get; private set; } = null!;

    /// <summary>What changed, on the UI thread.</summary>
    public event Action<ChangeKind>? Change;

    private EngineHub(DispatcherQueue ui)
    {
        this.ui = ui;
    }

    public static EngineHub Start(DispatcherQueue ui)
    {
        var hub = new EngineHub(ui);
        hub.Engine = Flavor.Start(hub);
        return hub;
    }

    /// <summary>The engine's thread.</summary>
    public void Changed(ChangeKind what)
    {
        if (pending.Mark(what)) ui.TryEnqueue(Flush);
    }

    private void Flush()
    {
        foreach (var kind in pending.Flush()) Change?.Invoke(kind);
    }
}
