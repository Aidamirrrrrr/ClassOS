//! `WTSQueryUserToken` + `CreateEnvironmentBlock` + `CreateProcessAsUserW`
//! Цепочка запуска процесса через WinAPI (спека §29-39).

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

/// Получает primary token пользователя `session_id`. Вызывающий должен
/// работать как LocalSystem с `SE_TCB_NAME`.
pub fn query_user_token(session_id: u32) -> Result<OwnedHandle> {
    let mut token = HANDLE::default();
    // SAFETY: при успехе WinAPI записывает owned handle в выходной `token`.
    unsafe { WTSQueryUserToken(session_id, &mut token) }.map_err(|source| {
        PlatformError::WindowsApi {
            api: "WTSQueryUserToken",
            source,
        }
    })?;

    // SAFETY: token получен успешным вызовом и ещё никому не принадлежит.
    Ok(unsafe { OwnedHandle::from_raw(token) })
}

/// Создаёт environment block пользователя.
pub fn create_environment_block(user_token: &OwnedHandle) -> Result<EnvironmentBlock> {
    let mut env_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
    // SAFETY: функция выделяет `env_ptr`, владение сразу принимает RAII-обёртка.
    unsafe { CreateEnvironmentBlock(&mut env_ptr, Some(user_token.raw()), false) }.map_err(
        |source| PlatformError::WindowsApi {
            api: "CreateEnvironmentBlock",
            source,
        },
    )?;

    // SAFETY: env_ptr получен успешным вызовом CreateEnvironmentBlock.
    Ok(unsafe { EnvironmentBlock::from_raw(env_ptr) })
}

/// Процесс в пользовательской session с PID и owned handle.
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

/// Запускает executable в интерактивном desktop пользователя session.
pub fn launch_in_session(
    session_id: u32,
    executable: &Path,
    args: &[OsString],
) -> Result<LaunchedProcess> {
    let user_token = query_user_token(session_id)?;
    let env_block = create_environment_block(&user_token)?;

    // Формируем quoted command line без секретов.
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

    // SAFETY: все указатели живы на время вызова. При успехе полученные
    // process/thread handles переходят во владение вызывающего кода.
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

    // SAFETY: hThread получен при успехе и больше не нужен, поэтому закрывается.
    let thread_handle = unsafe { OwnedHandle::from_raw(process_information.hThread) };
    drop(thread_handle);

    // SAFETY: hProcess получен тем же успешным вызовом.
    let process_handle = unsafe { OwnedHandle::from_raw(process_information.hProcess) };

    Ok(LaunchedProcess {
        pid: process_information.dwProcessId,
        process_handle,
    })
}

/// Проверяет через `STILL_ACTIVE`, продолжает ли процесс работу.
pub fn is_process_alive(process_handle: &OwnedHandle) -> Result<bool> {
    let mut exit_code: u32 = 0;
    // SAFETY: process handle открыт и жив на время вызова.
    unsafe { GetExitCodeProcess(process_handle.raw(), &mut exit_code) }.map_err(|source| {
        PlatformError::WindowsApi {
            api: "GetExitCodeProcess",
            source,
        }
    })?;
    Ok(exit_code == STILL_ACTIVE.0 as u32)
}

/// Завершает только процесс, handle которого получен из `launch_in_session`.
pub fn terminate_process(process_handle: &OwnedHandle) -> Result<()> {
    // SAFETY: process handle открыт и жив на время вызова.
    unsafe { TerminateProcess(process_handle.raw(), 1) }.map_err(|source| {
        PlatformError::WindowsApi {
            api: "TerminateProcess",
            source,
        }
    })
}

#[cfg(test)]
mod tests {
    // Эти Win32-вызовы требуют реальную Windows, LocalSystem и активную
    // session. Бизнес-поведение отдельно проверяется через supervisor и mock.
}
