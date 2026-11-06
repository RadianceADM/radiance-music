using System.Diagnostics;
using System.Numerics;
using Microsoft.UI.Composition;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Hosting;
using Microsoft.UI.Xaml.Media;
using RadianceMusic.Native;

namespace RadianceMusic.Controls;

/// <summary>
/// Now Playing's moving background (the GTK app's backdrop.rs): the art blurred and turning slowly
/// over the whole page, never zoomed past MAX_ZOOM; three soft patches of it on top, each wandering
/// around its own part of the page, swelling a little and turning its own way, so the colours flow
/// like a lava lamp. A new song's art fades in over the last one's (FADE_US). All of it is the
/// compositor's: keyframe animations that loop for ever, sampled from backdrop.rs's own sine waves;
/// the app draws nothing per frame. It follows the beat (#56, stage 3): a kick swells the art at once
/// (the "Beat" property set's Swell, read by the scale expressions) and pushes the flow ahead; strong
/// bass flows it faster (one AnimationController runs every wave and turn, its PlaybackRate the speed).
/// <see cref="BeatFollower"/> is fed the song's pulse at the playing moment, 30 times a second, only while
/// <see cref="Follow"/> says so (Now Playing shown, the window seen, the song playing).
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
    // the beat: the swell the scale expressions read, the flow's one clock, and what moves them
    private readonly CompositionPropertySet beat;
    private readonly AnimationController flow;
    private readonly BeatFollower follower = new();
    private readonly DispatcherQueueTimer tick;
    private PulseView? pulse;
    private Func<double?>? songAt;
    private long lastTick;

    public Backdrop()
    {
        IsHitTestVisible = false;
        compositor = ElementCompositionPreview.GetElementVisual(this).Compositor;
        root = compositor.CreateContainerVisual();
        root.Clip = compositor.CreateInsetClip();
        ElementCompositionPreview.SetElementChildVisual(this, root);
        beat = compositor.CreatePropertySet();
        beat.InsertScalar("Swell", 0f);
        flow = compositor.CreateAnimationController();
        tick = DispatcherQueue.GetForCurrentThread().CreateTimer();
        tick.Interval = TimeSpan.FromMilliseconds(33);
        tick.Tick += (_, _) => Beat();
        SizeChanged += (_, e) => Lay(e.NewSize);
        Unloaded += (_, _) => Follow(null);
    }

    /// <summary>The playing song's pulse (beat.rs: its beat and bass, frame by frame); null: none yet.</summary>
    internal void SetPulse(PulseView? p)
    {
        pulse = p;
        if (p == null) Follow(null);
    }

    /// <summary>Follow the beat (`at`: the song's moment now, null while it's not playing) or stop (null):
    /// nothing runs per tick then, and the flow is as without music.</summary>
    public void Follow(Func<double?>? at)
    {
        songAt = pulse != null ? at : null;
        if (songAt != null)
        {
            if (!tick.IsRunning)
            {
                lastTick = 0;
                tick.Start();
            }
            return;
        }
        if (tick.IsRunning) tick.Stop();
        follower.Rest();
        Apply();
    }

    private void Beat()
    {
        var now = Stopwatch.GetTimestamp();
        var dt = lastTick == 0 ? 0 : (now - lastTick) / (double)Stopwatch.Frequency;
        lastTick = now;
        float hit = 0, bass = 0;
        if (pulse is { } p && songAt?.Invoke() is { } at)
        {
            hit = BeatFollower.Sample(p.Level, p.Fps, p.Offset, at);
            bass = BeatFollower.Sample(p.Bass, p.Fps, p.Offset, at);
        }
        follower.Step(dt, hit, bass);
        Apply();
    }

    private void Apply()
    {
        beat.InsertScalar("Swell", (float)follower.Swell);
        flow.PlaybackRate = (float)Math.Min(follower.Speed, AnimationController.MaxPlaybackRate);
    }

    /// <summary>The art's scale now (the screenshots' check that it swells with the beat).</summary>
    internal (double Swell, double Speed) BeatNow => (follower.Swell, follower.Speed);

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
        // (the offsets are the top-left corner's: the centre minus half the side)
        Wave(c.Whole, "Offset.X", w / 2 - side / 2, 0.012f * w, 11, 0);
        Wave(c.Whole, "Offset.Y", h / 2 - side / 2, 0.012f * h, 9, 1);
        // its slow breathing, swollen by a kick (even swollen, never past MAX_ZOOM)
        Swollen(c.Whole, 0.02f / rest, 7, 0, 1f, MaxZoom / rest);
        Turn(c.Whole, 3, 0);
        flow.Resume();
        var longer = Math.Max(w, h);
        for (var i = 0; i < Patches.Length; i++)
        {
            var p = Patches[i];
            var k = i * 2.1;
            var s = c.Patches[i];
            var patch = longer * p.Part;
            Place(s, patch, 0);
            Wave(s, "Offset.X", w * p.X - patch / 2, 0.12f * w, 11 + k, k);
            Wave(s, "Offset.Y", h * p.Y - patch / 2, 0.12f * h, 13 - k, k + 0.7);
            Swollen(s, 0.06f, 7 + k, k, 1.3f, 100f);
            Turn(s, p.Turn, 90 * i);
        }
    }

    private static void Place(SpriteVisual s, float side, float angle)
    {
        s.Size = new Vector2(side, side);
        s.CenterPoint = new Vector3(side / 2, side / 2, 0);
        s.RotationAngleInDegrees = angle;
    }

    /// <summary>Its scale: 1 + swing·sin (a wave, on its own property set) times the kick's swell
    /// (`kick`·Swell, backdrop.rs: the art 1×, the patches 1.3×), never past `cap`.</summary>
    private void Swollen(SpriteVisual s, float swing, double period, double phase, float kick, float cap)
    {
        var breathing = compositor.CreatePropertySet();
        breathing.InsertScalar("S", 1f);
        Wave(breathing, "S", 1, swing, period, phase);
        var grow = $"Min(w.S * (1 + {kick.ToString(System.Globalization.CultureInfo.InvariantCulture)} * Beat.Swell), {cap.ToString(System.Globalization.CultureInfo.InvariantCulture)})";
        var scale = compositor.CreateExpressionAnimation($"Vector3({grow}, {grow}, 1)");
        scale.SetReferenceParameter("w", breathing);
        scale.SetReferenceParameter("Beat", beat);
        s.StartAnimation("Scale", scale);
    }

    /// <summary>`channel` = middle + swing·sin(t/period + phase), as backdrop.rs's wave(): one full turn
    /// of it is 2π·period seconds, sampled at 16 points, looped for ever (at the flow's speed).</summary>
    private void Wave(CompositionObject s, string channel, float middle, float swing, double period, double phase)
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
        s.StartAnimation(channel, a, flow);
    }

    /// <summary>Turning `degrees` a second (minus: the other way), from `from`.</summary>
    private void Turn(SpriteVisual s, float degrees, float from)
    {
        var a = compositor.CreateScalarKeyFrameAnimation();
        a.InsertKeyFrame(0f, from);
        a.InsertKeyFrame(1f, from + Math.Sign(degrees) * 360f, compositor.CreateLinearEasingFunction());
        a.Duration = TimeSpan.FromSeconds(360 / Math.Abs(degrees));
        a.IterationBehavior = AnimationIterationBehavior.Forever;
        s.StartAnimation("RotationAngleInDegrees", a, flow);
    }
}
