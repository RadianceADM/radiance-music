namespace RadianceMusic;

/// <summary>
/// When a pop-up of the app's own (the update, the rating) may show by itself, as the GTK app's
/// pop_up_when_free and feedback_moment: the window seen, the start's sunrise gone, the first-start guide
/// not up (and seen), no other dialog open. Otherwise it waits (the app looks again every 30 s).
/// </summary>
public static class UpdateGate
{
    public const int RetrySeconds = 30;

    public static bool MayShow(bool windowSeen, bool sunriseUp, bool guideUp, bool guideSeen, bool dialogOpen) =>
        windowSeen && !sunriseUp && !guideUp && guideSeen && !dialogOpen;
}
