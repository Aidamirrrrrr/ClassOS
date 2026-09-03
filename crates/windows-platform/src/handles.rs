//! RAII-обёртки для Win32 handles и ресурсов с ручным освобождением.
//! Raw HANDLE и указатели не должны покидать модуль без обёртки.

use std::ffi::c_void;

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Environment::DestroyEnvironmentBlock;

/// Владеющий Win32 `HANDLE`, закрываемый через `CloseHandle`. `Clone`
/// намеренно отсутствует: копирование требует явного `DuplicateHandle`.
#[derive(Debug)]
pub struct OwnedHandle(HANDLE);

// SAFETY: HANDLE — непрозрачный идентификатор, который можно перемещать
// между потоками. Используемые операции Win32 потокобезопасны.
unsafe impl Send for OwnedHandle {}
unsafe impl Sync for OwnedHandle {}

impl OwnedHandle {
    /// Принимает владение действительным открытым raw handle.
    ///
    /// # Safety
    /// `handle` должен быть открыт и не должен закрываться другим владельцем.
    pub unsafe fn from_raw(handle: HANDLE) -> Self {
        Self(handle)
    }

    pub fn raw(&self) -> HANDLE {
        self.0
    }

    /// Проверяет null или invalid handle.
    pub fn is_invalid(&self) -> bool {
        self.0.is_invalid()
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            // SAFETY: `self.0` — принадлежащий объекту открытый handle;
            // перечисленные WinAPI требуют освобождения через CloseHandle.
            let _ = unsafe { CloseHandle(self.0) };
        }
    }
}

/// RAII-обёртка environment block. Освобождает его через
/// `DestroyEnvironmentBlock` и передаёт в `CreateProcessAsUserW`.
pub struct EnvironmentBlock(*mut c_void);

// SAFETY: блок только читается `CreateProcessAsUserW` при живой обёртке;
// конкурентного доступа нет.
unsafe impl Send for EnvironmentBlock {}

impl EnvironmentBlock {
    /// # Safety
    /// `ptr` получен из `CreateEnvironmentBlock` и ещё не освобождён.
    pub unsafe fn from_raw(ptr: *mut c_void) -> Self {
        Self(ptr)
    }

    pub fn as_ptr(&self) -> *const c_void {
        self.0 as *const c_void
    }
}

impl Drop for EnvironmentBlock {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: указатель создан CreateEnvironmentBlock и имеет одного владельца.
            let _ = unsafe { DestroyEnvironmentBlock(self.0) };
        }
    }
}
