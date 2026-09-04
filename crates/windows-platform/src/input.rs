//! Тонкая безопасная обёртка над Win32 `SendInput` и размером desktop.

use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP, MOUSEEVENTF_ABSOLUTE,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP,
    MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL, MOUSEINPUT,
    SendInput, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

use crate::{PlatformError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEvent {
    MouseMove {
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    },
    MouseButton {
        button: MouseButton,
        is_down: bool,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    },
    MouseWheel {
        delta: i32,
    },
    Key {
        virtual_key_code: u16,
        is_down: bool,
    },
}

pub fn primary_display_size() -> Result<(u32, u32)> {
    let width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let height = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    (width > 0 && height > 0)
        .then_some((width as u32, height as u32))
        .ok_or(PlatformError::Unexpected {
            api: "GetSystemMetrics",
            reason: "primary desktop has zero dimensions".to_owned(),
        })
}

pub fn send_input(event: InputEvent) -> Result<()> {
    let input = match event {
        InputEvent::MouseMove {
            x,
            y,
            width,
            height,
        } => mouse_move(x, y, width, height),
        InputEvent::MouseButton {
            button,
            is_down,
            x,
            y,
            width,
            height,
        } => mouse_button(button, is_down, x, y, width, height),
        InputEvent::MouseWheel { delta } => mouse_input(0, 0, MOUSEEVENTF_WHEEL, delta as u32),
        InputEvent::Key {
            virtual_key_code,
            is_down,
        } => keyboard(virtual_key_code, is_down),
    };
    let applied = unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) };
    (applied == 1)
        .then_some(())
        .ok_or(PlatformError::Unexpected {
            api: "SendInput",
            reason: "Windows rejected synthetic input".to_owned(),
        })
}

fn mouse_move(x: i32, y: i32, width: u32, height: u32) -> INPUT {
    let absolute_x = (i64::from(x) * 65_535 / i64::from(width.saturating_sub(1).max(1))) as i32;
    let absolute_y = (i64::from(y) * 65_535 / i64::from(height.saturating_sub(1).max(1))) as i32;
    mouse_input(
        absolute_x,
        absolute_y,
        MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE,
        0,
    )
}

fn mouse_button(
    button: MouseButton,
    is_down: bool,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> INPUT {
    let mut input = mouse_move(x, y, width, height);
    let flag = match (button, is_down) {
        (MouseButton::Left, true) => MOUSEEVENTF_LEFTDOWN,
        (MouseButton::Left, false) => MOUSEEVENTF_LEFTUP,
        (MouseButton::Right, true) => MOUSEEVENTF_RIGHTDOWN,
        (MouseButton::Right, false) => MOUSEEVENTF_RIGHTUP,
        (MouseButton::Middle, true) => MOUSEEVENTF_MIDDLEDOWN,
        (MouseButton::Middle, false) => MOUSEEVENTF_MIDDLEUP,
    };
    unsafe {
        input.Anonymous.mi.dwFlags |= flag;
    }
    input
}

fn mouse_input(
    dx: i32,
    dy: i32,
    flags: windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS,
    data: u32,
) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: data,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn keyboard(virtual_key_code: u16, is_down: bool) -> INPUT {
    let flags = if is_down {
        Default::default()
    } else {
        KEYEVENTF_KEYUP
    };
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(virtual_key_code),
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}
