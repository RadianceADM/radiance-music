using System.Diagnostics;
using System.Numerics;
using Microsoft.UI.Composition;
using Microsoft.UI.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Documents;
using Microsoft.UI.Xaml.Hosting;
using Microsoft.UI.Xaml.Markup;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Shapes;
using Windows.Foundation;
using Windows.UI;

namespace RadianceMusic.Controls;

/// <summary>
/// The start, as the Linux app's (src/ui/splash.rs, #55, owner: "ay asan yung loading???"): the icon's sun
/// rises out of the sea and its four bars (drums, bass, other, vocals) play like an equalizer while the app
/// gets ready; then the bars come to rest, the sun sets again and the logo writes itself (the R, its leg
/// rolling on into the m, the wave fading out), the name under it, and the dark opens from the middle out
/// into the app. In the Linux app's Stone look (the beta has no looks of its own: Windows' light or dark
/// is behind). Over the whole window; what's behind stays hidden (nothing laid out) until the logo is
/// written, so nothing it does shows as a stutter. A click goes straight in once it's ready. Never there
/// with animations off, or for screenshots (unless --splash-at). <see cref="SplashTimeline"/> says when.
/// </summary>
internal sealed class Splash : Grid
{
    // the icon's own drawing (data/icons/io.github.radianceadm.Music.svg, 128 across)
    private const float SunX = 64f, SunY = 67.5f, SunR = 34.8f, BarW = 5.8f, BarFoot = 112.7f, SeaTop = 87.2f, SeaBottom = 105.8f, RiseFrom = 45f;
    private static readonly (float X, float Top)[] BarsAt = { (50.7f, 73.3f), (59.6f, 60.5f), (68.4f, 67.5f), (77.3f, 77.9f) };
    // the logo (data/brand/radiance-music-logo.svg, its 0..100 units, the R's middle at 50) and each stroke's length
    private const string LogoR = "M38 71V29H51.5A10.5 10.5 0 0 1 51.5 50H38";
    private const string LogoLeg = "M50.5 50C56.5 51.5 61 59 61 71C62.5467 60 63.4167 52.5 68.25 52.5C73.0833 52.5 73.9533 60 75.5 70.6C77.0467 60 77.9167 52.5 82.75 52.5C87.5833 52.5 88.0667 71 90.5 71";
    private const string LogoTail = "M90.5 71C94.5 71 97.5 66 100.5 61";
    private const double Pen = 8.8, RLength = 101.987, LegLength = 108.940, TailLength = 14.713;
    private const float Soft = 0.3f;

    private static readonly Color White = Rgb(0xF5F7FA), SunTop = Rgb(0xFFDB94), Orange = Rgb(0xF38A35), Glow = Rgb(0xFFAE63);
    // Stone: the sky at the top and at the bottom (the sea, the bars)
    private static readonly Color Top = Rgb(0x1C1917), Bottom = Rgb(0x0C0A09);

    private readonly SplashTimeline line = new();
    private readonly Stopwatch clock = new();
    private readonly List<(UIElement Element, Visibility Was)> behind = new();
    private readonly Border darkHost = new();
    private readonly Canvas sunGroup = new();
    private readonly Ellipse sun = new();
    private readonly Rectangle[] bars = new Rectangle[4];
    private readonly Rectangle seaFade = new(), sea = new();
    private readonly Canvas logo = new();
    private readonly CompositeTransform logoAt = new();
    private readonly Microsoft.UI.Xaml.Shapes.Path r, leg, tail;
    private readonly TextBlock name = new() { FontWeight = FontWeights.Bold, FontFamily = new FontFamily("Segoe UI Variable Display"), HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Top, IsHitTestVisible = false };
    private SpriteVisual? glow;
    private CompositionRadialGradientBrush? glowBrush;
    private readonly List<CompositionRadialGradientBrush> masks = new();
    private double lastFrame = -1;
    private int frames, late;
    private double worstLate;
    private bool started;

    /// <summary>The time on the sunrise's clock (the screenshots' --splash-at).</summary>
    public double Elapsed => clock.Elapsed.TotalSeconds;
    public bool Finished => line.Done;

    /// <summary>Over everything in `root` until it's done; null (nothing shown) with animations off or for
    /// screenshots, unless `forShots`.</summary>
    public static Splash? Cover(Grid root, bool forShots)
    {
        if (!new Windows.UI.ViewManagement.UISettings().AnimationsEnabled && !forShots) return null;
        var splash = new Splash();
        foreach (var child in root.Children)
        {
            splash.behind.Add((child, child.Visibility));
            child.Visibility = Visibility.Collapsed; // nothing behind laid out or drawn meanwhile
        }
        if (root.RowDefinitions.Count > 0) SetRowSpan(splash, root.RowDefinitions.Count);
        if (root.ColumnDefinitions.Count > 0) SetColumnSpan(splash, root.ColumnDefinitions.Count);
        root.Children.Add(splash);
        return splash;
    }

    private Splash()
    {
        Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent); // a click anywhere goes in
        Children.Add(darkHost);
        // the sun, rising (and setting again), its bars playing, the sea in front of its foot
        sun.Fill = new LinearGradientBrush
        {
            StartPoint = new Point(0, 0),
            EndPoint = new Point(0, 1),
            GradientStops = { new GradientStop { Color = SunTop, Offset = 0 }, new GradientStop { Color = Orange, Offset = 1 } },
        };
        sunGroup.Children.Add(sun);
        for (var i = 0; i < bars.Length; i++)
        {
            bars[i] = new Rectangle { Fill = new SolidColorBrush(Bottom) };
            sunGroup.Children.Add(bars[i]);
        }
        seaFade.Fill = new LinearGradientBrush
        {
            StartPoint = new Point(0, 0),
            EndPoint = new Point(0, 1),
            GradientStops = { new GradientStop { Color = WithAlpha(Bottom, 0), Offset = 0 }, new GradientStop { Color = Bottom, Offset = 1 } },
        };
        sea.Fill = new SolidColorBrush(Bottom);
        sunGroup.Children.Add(seaFade);
        sunGroup.Children.Add(sea);
        sunGroup.IsHitTestVisible = false;
        Children.Add(sunGroup);
        // then the logo writes itself, in its own units
        r = Stroke(LogoR, new SolidColorBrush(White), PenLineCap.Round);
        leg = Stroke(LogoLeg, new LinearGradientBrush
        {
            MappingMode = BrushMappingMode.Absolute,
            StartPoint = new Point(50.5, 0),
            EndPoint = new Point(90.5, 0),
            GradientStops = { new GradientStop { Color = White, Offset = 0 }, new GradientStop { Color = SunTop, Offset = 0.24 }, new GradientStop { Color = Orange, Offset = 1 } },
        }, PenLineCap.Round);
        tail = Stroke(LogoTail, new LinearGradientBrush
        {
            MappingMode = BrushMappingMode.Absolute,
            StartPoint = new Point(90.5, 71),
            EndPoint = new Point(100.5, 61),
            GradientStops = { new GradientStop { Color = Orange, Offset = 0 }, new GradientStop { Color = WithAlpha(Orange, 0), Offset = 1 } },
        }, PenLineCap.Flat);
        logo.Children.Add(r);
        logo.Children.Add(leg);
        logo.Children.Add(tail);
        logo.RenderTransform = logoAt;
        logo.IsHitTestVisible = false;
        Children.Add(logo);
        name.Inlines.Add(new Run { Text = "Radiance ", Foreground = new SolidColorBrush(White) });
        name.Inlines.Add(new Run { Text = "Music", Foreground = new SolidColorBrush(Orange) });
        name.Opacity = 0;
        Children.Add(name);
        Tapped += (_, _) =>
        {
            line.Skip();
            if (line.FadeFrom != null) IsHitTestVisible = false; // the app answers clicks from here on
        };
        Loaded += (_, _) => Start();
        SizeChanged += (_, _) => Place();
    }

    /// <summary>Everything's ready: the bars come to rest and the logo writes itself (once it has shown whole).</summary>
    public void Ready() => line.SetReady();

    private static Microsoft.UI.Xaml.Shapes.Path Stroke(string data, Brush brush, PenLineCap cap)
    {
        var path = (Microsoft.UI.Xaml.Shapes.Path)XamlReader.Load($"<Path xmlns='http://schemas.microsoft.com/winfx/2006/xaml/presentation' Data='{data}' />");
        path.Stroke = brush;
        path.StrokeThickness = Pen;
        path.StrokeStartLineCap = cap;
        path.StrokeEndLineCap = cap;
        path.StrokeDashCap = cap;
        path.StrokeLineJoin = PenLineJoin.Round;
        path.Visibility = Visibility.Collapsed;
        return path;
    }

    /// <summary>The dark (the sky and the sun's glow) as the compositor's: it opens from the middle out at the end.</summary>
    private void Start()
    {
        if (started) return;
        started = true;
        var compositor = ElementCompositionPreview.GetElementVisual(darkHost).Compositor;
        var both = compositor.CreateContainerVisual();
        both.RelativeSizeAdjustment = Vector2.One;
        var skyBrush = compositor.CreateLinearGradientBrush();
        skyBrush.StartPoint = new Vector2(0, 0);
        skyBrush.EndPoint = new Vector2(0.35f, 1);
        skyBrush.ColorStops.Add(compositor.CreateColorGradientStop(0, Top));
        skyBrush.ColorStops.Add(compositor.CreateColorGradientStop(1, Bottom));
        var glowing = compositor.CreateRadialGradientBrush();
        glowing.MappingMode = CompositionMappingMode.Absolute;
        glowing.ColorStops.Add(compositor.CreateColorGradientStop(0, WithAlpha(Glow, 0.4)));
        glowing.ColorStops.Add(compositor.CreateColorGradientStop(1, WithAlpha(Glow, 0)));
        glowBrush = glowing;
        var skyVisual = compositor.CreateSpriteVisual();
        var glowVisual = compositor.CreateSpriteVisual();
        glow = glowVisual;
        foreach (var (visual, source) in new (SpriteVisual, CompositionBrush)[] { (skyVisual, skyBrush), (glowVisual, glowing) })
        {
            // what's left of the dark: none inside the opening, all of it past its soft edge
            var mask = compositor.CreateRadialGradientBrush();
            mask.MappingMode = CompositionMappingMode.Absolute;
            foreach (var (at, alpha) in new[] { (0f, 0.0), (0f, 0.0), (0f, 1.0), (1f, 1.0) })
                mask.ColorStops.Add(compositor.CreateColorGradientStop(at, WithAlpha(Microsoft.UI.Colors.Black, alpha)));
            masks.Add(mask);
            var masked = compositor.CreateMaskBrush();
            masked.Source = source;
            masked.Mask = mask;
            visual.Brush = masked;
            visual.RelativeSizeAdjustment = Vector2.One;
            both.Children.InsertAtTop(visual);
        }
        ElementCompositionPreview.SetElementChildVisual(darkHost, both);
        Place();
        clock.Start();
        CompositionTarget.Rendering += OnFrame;
    }

    private void OnFrame(object? sender, object e)
    {
        var t = clock.Elapsed.TotalSeconds;
        // a late frame only shows while something moves (not with the logo written and standing still)
        var still = line.SettleFrom is { } settled && t >= settled + SplashTimeline.SettleS + SplashTimeline.WriteS + SplashTimeline.WrittenS && line.FadeFrom == null;
        if (lastFrame >= 0 && t - lastFrame > 0.05 && !still)
        {
            late++;
            worstLate = Math.Max(worstLate, t - lastFrame);
        }
        lastFrame = t;
        frames++;
        var wasShown = line.ContentShown;
        line.Frame(t);
        if (line.ContentShown && !wasShown) ShowBehind();
        if (line.FadeFrom != null) IsHitTestVisible = false;
        if (line.Done)
        {
            CompositionTarget.Rendering -= OnFrame;
            ShowBehind();
            ElementCompositionPreview.SetElementChildVisual(darkHost, null);
            (Parent as Panel)?.Children.Remove(this);
            App.Trace($"splash: {frames} frames in {t:0.0} s, {late} late (the slowest {worstLate * 1000:0} ms)");
            return;
        }
        Place();
    }

    private void ShowBehind()
    {
        foreach (var (element, was) in behind) element.Visibility = was;
        behind.Clear();
    }

    /// <summary>Everything where the timeline says, for this size (the Linux app's snapshot()).</summary>
    private void Place()
    {
        var (w, h) = ((float)ActualWidth, (float)ActualHeight);
        if (w <= 0 || h <= 0) return;
        var u = Math.Min(w, h) * 0.5f / 128f;
        Point At(float x, float y) => new(w / 2 + (x - 64) * u, h / 2 + (y - 64) * u);
        var (rise, play, set, reveal) = ((float)line.Rise, (float)line.Play, (float)line.Set, (float)line.Reveal);
        // the dark opening (the app showing through), and the sun's glow in it
        var inner = Math.Clamp(reveal * (1 + Soft) - Soft, 0, 1);
        var outer = Math.Clamp(reveal * (1 + Soft), inner, 1);
        var radius = MathF.Sqrt(w * w + h * h) / 2;
        foreach (var mask in masks)
        {
            mask.EllipseCenter = new Vector2(w / 2, h / 2);
            mask.EllipseRadius = new Vector2(radius, radius);
            mask.ColorStops[1].Offset = inner;
            mask.ColorStops[2].Offset = outer;
        }
        if (glowBrush != null && glow != null)
        {
            var g = At(64, 69.8f);
            glowBrush.EllipseCenter = new Vector2((float)g.X, (float)g.Y);
            glowBrush.EllipseRadius = new Vector2(100 * u, 100 * u);
            glow.Opacity = rise;
        }
        // the sun, its bars and the sea (gone once it has set)
        sunGroup.Visibility = set < 1 ? Visibility.Visible : Visibility.Collapsed;
        if (set < 1)
        {
            sunGroup.Opacity = 1 - set * 0.6;
            var lift = (1 - rise) * RiseFrom + set * RiseFrom;
            var c = At(SunX, SunY + lift);
            Put(sun, c.X - SunR * u, c.Y - SunR * u, 2 * SunR * u, 2 * SunR * u);
            var t = line.T;
            for (var i = 0; i < bars.Length; i++)
            {
                var (x, top) = BarsAt[i];
                var bounce = (float)Math.Sin(t * Math.Tau * 1.6 + i * 1.7) * 7f * play;
                var (a, b) = (At(x - BarW / 2, top + bounce + lift), At(x + BarW / 2, BarFoot + lift));
                bars[i].RadiusX = bars[i].RadiusY = BarW / 2 * u;
                Put(bars[i], a.X, a.Y - BarW / 2 * u, b.X - a.X, b.Y - a.Y + BarW * u);
            }
            var (y0, y1) = (At(0, SeaTop).Y, At(0, SeaBottom).Y);
            Put(seaFade, 0, y0, w, y1 - y0);
            Put(sea, 0, y1 - 0.5, w, h - y1 + 0.5);
        }
        // the logo writing itself: each stroke drawn so far along (a dash as long as the stroke, moved back)
        var (pr, pl, pt) = line.Strokes;
        Draw(r, pr, RLength);
        Draw(leg, pl, LegLength);
        Draw(tail, pt, TailLength);
        var v = Math.Min(w, h) * 0.55 / 100;
        logoAt.ScaleX = logoAt.ScaleY = v;
        logoAt.TranslateX = w / 2 - 50 * v;
        logoAt.TranslateY = h / 2 - 50 * v;
        var fadeOut = line.LogoOpacity;
        logo.Opacity = fadeOut;
        // and the name under it, coming up softly as the wave goes out
        var named = line.Name;
        name.FontSize = Math.Max(1, 11 * v);
        name.Margin = new Thickness(0, h / 2 + (36 + 4 * (1 - named)) * v, 0, 0);
        name.Opacity = named * fadeOut;
    }

    private static void Draw(Microsoft.UI.Xaml.Shapes.Path stroke, double part, double length)
    {
        if (part <= 0.001)
        {
            stroke.Visibility = Visibility.Collapsed;
            return;
        }
        stroke.Visibility = Visibility.Visible;
        var dash = length / Pen; // in the pen's widths
        if (stroke.StrokeDashArray.Count == 0)
        {
            stroke.StrokeDashArray.Add(dash);
            stroke.StrokeDashArray.Add(dash + 1);
        }
        stroke.StrokeDashOffset = (1 - part) * dash;
    }

    private static void Put(FrameworkElement e, double x, double y, double width, double height)
    {
        Canvas.SetLeft(e, x);
        Canvas.SetTop(e, y);
        e.Width = Math.Max(0, width);
        e.Height = Math.Max(0, height);
    }

    private static Color Rgb(uint hex) => Color.FromArgb(255, (byte)(hex >> 16), (byte)(hex >> 8), (byte)hex);

    private static Color WithAlpha(Color c, double a) => Color.FromArgb((byte)Math.Round(a * 255), c.R, c.G, c.B);
}
