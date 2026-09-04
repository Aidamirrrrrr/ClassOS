//! Безопасные действия пользовательской оболочки Windows.

use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::PCWSTR;

use crate::{PlatformError, Result};

pub fn open_url(url: &str) -> Result<()> {
    let url = wide_null(url);
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR::null(),
            PCWSTR(url.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    (result.0 as isize > 32)
        .then_some(())
        .ok_or(PlatformError::Unexpected {
            api: "ShellExecuteW",
            reason: format!("Windows вернула код {:?}", result.0),
        })
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
