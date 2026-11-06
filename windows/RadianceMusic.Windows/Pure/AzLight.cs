namespace RadianceMusic;

/// <summary>
/// The A to Z rail's lit letter, as the GTK app picks it (light_letter_in_view): of the letters
/// that have songs, the last one whose first row has reached the top of the list (40 px under
/// it, or above); none has: the first.
/// </summary>
public static class AzLight
{
    public const double Near = 40;

    /// <summary>`tops`: each letter's first row's top, in the list's view (in order down the list).
    /// The index of the letter to light, or null when there are none.</summary>
    public static int? Pick(IReadOnlyList<double> tops)
    {
        if (tops.Count == 0) return null;
        int? lit = null;
        for (var i = 0; i < tops.Count; i++)
            if (tops[i] <= Near) lit = i;
        return lit ?? 0;
    }
}
