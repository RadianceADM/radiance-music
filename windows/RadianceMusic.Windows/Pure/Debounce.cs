namespace RadianceMusic;

/// <summary>A wait that starts again each time (the app's: a DispatcherQueueTimer; a test's: by hand).</summary>
public interface IDelay
{
    void Restart(TimeSpan wait, Action then);
}

/// <summary>Search as you type: the search runs once the typing stops for 300 ms, with the last text
/// (the GTK app's SEARCH_DEBOUNCE_MS).</summary>
public sealed class Debounce(TimeSpan wait, Action<string> act, IDelay delay)
{
    public static readonly TimeSpan SearchWait = TimeSpan.FromMilliseconds(300);

    private string last = "";

    public void Poke(string text)
    {
        last = text;
        delay.Restart(wait, () => act(last));
    }
}
