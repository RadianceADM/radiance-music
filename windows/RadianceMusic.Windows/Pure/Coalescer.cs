namespace RadianceMusic;

/// <summary>
/// Changes told on the engine's thread, gathered for the UI thread: the first mark since the last
/// flush asks for a flush (one post to the UI thread), and a flush hands each kind marked once,
/// in the enum's order. Thread-safe.
/// </summary>
public sealed class Coalescer<T> where T : struct, Enum
{
    private readonly object gate = new();
    private readonly HashSet<T> marked = new();
    private bool posted;

    /// <summary>True: the first mark since the last flush (post a flush).</summary>
    public bool Mark(T kind)
    {
        lock (gate)
        {
            marked.Add(kind);
            if (posted) return false;
            posted = true;
            return true;
        }
    }

    public T[] Flush()
    {
        lock (gate)
        {
            posted = false;
            var all = marked.OrderBy(k => k).ToArray();
            marked.Clear();
            return all;
        }
    }
}
