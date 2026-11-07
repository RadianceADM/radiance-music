using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Ffi = RadianceMusic.Native.Native;

namespace RadianceMusic.Dialogs;

/// <summary>
/// How to Use It, as the Linux app's guide (src/ui/guide.rs, core's cards): cards to page through (a FlipView
/// with pips), Skip, Next, and Start Listening on the last; closing it any way counts as seen (the shared
/// settings' guide_seen). All five cards since stage 4b: the fifth says Share Songs with a Phone is in Settings → Help
/// (the Linux app's says "main menu"). The first start shows it once the sunrise has gone.
/// </summary>
internal static class GuideDialog
{
    /// <summary>The guide is up (the other pop-ups wait for it).</summary>
    public static bool Up { get; private set; }

    private static readonly string[] Glyphs = { "", "", "", "", "" };

    public static async Task Show(EngineHub hub, XamlRoot root)
    {
        if (Up) return;
        var w = Ffi.HelpWords();
        var cards = Ffi.GuideCards().ToArray(); // all five: the fifth says where the beta keeps Share Songs with a Phone (stage 4b)
        var flip = new FlipView { Height = 300, Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent) };
        for (var i = 0; i < cards.Length; i++)
        {
            var c = cards[i];
            var page = new StackPanel { Spacing = 14, VerticalAlignment = VerticalAlignment.Center, HorizontalAlignment = HorizontalAlignment.Center, MaxWidth = 440, Padding = new Thickness(24) };
            page.Children.Add(new Border
            {
                Width = 72,
                Height = 72,
                CornerRadius = new CornerRadius(36),
                Background = (Brush)Application.Current.Resources["AccentFillColorDefaultBrush"],
                Child = new FontIcon { Glyph = Glyphs[i], FontSize = 32, Foreground = (Brush)Application.Current.Resources["TextOnAccentFillColorPrimaryBrush"] },
                HorizontalAlignment = HorizontalAlignment.Center,
            });
            page.Children.Add(new TextBlock { Text = c.Title, Style = (Style)Application.Current.Resources["SubtitleTextBlockStyle"], HorizontalAlignment = HorizontalAlignment.Center, TextAlignment = TextAlignment.Center });
            page.Children.Add(new TextBlock { Text = c.Text, TextWrapping = TextWrapping.Wrap, TextAlignment = TextAlignment.Center });
            flip.Items.Add(page);
        }
        var pips = new PipsPager { NumberOfPages = cards.Length, HorizontalAlignment = HorizontalAlignment.Center };
        var body = new StackPanel { Spacing = 8, Width = 480 };
        body.Children.Add(flip);
        body.Children.Add(pips);
        var dialog = new ContentDialog
        {
            XamlRoot = root,
            Title = w.GuideTitle,
            Content = body,
            PrimaryButtonText = w.GuideNext,
            CloseButtonText = w.GuideSkip,
            DefaultButton = ContentDialogButton.Primary,
            CornerRadius = new CornerRadius(16),
        };
        void Sync()
        {
            pips.SelectedPageIndex = flip.SelectedIndex;
            dialog.PrimaryButtonText = flip.SelectedIndex >= cards.Length - 1 ? w.GuideStart : w.GuideNext;
        }
        flip.SelectionChanged += (_, _) => Sync();
        pips.SelectedIndexChanged += (_, _) => flip.SelectedIndex = pips.SelectedPageIndex;
        dialog.PrimaryButtonClick += (_, e) =>
        {
            if (flip.SelectedIndex < cards.Length - 1)
            {
                e.Cancel = true; // Next: the next card
                flip.SelectedIndex++;
            }
        };
        Sync();
        Up = true;
        try
        {
            await dialog.ShowAsync();
        }
        finally
        {
            Up = false;
            hub.Engine.SetGuideSeen(); // seen, whichever way it closed
        }
    }
}
