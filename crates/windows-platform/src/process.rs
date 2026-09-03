//! `WTSQueryUserToken` + `CreateEnvironmentBlock` + `CreateProcessAsUserW`
//! launch pipeline (spec §29-39).

use std::ffi::OsString;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows::Win32::Foundation::{HANDLE, STILL_ACTIVE};
use windows::Win32::System::Environment::CreateEnvironmentBlock;
use windows::Win32::System::RemoteDesktop::WTSQueryUserToken;
use windows::Win32::System::Threading::{
    CREATE_UNICODE_ENVIRONMENT, CreateProcessAsUserW, GetExitCodeProcess, PROCESS_INFORMATION,
    STARTUPINFOW, TerminateProcess,
};
use windows::core::PWSTR;

use crate::error::{PlatformError, Result};
use crate::handles::{EnvironmentBlock, OwnedHandle};

/// Obtains the primary token of the interactively logged-on user for
/// `session_id` (spec §29-31). Caller must run as LocalSystem with
/// `SE_TCB_NAME` (true for a Windows Service running as LocalSystem).
pub fn query_user_token(session_id: u32) -> Result<OwnedHandle> {
    let mut token = HANDLE::default();
    // SAFETY: `token` is an out-parameter written by WTSQueryUserToken on
    // success; ownership of the resulting handle transfers to us and is
    // wrapped immediately below.
    unsafe { WTSQueryUserToken(session_id, &mut token) }.map_err(|source| {
        PlatformError::WindowsApi {
            api: "WTSQueryUserToken",
            source,
        }
    })?;

    // SAFETY: `token` was just populated by a successful WTSQueryUserToken
    // call and is not otherwise owned.
    Ok(unsafe { OwnedHandle::from_raw(token) })
}

/// Creates a user environment block for `user_token` (spec §32-33).
pub fn create_environment_block(user_token: &OwnedHandle) -> Result<EnvironmentBlock> {
    let mut env_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
    // SAFETY: `env_ptr` receives a pointer allocated by
    // CreateEnvironmentBlock; ownership transfers to the EnvironmentBlock
    // RAII wrapper immediately below.
    unsafe { CreateEnvironmentBlock(&mut env_ptr, Some(user_token.raw()), false) }.map_err(
        |source| PlatformError::WindowsApi {
            api: "CreateEnvironmentBlock",
            source,
        },
    )?;

    // SAFETY: env_ptr was just populated by a successful
    // CreateEnvironmentBlock call.
    Ok(unsafe { EnvironmentBlock::from_raw(env_ptr) })
}

/// A process launched inside a user session, tracked by pid and an owned
/// process handle for later liveness/termination checks (spec §73).
pub struct LaunchedProcess {
    pub pid: u32,
    pub process_handle: OwnedHandle,
}

fn to_wide_null(s: &str) -> Vec<u16> {
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// Launches `executable` with `args` inside `session_id`'s interactive
/// desktop, using the session user's own token and environment
/// (spec §34-39). This is the only place `CreateProcessAsUserW` is called.
pub fn launch_in_session(
    session_id: u32,
    executable: &Path,
    args: &[OsString],
) -> Result<LaunchedProcess> {
    let user_token = query_user_token(session_id)?;
    let env_block = create_environment_block(&user_token)?;

    // Build the command line: quoted executable path followed by quoted
    // args. No secrets are ever placed here (spec §39, §136).
    let mut command_line = format!("\"{}\"", executable.display());
    for arg in args {
        command_line.push(' ');
        command_line.push('"');
        command_line.push_str(&arg.to_string_lossy());
        command_line.push('"');
    }
    let mut command_line_wide = to_wide_null(&command_line);

    let mut desktop = to_wide_null("winsta0\\default");

    let startup_info = STARTUPINFOW {
        cb: std::mem::size_of::<STARTUPINFOW>() as u32,
        lpDesktop: PWSTR(desktop.as_mut_ptr()),
        ..Default::default()
    };
    let mut process_information = PROCESS_INFORMATION::default();

    // SAFETY: all pointers passed (command line, desktop string,
    // environment block) are valid and alive for the duration of this
    // call; lpApplicationName is None so Windows parses the executable
    // from the (quoted) command line. On success, process_information
    // contains newly-owned process/thread handles that we wrap or close
    // below.
    let result = unsafe {
        CreateProcessAsUserW(
            Some(user_token.raw()),
            PWSTR::null(),
            Some(PWSTR(command_line_wide.as_mut_ptr())),
            None,
            None,
            false,
            CREATE_UNICODE_ENVIRONMENT,
            Some(env_block.as_ptr()),
            PWSTR::null(),
            &startup_info,
            &mut process_information,
        )
    };

    result.map_err(|source| PlatformError::WindowsApi {
        api: "CreateProcessAsUserW",
        source,
    })?;

    // SAFETY: hThread was just returned by a successful CreateProcessAsUserW
    // call; we only need the process handle going forward, so the thread
    // handle is closed immediately per Microsoft's guidance to close
    // handles that are no longer needed.
    let thread_handle = unsafe { OwnedHandle::from_raw(process_information.hThread) };
    drop(thread_handle);

    // SAFETY: hProcess was just returned by the same successful call.
    let process_handle = unsafe { OwnedHandle::from_raw(process_information.hProcess) };

    Ok(LaunchedProcess {
        pid: process_information.dwProcessId,
        process_handle,
    })
}

/// Returns whether `process_handle` still refers to a running process, by
/// checking its exit code (`STILL_ACTIVE` sentinel). Used by the
/// `SessionProcessLauncher` adapter's `is_alive` implementation.
pub fn is_process_alive(process_handle: &OwnedHandle) -> Result<bool> {
    let mut exit_code: u32 = 0;
    // SAFETY: process_handle.raw() is a valid, open process handle for the
    // lifetime of this call.
    unsafe { GetExitCodeProcess(process_handle.raw(), &mut exit_code) }.map_err(|source| {
        PlatformError::WindowsApi {
            api: "GetExitCodeProcess",
            source,
        }
    })?;
    Ok(exit_code == STILL_ACTIVE.0 as u32)
}

/// Terminates a managed process (spec §72-74). Must only ever be called on
/// a handle this crate itself obtained from `launch_in_session` — never
/// targeted by process name.
pub fn terminate_process(process_handle: &OwnedHandle) -> Result<()> {
    // SAFETY: process_handle.raw() is a valid, open process handle for the
    // lifetime of this call.
    unsafe { TerminateProcess(process_handle.raw(), 1) }.map_err(|source| {
        PlatformError::WindowsApi {
            api: "TerminateProcess",
            source,
        }
    })
}

#[cfg(test)]
mod tests {
    // Every function here calls into real Win32 (WTSQueryUserToken,
    // CreateEnvironmentBlock, CreateProcessAsUserW, GetExitCodeProcess,
    // TerminateProcess) and requires LocalSystem privilege plus an active
    // interactive session, so it is only meaningfully exercised on a real
    // Windows integration test host, not as a host-independent unit test
    // (spec §100-105, §146: business-logic testability lives in
    // agent-core's SessionSupervisor + mocks instead).
}
