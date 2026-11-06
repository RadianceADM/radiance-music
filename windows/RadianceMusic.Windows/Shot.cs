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
        window.Show(dev.Page);
        if (dev.Search != null) window.SearchFor(dev.Search);
        foreach (var act in dev.Acts)
        {
            await Task.Delay(1500);
            // wait:N: N seconds more before the next step (a song's other version is found online)
            if (act.StartsWith("wait:") && double.TryParse(act[5..], System.Globalization.CultureInfo.InvariantCulture, out var secs)) await Task.Delay(TimeSpan.FromSeconds(secs));
            else DevActs.Run(window, act);
        }
        await Task.Delay(TimeSpan.FromSeconds(dev.Wait));
        var hwnd = WinRT.Interop.WindowNative.GetWindowHandle(window);
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
