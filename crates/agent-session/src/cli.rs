//! CLI `classos-session` (спека §39, §126), переносимый между ОС.

use clap::Parser;

#[derive(Parser, Debug, Clone, PartialEq, Eq)]
#[command(name = "classos-session", about = "ClassOS Session Host")]
pub struct Cli {
    /// Windows session id запуска. Service независимо проверяет его через
    /// WinAPI и не доверяет этому аргументу.
    #[arg(long = "session-id")]
    pub session_id: u32,

    /// Имя Named Pipe, созданное Service для этого запуска.
    #[arg(long = "pipe")]
    pub pipe: String,
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
        assert_eq!(cli.session_id, 1);
        assert_eq!(cli.pipe, r"\\.\pipe\classos\session-1-abc");
    }
}
