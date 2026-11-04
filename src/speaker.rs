//! Windows: which speaker Windows plays to by default (#38). The sound output opens on
//! the default speaker and stays on it; when Windows moves the default (the monitor with
//! the speakers switched back on, headphones plugged in, another output picked in the
//! sound menu), the window's once-a-second look notices and the song carries on there.
//! Linux needs none of it: PipeWire moves the stream itself.

use std::cell::RefCell;

use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_APARTMENTTHREADED};

thread_local! {
    /// Windows' list of sound devices, made once (on the window's thread).
    static DEVICES: RefCell<Option<IMMDeviceEnumerator>> = const { RefCell::new(None) };
}

/// The default speaker's id (None: no speaker at all, or Windows didn't say).
pub fn default_id() -> Option<String> {
    DEVICES.with(|d| {
        let mut d = d.borrow_mut();
        if d.is_none() {
            // (GTK has COM going on this thread already: then this only says so)
            let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            *d = unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }.ok();
        }
        let devices = d.as_ref()?;
        unsafe {
            let speaker = devices.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
            let id = speaker.GetId().ok()?;
            let text = id.to_string().ok();
            CoTaskMemFree(Some(id.0 as *const _));
            text
        }
    })
}
