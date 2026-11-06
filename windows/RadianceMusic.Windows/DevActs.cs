using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Automation.Provider;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;

namespace RadianceMusic;

/// <summary>
/// --act STEP (developer screenshots on ribs03, through --shot): what a person would do before
/// the shot, done by the app itself, so no clicks or keys go to the owner's desktop:
///   like:TITLE    likes Home's recent song of that title (if it isn't already)
///   menu          opens the menu of the song row highest on screen
///   click:TEXT    presses the button or menu item whose words start with TEXT (a menu's or dialog's first)
///   group:KIND:NAME   an album's, artist's or genre's songs (KIND: albums, artists, genres)
/// </summary>
internal static class DevActs
{
    public static void Run(MainWindow window, string act)
    {
        var colon = act.IndexOf(':');
        var (what, arg) = colon >= 0 ? (act[..colon], act[(colon + 1)..]) : (act, "");
        var done = what switch
        {
            "like" => Like(arg),
            "menu" => window.Content is { } root && Controls.SongMenu.OpenFirst(root),
            "click" => window.Content?.XamlRoot is { } xr && Click(xr, window.Content, arg),
            "group" => Group(window, arg),
            _ => false,
        };
        Crash.Note($"act: {act}: {(done ? "done" : "not found")}");
    }

    /// <summary>group:albums|artists|genres:NAME: that album's, artist's or genre's songs.</summary>
    private static bool Group(MainWindow window, string arg)
    {
        var parts = arg.Split(':', 2);
        if (parts.Length != 2) return false;
        var kind = parts[0] switch { "albums" => Native.GroupKind.Albums, "artists" => Native.GroupKind.Artists, _ => Native.GroupKind.Genres };
        window.Show("group", (kind, parts[1]));
        return true;
    }

    private static bool Like(string title)
    {
        if (App.Hub.Engine.Home().Recent.FirstOrDefault(s => s.Title == title) is not { } song) return false;
        if (!song.Liked) App.Hub.Engine.ToggleLike(song.Key);
        return true;
    }

    /// <summary>The open pop-ups first (a menu or dialog is on top), then the window.</summary>
    private static bool Click(XamlRoot xr, UIElement content, string text)
    {
        var roots = VisualTreeHelper.GetOpenPopupsForXamlRoot(xr).Select(p => (DependencyObject)p.Child).Where(c => c != null).ToList();
        roots.Add(content);
        foreach (var root in roots)
            if (Find(root, text) is { } element && FrameworkElementAutomationPeer.CreatePeerForElement(element) is { } peer)
            {
                // a submenu opens (Expand); a button or menu item is pressed (Invoke)
                if (peer.GetPattern(PatternInterface.ExpandCollapse) is IExpandCollapseProvider e) e.Expand();
                else if (peer.GetPattern(PatternInterface.Invoke) is IInvokeProvider i) i.Invoke();
                else return false;
                return true;
            }
        return false;
    }

    private static UIElement? Find(DependencyObject at, string text)
    {
        switch (at)
        {
            case MenuFlyoutSubItem sub when sub.Text.StartsWith(text): return sub;
            case MenuFlyoutItem item when item.Text.StartsWith(text): return item;
            case Button b when Words(b).StartsWith(text): return b;
        }
        for (var i = 0; i < VisualTreeHelper.GetChildrenCount(at); i++)
            if (Find(VisualTreeHelper.GetChild(at, i), text) is { } found) return found;
        return null;
    }

    /// <summary>A button's words (its content's text blocks, or its string).</summary>
    private static string Words(Button b) => b.Content switch
    {
        string s => s,
        TextBlock t => t.Text,
        DependencyObject d => string.Join(" ", Texts(d)),
        _ => "",
    };

    private static IEnumerable<string> Texts(DependencyObject d)
    {
        if (d is TextBlock t && t.Text.Length > 0) yield return t.Text;
        if (d is Panel p)
            foreach (var c in p.Children)
                foreach (var s in Texts(c)) yield return s;
    }
}
