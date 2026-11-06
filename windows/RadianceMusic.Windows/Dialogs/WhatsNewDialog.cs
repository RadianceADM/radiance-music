using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Ffi = RadianceMusic.Native.Native;

namespace RadianceMusic.Dialogs;

/// <summary>What's New: the beta's own CHANGELOG sections (the newest three, as built), each heading with its
/// points. Soft corners.</summary>
internal static class WhatsNewDialog
{
    public static async Task Show(XamlRoot root)
    {
        var w = Ffi.HelpWords();
        var list = new StackPanel { Spacing = 14 };
        foreach (var s in Ffi.WhatsNew())
        {
            list.Children.Add(new TextBlock { Text = s.Heading, Style = (Style)Application.Current.Resources["BodyStrongTextBlockStyle"], TextWrapping = TextWrapping.Wrap });
            var points = new StackPanel { Spacing = 6 };
            foreach (var item in s.Items) points.Children.Add(new TextBlock { Text = "•  " + item, TextWrapping = TextWrapping.Wrap });
            list.Children.Add(points);
        }
        var dialog = new ContentDialog
        {
            XamlRoot = root,
            Title = w.WhatsNew,
            Content = new ScrollViewer { Content = list, MaxHeight = 480, Padding = new Thickness(0, 0, 12, 0) },
            CloseButtonText = w.Close,
            CornerRadius = new CornerRadius(16),
        };
        dialog.Resources["ContentDialogMaxWidth"] = 720.0;
        await dialog.ShowAsync();
    }
}
