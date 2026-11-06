using System.Numerics;
using Microsoft.UI.Composition;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Documents;
using Microsoft.UI.Xaml.Hosting;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Animation;
using RadianceMusic.Native;
using Ffi = RadianceMusic.Native.Native;

namespace RadianceMusic.Controls;

/// <summary>
/// The lyrics, as the GTK app's: the lines being sung lit (others at a third), each word lit whole
/// as it's sung (the rest of the line at 40 %), the singer's name small above a line where the voice
/// changes, "♪" for a break; lines grow toward the middle (on the compositor: no work per frame here)
/// and the sung line, big, stays in the middle; a click on a line goes there. Plain lyrics as text.
/// `big`: Now Playing's column (the sung line grows 2.5×); else the side panel's.
/// </summary>
internal sealed class LyricsColumn : Grid
{
    private sealed class Shown
    {
        /// <summary>The line's fade (a XAML OpacityTransition).</summary>
        public required StackPanel Box;
        /// <summary>Around Box: the lens scales this one (WinUI won't hand out the visual of an
        /// element with an OpacityTransition: "Access is denied", and the app goes down).</summary>
        public required Grid Frame;
        public required TextBlock Text;
        public required Run Lit;
        public required Run Rest;
        public required string Words;
        public CompositionPropertySet? Props;
    }

    private const double Grow = 2.5, Least = 1.5, Room = 0.6;
    private readonly EngineHub hub;
    private readonly bool big;
    private readonly ScrollViewer scroll = new() { VerticalScrollBarVisibility = ScrollBarVisibility.Hidden, HorizontalScrollMode = ScrollMode.Disabled };
    private readonly StackPanel panel = new() { Spacing = 14 };
    private readonly TextBlock note = new() { TextWrapping = TextWrapping.Wrap, VerticalAlignment = VerticalAlignment.Center, Opacity = 0.7 };
    private readonly List<Shown> lines = new();
    private readonly LyricFollower follower = new();
    private readonly DispatcherQueueTimer tick, check;
    private CompositionPropertySet? view;
    private string key = "";
    private int? lead;
    private int misses;

    private double Size => big ? 28 : 19.6;

    public LyricsColumn(EngineHub hub, bool big)
    {
        this.hub = hub;
        this.big = big;
        Padding = new Thickness(big ? 0 : 16, 0, 16, 0);
        scroll.Content = panel;
        Children.Add(scroll);
        Children.Add(note);
        tick = DispatcherQueue.GetForCurrentThread().CreateTimer();
        tick.Interval = TimeSpan.FromMilliseconds(33);
        tick.Tick += (_, _) => Light();
        check = DispatcherQueue.GetForCurrentThread().CreateTimer();
        check.Interval = TimeSpan.FromSeconds(1);
        check.Tick += (_, _) => KeepLeadShown();
        scroll.SizeChanged += (_, e) =>
        {
            panel.Padding = new Thickness(0, Math.Max(0, e.NewSize.Height / 2 - 40), 0, Math.Max(0, e.NewSize.Height / 2 - 40));
            view?.InsertScalar("Half", (float)(e.NewSize.Height / 2));
        };
        panel.LayoutUpdated += (_, _) => PlaceLenses();
        Loaded += (_, _) =>
        {
            var s = ElementCompositionPreview.GetScrollViewerManipulationPropertySet(scroll);
            view ??= s.Compositor.CreatePropertySet();
            view.InsertScalar("Half", (float)(scroll.ActualHeight / 2));
            Render();
            Clocks();
            check.Start();
        };
        Unloaded += (_, _) =>
        {
            tick.Stop();
            check.Stop();
        };
        RegisterPropertyChangedCallback(VisibilityProperty, (_, _) => Clocks());
        hub.Change += kind =>
        {
            if (kind is ChangeKind.Lyrics or ChangeKind.Online or ChangeKind.Now) Render();
            if (kind is ChangeKind.Now) Clocks();
            if (kind is ChangeKind.Position) Light();
        };
    }

    /// <summary>The 30-a-second count runs only while it's shown, the song plays, and there are lines.</summary>
    private void Clocks()
    {
        var on = IsLoaded && Visibility == Visibility.Visible && lines.Count > 0 && hub.Engine.Now().Playing;
        if (on && !tick.IsRunning) tick.Start();
        else if (!on && tick.IsRunning) tick.Stop();
    }

    private void Render()
    {
        var l = hub.Engine.Lyrics();
        var song = hub.Engine.Now().Song?.Key ?? "";
        var k = song + "|" + l.Kind + "|" + l.Lines.Length + "|" + l.Plain.Length + "|" + hub.Engine.Online();
        if (k == key) return;
        key = k;
        panel.Children.Clear();
        lines.Clear();
        follower.Reset();
        lead = null;
        note.Visibility = Visibility.Collapsed;
        switch (l.Kind)
        {
            case LyricsKind.Looking:
                Say("Looking for lyrics…");
                break;
            case LyricsKind.None:
                Say(hub.Engine.Online() ? "No lyrics found for this song." : "You're offline: the lyrics come when you're back online.");
                break;
            case LyricsKind.Plain:
                panel.Children.Add(new TextBlock { Text = l.Plain, FontSize = big ? 15.4 : 14, TextWrapping = TextWrapping.Wrap, LineHeight = big ? 26 : 22 });
                break;
            case LyricsKind.Lines:
                foreach (var v in l.Lines) Add(v);
                Light();
                break;
        }
        Clocks();
    }

    private void Say(string text)
    {
        note.Text = text;
        note.Visibility = Visibility.Visible;
    }

    private void Add(LineView v)
    {
        var box = new StackPanel { Spacing = 2, Opacity = 0.34, OpacityTransition = new ScalarTransition { Duration = TimeSpan.FromMilliseconds(350) } };
        if (v.Head is { } head) box.Children.Add(new TextBlock { Text = head, FontSize = Size * 0.55, FontWeight = Microsoft.UI.Text.FontWeights.Bold, Opacity = 0.75 });
        var text = new TextBlock { FontSize = Size, FontWeight = big ? Microsoft.UI.Text.FontWeights.ExtraBold : Microsoft.UI.Text.FontWeights.SemiBold, TextWrapping = TextWrapping.Wrap, RenderTransformOrigin = new Windows.Foundation.Point(0, 0.5) };
        var lit = new Run { Text = "" };
        var rest = new Run { Text = v.Text };
        text.Inlines.Add(lit);
        text.Inlines.Add(rest);
        box.Children.Add(text);
        var start = v.Start;
        box.Tapped += (_, _) => hub.Engine.Seek(start); // a click on a line goes there
        var frame = new Grid();
        frame.Children.Add(box);
        panel.Children.Add(frame);
        lines.Add(new Shown { Box = box, Frame = frame, Text = text, Lit = lit, Rest = rest, Words = v.Text });
        if (view != null) Lens(lines[^1]);
    }

    /// <summary>A line's size from its distance to the middle, on the compositor (LensScale's formula).</summary>
    private void Lens(Shown line)
    {
        var s = ElementCompositionPreview.GetScrollViewerManipulationPropertySet(scroll);
        line.Props = s.Compositor.CreatePropertySet();
        line.Props.InsertScalar("Mid", 0);
        var e = s.Compositor.CreateExpressionAnimation(LensScale.Expression);
        e.SetReferenceParameter("s", s);
        e.SetReferenceParameter("v", view!);
        e.SetReferenceParameter("p", line.Props);
        ElementCompositionPreview.GetElementVisual(line.Frame).StartAnimation("Scale", e);
    }

    /// <summary>Each line's middle (in the column's content) for its lens, and its pivot (its left edge).</summary>
    private void PlaceLenses()
    {
        foreach (var line in lines)
        {
            if (line.Props == null && view != null) Lens(line);
            if (line.Props == null || line.Frame.ActualHeight <= 0) continue;
            var y = line.Frame.TransformToVisual(panel).TransformPoint(new Windows.Foundation.Point(0, 0)).Y;
            line.Props.InsertScalar("Mid", (float)(y + line.Frame.ActualHeight / 2));
            ElementCompositionPreview.GetElementVisual(line.Frame).CenterPoint = new Vector3(0, (float)(line.Frame.ActualHeight / 2), 0);
        }
    }

    /// <summary>The lines being sung now (counted on between the engine's readings) and their words.</summary>
    private void Light()
    {
        if (lines.Count == 0) return;
        var now = hub.Engine.Now();
        var at = LyricClock.At(now.At, now.Since, Ffi.ClockMs(), now.Playing);
        var sung = hub.Engine.Sung(at);
        var lit = new Dictionary<int, int>();
        foreach (var s in sung) lit[(int)s.Line] = (int)s.Chars;
        for (var i = 0; i < lines.Count; i++)
        {
            var line = lines[i];
            if (lit.TryGetValue(i, out var chars))
            {
                line.Box.Opacity = 1;
                var n = Math.Clamp(chars, 0, line.Words.Length);
                if (line.Lit.Text.Length != n)
                {
                    line.Lit.Text = line.Words[..n];
                    line.Rest.Text = line.Words[n..];
                    line.Rest.Foreground = Faint(line.Text);
                }
            }
            else
            {
                line.Box.Opacity = 0.34;
                if (line.Lit.Text.Length != 0 || line.Rest.Foreground != null)
                {
                    line.Lit.Text = "";
                    line.Rest.Text = line.Words;
                    line.Rest.ClearValue(TextElement.ForegroundProperty);
                }
            }
        }
        if (follower.Next(sung.Select(s => s.Line).ToArray()) is int next && next < lines.Count) Lead(next);
    }

    /// <summary>The unsung part of a lit line: its colour at 40 % (as the GTK app's fgalpha).</summary>
    private static Brush Faint(TextBlock t)
    {
        var c = (t.Foreground as SolidColorBrush)?.Color ?? Microsoft.UI.Colors.White;
        return new SolidColorBrush(Windows.UI.Color.FromArgb(0x66, c.R, c.G, c.B));
    }

    /// <summary>A new line leads: it grows (big: 2.5×, less when it wouldn't fit 60 % of the view, never under
    /// 1.5×; eased in over 400 ms), the last one shrinks back, and it's brought to the middle.</summary>
    private void Lead(int i)
    {
        if (big)
        {
            if (lead is int was && was < lines.Count) lines[was].Text.FontSize = Size;
            var t = lines[i].Text;
            var g = Grow;
            t.FontSize = Size * g;
            t.Measure(new Windows.Foundation.Size(Math.Max(1, scroll.ActualWidth), double.PositiveInfinity));
            var room = scroll.ActualHeight * Room;
            if (room > 0 && t.DesiredSize.Height > room)
            {
                g = Math.Max(Least, g * room / t.DesiredSize.Height);
                t.FontSize = Size * g;
            }
            Ease(t, 1 / g);
        }
        lead = i;
        misses = 0;
        DispatcherQueue.TryEnqueue(DispatcherQueuePriority.Low, () => ToMiddle(i));
    }

    private static void Ease(TextBlock t, double from)
    {
        var scale = new ScaleTransform { ScaleX = from, ScaleY = from };
        t.RenderTransform = scale;
        var ease = new CubicEase { EasingMode = EasingMode.EaseOut };
        var story = new Storyboard();
        foreach (var prop in new[] { "ScaleX", "ScaleY" })
        {
            var a = new DoubleAnimation { From = from, To = 1, Duration = TimeSpan.FromMilliseconds(400), EasingFunction = ease };
            Storyboard.SetTarget(a, scale);
            Storyboard.SetTargetProperty(a, prop);
            story.Children.Add(a);
        }
        story.Begin();
    }

    private void ToMiddle(int i)
    {
        if (i >= lines.Count || scroll.ActualHeight <= 0) return;
        var box = lines[i].Box;
        var y = box.TransformToVisual(panel).TransformPoint(new Windows.Foundation.Point(0, 0)).Y + box.ActualHeight / 2;
        scroll.ChangeView(null, Math.Max(0, y - scroll.ActualHeight / 2), null, false);
    }

    /// <summary>Once a second: the sung line out of sight twice (scrolled away, a resize): back to it.</summary>
    private void KeepLeadShown()
    {
        if (lead is not int i || i >= lines.Count || Visibility != Visibility.Visible) return;
        var box = lines[i].Box;
        var y = box.TransformToVisual(scroll).TransformPoint(new Windows.Foundation.Point(0, 0)).Y;
        if (y + box.ActualHeight < 0 || y > scroll.ActualHeight)
        {
            if (++misses >= 2)
            {
                misses = 0;
                ToMiddle(i);
            }
        }
        else
        {
            misses = 0;
        }
    }
}
