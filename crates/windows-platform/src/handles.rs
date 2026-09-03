//! RAII wrappers for Win32 handles and other manually-freed resources
//! (spec §30-33, §89-90). No raw `HANDLE`/pointer should escape this module
//! without being wrapped.

use std::ffi::c_void;

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Environment::DestroyEnvironmentBlock;

/// An owned Win32 `HANDLE`, closed via `CloseHandle` on drop (spec §30-31).
/// Deliberately does not implement `Clone` — duplication must go through
/// `DuplicateHandle` explicitly if ever needed, not implicit copy.
#[derive(Debug)]
pub struct OwnedHandle(HANDLE);

// SAFETY: a Win32 HANDLE is an opaque identifier; moving it across threads
// is fine as long as it is not used concurrently without synchronization
// (Win32 handles are individually thread-safe for the operations we use:
// CloseHandle, WaitForSingleObject, etc.).
unsafe impl Send for OwnedHandle {}
unsafe impl Sync for OwnedHandle {}

impl OwnedHandle {
    /// Wraps a raw handle. The caller must guarantee this handle is valid
    /// and that ownership (the obligation to close it) transfers here.
    ///
    /// # Safety
    /// `handle` must be a valid, currently-open Win32 handle that nothing
    /// else will close.
    pub unsafe fn from_raw(handle: HANDLE) -> Self {
        Self(handle)
    }

    pub fn raw(&self) -> HANDLE {
        self.0
    }

    /// Returns true if the handle is the null/invalid handle value.
    pub fn is_invalid(&self) -> bool {
        self.0.is_invalid()
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            // SAFETY: `self.0` was validated on construction to be an
            // owned, open handle; CloseHandle is the correct release
            // function for HANDLEs obtained from WTSQueryUserToken,
            // CreateProcessAsUser (process/thread handles), and
            // CreateNamedPipeW/ConnectNamedPipe.
            let _ = unsafe { CloseHandle(self.0) };
        }
    }
}

/// RAII wrapper for the environment block returned by
/// `CreateEnvironmentBlock`, released via `DestroyEnvironmentBlock`
/// (spec §32-33). Never accessed directly by business logic — only used to
/// pass `lpEnvironment` to `CreateProcessAsUserW`.
pub struct EnvironmentBlock(*mut c_void);

// SAFETY: the environment block is only read by CreateProcessAsUserW
// while this struct is alive; no concurrent access occurs across threads
// in our usage.
unsafe impl Send for EnvironmentBlock {}

impl EnvironmentBlock {
    /// # Safety
    /// `ptr` must be a valid pointer returned by `CreateEnvironmentBlock`
    /// that has not already been destroyed.
    pub unsafe fn from_raw(ptr: *mut c_void) -> Self {
        Self(ptr)
    }

    pub fn as_ptr(&self) -> *const c_void {
        self.0 as *const c_void
    }
}

impl Drop for EnvironmentBlock {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: `self.0` was created by CreateEnvironmentBlock and
            // has not been freed yet (single ownership via this struct).
            let _ = unsafe { DestroyEnvironmentBlock(self.0) };
        }
    }
}
