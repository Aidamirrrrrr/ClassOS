//! Точка входа бинарника `classos-service`. Рабочая реализация существует
//! только для Windows; на других ОС программа печатает сообщение и выходит,
//! чтобы `cargo build` и `cargo run` оставались работоспособными.

#[cfg(windows)]
mod identity_store;
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
            // В режиме разработки SCM нет. Сразу закрываем отправителя,
            // а runtime продолжает работу по периодическому reconcile.
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
                None,
            ));
        }
        Command::Service => {
            // Храним guard до завершения SCM dispatcher, чтобы при остановке
            // успели записаться все сообщения из очереди.
            let _logging_guard = init_service_logging();
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
fn init_service_logging() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    // Храним семь ежедневных файлов: журнал не растёт бесконечно, но истории
    // достаточно для первичной диагностики (спека §81).
    let log_dir = agent_core::config::log_dir();
    if let Err(err) = std::fs::create_dir_all(&log_dir) {
        // Файловый журнал ещё недоступен. Используем stderr и не завершаем
        // службу аварийно только из-за невозможности создать каталог.
        eprintln!(
            "failed to create log directory {}: {err}",
            log_dir.display()
        );
        return None;
    }
    let config = match agent_core::config::AgentConfig::load() {
        Ok(config) => config,
        Err(err) => {
            eprintln!("failed to load agent config; using defaults: {err}");
            agent_core::config::AgentConfig::default()
        }
    };
    let appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("service.log")
        .max_log_files(7)
        .build(&log_dir);

    match appender {
        Ok(appender) => {
            let (writer, guard) = tracing_appender::non_blocking(appender);
            let _ = tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                        tracing_subscriber::EnvFilter::new(config.log_level.clone())
                    }),
                )
                .with_writer(writer)
                .with_ansi(false)
                .try_init();
            tracing::info!(event = "SERVICE_STARTING");
            Some(guard)
        }
        Err(err) => {
            eprintln!(
                "failed to initialize logging in {}: {err}",
                log_dir.display()
            );
            None
        }
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("classos-service only runs on Windows.");
    std::process::exit(1);
}
