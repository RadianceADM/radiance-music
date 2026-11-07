using System.Runtime.InteropServices;

namespace RadianceMusic;

/// <summary>The few Win32 calls the app needs.</summary>
internal static partial class Win32
{
    [LibraryImport("user32.dll", EntryPoint = "MessageBoxW", StringMarshalling = StringMarshalling.Utf16)]
    private static partial int MessageBox(nint hwnd, string text, string caption, uint type);

    public static void Message(string text, string caption) => MessageBox(0, text, caption, 0x40 /* MB_ICONINFORMATION */);

    [StructLayout(LayoutKind.Sequential)]
    public struct Rect { public int Left, Top, Right, Bottom; }

    /// <summary>This thread's id as Windows numbers it (developer runs: which thread is busy).</summary>
    [LibraryImport("kernel32.dll")]
    public static partial uint GetCurrentThreadId();

    [LibraryImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static partial bool GetWindowRect(nint hwnd, out Rect rect);

    [LibraryImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static partial bool SetForegroundWindow(nint hwnd);

    [LibraryImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static partial bool IsIconic(nint hwnd);

    [LibraryImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static partial bool ShowWindow(nint hwnd, int cmd);

    [LibraryImport("user32.dll")]
    public static partial nint GetDC(nint hwnd);

    [LibraryImport("user32.dll")]
    public static partial int ReleaseDC(nint hwnd, nint dc);

    [LibraryImport("gdi32.dll")]
    public static partial nint CreateCompatibleDC(nint dc);

    [LibraryImport("gdi32.dll")]
    public static partial nint CreateCompatibleBitmap(nint dc, int w, int h);

    [LibraryImport("gdi32.dll")]
    public static partial nint SelectObject(nint dc, nint obj);

    [LibraryImport("gdi32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static partial bool BitBlt(nint dst, int x, int y, int w, int h, nint src, int sx, int sy, uint rop);

    [LibraryImport("gdi32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static partial bool DeleteObject(nint obj);

    [LibraryImport("gdi32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static partial bool DeleteDC(nint dc);

    [StructLayout(LayoutKind.Sequential)]
    public struct BitmapInfoHeader
    {
        public int Size, Width, Height;
        public short Planes, BitCount;
        public int Compression, SizeImage, XPelsPerMeter, YPelsPerMeter, ClrUsed, ClrImportant;
    }

    [LibraryImport("gdi32.dll")]
    public static partial int GetDIBits(nint dc, nint bmp, uint start, uint lines, byte[] bits, ref BitmapInfoHeader info, uint usage);

    /// <summary>What's on screen inside `rect`, as top-down BGRA (the compositor's work included).</summary>
    [LibraryImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static partial bool PrintWindow(nint hwnd, nint dc, uint flags);

    [LibraryImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static partial bool SetWindowPos(nint hwnd, nint after, int x, int y, int cx, int cy, uint flags);

    /// <summary>The window to the bottom of the stack, without activating it (the hidden screenshot runs).</summary>
    /// <summary>A hidden developer run's window: past the right edge of every screen (never seen, never clicked:
    /// 2026-11-07, one behind the owner's windows still took a click meant for his own beta) and at the bottom;
    /// PrintWindow still draws it there.</summary>
    public static void Behind(nint hwnd) =>
        SetWindowPos(hwnd, 1 /* HWND_BOTTOM */, GetSystemMetrics(76 /* SM_XVIRTUALSCREEN */) + GetSystemMetrics(78 /* SM_CXVIRTUALSCREEN */) + 200, 0, 0, 0, 0x0001 | 0x0010 /* NOSIZE | NOACTIVATE */);

    [LibraryImport("user32.dll")]
    private static partial int GetSystemMetrics(int index);

    /// <summary>The window's own picture, even behind others (PrintWindow, PW_RENDERFULLCONTENT: its composition
    /// and swapchains too), as BGRA rows.</summary>
    public static byte[] Print(nint hwnd, out int w, out int h)
    {
        GetWindowRect(hwnd, out var rect);
        w = rect.Right - rect.Left;
        h = rect.Bottom - rect.Top;
        var screen = GetDC(0);
        var mem = CreateCompatibleDC(screen);
        var bmp = CreateCompatibleBitmap(screen, w, h);
        var old = SelectObject(mem, bmp);
        PrintWindow(hwnd, mem, 2 /* PW_RENDERFULLCONTENT */);
        SelectObject(mem, old);
        var info = new BitmapInfoHeader { Size = Marshal.SizeOf<BitmapInfoHeader>(), Width = w, Height = -h, Planes = 1, BitCount = 32 };
        var bits = new byte[w * h * 4];
        GetDIBits(mem, bmp, 0, (uint)h, bits, ref info, 0);
        DeleteObject(bmp);
        DeleteDC(mem);
        ReleaseDC(0, screen);
        return bits;
    }

    [LibraryImport("user32.dll")]
    private static partial nint FindWindowExW(nint parent, nint after, nint cls, nint title);

    [LibraryImport("user32.dll")]
    private static partial uint GetWindowThreadProcessId(nint hwnd, out uint pid);

    [LibraryImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool IsWindowVisible(nint hwnd);

    /// <summary>The window's picture with this app's own pop-up windows over it (Search's suggestion list,
    /// a menu that may leave the window: WinUI gives them windows of their own, which PrintWindow of the
    /// main one leaves out). Hidden screenshot runs.</summary>
    public static byte[] PrintWithPopups(nint hwnd, out int w, out int h, out int popups)
    {
        var bits = Print(hwnd, out w, out h);
        GetWindowRect(hwnd, out var main);
        popups = 0;
        var me = (uint)Environment.ProcessId;
        for (var other = FindWindowExW(0, 0, 0, 0); other != 0; other = FindWindowExW(0, other, 0, 0))
        {
            if (other == hwnd || !IsWindowVisible(other)) continue;
            GetWindowThreadProcessId(other, out var pid);
            if (pid != me) continue;
            GetWindowRect(other, out var r);
            if (r.Right <= main.Left || r.Left >= main.Right || r.Bottom <= main.Top || r.Top >= main.Bottom) continue;
            var over = Print(other, out var ow, out var oh);
            for (var y = 0; y < oh; y++)
            {
                var ty = r.Top - main.Top + y;
                if (ty < 0 || ty >= h) continue;
                for (var x = 0; x < ow; x++)
                {
                    var tx = r.Left - main.Left + x;
                    if (tx < 0 || tx >= w) continue;
                    var from = (y * ow + x) * 4;
                    // (a pop-up's rounded corners and shadow come out empty: the window shows there)
                    if (over[from] == 0 && over[from + 1] == 0 && over[from + 2] == 0) continue;
                    Array.Copy(over, from, bits, (ty * w + tx) * 4, 4);
                }
            }
            popups++;
        }
        return bits;
    }

    public static byte[] Grab(Rect rect, out int w, out int h)
    {
        w = rect.Right - rect.Left;
        h = rect.Bottom - rect.Top;
        var screen = GetDC(0);
        var mem = CreateCompatibleDC(screen);
        var bmp = CreateCompatibleBitmap(screen, w, h);
        var old = SelectObject(mem, bmp);
        BitBlt(mem, 0, 0, w, h, screen, rect.Left, rect.Top, 0x00CC0020 | 0x40000000 /* SRCCOPY | CAPTUREBLT */);
        SelectObject(mem, old);
        var info = new BitmapInfoHeader { Size = Marshal.SizeOf<BitmapInfoHeader>(), Width = w, Height = -h, Planes = 1, BitCount = 32 };
        var bits = new byte[w * h * 4];
        GetDIBits(mem, bmp, 0, (uint)h, bits, ref info, 0);
        DeleteObject(bmp);
        DeleteDC(mem);
        ReleaseDC(0, screen);
        return bits;
    }
}
