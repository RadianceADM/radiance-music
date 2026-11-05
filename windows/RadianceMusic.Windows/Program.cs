using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;

namespace RadianceMusic;

/// <summary>
/// The app's start. Its own bin\ (the engine's DLL, GStreamer, yt-dlp, Deno, FFmpeg) goes on this
/// process's PATH first (no system setting changes). Then:
///   RadianceMusicBeta.exe --self-check FILE   the self-check, no window (over SSH on ribs03)
///   RadianceMusicBeta.exe --probe-video VIDEO FILE   a video played the app's way, no window: the report
///   RadianceMusicBeta.exe [developer switches] the app (Dev: --shot, --page, --size, --search, --wait,
///                                                   --play-test, --video-probe VIDEO)
/// </summary>
public static class Program
{
    public static Dev Dev { get; private set; } = new();

    [STAThread]
    public static int Main(string[] args)
    {
        var bin = Path.Combine(AppContext.BaseDirectory, "bin");
        if (Directory.Exists(bin))
            Environment.SetEnvironmentVariable("PATH", bin + Path.PathSeparator + Environment.GetEnvironmentVariable("PATH"));
        if (args.Length == 2 && args[0] == "--self-check")
        {
            var (text, ok) = SelfCheck.Run();
            File.WriteAllText(args[1], text);
            return ok ? 0 : 1;
        }
        if (args.Length == 3 && args[0] == "--probe-video")
        {
            var lines = RadianceMusic.Native.Native.ProbeVideo(args[1], 3.0);
            File.WriteAllLines(args[2], lines);
            return lines.LastOrDefault() == "ok" ? 0 : 1;
        }
        Dev = Dev.Parse(args);
        WinRT.ComWrappersSupport.InitializeComWrappers();
        Application.Start(started =>
        {
            SynchronizationContext.SetSynchronizationContext(new DispatcherQueueSynchronizationContext(DispatcherQueue.GetForCurrentThread()));
            _ = new App();
        });
        return 0;
    }
}

/// <summary>Developer switches (screenshots and the CPU measurement on ribs03; never in a release's shortcut).</summary>
public sealed class Dev
{
    /// <summary>--shot FILE: the window drawn to a PNG after --wait seconds, then it quits.</summary>
    public string? Shot { get; private set; }
    public string Page { get; private set; } = "home";
    public int Width { get; private set; } = 1280;
    public int Height { get; private set; } = 800;
    public string? Search { get; private set; }
    public double Wait { get; private set; } = 8;
    /// <summary>--again S: a second shot S seconds after the first (FILE-2.png), to see what moved.</summary>
    public double Again { get; private set; }
    /// <summary>--play-test: plays the first of Home's recent songs after start.</summary>
    public bool PlayTest { get; private set; }
    /// <summary>--video-probe VIDEO: only VIDEO, playing in a window of its own (VideoProbeWindow).</summary>
    public string? VideoProbe { get; private set; }

    public static Dev Parse(string[] args)
    {
        var d = new Dev();
        for (var i = 0; i < args.Length; i++)
        {
            string Next() => i + 1 < args.Length ? args[++i] : "";
            switch (args[i])
            {
                case "--shot": d.Shot = Next(); break;
                case "--page": d.Page = Next(); break;
                case "--search": d.Search = Next(); break;
                case "--wait": d.Wait = double.TryParse(Next(), System.Globalization.CultureInfo.InvariantCulture, out var w) ? w : 8; break;
                case "--play-test": d.PlayTest = true; break;
                case "--video-probe": d.VideoProbe = Next(); break;
                case "--again": d.Again = double.TryParse(Next(), System.Globalization.CultureInfo.InvariantCulture, out var a) ? a : 0; break;
                case "--size":
                    var wh = Next().Split('x');
                    if (wh.Length == 2 && int.TryParse(wh[0], out var x) && int.TryParse(wh[1], out var y)) { d.Width = x; d.Height = y; }
                    break;
            }
        }
        return d;
    }
}
