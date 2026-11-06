namespace RadianceMusic;

/// <summary>
/// Once a second, as the GTK app's check_lyrics_shown: the line lit now, out of sight twice in a
/// row (scrolled away, a resize), is brought back to the middle; with nothing lit (a break, the
/// outro) the column is left where the person put it.
/// </summary>
public static class LeadCheck
{
    /// <returns>The misses counted now, and whether to bring the lit line back.</returns>
    public static (int Misses, bool Scroll) Step(int? lit, bool inSight, int misses)
    {
        if (lit == null || inSight) return (0, false);
        return misses + 1 >= 2 ? (0, true) : (misses + 1, false);
    }
}
