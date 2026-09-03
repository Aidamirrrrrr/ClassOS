//! Physical console session discovery (spec §25-27, §165).

use windows::Win32::System::RemoteDesktop::{ProcessIdToSessionId, WTSGetActiveConsoleSessionId};

use crate::error::{PlatformError, Result};

/// Sentinel value returned by `WTSGetActiveConsoleSessionId` when there is
/// currently no active console session (spec §25, §165).
const NO_ACTIVE_SESSION: u32 = 0xFFFFFFFF;

/// Returns the session id of the physical console session, or `None` if
/// there is currently no interactive console session (e.g. between boot
/// and first login, or a transient state during a session switch —
/// spec §165-166). This is never a fatal error condition.
pub fn active_console_session_id() -> Option<u32> {
    // SAFETY: WTSGetActiveConsoleSessionId takes no arguments and has no
    // preconditions; it is always safe to call.
    let session_id = unsafe { WTSGetActiveConsoleSessionId() };
    if session_id == NO_ACTIVE_SESSION {
        None
    } else {
        Some(session_id)
    }
}

/// Independently resolves the Windows session id that owns process `pid`,
/// via `ProcessIdToSessionId` — used by the IPC handshake to verify a
/// claimed session id instead of trusting the client's `SessionHello`
/// payload (spec §59-60, §132).
pub fn session_id_for_process(pid: u32) -> Result<u32> {
    let mut session_id: u32 = 0;
    // SAFETY: `session_id` is an out-parameter written on success; `pid`
    // is a plain process id value with no ownership implications.
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
    // active_console_session_id() calls into real Win32 and can only be
    // meaningfully exercised on an actual Windows host; there is
    // deliberately no unit test here. Behavioral coverage of "no session"
    // vs "session present" lives in agent-core::supervisor's tests via
    // MockSessionProvider, which this function's caller adapts to.
}
