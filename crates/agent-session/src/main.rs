//! `classos-session` binary entry point. Real logic only exists on
//! Windows; on other hosts this prints a message and exits so that
//! `cargo build`/`cargo run` still succeed on non-Windows development
//! machines.

#[cfg(windows)]
mod ipc_client;
#[cfg(windows)]
mod runtime;

#[cfg(windows)]
fn main() {
    use agent_session::cli::Cli;
    use clap::Parser;

    let cli = Cli::parse();

    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(err) => {
            eprintln!("failed to start tokio runtime: {err}");
            std::process::exit(1);
        }
    };

    if let Err(err) = rt.block_on(runtime::run(cli.session_id, &cli.pipe)) {
        tracing::error!(error = %err, "session host runtime exited with error");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("classos-session only runs on Windows.");
    std::process::exit(1);
}
