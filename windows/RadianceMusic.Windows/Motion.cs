using System.Numerics;
using Microsoft.UI.Composition;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Windows.UI.ViewManagement;

namespace RadianceMusic;

/// <summary>
/// How things move, as the Radiance animation blueprint (RadianceADM/blueprints ANIMATION_BLUEPRINT.md §2, §6.9;
/// radiance-music#64): one place for the curves and durations. Motion only, never how things look at rest. Every
/// helper checks Windows' "Animation effects" (reduced motion: the change happens at once).
/// </summary>
internal static class Motion
{
    public static readonly TimeSpan Press = TimeSpan.FromMilliseconds(100);
    public static readonly TimeSpan Release = TimeSpan.FromMilliseconds(160);
    public static readonly TimeSpan Micro = TimeSpan.FromMilliseconds(150);
    public static readonly TimeSpan MicroOut = TimeSpan.FromMilliseconds(120);
    public static readonly TimeSpan MenuIn = TimeSpan.FromMilliseconds(200);
    public static readonly TimeSpan MenuOut = TimeSpan.FromMilliseconds(160);
    public static readonly TimeSpan Page = TimeSpan.FromMilliseconds(220);
    public static readonly TimeSpan PageOut = TimeSpan.FromMilliseconds(176);
    public static readonly TimeSpan Shared = TimeSpan.FromMilliseconds(400);
    public static readonly TimeSpan Pulse = TimeSpan.FromMilliseconds(300);

    private static readonly UISettings Ui = new();

    /// <summary>False when Windows' "Animation effects" is off: implicit transitions and timers check it.</summary>
    public static bool Enabled => Ui.AnimationsEnabled;

    public static CubicBezierEasingFunction EaseOut(Compositor c) => c.CreateCubicBezierEasingFunction(new Vector2(0.22f, 1f), new Vector2(0.36f, 1f));
    public static CubicBezierEasingFunction EaseInOut(Compositor c) => c.CreateCubicBezierEasingFunction(new Vector2(0.65f, 0f), new Vector2(0.35f, 1f));
    public static CubicBezierEasingFunction EaseExit(Compositor c) => c.CreateCubicBezierEasingFunction(new Vector2(0.4f, 0f), new Vector2(1f, 1f));

    /// <summary>Rule 4: down to 0.97 while pressed (100 ms), back in 160 ms. handledEventsToo: Button marks
    /// pointer events handled.</summary>
    public static void AddPressScale(FrameworkElement el)
    {
        el.ScaleTransition = new Vector3Transition { Duration = Release };
        el.SizeChanged += (_, _) => el.CenterPoint = new Vector3((float)el.ActualWidth / 2, (float)el.ActualHeight / 2, 0);
        void Down(object s, PointerRoutedEventArgs e)
        {
            if (!Enabled) return;
            el.ScaleTransition.Duration = Press;
            el.Scale = new Vector3(0.97f, 0.97f, 1f);
        }
        void Up(object s, PointerRoutedEventArgs e)
        {
            el.ScaleTransition.Duration = Release;
            el.Scale = Vector3.One;
        }
        el.AddHandler(UIElement.PointerPressedEvent, new PointerEventHandler(Down), true);
        el.AddHandler(UIElement.PointerReleasedEvent, new PointerEventHandler(Up), true);
        el.AddHandler(UIElement.PointerCanceledEvent, new PointerEventHandler(Up), true);
        el.AddHandler(UIElement.PointerCaptureLostEvent, new PointerEventHandler(Up), true);
        el.AddHandler(UIElement.PointerExitedEvent, new PointerEventHandler(Up), true);
    }

    /// <summary>Rules 1 and 15: a small thing (the Offline sign) fades in over 150 ms with an ease-out, and out a
    /// little faster (120 ms); it takes no room once it's gone.</summary>
    public static void Fade(UIElement el, bool show)
    {
        if (!Enabled)
        {
            el.OpacityTransition = null;
            el.Opacity = show ? 1 : 0;
            el.Visibility = show ? Visibility.Visible : Visibility.Collapsed;
            return;
        }
        if (show)
        {
            if (el.Visibility == Visibility.Visible && el.Opacity == 1) return;
            el.OpacityTransition = new ScalarTransition { Duration = Micro };
            if (el.Visibility != Visibility.Visible)
            {
                el.Opacity = 0;
                el.Visibility = Visibility.Visible;
            }
            el.DispatcherQueue.TryEnqueue(DispatcherQueuePriority.Low, () => el.Opacity = 1);
        }
        else
        {
            if (el.Visibility == Visibility.Collapsed) return;
            el.OpacityTransition = new ScalarTransition { Duration = MicroOut };
            el.Opacity = 0;
            After(el.DispatcherQueue, MicroOut, () =>
            {
                if (el.Opacity == 0) el.Visibility = Visibility.Collapsed;
            });
        }
    }

    /// <summary>Rule 16: <paramref name="next"/> takes the place of what <paramref name="host"/> shows, the two
    /// crossfading over each other (220 ms in, 176 ms out): never a blank moment in between.</summary>
    public static void Crossfade(Panel host, UIElement next)
    {
        var old = host.Children.Where(c => c != next).ToList();
        if (!Enabled)
        {
            foreach (var o in old) host.Children.Remove(o);
            if (!host.Children.Contains(next)) host.Children.Add(next);
            next.Opacity = 1;
            return;
        }
        next.Opacity = 0;
        next.OpacityTransition = new ScalarTransition { Duration = Page };
        if (!host.Children.Contains(next)) host.Children.Add(next);
        foreach (var o in old)
        {
            o.IsHitTestVisible = false;
            o.OpacityTransition = new ScalarTransition { Duration = PageOut };
        }
        host.DispatcherQueue.TryEnqueue(DispatcherQueuePriority.Low, () =>
        {
            next.Opacity = 1;
            foreach (var o in old) o.Opacity = 0;
        });
        After(host.DispatcherQueue, PageOut, () =>
        {
            foreach (var o in old) host.Children.Remove(o);
        });
    }

    /// <summary>Timers kept until they fire (a timer nothing holds is collected and never fires).</summary>
    private static readonly HashSet<DispatcherQueueTimer> waiting = new();

    private static void After(DispatcherQueue queue, TimeSpan wait, Action then)
    {
        var t = queue.CreateTimer();
        t.Interval = wait;
        t.IsRepeating = false;
        waiting.Add(t);
        t.Tick += (_, _) =>
        {
            waiting.Remove(t);
            then();
        };
        t.Start();
    }
}
