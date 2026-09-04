//! Неблокирующий визуальный индикатор активного remote control.

use std::sync::{Arc, Mutex, OnceLock, mpsc};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, DrawTextW, EndPaint, PAINTSTRUCT, SetBkMode, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW,
    HWND_TOPMOST, MSG, PostMessageW, PostQuitMessage, RegisterClassW, SW_SHOWNA, SWP_NOACTIVATE,
    SWP_SHOWWINDOW, SetWindowPos, ShowWindow, WM_CLOSE, WM_DESTROY, WM_PAINT, WNDCLASSW,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::w;

use crate::{PlatformError, Result};

const CLASS_NAME: windows::core::PCWSTR = w!("ClassOSRemoteControlIndicator");
const INDICATOR_TEXT: windows::core::PCWSTR = w!("Teacher connected");

#[derive(Clone, Default)]
pub struct StudentIndicator {
    window: Arc<Mutex<Option<isize>>>,
}

impl StudentIndicator {
    pub fn show(&self) -> Result<()> {
        if self
            .window
            .lock()
            .expect("indicator mutex poisoned")
            .is_some()
        {
            return Ok(());
        }
        register_class()?;
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let window = Arc::clone(&self.window);
        std::thread::spawn(move || {
            let instance = unsafe { GetModuleHandleW(None) }.unwrap_or_default();
            let hwnd = match unsafe {
                CreateWindowExW(
                    WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                    CLASS_NAME,
                    INDICATOR_TEXT,
                    WS_POPUP,
                    24,
                    24,
                    280,
                    56,
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
            *window.lock().expect("indicator mutex poisoned") = Some(hwnd.0 as isize);
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOWNA);
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    24,
                    24,
                    280,
                    56,
                    SWP_SHOWWINDOW | SWP_NOACTIVATE,
                );
            }
            let _ = ready_tx.send(Some(hwnd.0 as isize));
            let mut message = MSG::default();
            while unsafe { GetMessageW(&mut message, None, 0, 0) }.as_bool() {
                unsafe {
                    DispatchMessageW(&message);
                }
            }
            *window.lock().expect("indicator mutex poisoned") = None;
        });
        ready_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .ok()
            .flatten()
            .map(|_| ())
            .ok_or(PlatformError::Unexpected {
                api: "CreateWindowExW",
                reason: "indicator window was not created".to_owned(),
            })
    }

    pub fn hide(&self) {
        if let Some(hwnd) = self.window.lock().expect("indicator mutex poisoned").take() {
            let hwnd = HWND(hwnd as _);
            unsafe {
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
    }
}

impl Drop for StudentIndicator {
    fn drop(&mut self) {
        self.hide();
    }
}

fn register_class() -> Result<()> {
    static REGISTERED: OnceLock<std::result::Result<(), String>> = OnceLock::new();
    match REGISTERED.get_or_init(|| {
        let instance = unsafe { GetModuleHandleW(None) }.map_err(|error| error.to_string())?;
        let class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            lpszClassName: CLASS_NAME,
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

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_PAINT => {
            let mut paint = PAINTSTRUCT::default();
            let hdc = unsafe { BeginPaint(hwnd, &mut paint) };
            let mut text: Vec<u16> = "Teacher connected".encode_utf16().collect();
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
