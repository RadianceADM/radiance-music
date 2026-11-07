using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Shapes;
using RadianceMusic.Native;
using Ffi = RadianceMusic.Native.Native;

namespace RadianceMusic.Dialogs;

/// <summary>
/// Share Songs with a Phone, as the Linux app's (src/ui/sharing.rs, the engine's share; stage 4b): To the Phone (this
/// computer's saved liked songs, with music videos or not) or From the Phone, the QR the phone scans (round dots and
/// rounded eyes on white, so every camera reads it), what's happening under it, how far, going gently while music plays
/// to Bluetooth, and Let It In when Windows' firewall may keep the phone out. Closing it stops the share.
/// </summary>
internal static class ShareDialog
{
    public static async Task Show(EngineHub hub, XamlRoot root, bool toPhone = true)
    {
        var w = Ffi.ShareWords();
        var way = new SelectorBar();
        way.Items.Add(new SelectorBarItem { Text = w.ToPhone, Icon = new FontIcon { Glyph = "" } });
        way.Items.Add(new SelectorBarItem { Text = w.FromPhone, Icon = new FontIcon { Glyph = "" } });
        way.SelectedItem = way.Items[toPhone ? 0 : 1];
        var videos = new ToggleSwitch { Header = w.WithVideos, IsOn = false };
        var videosSub = new TextBlock { Text = w.WithVideosSub, Style = (Style)Application.Current.Resources["CaptionTextBlockStyle"], Foreground = (Brush)Application.Current.Resources["TextFillColorSecondaryBrush"] };
        var qr = new Canvas { Width = 248, Height = 248 };
        var qrCard = new Border { Child = qr, Background = new SolidColorBrush(Microsoft.UI.Colors.White), CornerRadius = new CornerRadius(22), Padding = new Thickness(16), HorizontalAlignment = HorizontalAlignment.Center };
        var caption = new TextBlock { TextWrapping = TextWrapping.Wrap, TextAlignment = TextAlignment.Center };
        var bar = new ProgressBar { IsIndeterminate = true, Minimum = 0, Maximum = 100, Visibility = Visibility.Collapsed };
        var progress = new TextBlock { TextWrapping = TextWrapping.Wrap, TextAlignment = TextAlignment.Center, Style = (Style)Application.Current.Resources["CaptionTextBlockStyle"], Visibility = Visibility.Collapsed };
        var gentle = new TextBlock { Text = w.Gentle, TextWrapping = TextWrapping.Wrap, Style = (Style)Application.Current.Resources["CaptionTextBlockStyle"], Foreground = (Brush)Application.Current.Resources["TextFillColorSecondaryBrush"], Visibility = Visibility.Collapsed };
        var letIn = new Button { Content = Labelled("", w.LetItIn) };
        letIn.Click += (_, _) =>
        {
            letIn.IsEnabled = false;
            hub.Engine.ShareLetIn();
        };
        var firewall = new InfoBar { Title = w.FirewallTitle, Message = w.FirewallSub, Severity = InfoBarSeverity.Warning, IsClosable = false, ActionButton = letIn, IsOpen = false };
        var form = new StackPanel { Spacing = 12, Width = 440 };
        form.Children.Add(way);
        form.Children.Add(firewall);
        form.Children.Add(qrCard);
        form.Children.Add(caption);
        form.Children.Add(bar);
        form.Children.Add(progress);
        form.Children.Add(gentle);
        form.Children.Add(videos);
        form.Children.Add(videosSub);
        var dialog = new ContentDialog
        {
            XamlRoot = root,
            Title = w.Title,
            Content = new ScrollViewer { Content = form, VerticalScrollBarVisibility = ScrollBarVisibility.Auto },
            CloseButtonText = "Close",
            CornerRadius = new CornerRadius(16),
        };
        dialog.Resources["ContentDialogMaxWidth"] = 560.0;

        var shownQr = "-";
        void Render()
        {
            var s = hub.Engine.Share();
            if (s.Qr != shownQr)
            {
                shownQr = s.Qr;
                DrawQr(qr, s.Qr);
                qrCard.Opacity = s.Qr.Length > 0 ? 1 : 0.35;
            }
            caption.Text = s.Caption;
            var going = s.Progress.Length > 0;
            bar.Visibility = going && !s.Done ? Visibility.Visible : Visibility.Collapsed;
            bar.IsIndeterminate = s.Fraction < 0;
            if (s.Fraction >= 0) bar.Value = s.Fraction * 100;
            progress.Text = s.Progress;
            progress.Visibility = going ? Visibility.Visible : Visibility.Collapsed;
            gentle.Visibility = s.Gentle ? Visibility.Visible : Visibility.Collapsed;
            firewall.IsOpen = s.Firewall;
            if (!s.Firewall) letIn.IsEnabled = true;
        }
        void Start()
        {
            var to = way.SelectedItem == way.Items[0];
            videos.IsEnabled = to;
            hub.Engine.ShareStart(to, to && videos.IsOn);
        }
        void OnChange(ChangeKind kind)
        {
            if (kind == ChangeKind.Share) Render();
        }
        way.SelectionChanged += (_, _) => Start();
        videos.Toggled += (_, _) => Start();
        hub.Change += OnChange;
        dialog.Closed += (_, _) =>
        {
            hub.Change -= OnChange;
            hub.Engine.ShareStop(); // the QR stops working, transfers stop
        };
        Start();
        Render();
        await dialog.ShowAsync();
    }

    /// <summary>The QR from the engine's modules: soft round dots, the three eyes as rounded squares (the Linux
    /// app's draw_qr, nothing sharp), dark on white. Empty text: no QR (no network yet).</summary>
    private static void DrawQr(Canvas canvas, string text)
    {
        canvas.Children.Clear();
        if (text.Length == 0 || Ffi.QrModules(text) is not { } code) return;
        var n = (int)code.Width;
        var cell = canvas.Width / n;
        var ink = new SolidColorBrush(Microsoft.UI.ColorHelper.FromArgb(255, 0x0C, 0x0A, 0x09));
        var paper = new SolidColorBrush(Microsoft.UI.Colors.White);
        bool Eye(int x, int y) => (x < 7 && y < 7) || (x >= n - 7 && y < 7) || (x < 7 && y >= n - 7);
        for (var y = 0; y < n; y++)
            for (var x = 0; x < n; x++)
                if (code.Dark[y * n + x] && !Eye(x, y))
                {
                    var dot = new Ellipse { Width = cell * 0.92, Height = cell * 0.92, Fill = ink };
                    Canvas.SetLeft(dot, x * cell + cell * 0.04);
                    Canvas.SetTop(dot, y * cell + cell * 0.04);
                    canvas.Children.Add(dot);
                }
        foreach (var (ex, ey) in new[] { (0, 0), (n - 7, 0), (0, n - 7) })
        {
            void Square(double inset, double side, double radius, Brush fill)
            {
                var r = new Rectangle { Width = side * cell, Height = side * cell, RadiusX = radius * cell, RadiusY = radius * cell, Fill = fill };
                Canvas.SetLeft(r, (ex + inset) * cell);
                Canvas.SetTop(r, (ey + inset) * cell);
                canvas.Children.Add(r);
            }
            Square(0, 7, 2.2, ink);
            Square(1, 5, 1.5, paper);
            Square(2, 3, 1.1, ink);
        }
    }

    private static StackPanel Labelled(string glyph, string text)
    {
        var p = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        p.Children.Add(new FontIcon { Glyph = glyph, FontSize = 14 });
        p.Children.Add(new TextBlock { Text = text });
        return p;
    }
}
