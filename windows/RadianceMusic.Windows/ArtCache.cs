using Microsoft.UI.Xaml.Media.Imaging;
using RadianceMusic.Native;

namespace RadianceMusic;

/// <summary>Covers for the screens: the engine makes each a PNG once (off the UI thread), and the
/// last 200 stay decoded at their size.</summary>
public static class ArtCache
{
    private const int Most = 200;
    private static readonly Dictionary<string, BitmapImage> images = new();
    private static readonly LinkedList<string> order = new();

    /// <summary>On the UI thread; null: no cover.</summary>
    public static async Task<BitmapImage?> LoadAsync(MusicEngine engine, string source, uint size)
    {
        if (string.IsNullOrEmpty(source)) return null;
        var key = size + "|" + source;
        if (images.TryGetValue(key, out var hit))
        {
            order.Remove(key);
            order.AddFirst(key);
            return hit;
        }
        var path = await Task.Run(() => engine.ArtFile(source, size));
        if (path == null) return null;
        if (images.TryGetValue(key, out hit)) return hit;
        var image = new BitmapImage { DecodePixelWidth = (int)size, UriSource = new Uri(path) };
        images[key] = image;
        order.AddFirst(key);
        while (order.Count > Most)
        {
            images.Remove(order.Last!.Value);
            order.RemoveLast();
        }
        return image;
    }
}
