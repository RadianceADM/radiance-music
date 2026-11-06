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
        row.ContextRequested += (_, e) =>
        {
            var menu = Build(hub, row, key, index, place, play);
            if (e.TryGetPosition(row, out var at)) menu.ShowAt(row, new FlyoutShowOptions { Position = at });
            else menu.ShowAt(row);
            e.Handled = true;
        };
    }

    private static MenuFlyout Build(EngineHub hub, FrameworkElement row, string key, uint index, MenuPlace place, Action? play)
    {
        var menu = new MenuFlyout();
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
        _ => "",
    };
}
