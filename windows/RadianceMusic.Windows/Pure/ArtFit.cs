namespace RadianceMusic;

/// <summary>
/// Now Playing's art: the square cover, or a music video wider (16:9, as the GTK app's picture
/// where the cover goes), as far as `room` (the width the left column may take) lets it, and
/// never narrower than the cover.
/// </summary>
public static class ArtFit
{
    public static (double W, double H) Of(double side, double room, bool video)
    {
        if (!video) return (side, side);
        var w = Math.Max(side, Math.Min(room, side * 16 / 9));
        return (w, w * 9 / 16);
    }
}
