//! Обнаружение физической console session (спека §25-27, §165).

use windows::Win32::System::RemoteDesktop::{ProcessIdToSessionId, WTSGetActiveConsoleSessionId};

use crate::error::{PlatformError, Result};

/// Значение `WTSGetActiveConsoleSessionId`, означающее отсутствие активной session.
const NO_ACTIVE_SESSION: u32 = 0xFFFFFFFF;

/// Возвращает id физической console session либо `None`, если пользователь
/// ещё не вошёл или session временно переключается. Это не фатальная ошибка.
pub fn active_console_session_id() -> Option<u32> {
    // SAFETY: функция не принимает аргументов и не имеет предусловий.
    let session_id = unsafe { WTSGetActiveConsoleSessionId() };
    if session_id == NO_ACTIVE_SESSION {
        None
    } else {
        Some(session_id)
    }
}

/// Независимо определяет session id процесса через `ProcessIdToSessionId`.
/// Используется для проверки IPC-клиента вместо доверия `SessionHello`.
pub fn session_id_for_process(pid: u32) -> Result<u32> {
    let mut session_id: u32 = 0;
    // SAFETY: `session_id` — выходной параметр, а PID не передаёт владение.
    unsafe { ProcessIdToSessionId(pid, &mut session_id) }.map_err(|source| {
        PlatformError::WindowsApi {
            api: "ProcessIdToSessionId",
            source,
        }
    })?;
    Ok(session_id)
}

#[cfg(test)]
mod tests {
    // Реальный вызов Win32 проверяется интеграционно на Windows. Поведение
    // при наличии и отсутствии session покрыто MockSessionProvider.
}
