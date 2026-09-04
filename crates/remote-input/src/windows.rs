use windows_platform::input::{
    self, InputEvent as PlatformInputEvent, MouseButton as PlatformMouseButton,
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

pub fn primary_display_size() -> Result<(u32, u32), RemoteInputError> {
    input::primary_display_size().map_err(|_| RemoteInputError::BackendUnavailable)
}

impl RemoteInput for SendInputRemote {
    fn apply(
        &mut self,
        event: &RemoteInputEvent,
        width: u32,
        height: u32,
    ) -> Result<(), RemoteInputError> {
        let event = match event {
            RemoteInputEvent::MouseMove { x, y } => {
                let (x, y) = normalized_to_pixels(*x, *y, width, height)?;
                PlatformInputEvent::MouseMove {
                    x,
                    y,
                    width,
                    height,
                }
            }
            RemoteInputEvent::MouseButton {
                button,
                is_down,
                x,
                y,
            } => {
                let (x, y) = normalized_to_pixels(*x, *y, width, height)?;
                PlatformInputEvent::MouseButton {
                    button: match button {
                        MouseButton::Left => PlatformMouseButton::Left,
                        MouseButton::Right => PlatformMouseButton::Right,
                        MouseButton::Middle => PlatformMouseButton::Middle,
                    },
                    is_down: *is_down,
                    x,
                    y,
                    width,
                    height,
                }
            }
            RemoteInputEvent::MouseWheel { delta } => {
                PlatformInputEvent::MouseWheel { delta: *delta }
            }
            RemoteInputEvent::Key {
                virtual_key_code,
                is_down,
            } => PlatformInputEvent::Key {
                virtual_key_code: u16::try_from(*virtual_key_code)
                    .map_err(|_| RemoteInputError::InvalidVirtualKey)?,
                is_down: *is_down,
            },
        };
        input::send_input(event).map_err(|_| RemoteInputError::InjectionFailed)
    }
}
