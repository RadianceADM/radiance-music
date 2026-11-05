using System.Runtime.InteropServices;

namespace RadianceMusic;

/// <summary>
/// The notification-area icon while the app is open (the GTK app's tray.rs, #37): its tooltip is the
/// song, a click shows the window, its menu (Windows' own, as tray.rs's) is Play/Pause, Next,
/// Previous, Show Radiance Music, Quit. A message-only window of its own hears it, on the UI thread.
/// </summary>
internal sealed class Tray : IDisposable
{
    private const uint WmApp = 0x8000, Callback = WmApp + 1;
    private const uint NimAdd = 0, NimModify = 1, NimDelete = 2, NimSetVersion = 4;
    private const uint NifMessage = 1, NifIcon = 2, NifTip = 4, NifShowTip = 0x80;
    private const uint WmLButtonUp = 0x0202, WmContextMenu = 0x007B, NinSelect = 0x0400, NinKeySelect = 0x0401;
    private const int Play = 1, Next = 2, Previous = 3, Show = 4, Quit = 5;

    private readonly EngineHub hub;
    private readonly WndProc proc;
    private readonly nint window;
    private readonly nint icon;
    private readonly uint taskbarCreated;
    private string tip = "Radiance Music";
    private bool added;

    /// <summary>The icon is in the notification area.</summary>
    public bool Added => added;

    public Tray(EngineHub hub, string iconPath)
    {
        this.hub = hub;
        proc = OnMessage;
        var wc = new WndClassEx { cbSize = (uint)Marshal.SizeOf<WndClassEx>(), lpfnWndProc = proc, hInstance = GetModuleHandle(null), lpszClassName = "RadianceMusicBetaTray" };
        RegisterClassEx(ref wc);
        window = CreateWindowEx(0, wc.lpszClassName, "", 0, 0, 0, 0, 0, -3 /* HWND_MESSAGE */, 0, wc.hInstance, 0);
        var size = GetSystemMetrics(49 /* SM_CXSMICON */);
        icon = LoadImage(0, iconPath, 1 /* IMAGE_ICON */, size, size, 0x10 /* LR_LOADFROMFILE */);
        taskbarCreated = RegisterWindowMessage("TaskbarCreated");
        Add();
        hub.Change += kind =>
        {
            if (kind == Native.ChangeKind.Now) Update();
        };
        Update();
    }

    private NotifyIconData Data(uint flags) => new()
    {
        cbSize = (uint)Marshal.SizeOf<NotifyIconData>(),
        hWnd = window,
        uID = 1,
        uFlags = flags,
        uCallbackMessage = Callback,
        hIcon = icon,
        szTip = tip,
        uVersion = 4, // NOTIFYICON_VERSION_4
    };

    private void Add()
    {
        var d = Data(NifMessage | NifIcon | NifTip | NifShowTip);
        added = Shell_NotifyIcon(NimAdd, ref d);
        Shell_NotifyIcon(NimSetVersion, ref d);
    }

    private void Update()
    {
        var song = hub.Engine.Now().Song;
        var now = TrayText.Tooltip(song?.Title ?? "", song?.Byline ?? "");
        if (now == tip) return;
        tip = now;
        var d = Data(NifTip | NifShowTip);
        Shell_NotifyIcon(NimModify, ref d);
    }

    private nint OnMessage(nint hwnd, uint msg, nint wParam, nint lParam)
    {
        if (msg == Callback)
        {
            var what = (uint)(lParam.ToInt64() & 0xFFFF);
            if (what is WmLButtonUp or NinSelect or NinKeySelect) App.Window?.BringToFront();
            else if (what == WmContextMenu) Menu((short)(wParam.ToInt64() & 0xFFFF), (short)((wParam.ToInt64() >> 16) & 0xFFFF));
            return 0;
        }
        if (msg == taskbarCreated) Add(); // Explorer started again: the icon back
        return DefWindowProc(hwnd, msg, wParam, lParam);
    }

    private void Menu(int x, int y)
    {
        var now = hub.Engine.Now();
        var has = now.Song != null;
        var menu = CreatePopupMenu();
        AppendMenu(menu, has ? 0u : 1u /* MF_GRAYED */, Play, now.Playing ? "Pause" : "Play");
        AppendMenu(menu, now.CanNext ? 0u : 1u, Next, "Next");
        AppendMenu(menu, now.CanPrevious ? 0u : 1u, Previous, "Previous");
        AppendMenu(menu, 0x800 /* MF_SEPARATOR */, 0, null);
        AppendMenu(menu, 0, Show, "Show Radiance Music");
        AppendMenu(menu, 0, Quit, "Quit");
        SetForegroundWindow(window);
        var picked = TrackPopupMenuEx(menu, 0x100 /* TPM_RETURNCMD */ | 0x2 /* TPM_RIGHTBUTTON */, x, y, window, 0);
        DestroyMenu(menu);
        switch (picked)
        {
            case Play: hub.Engine.TogglePlay(); break;
            case Next: hub.Engine.Skip(1); break;
            case Previous: hub.Engine.Skip(-1); break;
            case Show: App.Window?.BringToFront(); break;
            case Quit: App.Quit(); break;
        }
    }

    public void Dispose()
    {
        if (added)
        {
            var d = Data(0);
            Shell_NotifyIcon(NimDelete, ref d);
            added = false;
        }
        if (icon != 0) DestroyIcon(icon);
        DestroyWindow(window);
    }

    private delegate nint WndProc(nint hwnd, uint msg, nint wParam, nint lParam);

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct WndClassEx
    {
        public uint cbSize, style;
        public WndProc lpfnWndProc;
        public int cbClsExtra, cbWndExtra;
        public nint hInstance, hIcon, hCursor, hbrBackground;
        public string? lpszMenuName;
        public string lpszClassName;
        public nint hIconSm;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct NotifyIconData
    {
        public uint cbSize;
        public nint hWnd;
        public uint uID, uFlags, uCallbackMessage;
        public nint hIcon;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)] public string szTip;
        public uint dwState, dwStateMask;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 256)] public string szInfo;
        public uint uVersion;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 64)] public string szInfoTitle;
        public uint dwInfoFlags;
        public Guid guidItem;
        public nint hBalloonIcon;
    }

    [DllImport("shell32.dll", CharSet = CharSet.Unicode, EntryPoint = "Shell_NotifyIconW")]
    private static extern bool Shell_NotifyIcon(uint message, ref NotifyIconData data);
    [DllImport("user32.dll", CharSet = CharSet.Unicode, EntryPoint = "RegisterClassExW")]
    private static extern ushort RegisterClassEx(ref WndClassEx wc);
    [DllImport("user32.dll", CharSet = CharSet.Unicode, EntryPoint = "CreateWindowExW")]
    private static extern nint CreateWindowEx(uint exStyle, string cls, string name, uint style, int x, int y, int w, int h, nint parent, nint menu, nint instance, nint param);
    [DllImport("user32.dll", EntryPoint = "DefWindowProcW")]
    private static extern nint DefWindowProc(nint hwnd, uint msg, nint wParam, nint lParam);
    [DllImport("user32.dll")]
    private static extern bool DestroyWindow(nint hwnd);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, EntryPoint = "GetModuleHandleW")]
    private static extern nint GetModuleHandle(string? name);
    [DllImport("user32.dll")]
    private static extern int GetSystemMetrics(int index);
    [DllImport("user32.dll", CharSet = CharSet.Unicode, EntryPoint = "LoadImageW")]
    private static extern nint LoadImage(nint instance, string name, uint type, int cx, int cy, uint load);
    [DllImport("user32.dll")]
    private static extern bool DestroyIcon(nint icon);
    [DllImport("user32.dll", CharSet = CharSet.Unicode, EntryPoint = "RegisterWindowMessageW")]
    private static extern uint RegisterWindowMessage(string name);
    [DllImport("user32.dll")]
    private static extern nint CreatePopupMenu();
    [DllImport("user32.dll", CharSet = CharSet.Unicode, EntryPoint = "AppendMenuW")]
    private static extern bool AppendMenu(nint menu, uint flags, nint id, string? text);
    [DllImport("user32.dll")]
    private static extern int TrackPopupMenuEx(nint menu, uint flags, int x, int y, nint hwnd, nint tpm);
    [DllImport("user32.dll")]
    private static extern bool DestroyMenu(nint menu);
    [DllImport("user32.dll")]
    private static extern bool SetForegroundWindow(nint hwnd);
}
