//! Точка входа бинарника `classos-session`. Рабочая реализация существует
//! только для Windows; заглушка для других ОС сохраняет переносимость сборки.

#[cfg(windows)]
mod ipc_client;
#[cfg(windows)]
mod runtime;

#[cfg(windows)]
fn main() {
    use agent_session::cli::Cli;
    use clap::Parser;

    let cli = Cli::parse();

    let log_dir = agent_core::config::log_dir();
    let _ = std::fs::create_dir_all(&log_dir);
    let appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix(format!("session-{}.log", cli.session_id))
        .max_log_files(7)
        .build(&log_dir);
    let _logging_guard = match appender {
        Ok(appender) => {
            let (writer, guard) = tracing_appender::non_blocking(appender);
            let _ = tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
                )
                .with_writer(writer)
                .with_ansi(false)
                .try_init();
            Some(guard)
        }
        Err(err) => {
            eprintln!("failed to initialize session logging: {err}");
            let _ = tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
                )
                .try_init();
            None
        }
    };

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
