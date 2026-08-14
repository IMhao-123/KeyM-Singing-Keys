//! 输入事件工作线程（AUD-001 / AUD-007 / AUD-004）。
//!
//! CGEventTap 回调只做最小采集并通过 `try_send` 非阻塞投递到有界队列；
//! 音频播放、应用识别与数据库事务都在本 worker 串行完成，保证单事件只提交一次。
//! 队列满时增加丢弃计数并通过 `RuntimeHealth` 暴露，绝不静默丢弃或阻塞系统回调。

use crate::app_info::get_active_app_name;
use crate::audio::{category_name, keycode_to_category, AudioEngine};
use crate::db::{Database, KeyRecord};
use crate::mute_shortcut::MuteShortcut;
use crate::runtime_health::RuntimeHealth;
use chrono::{Local, TimeZone};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};

const INPUT_QUEUE_CAPACITY: usize = 2048;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Other,
}

impl MouseButton {
    fn label(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Other => "other",
        }
    }
}

/// 回调采集的最小输入事件。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputEvent {
    KeyDown {
        keycode: u16,
        timestamp_ms: i64,
    },
    /// 修饰键状态变化。`is_down` 为真时按一次事件计入播放与统计；释放不计数（AUD-001）。
    ModifierChanged {
        keycode: u16,
        is_down: bool,
        cmd: bool,
        shift: bool,
        ctrl: bool,
        opt: bool,
        timestamp_ms: i64,
    },
    MouseDown {
        button: MouseButton,
        timestamp_ms: i64,
    },
    /// tap 被系统禁用后恢复时重置修饰键状态。
    ResetModifiers,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DispatchOutcome {
    Queued,
    QueueFull,
    WorkerStopped,
}

/// 回调持有的轻量投递句柄（Clone 即可，无锁）。
#[derive(Clone)]
pub struct InputDispatcher {
    sender: SyncSender<InputEvent>,
    health: RuntimeHealth,
}

impl InputDispatcher {
    pub fn try_send(&self, event: InputEvent) -> DispatchOutcome {
        match self.sender.try_send(event) {
            Ok(()) => DispatchOutcome::Queued,
            Err(TrySendError::Full(_)) => {
                self.health.record_dropped_input_event();
                DispatchOutcome::QueueFull
            }
            Err(TrySendError::Disconnected(_)) => {
                self.health.set_input_failed("输入处理线程已经停止");
                DispatchOutcome::WorkerStopped
            }
        }
    }
}

pub struct InputWorker {
    dispatcher: Option<InputDispatcher>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl InputWorker {
    pub fn start(
        audio: Arc<AudioEngine>,
        db: Arc<Database>,
        mute_shortcut: Arc<MuteShortcut>,
        health: RuntimeHealth,
    ) -> Result<Self, String> {
        Self::start_services(
            audio,
            db,
            mute_shortcut,
            health,
            Arc::new(get_active_app_name),
        )
    }

    fn start_services(
        audio: Arc<AudioEngine>,
        db: Arc<Database>,
        mute_shortcut: Arc<MuteShortcut>,
        health: RuntimeHealth,
        app_provider: Arc<dyn Fn() -> Option<String> + Send + Sync>,
    ) -> Result<Self, String> {
        let (sender, receiver) = mpsc::sync_channel(INPUT_QUEUE_CAPACITY);
        let dispatcher = InputDispatcher {
            sender,
            health: health.clone(),
        };
        let handle = std::thread::Builder::new()
            .name("input-worker".into())
            .spawn(move || run_worker(receiver, audio, db, mute_shortcut, health, app_provider))
            .map_err(|error| format!("无法启动输入处理线程: {error}"))?;
        Ok(Self {
            dispatcher: Some(dispatcher),
            handle: Some(handle),
        })
    }

    pub fn dispatcher(&self) -> InputDispatcher {
        self.dispatcher
            .as_ref()
            .expect("input dispatcher is available while worker is alive")
            .clone()
    }
}

impl Drop for InputWorker {
    fn drop(&mut self) {
        // 丢弃最后一个 sender 关闭 channel；recv() 会继续处理完已入队事件后再退出，
        // 因此关闭不会丢弃队列尾部。
        self.dispatcher.take();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn run_worker(
    receiver: Receiver<InputEvent>,
    audio: Arc<AudioEngine>,
    db: Arc<Database>,
    mute_shortcut: Arc<MuteShortcut>,
    health: RuntimeHealth,
    app_provider: Arc<dyn Fn() -> Option<String> + Send + Sync>,
) {
    let app_cache: Mutex<Option<(Option<String>, i64)>> = Mutex::new(None);
    while let Ok(event) = receiver.recv() {
        match event {
            InputEvent::KeyDown {
                keycode,
                timestamp_ms,
            } => process_key(
                keycode,
                timestamp_ms,
                false,
                &audio,
                &db,
                &mute_shortcut,
                &health,
                &app_provider,
                &app_cache,
            ),
            InputEvent::ModifierChanged {
                keycode,
                is_down,
                cmd,
                shift,
                ctrl,
                opt,
                timestamp_ms,
            } => {
                mute_shortcut.set_modifier_state(cmd, shift, ctrl, opt);
                if is_down {
                    process_key(
                        keycode,
                        timestamp_ms,
                        true,
                        &audio,
                        &db,
                        &mute_shortcut,
                        &health,
                        &app_provider,
                        &app_cache,
                    );
                }
            }
            InputEvent::MouseDown {
                button,
                timestamp_ms,
            } => {
                let app = cached_app(timestamp_ms, &app_provider, &app_cache);
                let date = local_date(timestamp_ms);
                if let Err(error) =
                    db.record_click_transaction(timestamp_ms, button.label(), app.as_deref(), &date)
                {
                    health.set_database_failed(error);
                } else {
                    health.clear_database_error();
                }
            }
            InputEvent::ResetModifiers => {
                mute_shortcut.set_modifier_state(false, false, false, false);
            }
        }
    }
}

/// 处理一次按键。`modifier_event` 为真表示这是修饰键按下（AUD-001：按一次记一次，释放不计数）。
#[allow(clippy::too_many_arguments)]
fn process_key(
    keycode: u16,
    timestamp_ms: i64,
    modifier_event: bool,
    audio: &AudioEngine,
    db: &Database,
    mute_shortcut: &MuteShortcut,
    health: &RuntimeHealth,
    app_provider: &Arc<dyn Fn() -> Option<String> + Send + Sync>,
    app_cache: &Mutex<Option<(Option<String>, i64)>>,
) {
    let app = cached_app(timestamp_ms, app_provider, app_cache);
    let combo_muted = !modifier_event && mute_shortcut.should_mute_combo(keycode);
    if should_play_key(modifier_event, combo_muted) {
        audio.play_key(keycode);
    }

    let local = Local
        .timestamp_millis_opt(timestamp_ms)
        .single()
        .unwrap_or_else(Local::now);
    let date = local.format("%Y-%m-%d").to_string();
    let hour = local.format("%H").to_string().parse::<u8>().unwrap_or(0);
    let result = db.record_key_transaction(KeyRecord {
        timestamp_ms,
        keycode,
        category: category_name(keycode_to_category(keycode)),
        app_name: app.as_deref(),
        date: &date,
        hour,
    });
    if let Err(error) = result {
        health.set_database_failed(error);
    } else {
        health.clear_database_error();
    }
}

fn should_play_key(modifier_event: bool, combo_muted: bool) -> bool {
    !modifier_event && !combo_muted
}

fn cached_app(
    timestamp_ms: i64,
    provider: &Arc<dyn Fn() -> Option<String> + Send + Sync>,
    cache: &Mutex<Option<(Option<String>, i64)>>,
) -> Option<String> {
    let mut cached = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((identity, updated_at)) = cached.as_ref() {
        if timestamp_ms.saturating_sub(*updated_at) <= 500 {
            return identity.clone();
        }
    }
    let identity = provider();
    *cached = Some((identity.clone(), timestamp_ms));
    identity
}

fn local_date(timestamp_ms: i64) -> String {
    Local
        .timestamp_millis_opt(timestamp_ms)
        .single()
        .unwrap_or_else(Local::now)
        .format("%Y-%m-%d")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifier_keys_are_silent_without_suppressing_normal_keys() {
        assert!(!should_play_key(true, false));
        assert!(!should_play_key(false, true));
        assert!(should_play_key(false, false));
    }

    #[test]
    fn full_queue_is_visible_and_never_blocks() {
        let health = RuntimeHealth::new();
        let (sender, _receiver) = mpsc::sync_channel(1);
        let dispatcher = InputDispatcher {
            sender,
            health: health.clone(),
        };
        assert_eq!(
            dispatcher.try_send(InputEvent::MouseDown {
                button: MouseButton::Left,
                timestamp_ms: 1,
            }),
            DispatchOutcome::Queued
        );
        assert_eq!(
            dispatcher.try_send(InputEvent::MouseDown {
                button: MouseButton::Right,
                timestamp_ms: 2,
            }),
            DispatchOutcome::QueueFull
        );
        assert_eq!(health.snapshot().dropped_input_events, 1);
    }

    #[test]
    fn disconnected_worker_is_reported() {
        let health = RuntimeHealth::new();
        let (sender, receiver) = mpsc::sync_channel(1);
        drop(receiver);
        let dispatcher = InputDispatcher {
            sender,
            health: health.clone(),
        };
        assert_eq!(
            dispatcher.try_send(InputEvent::MouseDown {
                button: MouseButton::Other,
                timestamp_ms: 1,
            }),
            DispatchOutcome::WorkerStopped
        );
        assert_eq!(
            health.snapshot().input.status,
            crate::runtime_health::ServiceStatus::Failed
        );
    }
}
