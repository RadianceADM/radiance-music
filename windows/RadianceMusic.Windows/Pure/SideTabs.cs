namespace RadianceMusic;

/// <summary>
/// The side panel's Lyrics tab, only where Now Playing doesn't show the lyrics itself (owner,
/// 2026-11-02, the GTK app's sync_lyrics_tab): a narrow Now Playing has no lyrics column, so the
/// tab stays there.
/// </summary>
public static class SideTabs
{
    public static bool HasLyrics(bool onNowPlaying, bool pageShowsLyrics) => !(onNowPlaying && pageShowsLyrics);

    /// <summary>The tab the panel shows: on Lyrics as the tab goes, the Queue.</summary>
    public static string Tab(string tab, bool hasLyrics) => tab == "lyrics" && !hasLyrics ? "queue" : tab;
}
