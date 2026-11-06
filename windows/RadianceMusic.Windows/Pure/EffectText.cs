namespace RadianceMusic;

/// <summary>
/// The Mixer's effects as the Linux app has them (ui.rs fx_values / fx_show / fx_tooltips, stage 3): each
/// slider (0..1) is off at its left; Low cut 20 → 500 Hz and High cut 20 kHz → 1.5 kHz on a log scale,
/// Reverb 0 → 100 %; each says what it does now.
/// </summary>
public static class EffectText
{
    public const double LowFrom = 20, LowTo = 500, HighFrom = 20000, HighTo = 1500;
    private const string Every = ". For every song, on every speaker.";

    private static double Clamp(double v) => Math.Clamp(v, 0, 1);

    public static double LowCut(double pos) => Math.Round(LowFrom * Math.Pow(LowTo / LowFrom, Clamp(pos)));
    public static double HighCut(double pos) => Math.Round(HighFrom * Math.Pow(HighTo / HighFrom, Clamp(pos)));
    public static double Reverb(double pos) => Math.Round(Clamp(pos) * 100);

    public static double LowPos(double hz) => Clamp(Math.Log(Math.Max(hz, LowFrom) / LowFrom) / Math.Log(LowTo / LowFrom));
    public static double HighPos(double hz) => Clamp(Math.Log(Math.Min(hz, HighFrom) / HighFrom) / Math.Log(HighTo / HighFrom));
    public static double ReverbPos(double percent) => Clamp(percent / 100);

    public static string Hz(double v) => v >= 1000
        ? (v / 1000).ToString("0.0", System.Globalization.CultureInfo.InvariantCulture) + " kHz"
        : v.ToString("0", System.Globalization.CultureInfo.InvariantCulture) + " Hz";

    public static string LowTip(double hz) => (hz <= LowFrom + 0.5 ? "Low cut: off" : $"Low cut: takes out the bass below {Hz(hz)}") + Every;
    public static string HighTip(double hz) => (hz >= HighFrom - 0.5 ? "High cut: off" : $"High cut: takes out the sounds above {Hz(hz)}") + Every;
    public static string ReverbTip(double percent) => (percent < 0.5 ? "Reverb: off" : $"Reverb: {percent:0} %, a room around the music") + Every;
}
