//! Замена бинарников агента на Windows и проверка работоспособности службы.
//!
//! Отдельный процесс `classos-updater.exe`: служба не заменяет себя вживую
//! (spec T8 §8.4).

use std::path::{Path, PathBuf};
use std::time::Duration;

use windows_platform::service_control::{self, ServiceState};

use crate::{BinaryStore, HealthCheck, UpdateError};

/// Файлы, которые обновляются вместе.
const MANAGED_BINARIES: &[&str] = &["classos-service.exe", "classos-session.exe"];

/// Сколько ждём остановки и запуска службы.
const SERVICE_TIMEOUT: Duration = Duration::from_secs(60);

pub struct WindowsBinaryStore {
    install_dir: PathBuf,
    backup_dir: PathBuf,
}

impl WindowsBinaryStore {
    pub fn new(install_dir: PathBuf, backup_dir: PathBuf) -> Self {
        Self {
            install_dir,
            backup_dir,
        }
    }

    fn copy_all(from: &Path, to: &Path) -> Result<(), UpdateError> {
        std::fs::create_dir_all(to).map_err(|error| UpdateError::Install(error.to_string()))?;
        for name in MANAGED_BINARIES {
            let source = from.join(name);
            if !source.exists() {
                continue;
            }
            std::fs::copy(&source, to.join(name))
                .map_err(|error| UpdateError::Install(format!("{name}: {error}")))?;
        }
        Ok(())
    }

    /// Останавливает службу перед заменой файлов.
    ///
    /// Если служба не остановилась, замена не начинается: заменить файл
    /// работающей службы на Windows всё равно нельзя, а частично применённое
    /// обновление хуже неприменённого.
    fn stop_service() -> Result<(), UpdateError> {
        service_control::stop().map_err(|error| UpdateError::Install(error.to_string()))?;
        let stopped = service_control::wait_for(ServiceState::Stopped, SERVICE_TIMEOUT)
            .map_err(|error| UpdateError::Install(error.to_string()))?;
        if stopped {
            Ok(())
        } else {
            Err(UpdateError::Install(
                "служба не остановилась в отведённое время".to_owned(),
            ))
        }
    }

    fn start_service() -> Result<(), UpdateError> {
        service_control::start().map_err(|error| UpdateError::Install(error.to_string()))?;
        Ok(())
    }
}

impl BinaryStore for WindowsBinaryStore {
    fn backup(&self) -> Result<(), UpdateError> {
        Self::copy_all(&self.install_dir, &self.backup_dir)
    }

    fn install(&self, payload: &[u8]) -> Result<(), UpdateError> {
        Self::stop_service()?;
        // Полезная нагрузка T8 — самораспаковывающийся набор бинарников;
        // сейчас поддерживается один файл службы, распаковка архива войдёт
        // вместе с реальным каналом публикации.
        std::fs::write(self.install_dir.join(MANAGED_BINARIES[0]), payload)
            .map_err(|error| UpdateError::Install(error.to_string()))?;
        Self::start_service()
    }

    fn restore(&self) -> Result<(), UpdateError> {
        // Откат обязан работать и когда служба уже запущена на сломанной
        // версии: сначала останавливаем, потом возвращаем файлы.
        let _ = Self::stop_service();
        Self::copy_all(&self.backup_dir, &self.install_dir)
            .map_err(|error| UpdateError::Rollback(error.to_string()))?;
        Self::start_service().map_err(|error| UpdateError::Rollback(error.to_string()))
    }
}

/// Health check после обновления: служба обязана дойти до RUNNING и удержаться.
pub struct ServiceHealthCheck {
    settle: Duration,
}

impl Default for ServiceHealthCheck {
    fn default() -> Self {
        Self {
            settle: Duration::from_secs(15),
        }
    }
}

impl HealthCheck for ServiceHealthCheck {
    fn is_healthy(&self) -> Result<(), String> {
        let running = service_control::wait_for(ServiceState::Running, SERVICE_TIMEOUT)
            .map_err(|error| error.to_string())?;
        if !running {
            return Err("служба не перешла в состояние RUNNING".to_owned());
        }
        // Служба, падающая через несколько секунд после старта, тоже
        // считается неисправной: SCM успел бы отчитаться об успехе.
        std::thread::sleep(self.settle);
        match service_control::query_state().map_err(|error| error.to_string())? {
            ServiceState::Running => Ok(()),
            other => Err(format!("служба вышла из RUNNING: {other:?}")),
        }
    }
}
