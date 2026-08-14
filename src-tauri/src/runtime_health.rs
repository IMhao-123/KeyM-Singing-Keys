use serde::Serialize;
use std::sync::{Arc, Mutex};

/// 单个服务的运行时健康状态（AUD-004）。
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceStatus {
    Starting,
    Running,
    Recovering,
    PermissionDenied,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ServiceHealth {
    pub status: ServiceStatus,
    pub message: Option<String>,
}

impl ServiceHealth {
    fn starting() -> Self {
        Self {
            status: ServiceStatus::Starting,
            message: None,
        }
    }

    fn running() -> Self {
        Self {
            status: ServiceStatus::Running,
            message: None,
        }
    }

    fn failed(message: impl Into<String>) -> Self {
        Self {
            status: ServiceStatus::Failed,
            message: Some(message.into()),
        }
    }
}

/// 运行时健康快照（输入/音频/数据库 + 丢弃事件计数）。
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RuntimeHealthSnapshot {
    pub input: ServiceHealth,
    pub audio: ServiceHealth,
    pub database: ServiceHealth,
    pub dropped_input_events: u64,
}

/// 线程安全的运行时健康聚合器，供输入 worker、音频 worker 与 IPC 共享。
#[derive(Clone)]
pub struct RuntimeHealth {
    inner: Arc<Mutex<RuntimeHealthSnapshot>>,
}

impl RuntimeHealth {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(RuntimeHealthSnapshot {
                input: ServiceHealth::starting(),
                audio: ServiceHealth::starting(),
                database: ServiceHealth::running(),
                dropped_input_events: 0,
            })),
        }
    }

    pub fn snapshot(&self) -> RuntimeHealthSnapshot {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn set_input_running(&self) {
        self.update(|state| state.input = ServiceHealth::running());
    }

    pub fn set_input_permission_denied(&self, message: impl Into<String>) {
        self.update(|state| {
            state.input = ServiceHealth {
                status: ServiceStatus::PermissionDenied,
                message: Some(message.into()),
            };
        });
    }

    pub fn set_input_failed(&self, message: impl Into<String>) {
        self.update(|state| state.input = ServiceHealth::failed(message));
    }

    pub fn set_audio_running(&self) {
        self.update(|state| state.audio = ServiceHealth::running());
    }

    pub fn set_audio_recovering(&self, message: impl Into<String>) {
        self.update(|state| {
            state.audio = ServiceHealth {
                status: ServiceStatus::Recovering,
                message: Some(message.into()),
            };
        });
    }

    pub fn set_audio_failed(&self, message: impl Into<String>) {
        self.update(|state| state.audio = ServiceHealth::failed(message));
    }

    pub fn set_database_failed(&self, message: impl Into<String>) {
        self.update(|state| state.database = ServiceHealth::failed(message));
    }

    pub fn clear_database_error(&self) {
        self.update(|state| state.database = ServiceHealth::running());
    }

    pub fn record_dropped_input_event(&self) {
        self.update(|state| {
            state.dropped_input_events = state.dropped_input_events.saturating_add(1)
        });
    }

    fn update(&self, update: impl FnOnce(&mut RuntimeHealthSnapshot)) {
        let mut state = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        update(&mut state);
    }
}

impl Default for RuntimeHealth {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_failures_and_dropped_events_without_losing_other_services() {
        let health = RuntimeHealth::new();
        health.set_input_running();
        health.set_audio_failed("没有输出设备");
        health.record_dropped_input_event();
        health.record_dropped_input_event();

        let snapshot = health.snapshot();
        assert_eq!(snapshot.input.status, ServiceStatus::Running);
        assert_eq!(snapshot.audio.status, ServiceStatus::Failed);
        assert_eq!(snapshot.audio.message.as_deref(), Some("没有输出设备"));
        assert_eq!(snapshot.database.status, ServiceStatus::Running);
        assert_eq!(snapshot.dropped_input_events, 2);
    }

    #[test]
    fn distinguishes_permission_denied_from_generic_failure() {
        let health = RuntimeHealth::new();
        health.set_input_permission_denied("请在系统设置中授权输入监控");
        assert_eq!(
            health.snapshot().input.status,
            ServiceStatus::PermissionDenied
        );
    }

    #[test]
    fn database_error_can_be_cleared() {
        let health = RuntimeHealth::new();
        health.set_database_failed("磁盘满");
        assert_eq!(health.snapshot().database.status, ServiceStatus::Failed);
        health.clear_database_error();
        assert_eq!(health.snapshot().database.status, ServiceStatus::Running);
    }
}
