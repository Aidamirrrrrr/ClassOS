//! Окна T5 в интерактивной пользовательской сессии.

use std::sync::{Arc, Mutex, OnceLock, mpsc};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, DrawTextW, EndPaint, PAINTSTRUCT, SetBkMode, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW,
    GetSystemMetrics, HWND_TOPMOST, MB_OK, MB_SETFOREGROUND, MB_TOPMOST, MSG, MessageBoxW,
    PostMessageW, PostQuitMessage, RegisterClassW, SM_CXSCREEN, SM_CYSCREEN, SW_SHOW,
    SWP_SHOWWINDOW, SetWindowPos, ShowWindow, WM_CLOSE, WM_DESTROY, WM_PAINT, WNDCLASSW,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{PCWSTR, w};

use crate::{PlatformError, Result};

const LOCK_CLASS: PCWSTR = w!("ClassOSLockOverlay");
const LOCK_TEXT: PCWSTR = w!("Экран временно заблокирован преподавателем");

/// Визуальный overlay T5. Он не является security boundary и будет заменён
/// policy enforcement в T6.
#[derive(Clone, Default)]
pub struct LockOverlay {
    window: Arc<Mutex<Option<isize>>>,
}

impl LockOverlay {
    pub fn show(&self) -> Result<()> {
        if self
            .window
            .lock()
            .expect("lock overlay mutex poisoned")
            .is_some()
        {
            return Ok(());
        }
        register_lock_class()?;
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let window = Arc::clone(&self.window);
        std::thread::spawn(move || {
            let instance = unsafe { GetModuleHandleW(None) }.unwrap_or_default();
            let width = unsafe { GetSystemMetrics(SM_CXSCREEN) }.max(1);
            let height = unsafe { GetSystemMetrics(SM_CYSCREEN) }.max(1);
            let hwnd = match unsafe {
                CreateWindowExW(
                    WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                    LOCK_CLASS,
                    LOCK_TEXT,
                    WS_POPUP,
                    0,
                    0,
                    width,
                    height,
                    None,
                    None,
                    Some(instance.into()),
                    None,
                )
            } {
                Ok(hwnd) => hwnd,
                Err(_) => {
                    let _ = ready_tx.send(None);
                    return;
                }
            };
            *window.lock().expect("lock overlay mutex poisoned") = Some(hwnd.0 as isize);
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOW);
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    width,
                    height,
                    SWP_SHOWWINDOW,
                );
            }
            let _ = ready_tx.send(Some(hwnd.0 as isize));
            let mut message = MSG::default();
            while unsafe { GetMessageW(&mut message, None, 0, 0) }.as_bool() {
                unsafe { DispatchMessageW(&message) };
            }
            *window.lock().expect("lock overlay mutex poisoned") = None;
        });
        ready_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .ok()
            .flatten()
            .map(|_| ())
            .ok_or(PlatformError::Unexpected {
                api: "CreateWindowExW",
                reason: "lock overlay was not created".to_owned(),
            })
    }

    pub fn hide(&self) {
        if let Some(hwnd) = self
            .window
            .lock()
            .expect("lock overlay mutex poisoned")
            .take()
        {
            unsafe {
                let _ = PostMessageW(Some(HWND(hwnd as _)), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
    }
}

impl Drop for LockOverlay {
    fn drop(&mut self) {
        self.hide();
    }
}

pub fn show_message(text: &str) -> Result<()> {
    if text.is_empty() || text.len() > 1_000 {
        return Err(PlatformError::Unexpected {
            api: "MessageBoxW",
            reason: "message must contain 1..=1000 bytes".to_owned(),
        });
    }
    let text = wide_null(text);
    std::thread::spawn(move || unsafe {
        let _ = MessageBoxW(
            None,
            PCWSTR(text.as_ptr()),
            w!("ClassOS — сообщение преподавателя"),
            MB_OK | MB_TOPMOST | MB_SETFOREGROUND,
        );
    });
    Ok(())
}

fn register_lock_class() -> Result<()> {
    static REGISTERED: OnceLock<std::result::Result<(), String>> = OnceLock::new();
    match REGISTERED.get_or_init(|| {
        let instance = unsafe { GetModuleHandleW(None) }.map_err(|error| error.to_string())?;
        let class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(lock_window_proc),
            hInstance: instance.into(),
            lpszClassName: LOCK_CLASS,
            ..Default::default()
        };
        unsafe { RegisterClassW(&class) };
        Ok(())
    }) {
        Ok(()) => Ok(()),
        Err(reason) => Err(PlatformError::Unexpected {
            api: "RegisterClassW",
            reason: reason.clone(),
        }),
    }
}

unsafe extern "system" fn lock_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_PAINT => {
            let mut paint = PAINTSTRUCT::default();
            let hdc = unsafe { BeginPaint(hwnd, &mut paint) };
            let mut text: Vec<u16> = "Экран временно заблокирован преподавателем"
                .encode_utf16()
                .collect();
            unsafe {
                SetBkMode(hdc, TRANSPARENT);
                let _ = DrawTextW(hdc, &mut text, &mut paint.rcPaint, Default::default());
                let _ = EndPaint(hwnd, &paint);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
