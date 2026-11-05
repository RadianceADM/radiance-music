namespace RadianceMusic;

/// <summary>
/// The lyrics growing toward the middle, as the GTK app's Lens: a line at the view's middle at full
/// size, smaller the farther it is (smoothstep), down to 0.58 above (sung) and 0.7 below (coming).
/// The same formula runs on the compositor (Expression), so scrolling costs no C# per frame.
/// </summary>
public static class LensScale
{
    public const double Sung = 0.58;
    public const double Coming = 0.7;

    /// <summary>`y`: the line's middle from the view's middle, in half-heights (-1 the top, 1 the bottom).</summary>
    public static double Scale(double y)
    {
        var d = Math.Clamp(Math.Abs(y), 0, 1);
        var s = d * d * (3 - 2 * d);
        return 1 + ((y < 0 ? Sung : Coming) - 1) * s;
    }

    /// <summary>The compositor's version: `y` from the line's middle (p.Mid, in the content; its own property set), the scroller's
    /// translation (s.Translation.Y, negative as it scrolls down) and the view's half-height (v.Half).</summary>
    public static string Expression =>
        "Vector3(K, K, 1)"
            .Replace("K", "(1 + ((((p.Mid + s.Translation.Y - v.Half) / v.Half) < 0 ? " + Sung.ToString(System.Globalization.CultureInfo.InvariantCulture) + " : " + Coming.ToString(System.Globalization.CultureInfo.InvariantCulture) + ") - 1) * (D * D * (3 - 2 * D)))")
            .Replace("D", "Clamp(Abs((p.Mid + s.Translation.Y - v.Half) / v.Half), 0, 1)");
}
