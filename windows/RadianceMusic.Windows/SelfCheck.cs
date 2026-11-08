using RadianceMusic.Native;

namespace RadianceMusic;

/// <summary>
/// The self-check, without a window: what the engine sees (its version, the data folder,
/// GStreamer and the plugins, yt-dlp, the history), and that it starts and stops. "ok" last, or
/// "failed: …". ribs03 checks each build this way over SSH (windows\build.ps1).
/// </summary>
public static class SelfCheck
{
    private sealed class Quiet : EngineListener
    {
        public void Changed(ChangeKind what) { }
    }

    public static (string Text, bool Ok) Run()
    {
        var lines = new List<string>();
        var failed = new List<string>();
        try
        {
            lines.AddRange(Native.Native.SelfCheck());
            var engine = Flavor.Start(new Quiet());
            lines.Add("engine started: " + engine.Home().Greeting.Split(',')[0]);
            engine.Quit();
        }
        catch (Exception e)
        {
            failed.Add(e.GetType().Name + ": " + e.Message);
        }
        if (!lines.Contains("missing plugins: none")) failed.Add("plugins missing");
        if (!lines.Contains("yt-dlp found")) failed.Add("yt-dlp missing");
        lines.Add(failed.Count == 0 ? "ok" : "failed: " + string.Join("; ", failed));
        return (string.Join("\n", lines) + "\n", failed.Count == 0);
    }
}
