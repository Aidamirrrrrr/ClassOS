//! Независимая от транспорта идемпотентность и дедлайны classroom-команд.

use std::collections::HashMap;

use protocol::network::CommandResult;

pub const COMMAND_CACHE_TTL_MS: i64 = 15 * 60 * 1000;
const MAX_CACHED_COMMANDS: usize = 1_024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogApplication {
    VsCode,
    Chrome,
    Python,
}

impl CatalogApplication {
    pub fn executable(self) -> &'static str {
        match self {
            Self::VsCode => "Code.exe",
            Self::Chrome => "chrome.exe",
            Self::Python => "python.exe",
        }
    }
}

/// Catalog намеренно принимает только фиксированные идентификаторы, а не путь
/// или командную строку, пришедшие от Teacher Console.
pub fn catalog_application(application_id: &str) -> Option<CatalogApplication> {
    match application_id {
        "vscode" => Some(CatalogApplication::VsCode),
        "chrome" => Some(CatalogApplication::Chrome),
        "python" => Some(CatalogApplication::Python),
        _ => None,
    }
}

pub fn is_allowed_url(url: &str) -> bool {
    (url.starts_with("https://") || url.starts_with("http://"))
        && !url.chars().any(char::is_whitespace)
        && url.len() <= 2_048
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandAdmission {
    Execute,
    Replay(CommandResult),
    InProgress,
    Expired,
}

#[derive(Debug, Clone)]
enum CommandEntry {
    InProgress,
    Completed {
        result: CommandResult,
        completed_at_ms: i64,
    },
}

/// Ограниченный кэш защищает опасные действия от повторного выполнения после
/// reconnect. В нём сохраняется именно публичный результат первой попытки.
#[derive(Debug, Default)]
pub struct CommandDeduplicator {
    entries: HashMap<String, CommandEntry>,
}

impl CommandDeduplicator {
    pub fn admit(
        &mut self,
        command_id: &str,
        expires_at_unix_ms: i64,
        now_unix_ms: i64,
    ) -> CommandAdmission {
        self.purge(now_unix_ms);
        if command_id.is_empty() || expires_at_unix_ms <= now_unix_ms {
            return CommandAdmission::Expired;
        }
        match self.entries.get(command_id) {
            Some(CommandEntry::Completed { result, .. }) => {
                CommandAdmission::Replay(result.clone())
            }
            Some(CommandEntry::InProgress) => CommandAdmission::InProgress,
            None => {
                self.make_room();
                self.entries
                    .insert(command_id.to_owned(), CommandEntry::InProgress);
                CommandAdmission::Execute
            }
        }
    }

    pub fn complete(&mut self, command_id: &str, result: CommandResult, now_unix_ms: i64) {
        if matches!(self.entries.get(command_id), Some(CommandEntry::InProgress)) {
            self.entries.insert(
                command_id.to_owned(),
                CommandEntry::Completed {
                    result,
                    completed_at_ms: now_unix_ms,
                },
            );
        }
    }

    pub fn abandon(&mut self, command_id: &str) {
        if matches!(self.entries.get(command_id), Some(CommandEntry::InProgress)) {
            self.entries.remove(command_id);
        }
    }

    fn purge(&mut self, now_unix_ms: i64) {
        self.entries.retain(|_, entry| match entry {
            CommandEntry::InProgress => true,
            CommandEntry::Completed {
                completed_at_ms, ..
            } => now_unix_ms.saturating_sub(*completed_at_ms) <= COMMAND_CACHE_TTL_MS,
        });
    }

    fn make_room(&mut self) {
        if self.entries.len() < MAX_CACHED_COMMANDS {
            return;
        }
        if let Some(oldest) = self
            .entries
            .iter()
            .filter_map(|(id, entry)| match entry {
                CommandEntry::Completed {
                    completed_at_ms, ..
                } => Some((id.clone(), *completed_at_ms)),
                CommandEntry::InProgress => None,
            })
            .min_by_key(|(_, completed_at_ms)| *completed_at_ms)
            .map(|(id, _)| id)
        {
            self.entries.remove(&oldest);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(command_id: &str, success: bool) -> CommandResult {
        CommandResult {
            command_id: command_id.to_owned(),
            success,
            error_code: String::new(),
            message: "готово".to_owned(),
        }
    }

    #[test]
    fn completed_command_is_replayed_without_second_execution() {
        let mut cache = CommandDeduplicator::default();
        assert_eq!(
            cache.admit("command-1", 200, 100),
            CommandAdmission::Execute
        );
        cache.complete("command-1", result("command-1", true), 110);

        assert_eq!(
            cache.admit("command-1", 200, 120),
            CommandAdmission::Replay(result("command-1", true))
        );
    }

    #[test]
    fn expired_command_is_never_admitted() {
        let mut cache = CommandDeduplicator::default();
        assert_eq!(
            cache.admit("command-1", 100, 100),
            CommandAdmission::Expired
        );
        assert_eq!(cache.admit("", 200, 100), CommandAdmission::Expired);
    }

    #[test]
    fn simultaneous_duplicate_is_not_executed_twice() {
        let mut cache = CommandDeduplicator::default();
        assert_eq!(
            cache.admit("command-1", 200, 100),
            CommandAdmission::Execute
        );
        assert_eq!(
            cache.admit("command-1", 200, 101),
            CommandAdmission::InProgress
        );
        cache.abandon("command-1");
        assert_eq!(
            cache.admit("command-1", 200, 102),
            CommandAdmission::Execute
        );
    }

    #[test]
    fn catalog_never_resolves_arbitrary_executable_path() {
        assert_eq!(
            catalog_application("vscode"),
            Some(CatalogApplication::VsCode)
        );
        assert_eq!(catalog_application("C:\\Windows\\System32\\cmd.exe"), None);
        assert_eq!(catalog_application("powershell"), None);
    }

    #[test]
    fn only_http_urls_are_allowed() {
        assert!(is_allowed_url("https://classos.example/lesson"));
        assert!(is_allowed_url("http://intranet.local"));
        assert!(!is_allowed_url("file:///C:/secret.txt"));
        assert!(!is_allowed_url("https://example.test/has space"));
    }
}
