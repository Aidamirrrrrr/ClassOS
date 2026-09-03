//! Command-line surface for `classos-service` (spec §12-15). Kept
//! host-portable (no Windows dependency) so argument parsing is
//! unit-testable on any host.

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "classos-service", about = "ClassOS Agent Service")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Run in the foreground as a normal (non-LocalSystem) process, for
    /// development (spec §13).
    Run,
    /// Run under the Windows Service Control Manager as LocalSystem
    /// (spec §14).
    Service,
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
}
