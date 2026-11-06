namespace RadianceMusic;

/// <summary>
/// Now Playing's background following the beat (#56), as the Linux app's backdrop.rs tick and its numbers:
/// a kick (the beat rising by more than a fifth) swells the art at once and pushes the flow ahead for a
/// moment; strong bass flows it faster, eased (up within a fifth of a second, down over half a second).
/// <see cref="Swell"/> is part of the art's size (the scale expressions cap it at MAX_ZOOM 1.15);
/// <see cref="Speed"/> is how fast the flow runs (its animations' playback rate).
/// </summary>
public sealed class BeatFollower
{
    public const double Push = 25.0, PushSettlesS = 0.12, SwellSize = 0.06, SwellFades = 3.0, BassSpeeds = 3.0;
    private double hit, push, swell, bass;

    /// <summary>A kick's swell now, part of the art's size (0: at rest).</summary>
    public double Swell => SwellSize * swell;

    /// <summary>How fast the flow runs now (1: as without music).</summary>
    public double Speed { get; private set; } = 1;

    /// <summary>`dt` seconds on (at most 0.1), the beat and the bass at this moment of the song (0..1).</summary>
    public void Step(double dt, float hitNow, float bassNow)
    {
        dt = Math.Clamp(dt, 0, 0.1);
        if (hitNow > hit + 0.2) push += Push * hitNow;
        hit = hitNow;
        swell = Math.Max(hitNow, swell - SwellFades * dt);
        var ease = bassNow > bass ? 0.2 : 0.5;
        bass += (bassNow - bass) * (1 - Math.Exp(-dt / ease));
        Speed = 1 + BassSpeeds * bass + push;
        push *= Math.Exp(-dt / PushSettlesS);
    }

    /// <summary>At rest again (a paused song, or another).</summary>
    public void Rest()
    {
        hit = push = swell = bass = 0;
        Speed = 1;
    }

    /// <summary>A pulse's value at `at` seconds into the song (beat.rs Pulse::value): its nearest frame,
    /// 0 before its first value or past its end.</summary>
    public static float Sample(IReadOnlyList<float> of, double fps, double offset, double at)
    {
        var i = Math.Round((at - offset) * fps, MidpointRounding.AwayFromZero); // (Rust's round(), not to even)
        if (i < 0 || i >= of.Count) return 0;
        return of[(int)i];
    }
}
