using RadianceMusic;
using Xunit;

namespace RadianceMusic.Tests;

/// <summary>A delay the test fires by hand.</summary>
internal sealed class HandDelay : IDelay
{
    private Action? pending;
    public int Starts { get; private set; }
    public void Restart(TimeSpan wait, Action then) { pending = then; Starts++; }
    public void Fire() { var p = pending; pending = null; p?.Invoke(); }
}

public class DebounceTests
{
    [Fact]
    public void pokes_within_300ms_run_once_with_the_last_text()
    {
        var delay = new HandDelay();
        var ran = new List<string>();
        var d = new Debounce(TimeSpan.FromMilliseconds(300), ran.Add, delay);
        d.Poke("b");
        d.Poke("bi");
        d.Poke("binhi");
        Assert.Equal(3, delay.Starts);
        Assert.Equal(TimeSpan.FromMilliseconds(300), Debounce.SearchWait);
        delay.Fire();
        delay.Fire();
        Assert.Equal(new[] { "binhi" }, ran);
    }
}

public class LyricScrollTests
{
    [Fact]
    public void scrolls_only_when_the_first_lit_line_changes()
    {
        var f = new LyricFollower();
        Assert.Equal(0, f.Next(new uint[] { 0 }));
        Assert.Null(f.Next(new uint[] { 0 }));
        Assert.Null(f.Next(new uint[] { 0, 1 }));
        Assert.Equal(1, f.Next(new uint[] { 1 }));
        Assert.Null(f.Next(Array.Empty<uint>()));
        f.Reset();
        Assert.Equal(1, f.Next(new uint[] { 1 }));
    }
}

public class SeekTests
{
    [Fact]
    public void position_updates_dont_move_a_dragged_slider()
    {
        var s = new SeekModel();
        s.Position(10);
        Assert.Equal(10, s.Shown);
        s.BeginDrag();
        s.Drag(20);
        s.Position(30);
        Assert.Equal(20, s.Shown);
        Assert.Equal(42, s.EndDrag(42));
        s.Position(42.2);
        Assert.Equal(42.2, s.Shown);
        Assert.Equal("4:05", SeekModel.Clock(245.3));
        Assert.Equal("0:00", SeekModel.Clock(-3));
        Assert.Equal("61:01", SeekModel.Clock(3661));
    }
}

public class TrayTextTests
{
    [Fact]
    public void cuts_at_127_utf16_units()
    {
        var t = TrayText.Tooltip(new string('a', 200), "Someone");
        Assert.True(t.Length <= 127);
        // a title of emoji (surrogate pairs) never ends in half of one
        var emoji = string.Concat(Enumerable.Repeat("\U0001F3B5", 100));
        var cut = TrayText.Tooltip(emoji, "");
        Assert.True(cut.Length <= 127);
        Assert.False(char.IsHighSurrogate(cut[^1]));
    }

    [Fact]
    public void joins_title_and_byline()
    {
        Assert.Equal("Binhi – Arthur Nery", TrayText.Tooltip("Binhi", "Arthur Nery"));
        Assert.Equal("Binhi", TrayText.Tooltip("Binhi", ""));
        Assert.Equal("Radiance Music", TrayText.Tooltip("", ""));
    }
}

public class AzLightTests
{
    [Fact]
    public void TheLastLetterAtTheTopLights()
    {
        Assert.Equal(2, AzLight.Pick(new double[] { -300, -10, 30, 400 }));
    }

    [Fact]
    public void NoneAtTheTopLightsTheFirst()
    {
        Assert.Equal(0, AzLight.Pick(new double[] { 120, 400 }));
        Assert.Null(AzLight.Pick(Array.Empty<double>()));
    }
}

public class QueueDropTests
{
    [Fact]
    public void ARowMovedDownGoesBeforeTheOneAfterItsNewPlace()
    {
        // the song playing at 4: the list shows 5, 6, 7, …
        Assert.Equal(((uint)5, (uint)8), QueueDrop.Map(5, 0, 2));
    }

    [Fact]
    public void ARowMovedUpGoesBeforeItsNewPlace()
    {
        Assert.Equal(((uint)8, (uint)6), QueueDrop.Map(5, 3, 1));
        Assert.Null(QueueDrop.Map(5, 1, 1));
    }
}

public class LyricClockTests
{
    [Fact]
    public void ItCountsOnWhilePlaying()
    {
        Assert.Equal(12.4, LyricClock.At(12.0, 1000, 1400, true), 6);
        Assert.Equal(12.0, LyricClock.At(12.0, 1000, 1400, false));
        Assert.Equal(13.0, LyricClock.At(12.0, 1000, 4000, true), 6);
    }
}

public class LensScaleTests
{
    [Fact]
    public void TheMiddleIsWholeAndTheEdgesSmaller()
    {
        Assert.Equal(1.0, LensScale.Scale(0), 6);
        Assert.Equal(0.58, LensScale.Scale(-1), 6);
        Assert.Equal(0.7, LensScale.Scale(1), 6);
        Assert.Equal(0.7, LensScale.Scale(3), 6);
        Assert.Contains("0.58", LensScale.Expression);
        Assert.Contains("0.7", LensScale.Expression);
    }
}

/// <summary>The side panel's Lyrics tab only where Now Playing doesn't show the lyrics itself (the GTK app's sync_lyrics_tab).</summary>
public class SideTabsTests
{
    [Fact]
    public void NowPlayingWithItsLyricsColumnLeavesThePanelNoLyricsTab()
    {
        Assert.False(SideTabs.HasLyrics(onNowPlaying: true, pageShowsLyrics: true));
        Assert.True(SideTabs.HasLyrics(onNowPlaying: true, pageShowsLyrics: false)); // a narrow window: no column, the tab stays
        Assert.True(SideTabs.HasLyrics(onNowPlaying: false, pageShowsLyrics: false));
    }

    [Fact]
    public void APanelOnLyricsGoesToTheQueueWhenTheTabGoes()
    {
        Assert.Equal("queue", SideTabs.Tab("lyrics", hasLyrics: false));
        Assert.Equal("lyrics", SideTabs.Tab("lyrics", hasLyrics: true));
        Assert.Equal("queue", SideTabs.Tab("queue", hasLyrics: false));
    }
}
