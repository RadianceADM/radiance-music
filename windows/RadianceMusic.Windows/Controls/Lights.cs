using System.Numerics;
using Microsoft.UI.Composition;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Hosting;

namespace RadianceMusic.Controls;

/// <summary>
/// Home's banner lights (the GTK app's lights.rs): two soft radial lights drifting across and back,
/// 18 s each way, eased. Here they're the compositor's own visuals and keyframe animations: the app
/// draws nothing per frame (no CompositionTarget.Rendering), and a hidden window costs nothing.
/// </summary>
public sealed class Lights : Grid
{
    private const double LegSeconds = 18;
    private readonly Compositor compositor;
    private readonly ContainerVisual root;
    private readonly SpriteVisual[] lights = new SpriteVisual[2];
    private readonly CompositionRadialGradientBrush[] brushes = new CompositionRadialGradientBrush[2];
    // (scale of the box, the light's centre in it, where the box goes from → to, colour); the second first (CSS layers)
    private static readonly (float Scale, Vector2 Centre, Vector2 From, Vector2 To, Windows.UI.Color Colour)[] Specs =
    {
        (1.6f, new Vector2(0.70f, 0.65f), new Vector2(1f, 1f), new Vector2(0f, 0.3f), Windows.UI.Color.FromArgb(56, 0xFF, 0xAE, 0x63)),
        (1.8f, new Vector2(0.35f, 0.45f), new Vector2(0f, 0f), new Vector2(1f, 0.7f), Windows.UI.Color.FromArgb(107, 0xF3, 0x8A, 0x35)),
    };

    public Lights()
    {
        IsHitTestVisible = false;
        compositor = ElementCompositionPreview.GetElementVisual(this).Compositor;
        root = compositor.CreateContainerVisual();
        for (var i = 0; i < Specs.Length; i++)
        {
            var s = Specs[i];
            var brush = brushes[i] = compositor.CreateRadialGradientBrush();
            brush.MappingMode = CompositionMappingMode.Absolute;
            brush.ColorStops.Add(compositor.CreateColorGradientStop(0f, s.Colour));
            brush.ColorStops.Add(compositor.CreateColorGradientStop(1f, Windows.UI.Color.FromArgb(0, s.Colour.R, s.Colour.G, s.Colour.B)));
            lights[i] = compositor.CreateSpriteVisual();
            lights[i].Brush = brush;
            root.Children.InsertAtTop(lights[i]);
        }
        ElementCompositionPreview.SetElementChildVisual(this, root);
        SizeChanged += (_, e) => Lay(e.NewSize);
    }

    /// <summary>Their boxes for this size, and their way across (from → to, there and back, for ever).</summary>
    private void Lay(Windows.Foundation.Size size)
    {
        var (w, h) = ((float)size.Width, (float)size.Height);
        if (w <= 0 || h <= 0) return;
        root.Size = new Vector2(w, h);
        root.Clip = compositor.CreateGeometricClip(compositor.CreateRoundedRectangleGeometry());
        ((CompositionRoundedRectangleGeometry)((CompositionGeometricClip)root.Clip).Geometry).Size = new Vector2(w, h);
        ((CompositionRoundedRectangleGeometry)((CompositionGeometricClip)root.Clip).Geometry).CornerRadius = new Vector2(26, 26);
        var ease = compositor.CreateCubicBezierEasingFunction(new Vector2(0.42f, 0f), new Vector2(0.58f, 1f));
        for (var i = 0; i < Specs.Length; i++)
        {
            var s = Specs[i];
            var (iw, ih) = (w * s.Scale, h * s.Scale);
            lights[i].Size = new Vector2(iw, ih);
            // a circle reaching the box's farthest side, as lights.rs draws it
            var r = Math.Max(Math.Max(s.Centre.X * iw, (1 - s.Centre.X) * iw), Math.Max(s.Centre.Y * ih, (1 - s.Centre.Y) * ih));
            brushes[i].EllipseCenter = new Vector2(s.Centre.X * iw, s.Centre.Y * ih);
            brushes[i].EllipseRadius = new Vector2(r, r);
            Vector3 At(Vector2 p) => new((w - iw) * p.X, (h - ih) * p.Y, 0);
            var move = compositor.CreateVector3KeyFrameAnimation();
            move.InsertKeyFrame(0f, At(s.From));
            move.InsertKeyFrame(1f, At(s.To), ease);
            move.Duration = TimeSpan.FromSeconds(LegSeconds);
            move.IterationBehavior = AnimationIterationBehavior.Forever;
            move.Direction = AnimationDirection.Alternate;
            lights[i].StartAnimation("Offset", move);
        }
    }
}
