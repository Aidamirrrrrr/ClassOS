//! Построение явного ACL Named Pipe: полный доступ SYSTEM и read/write
//! конкретному SID пользователя без default descriptor и `Everyone`.

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

/// Динамически получает SID пользователя token в формате SDDL.
pub fn user_sid_string(user_token: HANDLE) -> Result<String> {
    let mut needed: u32 = 0;

    // SAFETY: первый вызов с null buffer запрашивает необходимый размер.
    let query = unsafe { GetTokenInformation(user_token, TokenUser, None, 0, &mut needed) };
    if query.is_ok() || needed == 0 {
        return Err(PlatformError::Unexpected {
            api: "GetTokenInformation",
            reason: "expected ERROR_INSUFFICIENT_BUFFER size query to fail with a size".to_string(),
        });
    }

    let mut buffer = vec![0u8; needed as usize];
    let mut actual: u32 = 0;

    // SAFETY: buffer имеет запрошенный размер, превышения при TokenUser нет.
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

    // SAFETY: по контракту TokenUser buffer содержит структуру TOKEN_USER.
    let token_user = unsafe { &*(buffer.as_ptr() as *const TOKEN_USER) };
    let sid = token_user.User.Sid;

    let mut sid_str = PWSTR::null();
    // SAFETY: sid ссылается на живой buffer; новая строка выделяется через
    // LocalAlloc и ниже освобождается через LocalFree.
    unsafe { ConvertSidToStringSidW(sid, &mut sid_str) }.map_err(|source| {
        PlatformError::WindowsApi {
            api: "ConvertSidToStringSidW",
            source,
        }
    })?;

    // SAFETY: sid_str — только что полученная NUL-terminated wide string.
    let result = unsafe { sid_str.to_string() }.map_err(|_| PlatformError::Unexpected {
        api: "ConvertSidToStringSidW",
        reason: "SID string was not valid UTF-16".to_string(),
    });

    // SAFETY: выделенный LocalAlloc buffer должен освобождаться LocalFree.
    unsafe {
        let _ = LocalFree(Some(HLOCAL(sid_str.0 as *mut c_void)));
    }

    result
}

/// Owned security descriptor, освобождаемый через LocalFree.
pub struct PipeSecurityDescriptor {
    descriptor: PSECURITY_DESCRIPTOR,
}

// SAFETY: после создания память только читается и имеет одного владельца.
unsafe impl Send for PipeSecurityDescriptor {}

impl Drop for PipeSecurityDescriptor {
    fn drop(&mut self) {
        if !self.descriptor.0.is_null() {
            // SAFETY: этот WinAPI выделяет descriptor, освобождаемый LocalFree.
            unsafe {
                let _ = LocalFree(Some(HLOCAL(self.descriptor.0)));
            }
        }
    }
}

impl PipeSecurityDescriptor {
    /// Строит ACL T0: полный доступ LocalSystem, read/write заданному SID и
    /// никакого доступа остальным пользователям или сети.
    pub fn for_session_user(user_sid: &str) -> Result<Self> {
        let sddl = format!("D:(A;;GA;;;SY)(A;;GRGW;;;{user_sid})");
        let mut wide: Vec<u16> = sddl.encode_utf16().chain(std::iter::once(0)).collect();

        let mut descriptor = PSECURITY_DESCRIPTOR(std::ptr::null_mut());

        // SAFETY: wide — живая NUL-terminated UTF-16 строка; полученный
        // LocalAlloc pointer переходит во владение структуры.
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

    /// Создаёт `SECURITY_ATTRIBUTES`, заимствующий descriptor из `self`.
    pub fn as_security_attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.descriptor.0,
            bInheritHandle: windows::Win32::Foundation::FALSE,
        }
    }
}
