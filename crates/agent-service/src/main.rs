//! `classos-service` binary entry point. Real logic only exists on
//! Windows; on other hosts this prints a message and exits so that
//! `cargo build`/`cargo run` still succeed on non-Windows development
//! machines (see README-T0.md "Development on non-Windows hosts").

#[cfg(windows)]
mod ipc;
#[cfg(windows)]
mod runtime;
#[cfg(windows)]
mod service;
#[cfg(windows)]
mod windows_adapters;

#[cfg(windows)]
fn main() {
    use agent_service::cli::{Cli, Command};
    use clap::Parser;

    let cli = Cli::parse();

    match cli.command {
        Command::Run => {
            init_dev_logging();
            let (events_tx, events_rx) = tokio::sync::mpsc::unbounded_channel();
            // No SCM in dev mode: drop the sender immediately so
            // `service_events.recv()` in the runtime loop simply reports
            // the channel closed and the loop keeps running on the
            // reconcile tick alone (spec §12-13).
            drop(events_tx);

            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(err) => {
                    eprintln!("failed to start tokio runtime: {err}");
                    std::process::exit(1);
                }
            };
            rt.block_on(runtime::run(
                windows_adapters::LaunchMode::DevChildProcess,
                events_rx,
            ));
        }
        Command::Service => {
            init_service_logging();
            if let Err(err) = service::start_dispatcher() {
                tracing::error!(error = %err, "service dispatcher failed");
                std::process::exit(1);
            }
        }
    }
}

#[cfg(windows)]
fn init_dev_logging() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();
}

#[cfg(windows)]
fn init_service_logging() {
    // T0: service-mode logging still goes to a rolling file under
    // C:\ProgramData\ClassOS\logs (spec §79-81). A full rotation policy is
    // deferred; this opens/creates today's log file in append mode, which
    // is sufficient to observe service behavior across a reboot without
    // unbounded growth within a single run.
    let log_dir = agent_core::config::log_dir();
    if let Err(err) = std::fs::create_dir_all(&log_dir) {
        // Logging isn't available yet; there's nothing to log to. Fall
        // back to stderr, which the SCM discards, but this keeps the
        // service from panicking on a missing directory.
        eprintln!(
            "failed to create log directory {}: {err}",
            log_dir.display()
        );
        return;
    }
    let log_path = log_dir.join("service.log");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path);

    match file {
        Ok(file) => {
            let _ = tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
                )
                .with_writer(std::sync::Mutex::new(file))
                .with_ansi(false)
                .try_init();
        }
        Err(err) => {
            eprintln!("failed to open log file {}: {err}", log_path.display());
        }
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("classos-service only runs on Windows.");
    std::process::exit(1);
}
