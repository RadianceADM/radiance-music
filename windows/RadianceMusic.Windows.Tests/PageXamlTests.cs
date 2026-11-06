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
}
