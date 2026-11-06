using System.Runtime.CompilerServices;
using System.Text.RegularExpressions;
using Xunit;

namespace RadianceMusic.Tests;

/// <summary>
/// The Frame makes a page through the XAML compiler's type info, which knows only pages with a
/// .xaml file: a page made in C# alone crashes the app when it's opened (coreclr, 0xc0000005).
/// </summary>
public class PageXamlTests
{
    private static string Pages([CallerFilePath] string here = "") =>
        Path.Combine(Path.GetDirectoryName(here)!, "..", "RadianceMusic.Windows", "Pages");

    [Fact]
    public void EveryPageHasItsXaml()
    {
        var pages = Directory.GetFiles(Pages(), "*.cs")
            .SelectMany(f => Regex.Matches(File.ReadAllText(f), @"class\s+(\w+)\s*:\s*Page\b").Select(m => m.Groups[1].Value))
            .ToList();
        Assert.NotEmpty(pages);
        var known = Directory.GetFiles(Pages(), "*.xaml")
            .Select(f => Regex.Match(File.ReadAllText(f), "x:Class=\"RadianceMusic\\.Pages\\.(\\w+)\"").Groups[1].Value)
            .ToHashSet();
        Assert.Equal(Array.Empty<string>(), pages.Where(p => !known.Contains(p)).ToArray());
    }

    /// <summary>The lyrics editor has no default button: Enter in a line's box would save the half-edited
    /// lyrics as yours and close it (final review I6); the Linux editor saves only on Save.</summary>
    [Fact]
    public void TheLyricsEditorNeverSavesOnEnter()
    {
        var editor = File.ReadAllText(Path.Combine(Pages(), "..", "Dialogs", "LyricsDialogs.cs"));
        Assert.DoesNotContain("DefaultButton = ContentDialogButton.Primary", editor);
    }

    /// <summary>A page that may scroll sideways sizes to its longest row and drifts off-centre or off the edge in a
    /// wide window (#58 Search, #60 History, Albums, Artists, Genres): every page's ScrollViewer has it off
    /// (PageKit.Column does, as the Linux app's adw::Clamp).</summary>
    [Fact]
    public void NoPageScrollsSideways()
    {
        var loose = new List<string>();
        foreach (var f in Directory.GetFiles(Pages(), "*.cs"))
            foreach (Match m in Regex.Matches(File.ReadAllText(f), @"new ScrollViewer\s*\{[^}]*\}"))
                if (!m.Value.Contains("HorizontalScrollMode")) loose.Add($"{Path.GetFileName(f)}: {m.Value}");
        foreach (var f in Directory.GetFiles(Pages(), "*.xaml"))
            foreach (Match m in Regex.Matches(File.ReadAllText(f), @"<ScrollViewer\b[^>]*>"))
                if (!m.Value.Contains("HorizontalScrollMode=\"Disabled\"")) loose.Add($"{Path.GetFileName(f)}: {m.Value}");
        Assert.Equal(Array.Empty<string>(), loose.ToArray());
    }
}
