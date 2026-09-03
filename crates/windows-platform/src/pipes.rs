//! Named Pipe primitives (spec §41-48). This module only creates the raw,
//! ACL-protected pipe server handle and resolves the connecting client's
//! PID; the async read/write transport on top of the resulting handle is
//! wired up by the caller (e.g. via `tokio::net::windows::named_pipe`,
//! which `windows-platform` deliberately does not depend on, keeping this
//! crate free of async-runtime concerns per spec §148 purity and §154 "no
//! premature async everywhere").

use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
    FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, PIPE_ACCESS_DUPLEX,
};
use windows::Win32::System::Pipes::{
    CreateNamedPipeW, GetNamedPipeClientProcessId, PIPE_READMODE_MESSAGE,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_MESSAGE, PIPE_WAIT,
};
use windows::core::PWSTR;

use crate::error::{PlatformError, Result};
use crate::handles::OwnedHandle;
use crate::security::PipeSecurityDescriptor;

/// Recommended buffer sizes for the T0 IPC pipe: comfortably larger than
/// MAX_FRAME_SIZE (64 KiB) used by `protocol::framing` so a single frame
/// never spans multiple internal pipe buffer fills unnecessarily.
const PIPE_BUFFER_SIZE: u32 = 128 * 1024;

/// Builds the T0 pipe name: `\\.\pipe\classos\session-{sessionId}-{instanceId}`
/// (spec §41), never a bare `session-{sessionId}` to avoid stale-connection
/// collisions and spoofing convenience (spec §42).
pub fn pipe_name(session_id: u32, instance_id: &str) -> String {
    format!(r"\\.\pipe\classos\session-{session_id}-{instance_id}")
}

fn to_wide_null(s: &str) -> Vec<u16> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// Creates a new Named Pipe server instance for `pipe_name`, protected by
/// an explicit ACL restricting access to SYSTEM (full) and `user_sid`
/// (read/write) — spec §44-48. Opened with `FILE_FLAG_OVERLAPPED` so the
/// resulting handle is suitable for wrapping in an async I/O type
/// (e.g. `tokio::net::windows::named_pipe::NamedPipeServer::from_raw_handle`)
/// by the caller.
pub fn create_pipe_server(
    pipe_name: &str,
    user_sid: &str,
) -> Result<(OwnedHandle, PipeSecurityDescriptor)> {
    let descriptor = PipeSecurityDescriptor::for_session_user(user_sid)?;
    let security_attributes = descriptor.as_security_attributes();

    let mut wide_name = to_wide_null(pipe_name);

    // SAFETY: `wide_name` is a valid NUL-terminated wide string alive for
    // the duration of this call; `security_attributes` borrows from
    // `descriptor`, which outlives this call and is returned to the caller
    // alongside the handle so it is not dropped early.
    let handle: HANDLE = unsafe {
        CreateNamedPipeW(
            PWSTR(wide_name.as_mut_ptr()),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1, // this pipe instance serves exactly one Session Host connection
            PIPE_BUFFER_SIZE,
            PIPE_BUFFER_SIZE,
            0,
            Some(&security_attributes),
        )
    };

    if handle.is_invalid() {
        return Err(PlatformError::Unexpected {
            api: "CreateNamedPipeW",
            reason: "returned an invalid handle".to_string(),
        });
    }

    // SAFETY: `handle` was just returned by a successful CreateNamedPipeW
    // call.
    let owned = unsafe { OwnedHandle::from_raw(handle) };
    Ok((owned, descriptor))
}

/// Resolves the process id of the process connected to `pipe_handle`
/// (spec §60, preferred over trusting client-supplied PID/session values).
/// Takes a borrowed raw `HANDLE` rather than `&OwnedHandle` so it can be
/// used both with handles owned by this crate and with a pipe handle owned
/// by an external async runtime wrapper (e.g. tokio's
/// `NamedPipeServer::as_raw_handle`), which does not transfer ownership.
///
/// # Safety
/// `pipe_handle` must be a valid, open Named Pipe server handle for the
/// duration of this call.
pub unsafe fn client_process_id(pipe_handle: HANDLE) -> Result<u32> {
    let mut pid: u32 = 0;
    // SAFETY: caller guarantees `pipe_handle` is valid for this call, per
    // this function's own safety contract.
    unsafe { GetNamedPipeClientProcessId(pipe_handle, &mut pid) }.map_err(|source| {
        PlatformError::WindowsApi {
            api: "GetNamedPipeClientProcessId",
            source,
        }
    })?;
    Ok(pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipe_name_is_namespaced_and_unique_per_instance() {
        let a = pipe_name(1, "aaa");
        let b = pipe_name(1, "bbb");
        assert_ne!(a, b);
        assert!(a.starts_with(r"\\.\pipe\classos\session-1-"));
    }
}
