//! 键盘/鼠标全局事件监听（macOS CGEventTap）。
//!
//! 回调只做最小采集并通过 `InputDispatcher::try_send` 非阻塞投递到输入 worker
//! 有界队列（AUD-007）：不在系统回调里做音频播放、应用识别或 SQLite 写入。
//! tap 创建结果与权限状态写入 `RuntimeHealth`，供前端可见（AUD-004）。

use core_foundation::base::TCFType;
use core_foundation::mach_port::CFMachPort;
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
use core_graphics::event::{
    CGEvent, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
    EventField,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use crate::input_worker::{InputDispatcher, InputEvent, MouseButton};
use crate::runtime_health::RuntimeHealth;

const EVENT_KEY_DOWN: u32 = 10;
const EVENT_TAP_DISABLED_BY_TIMEOUT: u32 = 4_294_967_294;
const EVENT_TAP_DISABLED_BY_USER_INPUT: u32 = 4_294_967_295;
/// start() 等待监听线程上报 tap 创建结果的最长时间
const TAP_READY_TIMEOUT: Duration = Duration::from_secs(5);

extern "C" {
    fn CGEventTapEnable(tap: core_foundation::mach_port::CFMachPortRef, enable: bool);
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum EventTapEventKind {
    KeyDown,
    FlagsChanged,
    MouseDown,
    TapDisabledByTimeout,
    TapDisabledByUserInput,
    Other,
}

fn classify_event_type(event_type: u32) -> EventTapEventKind {
    match event_type {
        EVENT_KEY_DOWN => EventTapEventKind::KeyDown,
        e if e == CGEventType::FlagsChanged as u32 => EventTapEventKind::FlagsChanged,
        e if e == CGEventType::LeftMouseDown as u32
            || e == CGEventType::RightMouseDown as u32
            || e == CGEventType::OtherMouseDown as u32 =>
        {
            EventTapEventKind::MouseDown
        }
        EVENT_TAP_DISABLED_BY_TIMEOUT => EventTapEventKind::TapDisabledByTimeout,
        EVENT_TAP_DISABLED_BY_USER_INPUT => EventTapEventKind::TapDisabledByUserInput,
        _ => EventTapEventKind::Other,
    }
}

/// 禁用事件恢复决策（纯函数，可单测）。
fn decide_tap_recovery(kind: EventTapEventKind) -> Option<&'static str> {
    match kind {
        EventTapEventKind::TapDisabledByTimeout => Some("timeout"),
        EventTapEventKind::TapDisabledByUserInput => Some("user_input"),
        _ => None,
    }
}

/// 输入监控权限状态（macOS 14.4+ 可精确判定，旧系统无法预检）。
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum ListenPermissionStatus {
    Granted,
    Denied,
    ApiUnavailable,
}

fn should_request_permission(status: ListenPermissionStatus) -> bool {
    matches!(status, ListenPermissionStatus::Denied)
}

/// tap 就绪时的健康上报决策（纯函数，可单测 AUD-004）：
/// tap 创建成功不代表有权限——macOS 14.4+ 无权限的 tap 能创建但收不到事件，
/// 预检 denied 时必须保持 permission_denied 可见，不可用 running 覆盖。
fn ready_reports_running(status: ListenPermissionStatus) -> bool {
    !matches!(status, ListenPermissionStatus::Denied)
}

mod listen_event_access {
    use super::ListenPermissionStatus;
    use std::ffi::CString;

    type AccessFn = unsafe extern "C" fn() -> bool;

    unsafe fn resolve(name: &str) -> Option<AccessFn> {
        let c_name = CString::new(name).ok()?;
        let sym = libc::dlsym(libc::RTLD_DEFAULT, c_name.as_ptr());
        if sym.is_null() {
            None
        } else {
            Some(std::mem::transmute::<*mut libc::c_void, AccessFn>(sym))
        }
    }

    pub fn preflight() -> Option<bool> {
        unsafe { resolve("CGPreflightListenEventAccess").map(|f| f()) }
    }

    pub fn request() -> Option<bool> {
        unsafe { resolve("CGRequestListenEventAccess").map(|f| f()) }
    }

    pub fn status() -> ListenPermissionStatus {
        match preflight() {
            Some(true) => ListenPermissionStatus::Granted,
            Some(false) => ListenPermissionStatus::Denied,
            None => ListenPermissionStatus::ApiUnavailable,
        }
    }
}

/// 该 keycode 所属修饰键组的 flag 是否激活（纯函数，可单测 AUD-001）。
fn modifier_group_active(keycode: u16, cmd: bool, shift: bool, ctrl: bool, opt: bool) -> bool {
    match keycode {
        0x37 | 0x36 => cmd,   // L/R Cmd
        0x38 | 0x3C => shift, // L/R Shift
        0x3B | 0x3E => ctrl,  // L/R Ctrl
        0x3A | 0x3D => opt,   // L/R Opt
        _ => false,
    }
}

/// 修饰键按下/释放状态机（纯函数，可单测 AUD-001）。
/// CapsLock(0x39) 是锁存键：每次 FlagsChanged 都记为一次按下。
/// 同组修饰键（如左/右 Cmd）共享一个 flag：按住左 Cmd 再按右 Cmd 时 flag 不变，
/// 释放其一时 flag 仍在——只靠 flag 会把释放误判成新按下，必须结合 pressed 集合
/// 区分“该键码首次按下”与“释放”，保证按一次记一次、释放不计数。
fn classify_modifier_transition(
    keycode: u16,
    group_active: bool,
    pressed: &mut std::collections::HashSet<u16>,
) -> bool {
    if keycode == 0x39 {
        return true;
    }
    if group_active && !pressed.contains(&keycode) {
        pressed.insert(keycode);
        true
    } else {
        pressed.remove(&keycode);
        false
    }
}

/// 键盘事件监听器
pub struct KeyboardListener {
    running: Arc<AtomicBool>,
    run_loop: Arc<Mutex<Option<CFRunLoop>>>,
    thread_handle: Option<std::thread::JoinHandle<()>>,
    /// 输入 worker 由监听器持有，保证析构顺序：先停 tap 线程（释放回调里的
    /// dispatcher，关闭 channel），再 join worker，避免退出时死锁。
    input_worker: Option<crate::input_worker::InputWorker>,
}

impl KeyboardListener {
    pub fn start(
        input_worker: crate::input_worker::InputWorker,
        health: RuntimeHealth,
    ) -> Result<Self, String> {
        let dispatcher = input_worker.dispatcher();
        let running = Arc::new(AtomicBool::new(false));
        let running_clone = running.clone();
        let run_loop: Arc<Mutex<Option<CFRunLoop>>> = Arc::new(Mutex::new(None));
        let run_loop_clone = run_loop.clone();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();

        let handle = std::thread::Builder::new()
            .name("keyboard-listener".into())
            .spawn(move || {
                run_event_tap(dispatcher, health, running_clone, run_loop_clone, ready_tx);
            })
            .map_err(|e| format!("无法启动监听线程: {e}"))?;

        match ready_rx.recv_timeout(TAP_READY_TIMEOUT) {
            Ok(Ok(())) => {}
            Ok(Err(msg)) => {
                log::error!("keym_diag event=event_tap_start_failed error=\"{msg}\"");
            }
            Err(_) => {
                log::error!(
                    "keym_diag event=event_tap_start_failed error=\"等待 tap 就绪超时（{}ms）\"",
                    TAP_READY_TIMEOUT.as_millis()
                );
            }
        }

        Ok(KeyboardListener {
            running,
            run_loop,
            thread_handle: Some(handle),
            input_worker: Some(input_worker),
        })
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

impl Drop for KeyboardListener {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(rl) = self.run_loop.lock().unwrap().take() {
            rl.stop();
            unsafe {
                core_foundation::runloop::CFRunLoopWakeUp(rl.as_concrete_TypeRef());
            }
        }
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
        // tap 线程已退出、回调持有的 dispatcher 已释放，channel 关闭，
        // 此时 join 输入 worker 会立即返回（顺序固定，无死锁）。
        drop(self.input_worker.take());
    }
}

#[allow(clippy::too_many_arguments)]
fn run_event_tap(
    dispatcher: InputDispatcher,
    health: RuntimeHealth,
    running: Arc<AtomicBool>,
    run_loop: Arc<Mutex<Option<CFRunLoop>>>,
    ready_tx: mpsc::Sender<Result<(), String>>,
) {
    let perm_status = listen_event_access::status();
    match perm_status {
        ListenPermissionStatus::Granted => {
            log::info!("keym_diag event=listen_permission status=granted");
        }
        ListenPermissionStatus::Denied => {
            log::error!(
                "keym_diag event=listen_permission status=denied - 缺少 macOS 输入监控权限"
            );
            health.set_input_permission_denied(
                "缺少输入监控权限：系统设置 -> 隐私与安全性 -> 输入监控 -> 启用本应用后重启",
            );
            if should_request_permission(perm_status) {
                match listen_event_access::request() {
                    Some(true) => {
                        log::info!("keym_diag event=listen_permission_request result=granted")
                    }
                    Some(false) => {
                        log::error!("keym_diag event=listen_permission_request result=denied")
                    }
                    None => log::error!(
                        "keym_diag event=listen_permission_request result=api_unavailable"
                    ),
                }
            }
        }
        ListenPermissionStatus::ApiUnavailable => {
            log::warn!("keym_diag event=listen_permission status=api_unavailable");
        }
    }

    // tap 的 mach port 句柄：创建成功后回填，供禁用恢复时重新启用。
    // CFMachPort 非 Send/Sync，但此 Arc 只在监听线程与其 RunLoop 回调间共享，
    // 不跨线程，故对 arc_with_non_send_sync 局部豁免。
    #[allow(clippy::arc_with_non_send_sync)]
    let tap_port: Arc<Mutex<Option<CFMachPort>>> = Arc::new(Mutex::new(None));
    let tap_port_in_cb = tap_port.clone();
    // 修饰键按下集合（AUD-001 状态机），tap 被禁用时清空
    let pressed_modifiers: Arc<Mutex<std::collections::HashSet<u16>>> =
        Arc::new(Mutex::new(std::collections::HashSet::new()));
    let pressed_modifiers_cb = pressed_modifiers.clone();

    let tap = CGEventTap::new(
        CGEventTapLocation::Session,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::ListenOnly,
        vec![
            CGEventType::KeyDown,
            CGEventType::FlagsChanged,
            CGEventType::LeftMouseDown,
            CGEventType::RightMouseDown,
            CGEventType::OtherMouseDown,
        ],
        move |_proxy, etype, event: &CGEvent| {
            let etype_u32 = etype as u32;
            let kind = classify_event_type(etype_u32);
            let timestamp_ms = chrono::Utc::now().timestamp_millis();

            match kind {
                EventTapEventKind::KeyDown => {
                    let keycode =
                        event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;
                    dispatcher.try_send(InputEvent::KeyDown {
                        keycode,
                        timestamp_ms,
                    });
                }
                EventTapEventKind::FlagsChanged => {
                    let keycode =
                        event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;
                    let flags = event.get_flags();
                    let cmd =
                        flags.contains(core_graphics::event::CGEventFlags::CGEventFlagCommand);
                    let shift =
                        flags.contains(core_graphics::event::CGEventFlags::CGEventFlagShift);
                    let ctrl =
                        flags.contains(core_graphics::event::CGEventFlags::CGEventFlagControl);
                    let opt =
                        flags.contains(core_graphics::event::CGEventFlags::CGEventFlagAlternate);
                    let group_active = modifier_group_active(keycode, cmd, shift, ctrl, opt);
                    let is_down = {
                        let mut pressed = pressed_modifiers_cb
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        classify_modifier_transition(keycode, group_active, &mut pressed)
                    };
                    dispatcher.try_send(InputEvent::ModifierChanged {
                        keycode,
                        is_down,
                        cmd,
                        shift,
                        ctrl,
                        opt,
                        timestamp_ms,
                    });
                }
                EventTapEventKind::MouseDown => {
                    let button = match etype_u32 {
                        e if e == CGEventType::LeftMouseDown as u32 => MouseButton::Left,
                        e if e == CGEventType::RightMouseDown as u32 => MouseButton::Right,
                        _ => MouseButton::Other,
                    };
                    dispatcher.try_send(InputEvent::MouseDown {
                        button,
                        timestamp_ms,
                    });
                }
                _ => {
                    if let Some(reason) = decide_tap_recovery(kind) {
                        log::error!(
                            "keym_diag event=event_tap_disabled reason={} event_type={}",
                            reason,
                            etype_u32
                        );
                        let reenabled = {
                            let port = tap_port_in_cb.lock().unwrap();
                            match port.as_ref() {
                                Some(p) => {
                                    unsafe {
                                        CGEventTapEnable(p.as_concrete_TypeRef(), true);
                                    }
                                    true
                                }
                                None => false,
                            }
                        };
                        // 禁用期间可能丢失 FlagsChanged，修饰键状态已不可信，重置。
                        pressed_modifiers_cb
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .clear();
                        dispatcher.try_send(InputEvent::ResetModifiers);
                        log::info!(
                            "keym_diag event=event_tap_recovered reason={} reenabled={} modifiers_reset=true",
                            reason,
                            reenabled
                        );
                    }
                }
            }
            // ListenOnly：永远返回 None，不拦截任何事件
            None
        },
    );

    match tap {
        Ok(tap) => {
            *tap_port.lock().unwrap() = Some(tap.mach_port.clone());

            let loop_source = match tap.mach_port.create_runloop_source(0) {
                Ok(source) => source,
                Err(()) => {
                    let msg = "无法创建 RunLoop source";
                    log::error!("keym_diag event=event_tap_start_failed error=\"{msg}\"");
                    health.set_input_failed(msg);
                    let _ = ready_tx.send(Err(msg.into()));
                    return;
                }
            };

            let current = CFRunLoop::get_current();
            unsafe {
                current.add_source(&loop_source, kCFRunLoopCommonModes);
            }
            tap.enable();

            *run_loop.lock().unwrap() = Some(current.clone());
            running.store(true, Ordering::SeqCst);
            if ready_reports_running(perm_status) {
                health.set_input_running();
            } else {
                health.set_input_permission_denied(
                    "缺少输入监控权限：系统设置 -> 隐私与安全性 -> 输入监控 -> 启用本应用后重启",
                );
            }
            let _ = ready_tx.send(Ok(()));
            log::info!("keym_diag event=event_tap_ready running=true");

            CFRunLoop::run_current();

            running.store(false, Ordering::SeqCst);
            log::info!("keym_diag event=event_tap_stopped running=false");
        }
        Err(_) => {
            let msg = "CGEventTap 创建失败：键盘监听未启动。请检查 macOS 输入监控权限 - 系统设置 -> 隐私与安全性 -> 输入监控 -> 添加并启用本应用，然后重启应用";
            log::error!("keym_diag event=event_tap_start_failed error=\"{msg}\"");
            health.set_input_permission_denied(
                "输入监听未启动：请在 系统设置 -> 隐私与安全性 -> 输入监控 中授权本应用后重启",
            );
            let _ = ready_tx.send(Err(msg.into()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_macos_event_tap_event_types() {
        assert_eq!(EVENT_KEY_DOWN, CGEventType::KeyDown as u32);
        assert_eq!(classify_event_type(10), EventTapEventKind::KeyDown);
        assert_eq!(
            classify_event_type(CGEventType::FlagsChanged as u32),
            EventTapEventKind::FlagsChanged
        );
        assert_eq!(
            classify_event_type(CGEventType::LeftMouseDown as u32),
            EventTapEventKind::MouseDown
        );
        assert_eq!(
            classify_event_type(4_294_967_294),
            EventTapEventKind::TapDisabledByTimeout
        );
        assert_eq!(
            classify_event_type(4_294_967_295),
            EventTapEventKind::TapDisabledByUserInput
        );
        assert_eq!(classify_event_type(999), EventTapEventKind::Other);
    }

    #[test]
    fn recovery_decision_reenables_only_disabled_events() {
        assert_eq!(
            decide_tap_recovery(EventTapEventKind::TapDisabledByTimeout),
            Some("timeout")
        );
        assert_eq!(
            decide_tap_recovery(EventTapEventKind::TapDisabledByUserInput),
            Some("user_input")
        );
        assert_eq!(decide_tap_recovery(EventTapEventKind::KeyDown), None);
        assert_eq!(decide_tap_recovery(EventTapEventKind::Other), None);
    }

    #[test]
    fn permission_request_only_when_denied() {
        assert!(should_request_permission(ListenPermissionStatus::Denied));
        assert!(!should_request_permission(ListenPermissionStatus::Granted));
        assert!(!should_request_permission(
            ListenPermissionStatus::ApiUnavailable
        ));
    }

    /// AUD-004：预检 denied 时 tap 就绪也不得覆盖为 running。
    #[test]
    fn ready_health_keeps_permission_denied_visible() {
        assert!(!ready_reports_running(ListenPermissionStatus::Denied));
        assert!(ready_reports_running(ListenPermissionStatus::Granted));
        assert!(ready_reports_running(
            ListenPermissionStatus::ApiUnavailable
        ));
    }

    #[test]
    fn listen_event_access_symbols_resolve_safely() {
        let preflight = listen_event_access::preflight();
        let status = listen_event_access::status();
        match preflight {
            Some(true) => assert_eq!(status, ListenPermissionStatus::Granted),
            Some(false) => assert_eq!(status, ListenPermissionStatus::Denied),
            None => assert_eq!(status, ListenPermissionStatus::ApiUnavailable),
        }
    }

    /// AUD-001：修饰键状态机——按一次记一次、释放不计数、同组双键互不干扰、CapsLock 锁存。
    #[test]
    fn modifier_press_release_state_machine() {
        let mut pressed = std::collections::HashSet::new();
        // 左 Cmd(0x37) 按下 -> 记一次
        assert!(classify_modifier_transition(0x37, true, &mut pressed));
        // 按住左 Cmd 再按右 Cmd(0x36)：组 flag 不变，仍是新按下
        assert!(classify_modifier_transition(0x36, true, &mut pressed));
        // 释放左 Cmd 但右 Cmd 仍按住（组 flag 仍在）：是释放，不计数
        assert!(!classify_modifier_transition(0x37, true, &mut pressed));
        // 释放右 Cmd（组 flag 消失）：不计数
        assert!(!classify_modifier_transition(0x36, false, &mut pressed));
        // 右 Shift(0x3C) 按下/释放
        assert!(classify_modifier_transition(0x3C, true, &mut pressed));
        assert!(!classify_modifier_transition(0x3C, false, &mut pressed));
        // CapsLock(0x39) 锁存：每次 FlagsChanged 都记为一次按下
        assert!(classify_modifier_transition(0x39, true, &mut pressed));
        assert!(classify_modifier_transition(0x39, true, &mut pressed));
        // 非修饰键码：生产上 group_active 恒为 false（见 modifier_group_active），不计数
        assert!(!classify_modifier_transition(0x00, false, &mut pressed));
    }

    /// AUD-001：组 flag 判定只看对应修饰组，左右键码各自映射。
    #[test]
    fn modifier_group_active_maps_each_group() {
        assert!(modifier_group_active(0x37, true, false, false, false));
        assert!(!modifier_group_active(0x37, false, true, true, true));
        assert!(modifier_group_active(0x3C, false, true, false, false));
        assert!(modifier_group_active(0x3B, false, false, true, false));
        assert!(modifier_group_active(0x3D, false, false, false, true));
        assert!(!modifier_group_active(0x00, true, true, true, true));
    }
}
