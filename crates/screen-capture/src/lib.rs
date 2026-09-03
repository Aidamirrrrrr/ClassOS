//! Контракты T2 для захвата и кодирования одного кадра экрана.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Display {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    pub primary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawFrame {
    pub display_id: u32,
    pub width: u32,
    pub height: u32,
    /// RGB8 буфер без хранения на диске.
    pub pixels: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedFrame {
    pub display_id: u32,
    pub width: u32,
    pub height: u32,
    pub format: &'static str,
    pub data: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("дисплей {0} не найден")]
    DisplayNotFound(u32),
    #[error("захват не запущен")]
    NotStarted,
    #[error("некорректный размер RGB-буфера")]
    InvalidBuffer,
    #[error("ошибка JPEG-кодирования: {0}")]
    Encode(String),
}

pub trait ScreenCapture {
    fn displays(&self) -> Result<Vec<Display>, CaptureError>;
    fn start(&mut self, display_id: u32) -> Result<(), CaptureError>;
    fn next_frame(&mut self) -> Result<RawFrame, CaptureError>;
    fn stop(&mut self);
}

pub trait FrameEncoder {
    fn encode(&mut self, frame: RawFrame) -> Result<EncodedFrame, CaptureError>;
}

/// Предсказуемый источник кадров для unit-тестов pipeline и протокола.
pub struct MockCapture {
    displays: Vec<Display>,
    active_display: Option<Display>,
    next_value: u8,
}

impl MockCapture {
    pub fn new(displays: Vec<Display>) -> Self {
        Self {
            displays,
            active_display: None,
            next_value: 0,
        }
    }
}

impl ScreenCapture for MockCapture {
    fn displays(&self) -> Result<Vec<Display>, CaptureError> {
        Ok(self.displays.clone())
    }

    fn start(&mut self, display_id: u32) -> Result<(), CaptureError> {
        self.active_display = Some(
            *self
                .displays
                .iter()
                .find(|display| display.id == display_id)
                .ok_or(CaptureError::DisplayNotFound(display_id))?,
        );
        Ok(())
    }

    fn next_frame(&mut self) -> Result<RawFrame, CaptureError> {
        let display = self.active_display.ok_or(CaptureError::NotStarted)?;
        let pixel_count = (display.width as usize)
            .checked_mul(display.height as usize)
            .ok_or(CaptureError::InvalidBuffer)?;
        let mut pixels = vec![
            self.next_value;
            pixel_count
                .checked_mul(3)
                .ok_or(CaptureError::InvalidBuffer)?
        ];
        for pixel in pixels.as_chunks_mut::<3>().0 {
            pixel[1] = self.next_value.wrapping_add(32);
            pixel[2] = self.next_value.wrapping_add(64);
        }
        self.next_value = self.next_value.wrapping_add(1);
        Ok(RawFrame {
            display_id: display.id,
            width: display.width,
            height: display.height,
            pixels,
        })
    }

    fn stop(&mut self) {
        self.active_display = None;
    }
}

pub struct JpegEncoder {
    quality: u8,
}

impl JpegEncoder {
    pub fn new(quality: u8) -> Self {
        Self {
            quality: quality.clamp(1, 100),
        }
    }
}

impl FrameEncoder for JpegEncoder {
    fn encode(&mut self, frame: RawFrame) -> Result<EncodedFrame, CaptureError> {
        let expected = (frame.width as usize)
            .checked_mul(frame.height as usize)
            .and_then(|pixels| pixels.checked_mul(3))
            .ok_or(CaptureError::InvalidBuffer)?;
        if frame.pixels.len() != expected {
            return Err(CaptureError::InvalidBuffer);
        }
        let mut data = Vec::new();
        let encoder = jpeg_encoder::Encoder::new(&mut data, self.quality);
        encoder
            .encode(
                &frame.pixels,
                frame.width as u16,
                frame.height as u16,
                jpeg_encoder::ColorType::Rgb,
            )
            .map_err(|error| CaptureError::Encode(error.to_string()))?;
        Ok(EncodedFrame {
            display_id: frame.display_id,
            width: frame.width,
            height: frame.height,
            format: "jpeg",
            data,
        })
    }
}

impl fmt::Debug for MockCapture {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MockCapture")
            .field("displays", &self.displays)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_lists_displays_and_requires_start() {
        let mut capture = MockCapture::new(vec![
            Display {
                id: 0,
                width: 4,
                height: 2,
                primary: true,
            },
            Display {
                id: 1,
                width: 2,
                height: 2,
                primary: false,
            },
        ]);
        assert_eq!(capture.displays().unwrap().len(), 2);
        assert!(matches!(
            capture.next_frame(),
            Err(CaptureError::NotStarted)
        ));
        capture.start(0).unwrap();
        assert_eq!(capture.next_frame().unwrap().pixels.len(), 24);
        capture.stop();
        assert!(matches!(
            capture.next_frame(),
            Err(CaptureError::NotStarted)
        ));
    }

    #[test]
    fn jpeg_encoder_round_trips_dimensions() {
        let mut capture = MockCapture::new(vec![Display {
            id: 0,
            width: 8,
            height: 4,
            primary: true,
        }]);
        capture.start(0).unwrap();
        let frame = capture.next_frame().unwrap();
        let mut encoder = JpegEncoder::new(80);
        let encoded = encoder.encode(frame).unwrap();
        assert_eq!(encoded.format, "jpeg");
        assert!(!encoded.data.is_empty());
        let decoded = jpeg_decoder::Decoder::new(encoded.data.as_slice())
            .decode()
            .unwrap();
        assert_eq!(decoded.len(), 8 * 4 * 3);
    }
}
