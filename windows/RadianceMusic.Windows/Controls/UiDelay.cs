using Microsoft.UI.Dispatching;

namespace RadianceMusic.Controls;

/// <summary>Debounce's wait on the UI thread.</summary>
public sealed class UiDelay : IDelay
{
    private readonly DispatcherQueueTimer timer = DispatcherQueue.GetForCurrentThread().CreateTimer();
    private Action? then;

    public UiDelay()
    {
        timer.IsRepeating = false;
        timer.Tick += (_, _) =>
        {
            timer.Stop();
            then?.Invoke();
        };
    }

    public void Restart(TimeSpan wait, Action act)
    {
        then = act;
        timer.Stop();
        timer.Interval = wait;
        timer.Start();
    }
}
