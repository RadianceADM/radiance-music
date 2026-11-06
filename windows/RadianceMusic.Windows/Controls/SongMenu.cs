using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using RadianceMusic.Native;
using Ffi = RadianceMusic.Native.Native;

namespace RadianceMusic.Controls;

/// <summary>
/// A song's menu (right-click, long-press, the Menu key / Shift+F10: WinUI's ContextRequested),
/// built from the engine's song_menu: the GTK app's items, sections and words; Add to Playlist as
/// a submenu; each item with its icon.
/// </summary>
internal static class SongMenu
{
    /// <summary>`play`: what Play does where the row is (the page knows its list).</summary>
    public static void Attach(FrameworkElement row, EngineHub hub, string key, uint index, MenuPlace place, Action? play)
    {
        Openers.AddOrUpdate(row, () => Build(hub, row, key, index, place, play).ShowAt(row));
        row.ContextRequested += (_, e) =>
        {
            var menu = Build(hub, row, key, index, place, play);
            if (e.TryGetPosition(row, out var at)) menu.ShowAt(row, new FlyoutShowOptions { Position = at });
            else menu.ShowAt(row);
            e.Handled = true;
        };
    }

    /// <summary>Each row's menu, as its right-click opens it (DevActs' "menu": the screenshots).</summary>
    private static readonly System.Runtime.CompilerServices.ConditionalWeakTable<FrameworkElement, Action> Openers = new();

    /// <summary>That row's menu, as its right-click opens it (DevActs: the screenshots).</summary>
    public static bool Open(FrameworkElement row)
    {
        if (!Openers.TryGetValue(row, out var open)) return false;
        open();
        return true;
    }

    /// <summary>The menu of the song row highest on screen under `root`.</summary>
    public static bool OpenFirst(UIElement root)
    {
        var rows = new List<(double Y, Action Open)>();
        void Walk(DependencyObject at)
        {
            if (at is FrameworkElement f && f.ActualHeight > 0 && Openers.TryGetValue(f, out var open))
                rows.Add((f.TransformToVisual(root).TransformPoint(new Windows.Foundation.Point(0, 0)).Y, open));
            for (var i = 0; i < Microsoft.UI.Xaml.Media.VisualTreeHelper.GetChildrenCount(at); i++) Walk(Microsoft.UI.Xaml.Media.VisualTreeHelper.GetChild(at, i));
        }
        Walk(root);
        if (rows.Count == 0) return false;
        rows.MinBy(r => r.Y).Open();
        return true;
    }

    private static MenuFlyout Build(EngineHub hub, FrameworkElement row, string key, uint index, MenuPlace place, Action? play)
    {
        // (a hidden screenshot run: the menu drawn inside the window, so its PrintWindow picture has it)
        var menu = new MenuFlyout { ShouldConstrainToRootBounds = Program.Dev.Hidden };
        foreach (var section in hub.Engine.SongMenu(key, place))
        {
            if (menu.Items.Count > 0) menu.Items.Add(new MenuFlyoutSeparator());
            MenuFlyoutSubItem? lists = null;
            foreach (var item in section.Items)
            {
                var action = item.Action;
                var mi = new MenuFlyoutItem { Text = item.Label, Icon = new FontIcon { Glyph = Glyph(action) } };
                mi.Click += (_, _) => Do(hub, row, key, index, action, play);
                if (action is MenuAction.AddToPlaylist or MenuAction.NewPlaylistWith)
                {
                    if (lists == null)
                    {
                        lists = new MenuFlyoutSubItem { Text = Ffi.AddToPlaylistLabel(), Icon = new FontIcon { Glyph = "" } };
                        menu.Items.Add(lists);
                    }
                    lists.Items.Add(mi);
                }
                else
                {
                    menu.Items.Add(mi);
                }
            }
        }
        return menu;
    }

    private static void Do(EngineHub hub, FrameworkElement row, string key, uint index, MenuAction action, Action? play)
    {
        switch (action)
        {
            case MenuAction.Play:
                play?.Invoke();
                break;
            case MenuAction.NewPlaylistWith:
                _ = Dialogs.NameDialog.NewPlaylist(hub, row.XamlRoot, key);
                break;
            case MenuAction.ShowLyrics:
                _ = Dialogs.LyricsDialogs.Show(hub, row.XamlRoot, key);
                break;
            case MenuAction.EditLyrics:
                _ = Dialogs.LyricsDialogs.Edit(hub, row.XamlRoot, key);
                break;
            case MenuAction.GoToArtist g:
                App.Window?.SearchFor(g.Q);
                break;
            case MenuAction.GoToAlbum g:
                App.Window?.SearchFor(g.Q);
                break;
            default:
                hub.Engine.DoMenu(key, index, action);
                break;
        }
    }

    /// <summary>Each item's icon (Segoe Fluent Icons).</summary>
    private static string Glyph(MenuAction a) => a switch
    {
        MenuAction.Play => "",
        MenuAction.PlayNext => "",
        MenuAction.AddToQueue => "",
        MenuAction.RemoveFromQueue or MenuAction.RemoveFromPlaylist => "",
        MenuAction.AddToPlaylist => "",
        MenuAction.NewPlaylistWith => "",
        MenuAction.Like => "",
        MenuAction.Unlike => "",
        MenuAction.Radio => "",
        MenuAction.GoToArtist => "",
        MenuAction.GoToAlbum => "",
        MenuAction.ShowInFiles => "",
        MenuAction.SplitTwo or MenuAction.SplitFour => "\uE8C6",
        MenuAction.ShowLyrics => "\uE8D2",
        MenuAction.EditLyrics => "\uE70F",
        MenuAction.FindLyricsAgain => "\uE72C",
        _ => "",
    };
}
