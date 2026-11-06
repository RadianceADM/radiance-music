using System.Runtime.InteropServices;
using Microsoft.UI.Xaml.Controls;

namespace RadianceMusic.Interop;

/// <summary>
/// ISwapChainPanelNative (microsoft.ui.xaml.media.dxinterop.h): puts a DXGI swapchain on a
/// SwapChainPanel. The engine's d3d12swapchainsink makes the swapchain and draws the music
/// video into it on the GPU; the compositor shows it (no frame passes through the app).
/// </summary>
internal static unsafe class SwapChainPanelNative
{
    private static readonly Guid Iid = new("63aad0b8-7c24-40ff-85a8-640d944cc325");

    /// <summary>Sets <paramref name="swapChain"/> (an IDXGISwapChain1*; 0 takes it off) on the panel.
    /// The panel holds its own reference, so the picture stays until it's taken off.</summary>
    public static void SetSwapChain(SwapChainPanel panel, nint swapChain)
    {
        var unknown = ((WinRT.IWinRTObject)panel).NativeObject.ThisPtr;
        Marshal.ThrowExceptionForHR(Marshal.QueryInterface(unknown, Iid, out var native));
        try
        {
            // the vtable: IUnknown's three, then SetSwapChain
            var set = (delegate* unmanaged[Stdcall]<nint, nint, int>)(*(nint**)native)[3];
            Marshal.ThrowExceptionForHR(set(native, swapChain));
        }
        finally
        {
            Marshal.Release(native);
        }
    }
}
