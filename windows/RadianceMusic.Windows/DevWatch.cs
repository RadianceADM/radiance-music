using System.Diagnostics;
using System.Runtime.InteropServices;

namespace RadianceMusic;

/// <summary>
/// Developer runs only (screenshots, measures): every 5 s, if the app used more than a fifth of a core, its
/// busiest threads in the crash note, by name (Rust's named threads, .NET's pool, GStreamer's, the UI thread),
/// so a run that spins says where (2026-11-07: Home spun at 40-90 % now and then on ribs03).
/// </summary>
internal static partial class DevWatch
{
    private static Thread? watching;

    public static void Start(uint uiThread)
    {
        if (watching != null) return;
        watching = new Thread(() => Watch(uiThread)) { IsBackground = true, Name = "dev watch", Priority = ThreadPriority.BelowNormal };
        watching.Start();
    }

    private static void Watch(uint uiThread)
    {
        var me = Process.GetCurrentProcess();
        var last = new Dictionary<int, double>();
        var lastTotal = me.TotalProcessorTime.TotalMilliseconds;
        while (true)
        {
            Thread.Sleep(5000);
            me.Refresh();
            var total = me.TotalProcessorTime.TotalMilliseconds;
            var busy = (total - lastTotal) / 5000;
            lastTotal = total;
            var now = new Dictionary<int, double>();
            var used = new List<(int Id, double Ms)>();
            foreach (ProcessThread t in me.Threads)
            {
                try
                {
                    var ms = t.TotalProcessorTime.TotalMilliseconds;
                    now[t.Id] = ms;
                    if (last.TryGetValue(t.Id, out var was)) used.Add((t.Id, ms - was));
                }
                catch (Exception)
                {
                    // (a thread that just ended)
                }
            }
            last = now;
            if (busy < 0.2) continue;
            var top = used.OrderByDescending(u => u.Ms).Take(4).Select(u => $"{Name(u.Id, uiThread)} {u.Ms / 50:0} %");
            App.Trace($"busy: {busy * 100:0} % of a core in 5 s; the threads: {string.Join(", ", top)}");
        }
    }

    private static string Name(int id, uint uiThread)
    {
        if (id == uiThread) return $"{id} (the UI thread)";
        var h = OpenThread(0x0800 /* THREAD_QUERY_LIMITED_INFORMATION */, false, (uint)id);
        if (h == 0) return $"{id}";
        try
        {
            if (GetThreadDescription(h, out var text) < 0 || text == 0) return $"{id}";
            var name = Marshal.PtrToStringUni(text);
            LocalFree(text);
            return string.IsNullOrEmpty(name) ? $"{id}" : $"{id} ({name})";
        }
        finally
        {
            CloseHandle(h);
        }
    }

    [LibraryImport("kernel32.dll")]
    private static partial nint OpenThread(uint access, [MarshalAs(UnmanagedType.Bool)] bool inherit, uint id);

    [LibraryImport("kernel32.dll")]
    private static partial int GetThreadDescription(nint thread, out nint description);

    [LibraryImport("kernel32.dll")]
    private static partial nint LocalFree(nint mem);

    [LibraryImport("kernel32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool CloseHandle(nint handle);
}
