namespace RadianceMusic;

/// <summary>The tray icon's tooltip: "title – byline", as Windows takes it (127 UTF-16 units at
/// most, never half of a surrogate pair), as tray.rs does.</summary>
public static class TrayText
{
    public const int Most = 127;

    public static string Tooltip(string title, string byline)
    {
        var text = (title, byline) switch
        {
            ("", _) => "Radiance Music",
            (_, "") => title,
            _ => $"{title} – {byline}",
        };
        if (text.Length <= Most) return text;
        var cut = Most;
        if (char.IsHighSurrogate(text[cut - 1])) cut--;
        return text[..cut];
    }
}
