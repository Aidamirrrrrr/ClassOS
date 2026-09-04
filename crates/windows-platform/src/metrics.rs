//! Сбор метрик устройства: CPU, память, диск, uptime, версия Windows.
//!
//! Только примитивы; пороги и правила health живут в `device-health`.

use windows::Win32::Foundation::FILETIME;
use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows::Win32::System::SystemInformation::{
    GetTickCount64, GlobalMemoryStatusEx, MEMORYSTATUSEX,
};
use windows::Win32::System::Threading::GetSystemTimes;
use windows::core::PCWSTR;

use crate::{PlatformError, Result};

/// Доля занятой оперативной памяти в процентах.
pub fn memory_percent() -> Result<f64> {
    let mut status = MEMORYSTATUSEX {
        dwLength: u32::try_from(std::mem::size_of::<MEMORYSTATUSEX>()).unwrap_or(0),
        ..Default::default()
    };
    unsafe { GlobalMemoryStatusEx(&mut status) }.map_err(|error| PlatformError::WindowsApi {
        api: "GlobalMemoryStatusEx",
        source: error,
    })?;
    Ok(f64::from(status.dwMemoryLoad))
}

/// Доля занятого места на системном диске в процентах.
pub fn system_disk_percent() -> Result<f64> {
    let root: Vec<u16> = "C:\\".encode_utf16().chain(std::iter::once(0)).collect();
    let mut free_to_caller: u64 = 0;
    let mut total: u64 = 0;
    let mut free: u64 = 0;
    unsafe {
        GetDiskFreeSpaceExW(
            PCWSTR(root.as_ptr()),
            Some(&mut free_to_caller),
            Some(&mut total),
            Some(&mut free),
        )
    }
    .map_err(|error| PlatformError::WindowsApi {
        api: "GetDiskFreeSpaceExW",
        source: error,
    })?;
    if total == 0 {
        return Err(PlatformError::Unexpected {
            api: "GetDiskFreeSpaceExW",
            reason: "нулевой размер тома".to_owned(),
        });
    }
    let used = total.saturating_sub(free);
    #[allow(clippy::cast_precision_loss)]
    Ok((used as f64 / total as f64) * 100.0)
}

/// Время с момента загрузки в секундах.
pub fn uptime_seconds() -> i64 {
    i64::try_from(unsafe { GetTickCount64() } / 1_000).unwrap_or(i64::MAX)
}

/// Версия Windows из документированных значений реестра.
///
/// Реестр используется вместо `GetVersionEx`, который для приложений без
/// манифеста совместимости возвращает заниженную версию.
pub fn os_version() -> Result<String> {
    use crate::registry::{RegistryData, read_value};

    const CURRENT_VERSION: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
    let text = |name: &str| -> Option<String> {
        match read_value(CURRENT_VERSION, name) {
            Ok(Some(RegistryData::Text(value))) => Some(value),
            _ => None,
        }
    };
    let product = text("ProductName").unwrap_or_else(|| "Windows".to_owned());
    match text("DisplayVersion") {
        Some(display) => Ok(format!("{product} {display}")),
        None => Ok(product),
    }
}

fn filetime_to_u64(value: FILETIME) -> u64 {
    (u64::from(value.dwHighDateTime) << 32) | u64::from(value.dwLowDateTime)
}

/// Замер загрузки CPU по двум выборкам системных времён.
///
/// Мгновенного значения загрузки в Windows нет: считается разница между
/// вызовами, поэтому первый замер всегда возвращает 0.
#[derive(Debug, Default, Clone, Copy)]
pub struct CpuSampler {
    previous_idle: u64,
    previous_total: u64,
}

impl CpuSampler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Загрузка CPU в процентах с момента предыдущего вызова.
    pub fn sample(&mut self) -> Result<f64> {
        let mut idle = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)) }.map_err(
            |error| PlatformError::WindowsApi {
                api: "GetSystemTimes",
                source: error,
            },
        )?;

        // Время ядра уже включает время простоя, поэтому total = kernel + user.
        let idle = filetime_to_u64(idle);
        let total = filetime_to_u64(kernel).saturating_add(filetime_to_u64(user));
        let idle_delta = idle.saturating_sub(self.previous_idle);
        let total_delta = total.saturating_sub(self.previous_total);
        self.previous_idle = idle;
        self.previous_total = total;

        if total_delta == 0 {
            return Ok(0.0);
        }
        #[allow(clippy::cast_precision_loss)]
        let busy = (total_delta.saturating_sub(idle_delta)) as f64 / total_delta as f64;
        Ok((busy * 100.0).clamp(0.0, 100.0))
    }
}
