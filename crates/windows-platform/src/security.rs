//! Named Pipe ACL construction (spec §44-48). Builds an explicit security
//! descriptor granting full access to SYSTEM and read/write to a specific
//! session user's SID — never a default descriptor, never `Everyone`.

use std::ffi::c_void;

use windows::Win32::Foundation::{HANDLE, HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{
    GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_USER, TokenUser,
};
use windows::core::PWSTR;

use crate::error::{PlatformError, Result};

/// Reads the SID of the user represented by `user_token` and returns it as
/// an SDDL SID string (e.g. `S-1-5-21-...`), formed dynamically — never
/// hardcoded (spec §47).
pub fn user_sid_string(user_token: HANDLE) -> Result<String> {
    let mut needed: u32 = 0;

    // SAFETY: first call with a null buffer to query the required size;
    // this is the documented pattern for GetTokenInformation.
    let query = unsafe { GetTokenInformation(user_token, TokenUser, None, 0, &mut needed) };
    if query.is_ok() || needed == 0 {
        return Err(PlatformError::Unexpected {
            api: "GetTokenInformation",
            reason: "expected ERROR_INSUFFICIENT_BUFFER size query to fail with a size".to_string(),
        });
    }

    let mut buffer = vec![0u8; needed as usize];
    let mut actual: u32 = 0;

    // SAFETY: `buffer` is sized exactly to `needed` bytes as returned by
    // the size-query call above; GetTokenInformation writes at most
    // `needed` bytes into it for TokenUser.
    unsafe {
        GetTokenInformation(
            user_token,
            TokenUser,
            Some(buffer.as_mut_ptr() as *mut c_void),
            needed,
            &mut actual,
        )
    }
    .map_err(|source| PlatformError::WindowsApi {
        api: "GetTokenInformation",
        source,
    })?;

    // SAFETY: `buffer` was filled by GetTokenInformation with a TOKEN_USER
    // structure, guaranteed by Microsoft's documented contract for the
    // TokenUser information class.
    let token_user = unsafe { &*(buffer.as_ptr() as *const TOKEN_USER) };
    let sid = token_user.User.Sid;

    let mut sid_str = PWSTR::null();
    // SAFETY: `sid` points into `buffer`, which is still alive here;
    // ConvertSidToStringSidW allocates its own output buffer via LocalAlloc
    // which we free below with LocalFree.
    unsafe { ConvertSidToStringSidW(sid, &mut sid_str) }.map_err(|source| {
        PlatformError::WindowsApi {
            api: "ConvertSidToStringSidW",
            source,
        }
    })?;

    // SAFETY: sid_str was just populated by ConvertSidToStringSidW as a
    // NUL-terminated wide string.
    let result = unsafe { sid_str.to_string() }.map_err(|_| PlatformError::Unexpected {
        api: "ConvertSidToStringSidW",
        reason: "SID string was not valid UTF-16".to_string(),
    });

    // SAFETY: sid_str.0 is the LocalAlloc'd buffer from
    // ConvertSidToStringSidW; Microsoft's docs require the caller to free
    // it with LocalFree.
    unsafe {
        let _ = LocalFree(Some(HLOCAL(sid_str.0 as *mut c_void)));
    }

    result
}

/// An owned security descriptor produced by
/// `ConvertStringSecurityDescriptorToSecurityDescriptorW`, released via
/// `LocalFree` on drop (spec §90).
pub struct PipeSecurityDescriptor {
    descriptor: PSECURITY_DESCRIPTOR,
}

// SAFETY: the descriptor memory is only read (never mutated) after
// construction, and this struct has single ownership.
unsafe impl Send for PipeSecurityDescriptor {}

impl Drop for PipeSecurityDescriptor {
    fn drop(&mut self) {
        if !self.descriptor.0.is_null() {
            // SAFETY: `self.descriptor.0` was allocated by
            // ConvertStringSecurityDescriptorToSecurityDescriptorW, which
            // Microsoft documents must be released with LocalFree.
            unsafe {
                let _ = LocalFree(Some(HLOCAL(self.descriptor.0)));
            }
        }
    }
}

impl PipeSecurityDescriptor {
    /// Builds the explicit T0 pipe ACL (spec §45-46):
    /// - `SY` (LocalSystem): full access.
    /// - the given user SID: read/write (generic read + generic write).
    /// - nobody else (no Everyone/Authenticated Users/ANONYMOUS/NETWORK).
    pub fn for_session_user(user_sid: &str) -> Result<Self> {
        let sddl = format!("D:(A;;GA;;;SY)(A;;GRGW;;;{user_sid})");
        let mut wide: Vec<u16> = sddl.encode_utf16().chain(std::iter::once(0)).collect();

        let mut descriptor = PSECURITY_DESCRIPTOR(std::ptr::null_mut());

        // SAFETY: `wide` is a valid NUL-terminated UTF-16 string alive for
        // the duration of this call; `descriptor` receives a freshly
        // LocalAlloc'd pointer that this struct takes ownership of.
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PWSTR(wide.as_mut_ptr()),
                SDDL_REVISION_1,
                &mut descriptor,
                None,
            )
        }
        .map_err(|source| PlatformError::WindowsApi {
            api: "ConvertStringSecurityDescriptorToSecurityDescriptorW",
            source,
        })?;

        Ok(Self { descriptor })
    }

    /// Builds a `SECURITY_ATTRIBUTES` referencing this descriptor. The
    /// returned struct borrows `self` and must not outlive it.
    pub fn as_security_attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.descriptor.0,
            bInheritHandle: windows::Win32::Foundation::FALSE,
        }
    }
}
