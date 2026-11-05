using Microsoft.UI;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using RadianceMusic.Interop;
using RadianceMusic.Native;

namespace RadianceMusic;

/// <summary>
/// --video-probe FILE (ribs03, the music videos' spike; issue #45): FILE playing, silent, the way
/// the app plays videos, in a SwapChainPanel filling a window of its own (no engine, no data).
/// With --shot: a picture after --wait seconds, the window made smaller, a second picture
/// (NAME-2.png), the probe's report beside them (NAME.txt), then it quits. Without: it plays
/// until it's closed (measure-cpu.ps1 -Video).
/// </summary>
internal sealed class VideoProbeWindow : Window
{
    private readonly SwapChainPanel panel = new();
    private readonly VideoCheck video;

    public VideoProbeWindow(string file)
    {
        Title = "Radiance Music (beta): video check";
        var root = new Grid { Background = new SolidColorBrush(Colors.Black) };
        root.Children.Add(panel);
        Content = root;
        video = VideoCheck.Start(file);
        SwapChainPanelNative.SetSwapChain(panel, (nint)video.Swapchain());
        panel.SizeChanged += (_, _) => Fit();
        panel.CompositionScaleChanged += (_, _) => Fit();
        Closed += (_, _) =>
        {
            SwapChainPanelNative.SetSwapChain(panel, 0);
            video.Stop();
        };
        AppWindow.Resize(new Windows.Graphics.SizeInt32(Program.Dev.Width, Program.Dev.Height));
    }

    /// <summary>The swapchain's buffers at the panel's size (in its own units: the compositor
    /// scales them to the screen's pixels).</summary>
    private void Fit()
    {
        if (panel.ActualWidth >= 1 && panel.ActualHeight >= 1)
            video.Resize((uint)panel.ActualWidth, (uint)panel.ActualHeight);
    }

    public async Task ShootAsync(Dev dev)
    {
        var report = Path.ChangeExtension(dev.Shot!, ".txt");
        try
        {
            await Task.Delay(TimeSpan.FromSeconds(dev.Wait));
            var hwnd = WinRT.Interop.WindowNative.GetWindowHandle(this);
            Win32.SetForegroundWindow(hwnd);
            await Task.Delay(300);
            await Shot.Save(hwnd, dev.Shot!);
            var before = video.Frames();
            AppWindow.Resize(new Windows.Graphics.SizeInt32(dev.Width * 2 / 3, dev.Height * 2 / 3));
            await Task.Delay(1500);
            await Shot.Save(hwnd, Path.ChangeExtension(dev.Shot!, null) + "-2.png");
            var lines = new List<string>(video.Report())
            {
                $"panel {panel.ActualWidth:0}x{panel.ActualHeight:0}, scale {panel.CompositionScaleX:0.##}",
                $"frames in the 1.8 s between the pictures: {video.Frames() - before}",
            };
            File.WriteAllLines(report, lines);
        }
        catch (Exception e)
        {
            File.WriteAllText(report, "failed: " + e);
        }
        Close();
        Application.Current.Exit();
    }
}
