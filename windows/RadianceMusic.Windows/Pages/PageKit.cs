using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;

namespace RadianceMusic.Pages;

/// <summary>What a library page is shown with: the hub, and which album / artist / genre / playlist.</summary>
internal sealed record PageArgs(EngineHub Hub, object? What);

/// <summary>The library pages' parts, in Windows' own look (the type ramp, theme brushes).</summary>
internal static class PageKit
{
    public static TextBlock Title(string text) => new() { Text = text, Style = (Style)Application.Current.Resources["TitleLargeTextBlockStyle"], TextTrimming = TextTrimming.CharacterEllipsis };

    public static TextBlock Subtitle(string text) => new() { Text = text, Style = (Style)Application.Current.Resources["SubtitleTextBlockStyle"] };

    public static TextBlock Note(string text = "") => new()
    {
        Text = text,
        Style = (Style)Application.Current.Resources["BodyTextBlockStyle"],
        Foreground = (Brush)Application.Current.Resources["TextFillColorSecondaryBrush"],
        TextWrapping = TextWrapping.Wrap,
    };

    /// <summary>A button with its leading icon (every button has one).</summary>
    public static Button IconButton(string glyph, string text, Action click, bool accent = false)
    {
        var content = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        content.Children.Add(new FontIcon { Glyph = glyph, FontSize = 14 });
        var label = new TextBlock { Text = text };
        content.Children.Add(label);
        var b = new Button { Content = content, Padding = new Thickness(14, 6, 14, 6) };
        if (accent) b.Style = (Style)Application.Current.Resources["AccentButtonStyle"];
        b.Click += (_, _) => click();
        return b;
    }

    /// <summary>The words of an IconButton (Fix missing's count changes).</summary>
    public static void SetText(Button b, string text)
    {
        if (b.Content is StackPanel { Children.Count: 2 } p && p.Children[1] is TextBlock t) t.Text = text;
    }

    /// <summary>A list of songs as the stage-1 pages have them (Windows' own hover, focus, click).</summary>
    public static ListView SongList() => new() { SelectionMode = ListViewSelectionMode.None, IsItemClickEnabled = true, Margin = new Thickness(-16, 4, -16, 0) };

    /// <summary>The ScrollViewer inside a ListView (once it's loaded).</summary>
    public static ScrollViewer? ScrollerOf(DependencyObject root)
    {
        if (root is ScrollViewer s) return s;
        for (var i = 0; i < VisualTreeHelper.GetChildrenCount(root); i++)
            if (ScrollerOf(VisualTreeHelper.GetChild(root, i)) is { } found) return found;
        return null;
    }
}
