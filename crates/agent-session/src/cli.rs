//! CLI `classos-session` (спека §39, §126), переносимый между ОС.

use clap::Parser;

#[derive(Parser, Debug, Clone, PartialEq, Eq)]
#[command(name = "classos-session", about = "ClassOS Session Host")]
pub struct Cli {
    /// Windows session id запуска. Service независимо проверяет его через
    /// WinAPI и не доверяет этому аргументу.
    #[arg(long = "session-id")]
    pub session_id: Option<u32>,

    /// Имя Named Pipe, созданное Service для этого запуска.
    #[arg(long = "pipe")]
    pub pipe: Option<String>,

    /// Диагностика захвата экрана: перечислить адаптеры и выходы, попытаться
    /// получить один кадр и напечатать результат.
    ///
    /// Существует ради выбора железа. Пригодность машины для T2–T4 решает не
    /// наличие видеокарты, а то, поддерживает ли Desktop Duplication тот
    /// адаптер, которому принадлежит рабочий стол. На арендованной машине это
    /// дешевле проверить одной командой, чем прогоном.
    #[arg(long = "check-capture", conflicts_with_all = ["session_id", "pipe"])]
    pub check_capture: bool,
}

/// Разобранный режим запуска.
pub enum Mode {
    /// Обычный запуск из Service.
    SessionHost { session_id: u32, pipe: String },
    /// Ручная диагностика захвата.
    CheckCapture,
}

impl Cli {
    /// Приводит флаги к режиму запуска.
    ///
    /// `--session-id` и `--pipe` объявлены необязательными только ради
    /// `--check-capture`; для запуска Session Host обязательны оба, и их
    /// отсутствие — ошибка вызова, а не повод угадывать значения.
    pub fn mode(self) -> Result<Mode, &'static str> {
        if self.check_capture {
            return Ok(Mode::CheckCapture);
        }
        match (self.session_id, self.pipe) {
            (Some(session_id), Some(pipe)) => Ok(Mode::SessionHost { session_id, pipe }),
            _ => Err("нужны одновременно --session-id и --pipe, либо --check-capture"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_session_id_and_pipe() {
        let cli = Cli::parse_from([
            "classos-session",
            "--session-id",
            "1",
            "--pipe",
            r"\\.\pipe\classos\session-1-abc",
        ]);
        assert_eq!(cli.session_id, Some(1));
        assert_eq!(cli.pipe, Some(r"\\.\pipe\classos\session-1-abc".to_owned()));
        assert!(!cli.check_capture);
        assert!(matches!(
            cli.mode(),
            Ok(Mode::SessionHost { session_id: 1, .. })
        ));
    }

    #[test]
    fn parses_capture_diagnostics() {
        let cli = Cli::parse_from(["classos-session", "--check-capture"]);
        assert!(cli.check_capture);
        assert!(matches!(cli.mode(), Ok(Mode::CheckCapture)));
    }

    /// Service запускает хост только с обоими аргументами. Одиночный
    /// `--session-id` не должен превращаться в запуск с догаданным pipe.
    #[test]
    fn half_of_session_host_arguments_is_an_error() {
        let cli = Cli::parse_from(["classos-session", "--session-id", "1"]);
        assert!(cli.mode().is_err());
    }
}
