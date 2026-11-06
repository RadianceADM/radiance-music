//! A COM object's reference, held: AddRef when taken, Release when dropped (the music video's
//! swapchain, which the sink owns and the window's panel shows). Plain vtable calls (IUnknown's
//! slots 1 and 2), so it needs no Windows crate.

/// One reference to a COM object at `ptr` (0: none), released when dropped.
#[derive(Debug)]
pub struct ComRef(u64);

type RefFn = unsafe extern "system" fn(*mut std::ffi::c_void) -> u32;

unsafe fn slot(ptr: u64, i: usize) -> RefFn {
    let vtable = *(ptr as *const *const usize);
    std::mem::transmute::<usize, RefFn>(*vtable.add(i))
}

// A COM object's AddRef/Release may be called from any thread (DXGI's are free-threaded).
unsafe impl Send for ComRef {}
unsafe impl Sync for ComRef {}

impl ComRef {
    /// A reference of our own to `ptr` (AddRef). `ptr` must be alive now.
    ///
    /// # Safety
    /// `ptr` is 0 or a live COM object.
    pub unsafe fn take(ptr: u64) -> ComRef {
        if ptr != 0 {
            slot(ptr, 1)(ptr as *mut _);
        }
        ComRef(ptr)
    }

    pub fn ptr(&self) -> u64 {
        self.0
    }

    /// One more reference, for someone else to Release (the C# side, once its panel holds its own).
    pub fn give(&self) -> u64 {
        if self.0 != 0 {
            unsafe { slot(self.0, 1)(self.0 as *mut _) };
        }
        self.0
    }
}

impl Drop for ComRef {
    fn drop(&mut self) {
        if self.0 != 0 {
            unsafe { slot(self.0, 2)(self.0 as *mut _) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI32, Ordering};

    /// A pretend COM object: its vtable (QueryInterface, AddRef, Release) and its count.
    #[repr(C)]
    struct Fake {
        vtable: *const [RefFn; 3],
        refs: AtomicI32,
    }

    unsafe extern "system" fn qi(_: *mut std::ffi::c_void) -> u32 {
        0
    }
    unsafe extern "system" fn add(this: *mut std::ffi::c_void) -> u32 {
        (*(this as *const Fake)).refs.fetch_add(1, Ordering::SeqCst) as u32 + 1
    }
    unsafe extern "system" fn release(this: *mut std::ffi::c_void) -> u32 {
        (*(this as *const Fake)).refs.fetch_sub(1, Ordering::SeqCst) as u32 - 1
    }
    static VTABLE: [RefFn; 3] = [qi, add, release];

    #[test]
    fn a_reference_is_held_until_dropped_and_given_ones_are_extra() {
        let fake = Box::new(Fake { vtable: &VTABLE, refs: AtomicI32::new(1) });
        let ptr = &*fake as *const Fake as u64;
        let held = unsafe { ComRef::take(ptr) };
        assert_eq!(fake.refs.load(Ordering::SeqCst), 2, "ours, beside the sink's");
        assert_eq!(held.give(), ptr);
        assert_eq!(fake.refs.load(Ordering::SeqCst), 3, "one for the caller to Release");
        drop(held);
        assert_eq!(fake.refs.load(Ordering::SeqCst), 2, "ours gone, the caller's stays");
        let none = unsafe { ComRef::take(0) };
        assert_eq!(none.give(), 0);
    }
}
