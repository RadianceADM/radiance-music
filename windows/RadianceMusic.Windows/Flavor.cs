using System.Reflection;
using RadianceMusic.Native;

namespace RadianceMusic;

/// <summary>
/// Which build this is (stage 5): the beta (RadianceMusicBeta.exe, its own folder, beta updates) or the native app
/// (`-p:Flavor=native`: RadianceMusic.exe, "Radiance Music", today's folder after the switch, the main release line's
/// updates). One source; the differences are only these names and how the engine is started.
/// </summary>
internal static class Flavor
{
#if NATIVE
    public static readonly bool IsNative = true;
    public const string Name = "Radiance Music";
    public const string CrashLog = "windows-crash.log";
#else
    public static readonly bool IsNative = false;
    public const string Name = "Radiance Music (beta)";
    public const string CrashLog = "beta-crash.log";
#endif

    /// <summary>This build's version (the native app's is the release's, given at its build).</summary>
    public static string Version =>
        (typeof(Flavor).Assembly.GetCustomAttribute<AssemblyInformationalVersionAttribute>()?.InformationalVersion ?? "").Split('+')[0];

    /// <summary>The engine, started as this build.</summary>
    public static MusicEngine Start(EngineListener listener) => IsNative ? MusicEngine.StartNative(listener, Version) : MusicEngine.Start(listener);
}
