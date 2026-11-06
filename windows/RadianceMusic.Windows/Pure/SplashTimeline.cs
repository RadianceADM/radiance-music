namespace RadianceMusic;

/// <summary>
/// The start's sunrise (the Linux app's src/ui/splash.rs, its numbers, #55): the sun rises out of the sea
/// and its bars play at least <see cref="ShownS"/>; once the app is ready (or after <see cref="LongestS"/>)
/// they come to rest, the sun sets and the logo writes itself, the name under it; the app shows behind,
/// and once it's been drawn the dark opens from the middle out. A click goes straight in once ready.
/// Held (splitting getting ready at the first start, stage 3), the sun keeps playing, past the 10 s too,
/// until each hold is let go.
/// Seconds since the first frame; the Splash control draws what this says.
/// </summary>
public sealed class SplashTimeline
{
    public const double RiseS = 1.0, ShownS = 1.4, SettleS = 0.35, SetS = 0.5, WriteS = 0.3, WrittenS = 1.75, HoldS = 0.35, FadeS = 0.9, LongestS = 10.0;

    public double T { get; private set; }
    public bool Ready { get; private set; }
    public double? SettleFrom { get; private set; }
    public double? FadeFrom { get; private set; }
    /// <summary>The window behind is shown (it's laid out while the logo stands still).</summary>
    public bool ContentShown { get; private set; }
    public bool Done { get; private set; }
    /// <summary>What it waits for first (the first start's download): the sun plays on until each is let go.</summary>
    public int Holds { get; private set; }
    /// <summary>Frames drawn since the window behind was shown (-1: not yet): it fades in once it's been drawn twice.</summary>
    private int shownFrames = -1;

    public void SetReady() => Ready = true;

    public void Hold() => Holds++;

    public void Release() => Holds = Math.Max(0, Holds - 1);

    /// <summary>A frame, `t` seconds after the first.</summary>
    public void Frame(double t)
    {
        T = t;
        if (SettleFrom == null && Holds == 0 && ((Ready && t >= ShownS) || t >= LongestS)) SettleFrom = t;
        if (SettleFrom is { } s && FadeFrom == null)
        {
            var written = s + SettleS + WriteS + WrittenS;
            if (t >= written) ShowContent();
            if (t >= written + HoldS && shownFrames >= 2) FadeFrom = t;
        }
        if (shownFrames >= 0) shownFrames++;
        if (FadeFrom is { } f && t >= f + FadeS) Done = true;
    }

    /// <summary>A click: once ready, straight into the app (the dark opens now).</summary>
    public void Skip()
    {
        if (!Ready || Holds > 0 || FadeFrom != null) return;
        ShowContent();
        FadeFrom = T;
    }

    private void ShowContent()
    {
        if (ContentShown) return;
        ContentShown = true;
        shownFrames = 0;
    }

    public static double Ease(double x)
    {
        x = Math.Clamp(x, 0, 1);
        return x * x * (3 - 2 * x);
    }

    /// <summary>The sun out of the sea (0..1), and its warm glow with it.</summary>
    public double Rise => Ease(T / RiseS);
    /// <summary>The bars playing (0: at rest).</summary>
    public double Play => Ease((T - 0.8) / 0.3) * (1 - (SettleFrom is { } s ? Ease((T - s) / SettleS) : 0));
    /// <summary>The sun going down again once they've settled (1: gone).</summary>
    public double Set => SettleFrom is { } s ? Ease((T - s - 0.1) / SetS) : 0;
    /// <summary>The dark opening from the middle (1: all open).</summary>
    public double Reveal => FadeFrom is { } f ? Ease((T - f) / FadeS) : 0;
    /// <summary>The logo's three strokes, each 0..1 of the way along: the R, its leg into the m, the wave's tail.</summary>
    public (double R, double Leg, double Tail) Strokes
    {
        get
        {
            if (SettleFrom is not { } s) return (0, 0, 0);
            var k = T - s - SettleS - WriteS;
            return (Ease(k / 0.65), Ease((k - 0.55) / 0.75), Ease((k - 1.25) / 0.3));
        }
    }
    /// <summary>The name under the logo, coming up softly as the wave goes out.</summary>
    public double Name => SettleFrom is { } s ? Ease((T - s - SettleS - WriteS - 1.1) / 0.55) : 0;
    /// <summary>The logo (and name) stays a moment over the opening, then goes.</summary>
    public double LogoOpacity => 1 - Ease((Reveal - 0.35) / 0.55);
}
