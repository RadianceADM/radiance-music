namespace RadianceMusic;

/// <summary>
/// The player bar's end (heart, mute, the volume slider, Lyrics, Queue: four 36 px buttons, 6 px
/// apart): where its column can't hold it all, the slider shrinks, then goes, so the heart stays.
/// </summary>
public static class BarFit
{
    public const double Slider = 120, MinSlider = 60, Middle = 640;
    /// <summary>The four buttons and their four gaps (the slider's own included).</summary>
    public const double EndWithoutSlider = 4 * 36 + 4 * 6;

    /// <summary>The end column's width in a bar `inner` px wide (columns 1* | 2*, at most 640 | 1*).</summary>
    public static double Side(double inner) => inner / 2 > Middle ? (inner - Middle) / 2 : inner / 4;

    /// <summary>The slider's width (0: hidden).</summary>
    public static double Volume(double inner)
    {
        var room = Math.Min(Slider, Math.Floor(Side(inner) - EndWithoutSlider));
        return room >= MinSlider ? room : 0;
    }
}
