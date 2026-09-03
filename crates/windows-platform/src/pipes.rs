//! Примитивы Named Pipe. Модуль создаёт защищённый ACL server handle и
//! определяет PID клиента. Асинхронный transport подключает вызывающий код,
//! чтобы `windows-platform` не зависел от конкретного runtime.

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

/// Буферы IPC больше максимального frame, чтобы избежать лишнего дробления.
const PIPE_BUFFER_SIZE: u32 = 128 * 1024;

/// Создаёт уникальное имя канала с session id и instance id.
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

/// Создаёт Named Pipe с явным ACL для SYSTEM и `user_sid`. Флаг
/// `FILE_FLAG_OVERLAPPED` позволяет обернуть handle в async I/O.
pub fn create_pipe_server(
    pipe_name: &str,
    user_sid: &str,
) -> Result<(OwnedHandle, PipeSecurityDescriptor)> {
    let descriptor = PipeSecurityDescriptor::for_session_user(user_sid)?;
    let security_attributes = descriptor.as_security_attributes();

    let mut wide_name = to_wide_null(pipe_name);

    // SAFETY: `wide_name` — живая NUL-terminated строка, а descriptor живёт
    // дольше вызова и возвращается вместе с handle.
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

    // SAFETY: handle получен успешным вызовом CreateNamedPipeW.
    let owned = unsafe { OwnedHandle::from_raw(handle) };
    Ok((owned, descriptor))
}

/// Определяет PID процесса, подключённого к `pipe_handle`. Принимает
/// заимствованный raw HANDLE без передачи владения.
///
/// # Safety
/// `pipe_handle` должен быть действительным открытым server handle.
pub unsafe fn client_process_id(pipe_handle: HANDLE) -> Result<u32> {
    let mut pid: u32 = 0;
    // SAFETY: вызывающий гарантирует действительность handle.
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
