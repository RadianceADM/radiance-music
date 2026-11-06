namespace RadianceMusic;

/// <summary>
/// Where the song is between the engine's readings (5 a second): counted on from the last one
/// (`since`, on the engine's clock) while it plays, at most a second past it (a reading late:
/// never run ahead of the song); paused: the reading itself.
/// </summary>
public static class LyricClock
{
    public static double At(double engineAt, ulong since, ulong nowMs, bool playing)
    {
        if (!playing || nowMs <= since) return engineAt;
        return engineAt + Math.Min(1.0, (nowMs - since) / 1000.0);
    }
}
