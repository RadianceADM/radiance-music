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

/// <summary>The player bar's end (heart, mute, volume, Lyrics, Queue): the volume slider shrinks, then goes, so the heart stays.</summary>
public class BarFitTests
{
    // the bar's inner width: the window's less its padding (32) and column gaps (32)
    [Fact]
    public void AWideBarKeepsTheWholeSlider() => Assert.Equal(120, BarFit.Volume(1336)); // a 1400 px window

    // (five buttons since the Mixer's pop-up joined the bar: heart, mute, Mixer, Lyrics, Queue)
    [Fact]
    public void AMidSizeBarShrinksItSoTheHeartStays()
    {
        var slider = BarFit.Volume(1136); // a 1200 px window: its end column is 284 px
        Assert.InRange(slider, BarFit.MinSlider, 119);
        Assert.True(BarFit.EndWithoutSlider + slider <= BarFit.Side(1136));
    }

    [Fact]
    public void ANarrowBarLetsItGo()
    {
        Assert.Equal(0, BarFit.Volume(936)); // a 1000 px window: the slider goes, mute stays
        Assert.Equal(0, BarFit.Volume(796));
        Assert.Equal(5 * 36 + 5 * 6, BarFit.EndWithoutSlider);
    }

    // the quality pill ("Loading…", then what's playing, as the Linux bar's): only where the whole slider fits beside it
    [Fact]
    public void AMaximizedBarHasRoomForTheQuality() => Assert.True(BarFit.Quality(1856)); // a 1920 px window

    [Fact]
    public void A1400BarKeepsTheSliderInstead() => Assert.False(BarFit.Quality(1336));
}

/// <summary>Now Playing's art: the square cover, or a video wider (16:9, as the GTK app's) within the room it has.</summary>
public class ArtFitTests
{
    [Fact]
    public void ACoverStaysSquare() => Assert.Equal((340.0, 340.0), ArtFit.Of(340, 900, video: false));

    [Fact]
    public void AVideoIsWiderAsFarAsTheRoomGoes()
    {
        var (w, h) = ArtFit.Of(340, 2000, video: true);
        Assert.Equal(340 * 16 / 9.0, w, 3);
        Assert.Equal(340, h, 3);
        Assert.Equal((500.0, 281.25), ArtFit.Of(340, 500, video: true));
    }

    [Fact]
    public void AVideoNeverGetsNarrowerThanTheCover() => Assert.Equal((340.0, 191.25), ArtFit.Of(340, 200, video: true));
}

/// <summary>Once a second, as the GTK app's check_lyrics_shown: the line lit now, out of sight
/// twice, is brought back; with nothing lit (a break, the outro) the column is left alone (review I4).</summary>
public class LeadCheckTests
{
    [Fact]
    public void NothingLitNeverScrolls() => Assert.Equal((0, false), LeadCheck.Step(lit: null, inSight: false, misses: 1));

    [Fact]
    public void ALitLineOutOfSightTwiceComesBack()
    {
        Assert.Equal((1, false), LeadCheck.Step(lit: 3, inSight: false, misses: 0));
        Assert.Equal((0, true), LeadCheck.Step(lit: 3, inSight: false, misses: 1));
    }

    [Fact]
    public void InSightStartsTheCountAgain() => Assert.Equal((0, false), LeadCheck.Step(lit: 3, inSight: true, misses: 1));
}

/// <summary>The start's sunrise (the Linux app's splash.rs and its numbers, #55): the sun holds at least 1.4 s,
/// settles once the app is ready (or after 10 s), the logo writes itself, the app shows, then the dark opens.</summary>
public class SplashTimelineTests
{
    private static SplashTimeline At(SplashTimeline line, params double[] frames)
    {
        foreach (var t in frames) line.Frame(t);
        return line;
    }

    [Fact]
    public void TheSunHoldsUntilItHasShownEvenWhenReady()
    {
        var line = new SplashTimeline();
        line.SetReady();
        At(line, 0, 0.5, 1.0);
        Assert.Null(line.SettleFrom);
        At(line, 1.4);
        Assert.Equal(1.4, line.SettleFrom);
    }

    [Fact]
    public void ItSettlesAfterTenSecondsReadyOrNot()
    {
        var line = At(new SplashTimeline(), 0, 5, 9.9);
        Assert.Null(line.SettleFrom);
        At(line, 10);
        Assert.Equal(10, line.SettleFrom);
    }

    [Fact]
    public void TheAppShowsOnceTheLogoIsWrittenThenTheDarkOpens()
    {
        var line = new SplashTimeline();
        line.SetReady();
        At(line, 0, 1.4, 3.7);
        Assert.False(line.ContentShown);
        At(line, 3.8); // 1.4 + 0.35 + 0.3 + 1.75: written
        Assert.True(line.ContentShown);
        At(line, 4.0, 4.1);
        Assert.Null(line.FadeFrom); // it stays a moment (0.35 s)
        At(line, 4.2);
        Assert.Equal(4.2, line.FadeFrom);
        Assert.False(line.Done);
        At(line, 5.2); // 0.9 s later: the app alone
        Assert.True(line.Done);
    }

    [Fact]
    public void AClickGoesStraightInOnlyOnceReady()
    {
        var line = At(new SplashTimeline(), 0, 0.5);
        line.Skip();
        Assert.Null(line.FadeFrom);
        line.SetReady();
        At(line, 0.6);
        line.Skip();
        Assert.Equal(0.6, line.FadeFrom);
        Assert.True(line.ContentShown);
    }

    [Fact]
    public void TheLogoWritesItselfStrokeByStroke()
    {
        var line = new SplashTimeline();
        line.SetReady();
        At(line, 0, 1.4);
        At(line, 1.4 + 0.35 + 0.3 + 0.65); // the R written, the leg just begun, no tail yet
        var (r, leg, tail) = line.Strokes;
        Assert.Equal(1, r, 3);
        Assert.InRange(leg, 0.001, 0.2);
        Assert.Equal(0, tail);
        Assert.Equal(0, line.Name);
        At(line, 1.4 + 0.35 + 0.3 + 1.7); // all of it, and the name under it
        Assert.Equal((1.0, 1.0, 1.0), line.Strokes);
        Assert.Equal(1, line.Name, 3);
    }

    [Fact]
    public void TheSunRisesAndItsBarsPlayUntilTheySettle()
    {
        var line = At(new SplashTimeline(), 0.5);
        Assert.InRange(line.Rise, 0.4, 0.6);
        Assert.Equal(0, line.Play);
        At(line, 1.2);
        Assert.Equal(1, line.Play, 3);
        line.SetReady();
        At(line, 1.4, 1.4 + 0.35);
        Assert.Equal(0, line.Play, 3); // at rest
    }
}

/// <summary>The Mixer's effects as the Linux app shows them (ui.rs fx_values / fx_tooltips): each slider off
/// at its left, Low cut and High cut on a log scale in Hz, Reverb in %.</summary>
public class EffectTextTests
{
    [Fact]
    public void TheSlidersRunTheLinuxAppsRanges()
    {
        Assert.Equal(20, EffectText.LowCut(0));
        Assert.Equal(500, EffectText.LowCut(1));
        Assert.Equal(20000, EffectText.HighCut(0));
        Assert.Equal(1500, EffectText.HighCut(1));
        Assert.Equal(42, EffectText.Reverb(0.42));
        Assert.Equal(0.5, EffectText.LowPos(EffectText.LowCut(0.5)), 2);
        Assert.Equal(0.25, EffectText.HighPos(EffectText.HighCut(0.25)), 2);
    }

    [Fact]
    public void HertzAsTheLinuxAppSaysThem()
    {
        Assert.Equal("500 Hz", EffectText.Hz(500));
        Assert.Equal("1.5 kHz", EffectText.Hz(1500));
        Assert.Equal("20.0 kHz", EffectText.Hz(20000));
    }

    [Fact]
    public void EachEffectSaysWhatItDoesNow()
    {
        Assert.Equal("Low cut: off. For every song, on every speaker.", EffectText.LowTip(20));
        Assert.Equal("Low cut: takes out the bass below 120 Hz. For every song, on every speaker.", EffectText.LowTip(120));
        Assert.Equal("High cut: off. For every song, on every speaker.", EffectText.HighTip(20000));
        Assert.Equal("High cut: takes out the sounds above 8.0 kHz. For every song, on every speaker.", EffectText.HighTip(8000));
        Assert.Equal("Reverb: off. For every song, on every speaker.", EffectText.ReverbTip(0));
        Assert.Equal("Reverb: 42 %, a room around the music. For every song, on every speaker.", EffectText.ReverbTip(42));
    }
}
