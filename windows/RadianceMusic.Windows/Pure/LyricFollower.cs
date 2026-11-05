namespace RadianceMusic;

/// <summary>Now Playing's lyrics follow the song: a scroll only when the first lit line changes
/// (the line to bring into view), never on each tick.</summary>
public sealed class LyricFollower
{
    private int? lead;

    /// <summary>The line to scroll to now, or null (the same as before, or nothing lit).</summary>
    public int? Next(IReadOnlyList<uint> lit)
    {
        if (lit.Count == 0) return null;
        var first = (int)lit[0];
        if (first == lead) return null;
        lead = first;
        return first;
    }

    /// <summary>A new song: its first lit line scrolls into view.</summary>
    public void Reset() => lead = null;
}
