using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using RadianceMusic.Native;

namespace RadianceMusic.Controls;

/// <summary>
/// Liked Songs' A to Z rail, as the GTK app's: click a letter, or drag along it, to jump there
/// (tooltip "Jump to a letter"); letters with no song are faint; the letter of the songs at the
/// top of the list is lit (the accent colour).
/// </summary>
internal sealed class AzRail : StackPanel
{
    private readonly List<TextBlock> letters = new();

    /// <summary>A letter picked (its place on the rail, 0 = A … 26 = #).</summary>
    public event Action<int>? Jump;

    public AzRail()
    {
        VerticalAlignment = VerticalAlignment.Center;
        Padding = new Thickness(2, 6, 2, 6);
        CornerRadius = new CornerRadius(10);
        Background = (Brush)Application.Current.Resources["LayerFillColorDefaultBrush"];
        ToolTipService.SetToolTip(this, "Jump to a letter");
        foreach (var c in "ABCDEFGHIJKLMNOPQRSTUVWXYZ#")
        {
            var t = new TextBlock { Text = c.ToString(), FontSize = 11, Width = 20, TextAlignment = TextAlignment.Center, Padding = new Thickness(0, 1, 0, 1) };
            letters.Add(t);
            Children.Add(t);
        }
        PointerPressed += (_, e) =>
        {
            CapturePointer(e.Pointer);
            Pick(e);
        };
        PointerMoved += (_, e) =>
        {
            if (e.Pointer.IsInContact) Pick(e);
        };
        PointerReleased += (_, _) => ReleasePointerCaptures();
    }

    private void Pick(PointerRoutedEventArgs e)
    {
        if (ActualHeight <= 0) return;
        var y = e.GetCurrentPoint(this).Position.Y - Padding.Top;
        var i = (int)(y / ((ActualHeight - Padding.Top - Padding.Bottom) / letters.Count));
        Jump?.Invoke(Math.Clamp(i, 0, letters.Count - 1));
    }

    /// <summary>Which letters have songs (from the engine's Liked page).</summary>
    public void SetLetters(LetterView[] v)
    {
        for (var i = 0; i < letters.Count && i < v.Length; i++) letters[i].Opacity = v[i].First == null ? 0.35 : 1;
    }

    /// <summary>The letter lit (null: none).</summary>
    public void Light(int? lit)
    {
        var accent = (Brush)Application.Current.Resources["AccentTextFillColorPrimaryBrush"];
        var plain = (Brush)Application.Current.Resources["TextFillColorPrimaryBrush"];
        for (var i = 0; i < letters.Count; i++)
        {
            letters[i].Foreground = i == lit ? accent : plain;
            letters[i].FontWeight = i == lit ? Microsoft.UI.Text.FontWeights.Bold : Microsoft.UI.Text.FontWeights.Normal;
        }
    }
}
