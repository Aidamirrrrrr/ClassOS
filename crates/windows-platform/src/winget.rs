//! Примитивы Windows Package Manager.
//!
//! Модуль принимает только идентификатор пакета и, при необходимости, версию.
//! Произвольная строка запроса, командная строка или путь установщика сюда не
//! передаются: approved catalog проверяется выше по стеку (spec T7 §6.2).

use std::process::Command;

use crate::{PlatformError, Result};

/// Аргументы, которые нужны каждой неинтерактивной операции winget.
const COMMON_ARGS: &[&str] = &[
    "--exact",
    "--disable-interactivity",
    "--accept-source-agreements",
];

fn run_winget(args: &[&str]) -> Result<String> {
    let output = Command::new("winget")
        .args(args)
        .output()
        .map_err(|error| PlatformError::Unexpected {
            api: "winget",
            reason: error.to_string(),
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        // winget сообщает "пакет не найден" ненулевым кодом; вызывающая
        // сторона отличает это по содержимому, а не по коду возврата.
        return Err(PlatformError::Unexpected {
            api: "winget",
            reason: if stderr.is_empty() {
                stdout.trim().to_owned()
            } else {
                stderr
            },
        });
    }
    Ok(stdout)
}

/// Область установки управляемых приложений.
///
/// Только машинная: приложение, установленное в профиль пользователя, лежит
/// в каталоге, доступном ему на запись, и потому не может быть разрешено
/// политикой (ADR-0017). Ставить его туда означало бы установить программу,
/// которую политика затем не запустит.
const MACHINE_SCOPE: &[&str] = &["--scope", "machine"];

/// Установленная версия пакета, если он есть на устройстве.
pub fn installed_version(package_id: &str) -> Result<Option<String>> {
    let mut args = vec!["list", "--id", package_id];
    args.extend_from_slice(COMMON_ARGS);
    match run_winget(&args) {
        Ok(output) => Ok(parse_installed_version(&output, package_id)),
        // Отсутствие пакета — нормальный ответ, а не сбой операции.
        Err(_) => Ok(None),
    }
}

/// Разбирает табличный вывод `winget list`.
///
/// Формат таблицы не является стабильным контрактом, поэтому разбор
/// намеренно консервативен: ищется строка с нужным идентификатором, версией
/// считается следующее за ним поле.
fn parse_installed_version(output: &str, package_id: &str) -> Option<String> {
    output
        .lines()
        .filter(|line| line.contains(package_id))
        .find_map(|line| {
            let mut fields = line.split_whitespace().skip_while(|f| *f != package_id);
            fields.next()?;
            fields
                .next()
                .filter(|value| value.chars().next().is_some_and(|c| c.is_ascii_digit()))
                .map(str::to_owned)
        })
}

/// Устанавливает пакет; при указанной версии ставится именно она.
pub fn install(package_id: &str, version: Option<&str>) -> Result<()> {
    let mut args = vec!["install", "--id", package_id];
    args.extend_from_slice(COMMON_ARGS);
    args.extend_from_slice(&["--silent", "--accept-package-agreements"]);
    args.extend_from_slice(MACHINE_SCOPE);
    if let Some(version) = version {
        args.extend_from_slice(&["--version", version]);
    }
    run_winget(&args).map(|_| ())
}

/// Приводит пакет к нужной версии: обновление или переустановка.
pub fn upgrade(package_id: &str, version: Option<&str>) -> Result<()> {
    let mut args = vec!["upgrade", "--id", package_id];
    args.extend_from_slice(COMMON_ARGS);
    args.extend_from_slice(&["--silent", "--accept-package-agreements"]);
    args.extend_from_slice(MACHINE_SCOPE);
    if let Some(version) = version {
        args.extend_from_slice(&["--version", version]);
    }
    run_winget(&args).map(|_| ())
}

/// Доступен ли winget на устройстве.
pub fn is_available() -> bool {
    Command::new("winget")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "Name               Id                          Version   Available\n\
------------------------------------------------------------------------\n\
Python 3.13        Python.Python.3.13          3.13.2\n";

    #[test]
    fn parses_version_from_list_output() {
        assert_eq!(
            parse_installed_version(SAMPLE, "Python.Python.3.13"),
            Some("3.13.2".to_owned())
        );
    }

    #[test]
    fn missing_package_yields_no_version() {
        assert_eq!(parse_installed_version(SAMPLE, "Git.Git"), None);
    }

    #[test]
    fn header_line_is_not_mistaken_for_a_version() {
        // Строка заголовка содержит слово Version, но не является записью.
        let output = "Name Id Version\n";
        assert_eq!(parse_installed_version(output, "Id"), None);
    }
}
