namespace RadianceMusic;

/// <summary>What went wrong, written to radiance-music\beta-crash.log in the app's cache folder
/// (XDG_CACHE_HOME when set, as the engine's), for the developer reading it on ribs03.</summary>
internal static class Crash
{
    public static void Note(object? what)
    {
        try
        {
            var cache = Environment.GetEnvironmentVariable("XDG_CACHE_HOME") ?? Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
            var dir = Path.Combine(cache, "radiance-music");
            Directory.CreateDirectory(dir);
            File.AppendAllText(Path.Combine(dir, Flavor.CrashLog), $"{DateTime.Now:yyyy-MM-dd HH:mm:ss} {what}\n\n");
        }
        catch
        {
            // nothing more to do
        }
    }
}
