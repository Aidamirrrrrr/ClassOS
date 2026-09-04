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
    #[error("DXGI Desktop Duplication недоступен в текущем Session Host")]
    BackendUnavailable,
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

/// Масштабирует RGB-кадр до заданной ширины без обращения к диску.
///
/// T3 использует ближайшего соседа: для thumbnail это предсказуемо, не требует
/// дополнительной зависимости и выполняется до JPEG-кодирования в Session Host.
pub fn scale_to_max_width(frame: RawFrame, max_width: u32) -> Result<RawFrame, CaptureError> {
    if max_width == 0 || frame.width <= max_width {
        return Ok(frame);
    }
    let target_height = (u64::from(frame.height) * u64::from(max_width) / u64::from(frame.width))
        .max(1)
        .try_into()
        .map_err(|_| CaptureError::InvalidBuffer)?;
    let source_len = (frame.width as usize)
        .checked_mul(frame.height as usize)
        .and_then(|pixels| pixels.checked_mul(3))
        .ok_or(CaptureError::InvalidBuffer)?;
    if frame.pixels.len() != source_len {
        return Err(CaptureError::InvalidBuffer);
    }
    let target_len = (max_width as usize)
        .checked_mul(target_height as usize)
        .and_then(|pixels| pixels.checked_mul(3))
        .ok_or(CaptureError::InvalidBuffer)?;
    let mut pixels = vec![0; target_len];
    for target_y in 0..target_height {
        let source_y =
            (u64::from(target_y) * u64::from(frame.height) / u64::from(target_height)) as u32;
        for target_x in 0..max_width {
            let source_x =
                (u64::from(target_x) * u64::from(frame.width) / u64::from(max_width)) as u32;
            let source_offset = ((source_y * frame.width + source_x) * 3) as usize;
            let target_offset = ((target_y * max_width + target_x) * 3) as usize;
            pixels[target_offset..target_offset + 3]
                .copy_from_slice(&frame.pixels[source_offset..source_offset + 3]);
        }
    }
    Ok(RawFrame {
        display_id: frame.display_id,
        width: max_width,
        height: target_height,
        pixels,
    })
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

impl EncodedFrame {
    /// Преобразует кадр в сетевое сообщение, не создавая временный файл.
    pub fn into_network(
        self,
        device_id: String,
        captured_at_unix_ms: i64,
    ) -> protocol::network::ScreenFrame {
        protocol::network::ScreenFrame {
            device_id,
            display_id: self.display_id,
            width: self.width,
            height: self.height,
            encoded_data: self.data,
            format: self.format.to_owned(),
            captured_at_unix_ms,
            mode: protocol::network::StreamMode::Selected as i32,
            sequence: 0,
        }
    }
}

/// Точка расширения для Windows DXGI Desktop Duplication.
/// Реализация будет добавлена после проверки Win32 API на целевом toolchain.
#[cfg(not(windows))]
pub struct DxgiDesktopCapture;

#[cfg(not(windows))]
impl DxgiDesktopCapture {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(not(windows))]
impl Default for DxgiDesktopCapture {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(windows))]
impl ScreenCapture for DxgiDesktopCapture {
    fn displays(&self) -> Result<Vec<Display>, CaptureError> {
        Err(CaptureError::BackendUnavailable)
    }
    fn start(&mut self, _display_id: u32) -> Result<(), CaptureError> {
        Err(CaptureError::BackendUnavailable)
    }
    fn next_frame(&mut self) -> Result<RawFrame, CaptureError> {
        Err(CaptureError::BackendUnavailable)
    }
    fn stop(&mut self) {}
}

#[cfg(windows)]
mod dxgi;
#[cfg(windows)]
pub use dxgi::DxgiDesktopCapture;

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

/// Порядок дисплеев, в котором основной идёт первым.
///
/// Возвращает индексы исходного списка. Запрос без явного выбора дисплея
/// приходит с `display_id = 0`, поэтому основной монитор обязан получить
/// именно этот идентификатор: порядок перечисления выходов DXGI совпадения с
/// основным монитором не гарантирует.
///
/// Сортировка устойчива, поэтому остальные дисплеи сохраняют исходный
/// порядок, а идентификаторы остаются предсказуемыми между запусками.
pub fn primary_first(displays: &[Display]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..displays.len()).collect();
    order.sort_by_key(|index| !displays[*index].primary);
    order
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

    #[test]
    fn scaling_preserves_aspect_ratio_and_rgb_shape() {
        let frame = RawFrame {
            display_id: 3,
            width: 8,
            height: 4,
            pixels: (0..96).map(|value| value as u8).collect(),
        };
        let scaled = scale_to_max_width(frame, 4).unwrap();
        assert_eq!((scaled.display_id, scaled.width, scaled.height), (3, 4, 2));
        assert_eq!(scaled.pixels.len(), 4 * 2 * 3);
    }

    fn display(id: u32, primary: bool) -> Display {
        Display {
            id,
            width: 1920,
            height: 1080,
            primary,
        }
    }

    /// Основной монитор, перечисленный вторым, обязан стать дисплеем 0 —
    /// иначе преподаватель увидит соседний экран (spec T2 §13.2).
    #[test]
    fn primary_display_becomes_display_zero() {
        let displays = [display(0, false), display(1, true), display(2, false)];
        assert_eq!(primary_first(&displays), vec![1, 0, 2]);
    }

    /// Если основной уже первый, порядок не меняется.
    #[test]
    fn already_first_primary_keeps_order() {
        let displays = [display(0, true), display(1, false)];
        assert_eq!(primary_first(&displays), vec![0, 1]);
    }

    /// Ни один дисплей не помечен основным — порядок перечисления
    /// сохраняется, а не переставляется произвольно.
    #[test]
    fn without_primary_enumeration_order_is_kept() {
        let displays = [display(0, false), display(1, false)];
        assert_eq!(primary_first(&displays), vec![0, 1]);
    }
}
