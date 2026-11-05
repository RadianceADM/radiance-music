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
        window.Show(dev.Page);
        if (dev.Search != null) window.SearchFor(dev.Search);
        await Task.Delay(TimeSpan.FromSeconds(dev.Wait));
        var hwnd = WinRT.Interop.WindowNative.GetWindowHandle(window);
        Win32.SetForegroundWindow(hwnd);
        await Task.Delay(300);
        await Save(hwnd, dev.Shot!);
        if (dev.Again > 0)
        {
            await Task.Delay(TimeSpan.FromSeconds(dev.Again));
            await Save(hwnd, Path.ChangeExtension(dev.Shot!, null) + "-2.png");
        }
        App.Quit();
    }

    private static async Task Save(nint hwnd, string path)
    {
        Win32.GetWindowRect(hwnd, out var rect);
        var bits = Win32.Grab(rect, out var w, out var h);
        using var file = File.Create(path);
        var encoder = await BitmapEncoder.CreateAsync(BitmapEncoder.PngEncoderId, file.AsRandomAccessStream());
        encoder.SetPixelData(BitmapPixelFormat.Bgra8, BitmapAlphaMode.Ignore, (uint)w, (uint)h, 96, 96, bits);
        await encoder.FlushAsync();
    }
}
