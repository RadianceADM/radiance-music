using System.Numerics;
using Microsoft.UI.Composition;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Hosting;
using Microsoft.UI.Xaml.Media;

namespace RadianceMusic.Controls;

/// <summary>
/// Now Playing's moving background (the GTK app's backdrop.rs): the art blurred and turning slowly
/// over the whole page, never zoomed past MAX_ZOOM; three soft patches of it on top, each wandering
/// around its own part of the page, swelling a little and turning its own way, so the colours flow
/// like a lava lamp. A new song's art fades in over the last one's (FADE_US). All of it is the
/// compositor's: keyframe animations that loop for ever, sampled from backdrop.rs's own sine waves;
/// the app draws nothing per frame. (The beat swell and bass flow come after stage 1.)
/// </summary>
public sealed class Backdrop : Grid
{
    private const float MaxZoom = 1.15f, Swell = 0.06f;
    private const double FadeSeconds = 0.9;
    /// <summary>(where it wanders: part of the page, its size: part of the longer side, its turn: degrees a second, how much it shows)</summary>
    private static readonly (float X, float Y, float Part, float Turn, float Shows)[] Patches =
    {
        (0.28f, 0.3f, 0.95f, 7f, 0.85f),
        (0.76f, 0.42f, 0.8f, -9f, 0.8f),
        (0.45f, 0.82f, 1.05f, 6f, 0.75f),
    };

    private readonly Compositor compositor;
    private readonly ContainerVisual root;
    private (ContainerVisual Layer, SpriteVisual Whole, SpriteVisual[] Patches)? current;
    private string? source;

    public Backdrop()
    {
        IsHitTestVisible = false;
        compositor = ElementCompositionPreview.GetElementVisual(this).Compositor;
        root = compositor.CreateContainerVisual();
        root.Clip = compositor.CreateInsetClip();
        ElementCompositionPreview.SetElementChildVisual(this, root);
        SizeChanged += (_, e) => Lay(e.NewSize);
    }

    /// <summary>The song's art (the engine's wash and blob files); null: none (the page's own colour).</summary>
    public void SetArt(string? wash, string? blob)
    {
        if (wash == source) return;
        source = wash;
        var old = current;
        if (wash == null || blob == null)
        {
            current = null;
            FadeOut(old);
            return;
        }
        var layer = compositor.CreateContainerVisual();
        var whole = Sprite(wash);
        layer.Children.InsertAtTop(whole);
        var patches = Patches.Select(p =>
        {
            var s = Sprite(blob);
            s.Opacity = p.Shows;
            layer.Children.InsertAtTop(s);
            return s;
        }).ToArray();
        layer.Opacity = 0;
        root.Children.InsertAtTop(layer);
        current = (layer, whole, patches);
        Lay(new Windows.Foundation.Size(ActualWidth, ActualHeight));
        var fade = compositor.CreateScalarKeyFrameAnimation();
        fade.InsertKeyFrame(1f, 1f);
        fade.Duration = TimeSpan.FromSeconds(FadeSeconds);
        layer.StartAnimation("Opacity", fade);
        FadeOut(old);
    }

    private void FadeOut((ContainerVisual Layer, SpriteVisual Whole, SpriteVisual[] Patches)? old)
    {
        if (old is not { } o) return;
        var batch = compositor.CreateScopedBatch(CompositionBatchTypes.Animation);
        var fade = compositor.CreateScalarKeyFrameAnimation();
        fade.InsertKeyFrame(1f, 0f);
        fade.Duration = TimeSpan.FromSeconds(FadeSeconds);
        o.Layer.StartAnimation("Opacity", fade);
        batch.End();
        batch.Completed += (_, _) => root.Children.Remove(o.Layer);
    }

    private SpriteVisual Sprite(string file)
    {
        var surface = LoadedImageSurface.StartLoadFromUri(new Uri(file));
        var brush = compositor.CreateSurfaceBrush(surface);
        brush.Stretch = CompositionStretch.Fill;
        var s = compositor.CreateSpriteVisual();
        s.Brush = brush;
        s.AnchorPoint = new Vector2(0.5f, 0.5f);
        return s;
    }

    /// <summary>The motion for this size: the art and its patches, each on its own waves, for ever.</summary>
    private void Lay(Windows.Foundation.Size size)
    {
        var (w, h) = ((float)size.Width, (float)size.Height);
        if (w <= 0 || h <= 0) return;
        root.Size = new Vector2(w, h);
        if (current is not { } c) return;
        c.Layer.Size = new Vector2(w, h);
        var diag = MathF.Sqrt(w * w + h * h);
        // the whole art: wide enough that no corner shows however it's turned (never past MAX_ZOOM)
        var rest = MaxZoom - Swell * 1.1f - 0.02f;
        var side = diag * rest;
        Place(c.Whole, side, 0);
        Wave(c.Whole, "Offset.X", w / 2, 0.012f * w, 11, 0);
        Wave(c.Whole, "Offset.Y", h / 2, 0.012f * h, 9, 1);
        Wave(c.Whole, "Scale.X", 1, 0.02f / rest, 7, 0);
        Wave(c.Whole, "Scale.Y", 1, 0.02f / rest, 7, 0);
        Turn(c.Whole, 3, 0);
        var longer = Math.Max(w, h);
        for (var i = 0; i < Patches.Length; i++)
        {
            var p = Patches[i];
            var k = i * 2.1;
            var s = c.Patches[i];
            Place(s, longer * p.Part, 0);
            Wave(s, "Offset.X", w * p.X, 0.12f * w, 11 + k, k);
            Wave(s, "Offset.Y", h * p.Y, 0.12f * h, 13 - k, k + 0.7);
            Wave(s, "Scale.X", 1, 0.06f, 7 + k, k);
            Wave(s, "Scale.Y", 1, 0.06f, 7 + k, k);
            Turn(s, p.Turn, 90 * i);
        }
    }

    private static void Place(SpriteVisual s, float side, float angle)
    {
        s.Size = new Vector2(side, side);
        s.CenterPoint = new Vector3(side / 2, side / 2, 0);
        s.RotationAngleInDegrees = angle;
    }

    /// <summary>`channel` = middle + swing·sin(t/period + phase), as backdrop.rs's wave(): one full turn
    /// of it is 2π·period seconds, sampled at 16 points, looped for ever.</summary>
    private void Wave(SpriteVisual s, string channel, float middle, float swing, double period, double phase)
    {
        const int Points = 16;
        var a = compositor.CreateScalarKeyFrameAnimation();
        var linear = compositor.CreateLinearEasingFunction();
        for (var j = 0; j <= Points; j++)
        {
            var x = (float)j / Points;
            a.InsertKeyFrame(x, middle + swing * (float)Math.Sin(2 * Math.PI * x + phase), linear);
        }
        a.Duration = TimeSpan.FromSeconds(2 * Math.PI * period);
        a.IterationBehavior = AnimationIterationBehavior.Forever;
        s.StartAnimation(channel, a);
    }

    /// <summary>Turning `degrees` a second (minus: the other way), from `from`.</summary>
    private void Turn(SpriteVisual s, float degrees, float from)
    {
        var a = compositor.CreateScalarKeyFrameAnimation();
        a.InsertKeyFrame(0f, from);
        a.InsertKeyFrame(1f, from + Math.Sign(degrees) * 360f, compositor.CreateLinearEasingFunction());
        a.Duration = TimeSpan.FromSeconds(360 / Math.Abs(degrees));
        a.IterationBehavior = AnimationIterationBehavior.Forever;
        s.StartAnimation("RotationAngleInDegrees", a);
    }
}
