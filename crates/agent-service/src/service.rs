//! Интеграция с Windows Service Control Manager (спека §16-22). Callback
//! SCM не блокируется и не запускает процессы или IPC: он только передаёт
//! `ServiceEvent` в канал Tokio runtime.

use std::ffi::OsString;
use std::time::Duration;

use agent_core::supervisor::ServiceEvent;
use tokio::sync::mpsc as tokio_mpsc;
use windows_service::service::{
    ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::{define_windows_service, service_dispatcher};

pub const SERVICE_NAME: &str = "ClassOSAgent";

define_windows_service!(ffi_service_main, service_main_entry);

/// Точка входа, которую dispatcher `windows-service` вызывает в потоке SCM.
/// Регистрирует обработчик и запускает Tokio runtime на всё время службы.
fn service_main_entry(_arguments: Vec<OsString>) {
    if let Err(err) = run_service() {
        tracing::error!(error = %err, "service run failed");
    }
}

fn run_service() -> windows_service::Result<()> {
    let (events_tx, events_rx) = tokio_mpsc::unbounded_channel::<ServiceEvent>();

    let event_handler = {
        let events_tx = events_tx.clone();
        move |control_event: ServiceControl| -> ServiceControlHandlerResult {
            match control_event {
                ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
                ServiceControl::Stop => {
                    let _ = events_tx.send(ServiceEvent::Stop);
                    ServiceControlHandlerResult::NoError
                }
                ServiceControl::Shutdown => {
                    let _ = events_tx.send(ServiceEvent::Shutdown);
                    ServiceControlHandlerResult::NoError
                }
                ServiceControl::SessionChange(param) => {
                    use windows_service::service::SessionChangeReason as R;
                    let session_id = param.notification.session_id;
                    let event = match param.reason {
                        R::SessionLogon => Some(ServiceEvent::SessionLogon(session_id)),
                        R::SessionLogoff => Some(ServiceEvent::SessionLogoff(session_id)),
                        R::SessionLock => Some(ServiceEvent::SessionLock(session_id)),
                        R::SessionUnlock => Some(ServiceEvent::SessionUnlock(session_id)),
                        R::ConsoleConnect => Some(ServiceEvent::ConsoleConnect(session_id)),
                        R::ConsoleDisconnect => Some(ServiceEvent::ConsoleDisconnect(session_id)),
                        _ => None,
                    };
                    if let Some(event) = event {
                        let _ = events_tx.send(event);
                    }
                    ServiceControlHandlerResult::NoError
                }
                _ => ServiceControlHandlerResult::NotImplemented,
            }
        }
    };

    let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)?;

    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP
            | ServiceControlAccept::SHUTDOWN
            | ServiceControlAccept::SESSION_CHANGE,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    })?;

    tracing::info!(event = "SERVICE_RUNNING");

    // Запускаем Tokio runtime в этом потоке до остановки службы.
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(err) => {
            tracing::error!(error = %err, "failed to create tokio runtime");
            let _ = status_handle.set_service_status(ServiceStatus {
                service_type: ServiceType::OWN_PROCESS,
                current_state: ServiceState::Stopped,
                controls_accepted: ServiceControlAccept::empty(),
                exit_code: ServiceExitCode::ServiceSpecific(1),
                checkpoint: 0,
                wait_hint: Duration::default(),
                process_id: None,
            });
            return Ok(());
        }
    };

    rt.block_on(crate::runtime::run(
        crate::windows_adapters::LaunchMode::Privileged,
        events_rx,
        Some(status_handle),
    ));

    tracing::info!(event = "SERVICE_STOPPED");
    let _ = status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    });

    Ok(())
}

/// Запускает dispatcher SCM и блокирует поток до остановки службы. Функцию
/// вызывает SCM через `classos-service.exe service`, не пользователь.
pub fn start_dispatcher() -> windows_service::Result<()> {
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)
}
