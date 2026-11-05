using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using RadianceMusic.Native;

namespace RadianceMusic.Dialogs;

/// <summary>
/// A playlist's name, asked in a dialog as the GTK app's name_dialog: "New Playlist" / "Create" with
/// "My Playlist N" filled in, or "Rename Playlist" / "Rename" with its name; placeholder "Playlist name".
/// </summary>
internal static class NameDialog
{
    /// <summary>The name typed (trimmed), or null when cancelled or left empty.</summary>
    public static async Task<string?> Ask(XamlRoot root, string title, string button, string start)
    {
        var box = new TextBox { Text = start, PlaceholderText = "Playlist name", SelectionStart = 0, SelectionLength = start.Length };
        var dialog = new ContentDialog
        {
            XamlRoot = root,
            Title = title,
            Content = box,
            PrimaryButtonText = button,
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Primary,
        };
        var entered = false; // Enter in the box: as the button
        box.KeyDown += (_, e) =>
        {
            if (e.Key != Windows.System.VirtualKey.Enter) return;
            entered = true;
            dialog.Hide();
        };
        var result = await dialog.ShowAsync();
        var name = box.Text.Trim();
        return (result == ContentDialogResult.Primary || entered) && name.Length > 0 ? name : null;
    }

    /// <summary>A new playlist (with the song `first` in it, from a song's menu); its id.</summary>
    public static async Task<long?> NewPlaylist(EngineHub hub, XamlRoot root, string? first)
    {
        var name = await Ask(root, "New Playlist", "Create", hub.Engine.NextPlaylistName());
        return name == null ? null : hub.Engine.CreatePlaylist(name, first);
    }
}
