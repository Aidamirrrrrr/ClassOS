use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP, MOUSEEVENTF_ABSOLUTE,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP,
    MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL, MOUSEINPUT,
    SendInput, VIRTUAL_KEY,
};

use crate::{MouseButton, RemoteInput, RemoteInputError, RemoteInputEvent, normalized_to_pixels};

pub struct SendInputRemote;

impl SendInputRemote {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SendInputRemote {
    fn default() -> Self {
        Self::new()
    }
}

impl RemoteInput for SendInputRemote {
    fn apply(
        &mut self,
        event: &RemoteInputEvent,
        width: u32,
        height: u32,
    ) -> Result<(), RemoteInputError> {
        let input = match event {
            RemoteInputEvent::MouseMove { x, y } => mouse_move(*x, *y, width, height)?,
            RemoteInputEvent::MouseButton {
                button,
                is_down,
                x,
                y,
            } => mouse_button(*button, *is_down, *x, *y, width, height)?,
            RemoteInputEvent::MouseWheel { delta } => mouse_wheel(*delta),
            RemoteInputEvent::Key {
                virtual_key_code,
                is_down,
            } => keyboard(*virtual_key_code, *is_down)?,
        };
        let applied = unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) };
        (applied == 1)
            .then_some(())
            .ok_or(RemoteInputError::InjectionFailed)
    }
}

fn mouse_move(x: f32, y: f32, width: u32, height: u32) -> Result<INPUT, RemoteInputError> {
    let (x, y) = normalized_to_pixels(x, y, width, height)?;
    let absolute_x = (i64::from(x) * 65_535 / i64::from(width.saturating_sub(1).max(1))) as i32;
    let absolute_y = (i64::from(y) * 65_535 / i64::from(height.saturating_sub(1).max(1))) as i32;
    Ok(mouse_input(
        absolute_x,
        absolute_y,
        MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE,
        0,
    ))
}

fn mouse_button(
    button: MouseButton,
    is_down: bool,
    x: f32,
    y: f32,
    width: u32,
    height: u32,
) -> Result<INPUT, RemoteInputError> {
    let mut input = mouse_move(x, y, width, height)?;
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
    Ok(input)
}

fn mouse_wheel(delta: i32) -> INPUT {
    mouse_input(0, 0, MOUSEEVENTF_WHEEL, delta as u32)
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

fn keyboard(virtual_key_code: u32, is_down: bool) -> Result<INPUT, RemoteInputError> {
    let vk = u16::try_from(virtual_key_code).map_err(|_| RemoteInputError::InvalidVirtualKey)?;
    let flags = if is_down {
        Default::default()
    } else {
        KEYEVENTF_KEYUP
    };
    Ok(INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    })
}
