using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;

namespace RadianceMusic;

/// <summary>
/// The app's start. Its own bin\ (the engine's DLL, GStreamer, yt-dlp, Deno, FFmpeg) goes on this
/// process's PATH first (no system setting changes). Then:
///   RadianceMusic.exe --warm-up               GStreamer's list of its plugins made, no window (the installer, #81)
///   RadianceMusicBeta.exe --self-check FILE   the self-check, no window (over SSH on ribs03)
///   RadianceMusicBeta.exe --probe-video VIDEO FILE   a video played the app's way, no window: the report
///   RadianceMusicBeta.exe [developer switches] the app (Dev: --shot, --page, --size, --search, --wait,
///                                                   --play-test, --play-title TITLE, --side TAB, --act STEP, --video-probe VIDEO)
/// </summary>
public static class Program
{
    public static Dev Dev { get; private set; } = new();

    /// <summary>The radiancemusic:// link this start came with (none: null).</summary>
    public static string? Link { get; private set; }

    [STAThread]
    public static int Main(string[] args)
    {
        var bin = Path.Combine(AppContext.BaseDirectory, "bin");
        if (Directory.Exists(bin))
            Environment.SetEnvironmentVariable("PATH", bin + Path.PathSeparator + Environment.GetEnvironmentVariable("PATH"));
        if (args.Length == 1 && args[0] == "--warm-up")
            return RadianceMusic.Native.Native.WarmUp() ? 0 : 1;
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
        // a radiancemusic:// link it was opened with (a server's invite: Servers… opens with it; stage 4c)
        Link = args.FirstOrDefault(a => a.StartsWith("radiancemusic:", StringComparison.OrdinalIgnoreCase));
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
    /// <summary>--play-title WORDS: as --play-test, the first recent song whose title has WORDS (else the first).</summary>
    public string? PlayTitle { get; private set; }
    /// <summary>--act STEP (again for more): done in order before the shot (DevActs).</summary>
    public List<string> Acts { get; } = new();
    /// <summary>--side queue|lyrics: the side panel open on that tab.</summary>
    public string? Side { get; private set; }
    /// <summary>--video-probe VIDEO: only VIDEO, playing in a window of its own (VideoProbeWindow).</summary>
    public string? VideoProbe { get; private set; }
    /// <summary>--shot runs are hidden (owner, 2026-11-07: never over what he's doing): shown without focus, at the
    /// bottom, out of the taskbar and Alt-Tab, captured with PrintWindow. --front: the old way, in front.</summary>
    public bool Front { get; private set; }
    /// <summary>--setup-act: the first start's download acted out (its steps, then offline), for the sunrise's checks.</summary>
    public bool SetupAct { get; private set; }
    /// <summary>--behind: started hidden (behind everything, no focus, not in the taskbar) without a screenshot:
    /// measure-cpu.ps1's runs (the owner's PC stays his).</summary>
    public bool Behind { get; private set; }
    /// <summary>--minimize-at S, --tray-at S: the app minimises itself (no focus change) or hides to the tray
    /// S seconds after it starts (measure-cpu.ps1: its SSH session can't reach the desktop's windows).</summary>
    public double MinimizeAt { get; private set; }
    public double TrayAt { get; private set; }
    /// <summary>--no-beat: Now Playing's background flows without following the beat (to measure the beat's cost).</summary>
    public bool NoBeat { get; private set; }
    /// <summary>--offline: the app takes it there's no internet (the Offline sign's screenshots).</summary>
    public bool Offline { get; private set; }
    /// <summary>--update-shot out|downloading|ready|failed: the update pop-up at that step (screenshots).</summary>
    public string? UpdateShot { get; private set; }
    public bool Hidden => (Shot != null || Behind) && !Front;
    /// <summary>--splash-at 1.2,3.3: the start's sunrise in a --shot run too, drawn to FILE-splash-S.png at those seconds.</summary>
    public List<double> SplashAt { get; } = new();

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
                case "--play-title": d.PlayTest = true; d.PlayTitle = Next(); break;
                case "--video-probe": d.VideoProbe = Next(); break;
                case "--side": d.Side = Next(); break;
                case "--front": d.Front = true; break;
                case "--behind": d.Behind = true; break;
                case "--minimize-at": d.MinimizeAt = double.TryParse(Next(), System.Globalization.CultureInfo.InvariantCulture, out var m) ? m : 0; break;
                case "--tray-at": d.TrayAt = double.TryParse(Next(), System.Globalization.CultureInfo.InvariantCulture, out var tr) ? tr : 0; break;
                case "--no-beat": d.NoBeat = true; break;
                case "--offline": d.Offline = true; break;
                case "--update-shot": d.UpdateShot = Next(); break;
                case "--setup-act": d.SetupAct = true; break;
                case "--splash-at":
                    foreach (var s in Next().Split(','))
                        if (double.TryParse(s, System.Globalization.CultureInfo.InvariantCulture, out var at)) d.SplashAt.Add(at);
                    break;
                case "--act": d.Acts.Add(Next()); break;
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
