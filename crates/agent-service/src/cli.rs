//! Командный интерфейс `classos-service` (спека §12-15). Модуль не зависит
//! от Windows, поэтому разбор аргументов тестируется на любой ОС.

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "classos-service", about = "ClassOS Agent Service")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Запустить для разработки на переднем плане без LocalSystem (спека §13).
    Run,
    /// Запустить через Windows SCM от имени LocalSystem (спека §14).
    Service,
    /// Сохранить одноразовый enrollment-код.
    Enroll {
        #[arg(long)]
        code: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_run() {
        let cli = Cli::parse_from(["classos-service", "run"]);
        assert_eq!(cli.command, Command::Run);
    }

    #[test]
    fn parses_service() {
        let cli = Cli::parse_from(["classos-service", "service"]);
        assert_eq!(cli.command, Command::Service);
    }

    #[test]
    fn parses_enrollment_code() {
        let cli = Cli::parse_from(["classos-service", "enroll", "--code", "ABC123"]);
        assert_eq!(
            cli.command,
            Command::Enroll {
                code: "ABC123".to_owned()
            }
        );
    }
}
