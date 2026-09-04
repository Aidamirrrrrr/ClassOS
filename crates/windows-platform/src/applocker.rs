//! Примитивы AppLocker: генерация XML, чтение, проверка и применение политики.
//!
//! Модуль не знает продуктовых понятий и работает со списками имён
//! исполняемых файлов. Выбор AppLocker как механизма зафиксирован в ADR-0014.
//!
//! Модель правил:
//!
//! * если задан хотя бы один разрешённый файл, строится **allowlist**: всё, что
//!   не разрешено явно, запрещается самим AppLocker (непустая коллекция правил
//!   запрещает остальное). Отдельные deny-правила при этом не нужны;
//! * если разрешающих правил нет, а запретить что-то нужно, строится
//!   allow-all + точечные deny.
//!
//! Локальные администраторы всегда получают отдельное разрешающее правило,
//! чтобы ошибочная политика не отняла машину у администратора (spec T6 §9).
//! В варианте allow-all + deny это правило не спасает от deny (в AppLocker
//! запрет сильнее разрешения) — восстановление в таком случае выполняется
//! через `classos-service.exe recover-policy`, который разрешён всегда.

use std::path::Path;
use std::process::Command;

use crate::{PlatformError, Result};

/// SID группы "Все".
const EVERYONE_SID: &str = "S-1-1-0";
/// SID локальной группы администраторов.
const ADMINISTRATORS_SID: &str = "S-1-5-32-544";

/// Решение AppLocker по конкретному файлу.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyDecision {
    Allowed,
    Denied,
    Unknown,
}

/// Детерминированный идентификатор правила: одинаковая политика даёт побайтово
/// одинаковый XML, поэтому применение идемпотентно.
fn rule_id(index: usize) -> String {
    format!("8f3c0a11-0000-4000-8000-{index:012}")
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Каталоги, недоступные standard-user на запись.
///
/// Разрешать исполняемый файл где угодно нельзя: пользователь положит
/// переименованную программу в свой профиль и обойдёт allowlist, не имея
/// никаких привилегий (ADR-0017). `%PROGRAMFILES%` в AppLocker покрывает и
/// 32-битный каталог.
const ALLOWED_ROOTS: &[&str] = &["%PROGRAMFILES%", "%WINDIR%"];

fn rule(index: usize, name: &str, sid: &str, action: &str, path: &str) -> String {
    format!(
        concat!(
            r#"<FilePathRule Id="{id}" Name="{action} {name}" Description="ClassOS" "#,
            r#"UserOrGroupSid="{sid}" Action="{action}">"#,
            r#"<Conditions><FilePathCondition Path="{path}" /></Conditions>"#,
            r#"</FilePathRule>"#
        ),
        id = rule_id(index),
        action = action,
        name = escape_xml(name),
        sid = sid,
        path = escape_xml(path),
    )
}

/// Разрешающие правила: имя файла в каждом из доверенных каталогов.
///
/// Возвращает по правилу на каталог, поэтому вызывающий обязан увеличивать
/// индекс на их число.
fn allow_rules(index: usize, name: &str, sid: &str) -> (String, usize) {
    let mut xml = String::new();
    for (offset, root) in ALLOWED_ROOTS.iter().enumerate() {
        let path = format!(r"{root}\*\{name}");
        xml.push_str(&rule(index + offset, name, sid, "Allow", &path));
    }
    (xml, ALLOWED_ROOTS.len())
}

/// Запрещающее правило: запрет обязан действовать в любом каталоге, иначе он
/// обходится переносом файла (ADR-0017).
fn deny_rule(index: usize, name: &str, sid: &str) -> String {
    rule(index, name, sid, "Deny", &format!(r"*\{name}"))
}

fn any_path_rule(index: usize, sid: &str) -> String {
    format!(
        concat!(
            r#"<FilePathRule Id="{id}" Name="Allow any" Description="ClassOS" "#,
            r#"UserOrGroupSid="{sid}" Action="Allow">"#,
            r#"<Conditions><FilePathCondition Path="*" /></Conditions>"#,
            r#"</FilePathRule>"#
        ),
        id = rule_id(index),
        sid = sid,
    )
}

/// Строит XML политики AppLocker для коллекции Exe.
///
/// `allowed` и `denied` — имена файлов (`Code.exe`), не пути. Разрешение
/// действует только в каталогах из [`ALLOWED_ROOTS`], запрет — везде
/// (ADR-0017): именно эта асимметрия делает allowlist невозможным обойти
/// переименованием файла в собственном профиле.
pub fn build_policy_xml(allowed: &[String], denied: &[String]) -> String {
    let mut rules = String::new();
    let mut index = 0;

    // Администратор не должен терять машину из-за ошибки в политике.
    rules.push_str(&any_path_rule(index, ADMINISTRATORS_SID));
    index += 1;

    if allowed.is_empty() {
        // Запрещать нечего в allowlist-смысле: разрешаем всё и точечно
        // запрещаем перечисленное.
        rules.push_str(&any_path_rule(index, EVERYONE_SID));
        index += 1;
    } else {
        for name in allowed {
            let (xml, count) = allow_rules(index, name, EVERYONE_SID);
            rules.push_str(&xml);
            index += count;
        }
    }

    for name in denied {
        rules.push_str(&deny_rule(index, name, EVERYONE_SID));
        index += 1;
    }

    format!(
        concat!(
            r#"<AppLockerPolicy Version="1">"#,
            r#"<RuleCollection Type="Exe" EnforcementMode="Enabled">{rules}</RuleCollection>"#,
            r#"</AppLockerPolicy>"#
        ),
        rules = rules
    )
}

/// Пустая политика: используется, когда снимок предыдущего состояния показал,
/// что политики не было вовсе.
pub fn empty_policy_xml() -> String {
    concat!(
        r#"<AppLockerPolicy Version="1">"#,
        r#"<RuleCollection Type="Exe" EnforcementMode="NotConfigured" />"#,
        r#"</AppLockerPolicy>"#
    )
    .to_owned()
}

fn quote_powershell(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn run_powershell(script: &str) -> Result<String> {
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .output()
        .map_err(|error| PlatformError::Unexpected {
            api: "powershell.exe",
            reason: error.to_string(),
        })?;
    if !output.status.success() {
        return Err(PlatformError::Unexpected {
            api: "powershell.exe",
            reason: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Текущая локальная политика AppLocker в виде XML — основа snapshot.
pub fn current_policy_xml() -> Result<String> {
    run_powershell("Get-AppLockerPolicy -Local -Xml")
}

/// Применяет политику из файла. XML передаётся файлом, а не строкой в команде:
/// так содержимое политики не проходит через разбор командной строки.
pub fn set_policy_from_file(path: &Path) -> Result<()> {
    let script = format!(
        "Set-AppLockerPolicy -XmlPolicy {} -ErrorAction Stop",
        quote_powershell(&path.display().to_string())
    );
    run_powershell(&script).map(|_| ())
}

/// Проверяет решение политики из файла по конкретному исполняемому файлу.
/// Это шаг Validate: политика проверяется **до** применения (spec T6 §6).
pub fn test_policy_file(policy_path: &Path, executable: &Path) -> Result<PolicyDecision> {
    let script = format!(
        "(Test-AppLockerPolicy -XmlPolicy {} -Path {} -User Everyone -ErrorAction Stop).PolicyDecision",
        quote_powershell(&policy_path.display().to_string()),
        quote_powershell(&executable.display().to_string()),
    );
    Ok(parse_decision(&run_powershell(&script)?))
}

/// Проверяет решение действующей политики устройства — шаг Verify после Apply.
pub fn test_effective_policy(executable: &Path) -> Result<PolicyDecision> {
    let script = format!(
        "(Test-AppLockerPolicy -Path {} -User Everyone -ErrorAction Stop).PolicyDecision",
        quote_powershell(&executable.display().to_string()),
    );
    Ok(parse_decision(&run_powershell(&script)?))
}

fn parse_decision(value: &str) -> PolicyDecision {
    match value.trim() {
        "Allowed" => PolicyDecision::Allowed,
        "Denied" | "DeniedByDefault" => PolicyDecision::Denied,
        _ => PolicyDecision::Unknown,
    }
}

/// Работает ли служба Application Identity, без которой AppLocker не
/// применяется. Отсутствие службы — причина честно отказать в enforcement,
/// а не создавать видимость защиты (ADR-0014).
pub fn application_identity_running() -> Result<bool> {
    let status = run_powershell(
        "(Get-Service -Name AppIDSvc -ErrorAction SilentlyContinue).Status -as [string]",
    )?;
    Ok(status.trim().eq_ignore_ascii_case("Running"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_policy_contains_admin_escape_and_allowed_files() {
        let xml = build_policy_xml(&["Code.exe".to_owned()], &[]);
        assert!(xml.contains(ADMINISTRATORS_SID));
        assert!(xml.contains(r"%PROGRAMFILES%\*\Code.exe"));
        assert!(xml.contains(r#"Action="Allow""#));
        // В allowlist-модели deny-правила не нужны и не должны появляться.
        assert!(!xml.contains(r#"Action="Deny""#));
    }

    /// Главная проверка T6: allowlist не должен разрешать файл с нужным
    /// именем где угодно. Иначе ученик переименовывает программу в
    /// `Code.exe`, кладёт в свой профиль и обходит Focus Mode без единой
    /// привилегии (ADR-0017).
    #[test]
    fn allow_rules_never_match_a_user_writable_location() {
        let xml = build_policy_xml(&["Code.exe".to_owned()], &[]);
        assert!(
            !xml.contains(r#"Path="*\Code.exe""#),
            "разрешение по голому имени файла обходится переименованием"
        );
        for root in ALLOWED_ROOTS {
            assert!(xml.contains(&format!(r"{root}\*\Code.exe")));
        }
    }

    /// Запрет, наоборот, обязан действовать в любом каталоге: иначе он
    /// обходится переносом файла.
    #[test]
    fn deny_rules_still_match_every_location() {
        let xml = build_policy_xml(&["Code.exe".to_owned()], &["cmd.exe".to_owned()]);
        assert!(xml.contains(r#"Path="*\cmd.exe""#));
    }

    #[test]
    fn policy_without_allowlist_falls_back_to_allow_all_plus_deny() {
        let xml = build_policy_xml(&[], &["cmd.exe".to_owned()]);
        assert!(xml.contains(r#"<FilePathCondition Path="*" />"#));
        assert!(xml.contains(r"*\cmd.exe"));
        assert!(xml.contains(r#"Action="Deny""#));
    }

    #[test]
    fn rule_ids_are_deterministic_and_unique() {
        let xml = build_policy_xml(&["a.exe".to_owned(), "b.exe".to_owned()], &[]);
        assert_eq!(
            xml,
            build_policy_xml(&["a.exe".to_owned(), "b.exe".to_owned()], &[])
        );
        // Правило администратора плюс по одному разрешению на каждый
        // доверенный каталог для каждого файла.
        let expected = 1 + 2 * ALLOWED_ROOTS.len();
        assert!(xml.contains(&rule_id(0)) && xml.contains(&rule_id(expected - 1)));
        assert!(!xml.contains(&rule_id(expected)));
    }

    #[test]
    fn file_names_are_xml_escaped() {
        let xml = build_policy_xml(&[r#"we"ird.exe"#.to_owned()], &[]);
        assert!(xml.contains("we&quot;ird.exe"));
        assert!(!xml.contains(r#"we"ird.exe"#));
    }

    #[test]
    fn powershell_quoting_escapes_single_quote() {
        assert_eq!(quote_powershell(r"C:\it's\p.xml"), r"'C:\it''s\p.xml'");
    }

    #[test]
    fn decision_parsing_treats_default_denial_as_denied() {
        assert_eq!(parse_decision("Allowed"), PolicyDecision::Allowed);
        assert_eq!(parse_decision("DeniedByDefault"), PolicyDecision::Denied);
        assert_eq!(parse_decision(""), PolicyDecision::Unknown);
    }
}
