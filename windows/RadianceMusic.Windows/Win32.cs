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
    public static void Behind(nint hwnd) => SetWindowPos(hwnd, 1 /* HWND_BOTTOM */, 0, 0, 0, 0, 0x0001 | 0x0002 | 0x0010 /* NOSIZE | NOMOVE | NOACTIVATE */);

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
