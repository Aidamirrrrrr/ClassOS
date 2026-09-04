//! Безопасная оболочка для строго типизированного выключения и перезагрузки.

use windows::Win32::System::Shutdown::{
    EWX_FORCEIFHUNG, EWX_REBOOT, EWX_SHUTDOWN, ExitWindowsEx, SHUTDOWN_REASON,
};

use crate::{PlatformError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerAction {
    Restart,
    Shutdown,
}

/// Вызывает системное завершение работы только для заранее определённого вида
/// действия. API для произвольной системной команды намеренно отсутствует.
pub fn execute_power_action(action: PowerAction) -> Result<()> {
    let flags = match action {
        PowerAction::Restart => EWX_REBOOT | EWX_FORCEIFHUNG,
        PowerAction::Shutdown => EWX_SHUTDOWN | EWX_FORCEIFHUNG,
    };
    unsafe { ExitWindowsEx(flags, SHUTDOWN_REASON(0)) }.map_err(|error| PlatformError::Unexpected {
        api: "ExitWindowsEx",
        reason: error.to_string(),
    })
}
