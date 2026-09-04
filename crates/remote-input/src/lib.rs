//! Безопасный контракт T4 для удалённого ввода в interactive Session Host.

#[derive(Debug, Clone, PartialEq)]
pub enum RemoteInputEvent {
    MouseMove {
        x: f32,
        y: f32,
    },
    MouseButton {
        button: MouseButton,
        is_down: bool,
        x: f32,
        y: f32,
    },
    MouseWheel {
        delta: i32,
    },
    Key {
        virtual_key_code: u32,
        is_down: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, thiserror::Error)]
pub enum RemoteInputError {
    #[error("координаты remote input должны находиться в диапазоне 0.0..=1.0")]
    InvalidCoordinates,
    #[error("виртуальная клавиша не поддерживается")]
    InvalidVirtualKey,
    #[error("remote input недоступен вне Windows interactive session")]
    BackendUnavailable,
    #[error("SendInput не применил все события")]
    InjectionFailed,
}

pub trait RemoteInput {
    fn apply(
        &mut self,
        event: &RemoteInputEvent,
        width: u32,
        height: u32,
    ) -> Result<(), RemoteInputError>;
}

pub fn normalized_to_pixels(
    x: f32,
    y: f32,
    width: u32,
    height: u32,
) -> Result<(i32, i32), RemoteInputError> {
    if !x.is_finite()
        || !y.is_finite()
        || !(0.0..=1.0).contains(&x)
        || !(0.0..=1.0).contains(&y)
        || width == 0
        || height == 0
    {
        return Err(RemoteInputError::InvalidCoordinates);
    }
    let pixel_x = (x * (width.saturating_sub(1)) as f32).round() as i32;
    let pixel_y = (y * (height.saturating_sub(1)) as f32).round() as i32;
    Ok((pixel_x, pixel_y))
}

#[cfg(not(windows))]
pub struct SendInputRemote;

#[cfg(not(windows))]
impl SendInputRemote {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(not(windows))]
impl Default for SendInputRemote {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(windows))]
impl RemoteInput for SendInputRemote {
    fn apply(
        &mut self,
        _event: &RemoteInputEvent,
        _width: u32,
        _height: u32,
    ) -> Result<(), RemoteInputError> {
        Err(RemoteInputError::BackendUnavailable)
    }
}

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::SendInputRemote;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_coordinates_cover_display_edges() {
        assert_eq!(normalized_to_pixels(0.0, 0.0, 1920, 1080).unwrap(), (0, 0));
        assert_eq!(
            normalized_to_pixels(1.0, 1.0, 1920, 1080).unwrap(),
            (1919, 1079)
        );
    }

    #[test]
    fn invalid_coordinates_are_rejected_before_injection() {
        assert!(matches!(
            normalized_to_pixels(-0.1, 0.5, 1, 1),
            Err(RemoteInputError::InvalidCoordinates)
        ));
    }
}
