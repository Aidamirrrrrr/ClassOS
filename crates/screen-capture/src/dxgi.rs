//! Windows DXGI Desktop Duplication backend.

use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_FLAG, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ERROR_NOT_FOUND, IDXGIAdapter1, IDXGIFactory1, IDXGIOutput,
    IDXGIOutput1, IDXGIOutputDuplication,
};
use windows::core::Interface;

use crate::{CaptureError, Display, RawFrame, ScreenCapture};

pub struct DxgiDesktopCapture {
    displays: Vec<Display>,
    outputs: Vec<IDXGIOutput>,
    adapters: Vec<IDXGIAdapter1>,
    device: Option<ID3D11Device>,
    duplication: Option<IDXGIOutputDuplication>,
}

impl DxgiDesktopCapture {
    pub fn new() -> Result<Self, CaptureError> {
        let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }
            .map_err(|error| CaptureError::Encode(format!("DXGI factory: {error}")))?;
        let mut displays = Vec::new();
        let mut outputs = Vec::new();
        let mut adapters = Vec::new();
        let mut adapter_index = 0;
        loop {
            let adapter = match unsafe { factory.EnumAdapters1(adapter_index) } {
                Ok(adapter) => adapter,
                Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
                Err(error) => return Err(CaptureError::Encode(format!("DXGI adapter: {error}"))),
            };
            adapters.push(adapter.clone());
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
                outputs.push(output);
                output_index += 1;
            }
            adapter_index += 1;
        }
        Ok(Self {
            displays,
            outputs,
            adapters,
            device: None,
            duplication: None,
        })
    }
}

impl ScreenCapture for DxgiDesktopCapture {
    fn displays(&self) -> Result<Vec<Display>, CaptureError> {
        Ok(self.displays.clone())
    }

    fn start(&mut self, display_id: u32) -> Result<(), CaptureError> {
        self.displays
            .iter()
            .find(|display| display.id == display_id)
            .ok_or(CaptureError::DisplayNotFound(display_id))?;
        let output = self
            .outputs
            .get(display_id as usize)
            .ok_or(CaptureError::DisplayNotFound(display_id))?;
        let output1: IDXGIOutput1 = output
            .cast()
            .map_err(|error| CaptureError::Encode(format!("DXGI output interface: {error}")))?;
        let adapter = self
            .adapters
            .first()
            .ok_or(CaptureError::BackendUnavailable)?;
        let mut device = None;
        unsafe {
            D3D11CreateDevice(
                adapter,
                D3D_DRIVER_TYPE_UNKNOWN,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_FLAG(0),
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                None,
            )
        }
        .map_err(|error| CaptureError::Encode(format!("D3D11 device: {error}")))?;
        let device = device.ok_or(CaptureError::BackendUnavailable)?;
        let duplication = unsafe { output1.DuplicateOutput(&device) }
            .map_err(|error| CaptureError::Encode(format!("DXGI duplication: {error}")))?;
        self.device = Some(device);
        self.duplication = Some(duplication);
        Ok(())
    }

    fn next_frame(&mut self) -> Result<RawFrame, CaptureError> {
        let duplication = self.duplication.as_ref().ok_or(CaptureError::NotStarted)?;
        let mut frame_info = Default::default();
        let mut resource = None;
        unsafe { duplication.AcquireNextFrame(250, &mut frame_info, &mut resource) }
            .map_err(|error| CaptureError::Encode(format!("DXGI AcquireNextFrame: {error}")))?;
        unsafe { duplication.ReleaseFrame() }
            .map_err(|error| CaptureError::Encode(format!("DXGI ReleaseFrame: {error}")))?;
        Err(CaptureError::BackendUnavailable)
    }

    fn stop(&mut self) {
        self.duplication = None;
        self.device = None;
    }
}
