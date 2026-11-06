using Microsoft.UI.Xaml;
using Windows.Graphics.Imaging;

namespace RadianceMusic;

/// <summary>
/// --shot FILE (developer runs on ribs03, through the RadianceMusicTest task): after --wait seconds
/// the window as it is on screen (the compositor's moving background included) goes to a PNG, and
/// the app quits.
/// </summary>
internal static class Shot
{
    public static async Task TakeAsync(MainWindow window, Dev dev)
    {
        try
        {
            await Take(window, dev);
        }
        catch (Exception e)
        {
            Crash.Note("shot: " + e);
            App.Quit();
        }
    }

    private static async Task Take(MainWindow window, Dev dev)
    {
        Crash.Note($"shot: {dev.Page} {dev.Width}x{dev.Height}, in {dev.Wait} s");
        // --splash-at: the start's sunrise at those seconds on its own clock (FILE-splash-S.png)
        if (dev.SplashAt.Count > 0 && window.Splash is { } splash)
        {
            var front = WinRT.Interop.WindowNative.GetWindowHandle(window);
            Win32.SetForegroundWindow(front);
            foreach (var at in dev.SplashAt.OrderBy(a => a))
            {
                while (splash.Elapsed < at && !splash.Finished) await Task.Delay(10);
                var file = Path.ChangeExtension(dev.Shot!, null) + $"-splash-{at.ToString("0.0", System.Globalization.CultureInfo.InvariantCulture)}.png";
                Crash.Note($"shot: the sunrise at {splash.Elapsed:0.00} s");
                await Save(front, file);
            }
        }
        window.Show(dev.Page);
        if (dev.Search != null) window.SearchFor(dev.Search);
        if (dev.Acts.Count > 0) Crash.Note("shot: queue before the steps: " + window.SidePanel.Titles());
        // the side panel open for the steps too (a queue row's menu needs its row on screen)
        if (dev.Side != null && dev.Acts.Count > 0) window.OpenSide(dev.Side);
        foreach (var act in dev.Acts)
        {
            await Task.Delay(1500);
            // wait:N: N seconds more before the next step (a song's other version is found online)
            if (act.StartsWith("wait:") && double.TryParse(act[5..], System.Globalization.CultureInfo.InvariantCulture, out var secs)) await Task.Delay(TimeSpan.FromSeconds(secs));
            else
            {
                if (act.StartsWith("queue-") || act.StartsWith("click:")) Crash.Note($"shot: queue before {act}: " + window.SidePanel.Titles());
                DevActs.Run(window, act);
            }
        }
        await Task.Delay(TimeSpan.FromSeconds(dev.Wait));
        var hwnd = WinRT.Interop.WindowNative.GetWindowHandle(window);
        if (dev.Acts.Count > 0) Crash.Note("shot: queue after the steps: " + window.SidePanel.Titles());
        if (dev.Acts.Count > 0) Crash.Note($"shot: playing at {App.Hub.Engine.Now().At:F1} s");
        Win32.SetForegroundWindow(hwnd);
        await Task.Delay(300);
        // opened in front, as a click on the bar's button does (over the page, the panel closes when the window loses focus)
        if (dev.Side != null)
        {
            window.OpenSide(dev.Side);
            await Task.Delay(800);
            Crash.Note($"shot: side panel open on {dev.Side}: {window.SideState}");
        }
        await Save(hwnd, dev.Shot!);
        Crash.Note("shot: saved " + dev.Shot);
        Crash.Note("shot: the lyrics column's time: " + Controls.LyricsColumn.Stats());
        if (dev.Again > 0)
        {
            await Task.Delay(TimeSpan.FromSeconds(dev.Again));
            await Save(hwnd, Path.ChangeExtension(dev.Shot!, null) + "-2.png");
        }
        App.Quit();
    }

    internal static async Task Save(nint hwnd, string path)
    {
        if (Win32.IsIconic(hwnd))
        {
            Crash.Note("shot: the window was minimized; restored");
            Win32.ShowWindow(hwnd, 9); // SW_RESTORE
            Win32.SetForegroundWindow(hwnd);
            await Task.Delay(800);
        }
        Win32.GetWindowRect(hwnd, out var rect);
        Crash.Note($"shot: window {rect.Right - rect.Left}x{rect.Bottom - rect.Top} at {rect.Left},{rect.Top}");
        var bits = Win32.Grab(rect, out var w, out var h);
        using var file = File.Create(path);
        var encoder = await BitmapEncoder.CreateAsync(BitmapEncoder.PngEncoderId, file.AsRandomAccessStream());
        encoder.SetPixelData(BitmapPixelFormat.Bgra8, BitmapAlphaMode.Ignore, (uint)w, (uint)h, 96, 96, bits);
        await encoder.FlushAsync();
    }
}
