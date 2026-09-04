//! Примитивы доступа к HKEY_LOCAL_MACHINE для policy-провайдера.
//!
//! Модуль намеренно не знает ни одного продуктового понятия: он умеет читать,
//! писать и удалять значения по явно переданному пути. Соответствие
//! "ограничение → ключ реестра" живёт в policy-провайдере (ADR-0006).

use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Registry::{
    HKEY, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WRITE, REG_DWORD, REG_OPTION_NON_VOLATILE, REG_SZ,
    REG_VALUE_TYPE, RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW,
    RegSetValueExW,
};
use windows::core::PCWSTR;

use crate::{PlatformError, Result};

/// Значение реестра в том виде, в каком его использует policy-провайдер.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryData {
    Dword(u32),
    Text(String),
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

/// RAII-обёртка: незакрытый HKEY — это утечка дескриптора в долгоживущей службе.
struct OwnedKey(HKEY);

impl Drop for OwnedKey {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

fn open_key(subkey: &str, write: bool) -> Result<Option<OwnedKey>> {
    let mut key = HKEY::default();
    let access = if write {
        KEY_READ | KEY_WRITE
    } else {
        KEY_READ
    };
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(wide(subkey).as_ptr()),
            None,
            access,
            &mut key,
        )
    };
    match status {
        ERROR_SUCCESS => Ok(Some(OwnedKey(key))),
        ERROR_FILE_NOT_FOUND => Ok(None),
        WIN32_ERROR(code) => Err(PlatformError::Unexpected {
            api: "RegOpenKeyExW",
            reason: format!("{subkey}: код {code}"),
        }),
    }
}

fn create_key(subkey: &str) -> Result<OwnedKey> {
    let mut key = HKEY::default();
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(wide(subkey).as_ptr()),
            None,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_READ | KEY_WRITE,
            None,
            &mut key,
            None,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(PlatformError::Unexpected {
            api: "RegCreateKeyExW",
            reason: format!("{subkey}: код {}", status.0),
        });
    }
    Ok(OwnedKey(key))
}

/// Читает значение. `None` означает, что ключа или значения нет — это
/// нормальное состояние, которое обязан различать snapshot.
pub fn read_value(subkey: &str, name: &str) -> Result<Option<RegistryData>> {
    let Some(key) = open_key(subkey, false)? else {
        return Ok(None);
    };
    let name_wide = wide(name);
    let mut value_type = REG_VALUE_TYPE::default();
    let mut size: u32 = 0;
    let status = unsafe {
        RegQueryValueExW(
            key.0,
            PCWSTR(name_wide.as_ptr()),
            None,
            Some(&mut value_type as *mut REG_VALUE_TYPE),
            None,
            Some(&mut size as *mut u32),
        )
    };
    match status {
        ERROR_SUCCESS => {}
        ERROR_FILE_NOT_FOUND => return Ok(None),
        WIN32_ERROR(code) => {
            return Err(PlatformError::Unexpected {
                api: "RegQueryValueExW",
                reason: format!("{subkey}\\{name}: код {code}"),
            });
        }
    }

    let mut buffer = vec![0u8; size as usize];
    let status = unsafe {
        RegQueryValueExW(
            key.0,
            PCWSTR(name_wide.as_ptr()),
            None,
            Some(&mut value_type as *mut REG_VALUE_TYPE),
            Some(buffer.as_mut_ptr()),
            Some(&mut size as *mut u32),
        )
    };
    if status != ERROR_SUCCESS {
        return Err(PlatformError::Unexpected {
            api: "RegQueryValueExW",
            reason: format!("{subkey}\\{name}: код {}", status.0),
        });
    }
    buffer.truncate(size as usize);

    match value_type {
        REG_DWORD if buffer.len() >= 4 => Ok(Some(RegistryData::Dword(u32::from_ne_bytes([
            buffer[0], buffer[1], buffer[2], buffer[3],
        ])))),
        REG_SZ => {
            let units: Vec<u16> = buffer
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_ne_bytes(*pair))
                .take_while(|unit| *unit != 0)
                .collect();
            Ok(Some(RegistryData::Text(String::from_utf16_lossy(&units))))
        }
        other => Err(PlatformError::Unexpected {
            api: "RegQueryValueExW",
            reason: format!("{subkey}\\{name}: неподдерживаемый тип {}", other.0),
        }),
    }
}

/// Записывает значение, создавая ключ при необходимости.
pub fn write_value(subkey: &str, name: &str, data: &RegistryData) -> Result<()> {
    let key = create_key(subkey)?;
    let name_wide = wide(name);
    let (value_type, bytes) = match data {
        RegistryData::Dword(value) => (REG_DWORD, value.to_ne_bytes().to_vec()),
        RegistryData::Text(value) => (
            REG_SZ,
            wide(value)
                .iter()
                .flat_map(|unit| unit.to_ne_bytes())
                .collect(),
        ),
    };
    let status = unsafe {
        RegSetValueExW(
            key.0,
            PCWSTR(name_wide.as_ptr()),
            None,
            value_type,
            Some(&bytes),
        )
    };
    if status != ERROR_SUCCESS {
        return Err(PlatformError::Unexpected {
            api: "RegSetValueExW",
            reason: format!("{subkey}\\{name}: код {}", status.0),
        });
    }
    Ok(())
}

/// Удаляет значение. Отсутствие значения не считается ошибкой: rollback
/// должен приводить к состоянию "значения нет" идемпотентно.
pub fn delete_value(subkey: &str, name: &str) -> Result<()> {
    let Some(key) = open_key(subkey, true)? else {
        return Ok(());
    };
    let status = unsafe { RegDeleteValueW(key.0, PCWSTR(wide(name).as_ptr())) };
    match status {
        ERROR_SUCCESS | ERROR_FILE_NOT_FOUND => Ok(()),
        WIN32_ERROR(code) => Err(PlatformError::Unexpected {
            api: "RegDeleteValueW",
            reason: format!("{subkey}\\{name}: код {code}"),
        }),
    }
}

/// Восстанавливает значение из snapshot: либо возвращает прежние данные, либо
/// удаляет значение, если раньше его не существовало.
pub fn restore_value(subkey: &str, name: &str, previous: Option<&RegistryData>) -> Result<()> {
    match previous {
        Some(data) => write_value(subkey, name, data),
        None => delete_value(subkey, name),
    }
}
