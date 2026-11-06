namespace RadianceMusic;

/// <summary>
/// The lyrics editor's Tap Timing, as the Linux app's (ui.rs lyrics_editor): while it's on, Enter stamps the
/// chosen line with the playing time and the next line is chosen; past the last, nothing more.
/// </summary>
public static class TapTiming
{
    /// <summary>`times[next] = at`; the line chosen next (`times.Count`: the end).</summary>
    public static int Stamp(IList<double?> times, int next, double at)
    {
        if (next < 0 || next >= times.Count) return times.Count;
        times[next] = at;
        return next + 1;
    }
}
