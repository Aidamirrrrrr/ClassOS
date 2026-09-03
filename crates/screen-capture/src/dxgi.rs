//! Windows DXGI Desktop Duplication backend.

use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, DXGI_ERROR_NOT_FOUND, IDXGIFactory1};

use crate::{CaptureError, Display, RawFrame, ScreenCapture};

pub struct DxgiDesktopCapture {
    displays: Vec<Display>,
}

impl DxgiDesktopCapture {
    pub fn new() -> Result<Self, CaptureError> {
        let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }
            .map_err(|error| CaptureError::Encode(format!("DXGI factory: {error}")))?;
        let mut displays = Vec::new();
        let mut adapter_index = 0;
        loop {
            let adapter = match unsafe { factory.EnumAdapters1(adapter_index) } {
                Ok(adapter) => adapter,
                Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
                Err(error) => return Err(CaptureError::Encode(format!("DXGI adapter: {error}"))),
            };
            let mut output_index = 0;
            loop {
                let output = match unsafe { adapter.EnumOutputs(output_index) } {
                    Ok(output) => output,
                    Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
                    Err(error) => {
                        return Err(CaptureError::Encode(format!("DXGI output: {error}")));
                    }
                };
                let desc = unsafe { output.GetDesc() }.map_err(|error| {
                    CaptureError::Encode(format!("DXGI output description: {error}"))
                })?;
                let width =
                    (desc.DesktopCoordinates.right - desc.DesktopCoordinates.left).max(0) as u32;
                let height =
                    (desc.DesktopCoordinates.bottom - desc.DesktopCoordinates.top).max(0) as u32;
                displays.push(Display {
                    id: displays.len() as u32,
                    width,
                    height,
                    primary: displays.is_empty(),
                });
                output_index += 1;
            }
            adapter_index += 1;
        }
        Ok(Self { displays })
    }
}

impl ScreenCapture for DxgiDesktopCapture {
    fn displays(&self) -> Result<Vec<Display>, CaptureError> {
        Ok(self.displays.clone())
    }

    fn start(&mut self, display_id: u32) -> Result<(), CaptureError> {
        self.displays
            .iter()
            .any(|display| display.id == display_id)
            .then_some(())
            .ok_or(CaptureError::DisplayNotFound(display_id))
    }

    fn next_frame(&mut self) -> Result<RawFrame, CaptureError> {
        Err(CaptureError::BackendUnavailable)
    }

    fn stop(&mut self) {}
}
