namespace RadianceMusic;

/// <summary>The seek bar: the song's position moves it, except while it's being dragged.</summary>
public sealed class SeekModel
{
    private bool dragging;

    /// <summary>Where the slider shows (seconds).</summary>
    public double Shown { get; private set; }

    public void Position(double at)
    {
        if (!dragging) Shown = at;
    }

    public void BeginDrag() => dragging = true;

    public void Drag(double at) => Shown = at;

    /// <summary>The drag let go at `at`: where to seek to.</summary>
    public double EndDrag(double at)
    {
        dragging = false;
        Shown = at;
        return at;
    }

    /// <summary>245.3 → "4:05" (the engine's track::clock).</summary>
    public static string Clock(double seconds)
    {
        var s = (long)Math.Max(0, seconds);
        return $"{s / 60}:{s % 60:00}";
    }
}
