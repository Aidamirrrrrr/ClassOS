//! Защита небольших секретов через Windows DPAPI в machine scope.

use std::ptr;

use windows::Win32::Foundation::{HLOCAL, LocalFree};
use windows::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_LOCAL_MACHINE, CryptProtectData, CryptUnprotectData,
};

use crate::error::{PlatformError, Result};

/// Шифрует секрет с привязкой к текущей Windows-машине.
pub fn protect_machine_secret(secret: &[u8]) -> Result<Vec<u8>> {
    transform("CryptProtectData", secret, true)
}

/// Расшифровывает ранее защищённый на этой машине секрет.
pub fn unprotect_machine_secret(protected: &[u8]) -> Result<Vec<u8>> {
    transform("CryptUnprotectData", protected, false)
}

fn transform(api: &'static str, input: &[u8], protect: bool) -> Result<Vec<u8>> {
    let input_len = u32::try_from(input.len()).map_err(|_| PlatformError::Unexpected {
        api,
        reason: "входной буфер DPAPI превышает u32".to_owned(),
    })?;
    let input_blob = CRYPT_INTEGER_BLOB {
        cbData: input_len,
        pbData: input.as_ptr().cast_mut(),
    };
    let mut output_blob = CRYPT_INTEGER_BLOB::default();

    // SAFETY: входной slice остаётся жив на время вызова, output_blob передан
    // как доступная для записи структура. Выделенную DPAPI память копируем и
    // освобождаем через LocalFree ниже.
    let result = unsafe {
        if protect {
            CryptProtectData(
                &input_blob,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_LOCAL_MACHINE,
                &mut output_blob,
            )
        } else {
            CryptUnprotectData(&input_blob, None, None, None, None, 0, &mut output_blob)
        }
    };
    result.map_err(|source| PlatformError::WindowsApi { api, source })?;

    if output_blob.cbData > 0 && output_blob.pbData.is_null() {
        return Err(PlatformError::Unexpected {
            api,
            reason: "DPAPI вернул нулевой указатель для непустого результата".to_owned(),
        });
    }

    // SAFETY: успешный DPAPI вызов возвращает буфер длиной cbData, которым до
    // LocalFree владеет вызывающая сторона.
    let output = if output_blob.cbData == 0 {
        Vec::new()
    } else {
        // SAFETY: непустой указатель проверен выше; успешный DPAPI вызов
        // возвращает доступный буфер длиной cbData.
        unsafe {
            std::slice::from_raw_parts(output_blob.pbData, output_blob.cbData as usize).to_vec()
        }
    };

    // Расшифрованные данные зануляются в системном буфере перед освобождением.
    if !protect && !output_blob.pbData.is_null() {
        // SAFETY: тот же валидный DPAPI buffer, ещё не переданный LocalFree.
        unsafe { ptr::write_bytes(output_blob.pbData, 0, output_blob.cbData as usize) };
    }
    if !output_blob.pbData.is_null() {
        // SAFETY: DPAPI документирует освобождение pdataout через LocalFree.
        unsafe {
            let _ = LocalFree(Some(HLOCAL(output_blob.pbData.cast())));
        }
    }
    Ok(output)
}
