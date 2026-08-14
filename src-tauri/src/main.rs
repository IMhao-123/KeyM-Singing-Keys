// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;
use tauri::{
    menu::{IconMenuItem, Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Listener, Manager,
};

use keym_lib::audio::AudioEngine;
use keym_lib::commands::AppState;
use keym_lib::db::Database;
use keym_lib::event_tap::KeyboardListener;
use keym_lib::input_worker::InputWorker;
use keym_lib::mute_shortcut::MuteShortcut;
use keym_lib::runtime_health::RuntimeHealth;
use keym_lib::window_layout::{find_work_area, popup_origin, WorkArea};

/// Popup 与屏幕边缘/菜单栏的间距（逻辑像素，乘以 scale factor 后用于物理定位）
const POPUP_MARGIN_PT: f64 = 6.0;

/// 托盘图标最近一次鼠标事件位置（物理像素），作为 Popup 定位锚点
type TrayAnchor = std::sync::Mutex<Option<(f64, f64)>>;

/// 构建主窗口并绑定统一的“关闭即隐藏”行为（AUD-033/SR-02）。
/// 首次创建与托盘重建只走这一个入口，保证生命周期行为一致。
/// 注意：主窗口只能由代码创建——macOS 27 beta 上配置预声明窗口会触发 WebView 崩溃白屏（G2-A3）。
/// FRB-005：窗口必须隐藏创建（visible(false)）。经 LaunchServices(open) 启动时，
/// setup 阶段直接亮窗会让 AppKit 在应用完成启动前 order-in 窗口，occlusion/visibility
/// 更新不会投递给 WebKit（页面 ActivityState 停在 IsVisibleAndOccluded、缺 IsVisible），
/// 数秒后 WebKit 把图层标记为 volatile 并停止合成——主窗口白屏，激活应用后才恢复。
/// 首屏显示统一由 main() 的 RunEvent::Ready 处理（事件驱动，不用固定 sleep）。
fn build_main_window(app: &AppHandle) -> tauri::Result<tauri::WebviewWindow> {
    let window =
        tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("index.html".into()))
            .title("键标")
            .inner_size(1100.0, 800.0)
            .min_inner_size(900.0, 600.0)
            .resizable(true)
            .visible(false)
            .center()
            .build()?;
    let window_for_close = window.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            if let Err(error) = window_for_close.hide() {
                log::error!("隐藏主窗口失败: {error}");
            }
        }
    });
    Ok(window)
}

/// 显示并聚焦主窗口；窗口对象已不存在时安全重建（AUD-033）
fn show_main_window(app: &AppHandle) {
    let window = match app.get_webview_window("main") {
        Some(window) => Some(window),
        None => match build_main_window(app) {
            Ok(window) => Some(window),
            Err(error) => {
                log::error!("重建主窗口失败: {error}");
                None
            }
        },
    };
    if let Some(window) = window {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// 把 Popup 定位到托盘图标所在屏幕的顶部（菜单栏下沿附近），并夹取在工作区内（FRB-003）
fn position_popup_near_tray(app: &AppHandle, window: &tauri::WebviewWindow, anchor: &TrayAnchor) {
    let anchor_point = anchor.lock().ok().and_then(|a| *a);
    let monitors = app.available_monitors().unwrap_or_default();
    let areas: Vec<WorkArea> = monitors
        .iter()
        .map(|m| {
            let r = m.work_area();
            WorkArea {
                x: f64::from(r.position.x),
                y: f64::from(r.position.y),
                width: f64::from(r.size.width),
                height: f64::from(r.size.height),
            }
        })
        .collect();
    // 优先选择包含锚点（托盘图标）的屏幕；无锚点或未命中时回退主屏，再退化为第一个屏幕
    let primary_area = app.primary_monitor().ok().flatten().map(|m| {
        let r = m.work_area();
        WorkArea {
            x: f64::from(r.position.x),
            y: f64::from(r.position.y),
            width: f64::from(r.size.width),
            height: f64::from(r.size.height),
        }
    });
    let chosen = anchor_point
        .and_then(|(ax, ay)| find_work_area(&areas, ax, ay))
        .or_else(|| primary_area.and_then(|pa| areas.iter().position(|a| *a == pa)))
        .or(if monitors.is_empty() { None } else { Some(0) });
    let Some(idx) = chosen else { return };
    let area = areas[idx];
    let scale = monitors[idx].scale_factor();
    let size = window
        .outer_size()
        .unwrap_or(tauri::PhysicalSize::new(280, 460));
    let (x, y) = popup_origin(
        area,
        f64::from(size.width),
        f64::from(size.height),
        anchor_point.map(|(ax, _)| ax),
        POPUP_MARGIN_PT * scale,
    );
    let _ = window.set_position(tauri::PhysicalPosition::new(
        x.round() as i32,
        y.round() as i32,
    ));
}

fn main() {
    // 初始化日志（RUST_LOG 环境变量控制级别，默认 info）
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    tauri::Builder::default()
        // AUD-032：单实例必须是第一个插件——在任何服务启动前拒绝第二个进程，
        // 第二次启动只聚焦已有主窗口，不产生双声音/双统计/双托盘。
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .on_page_load(|webview, payload| {
            let state = match payload.event() {
                tauri::webview::PageLoadEvent::Started => "Started",
                tauri::webview::PageLoadEvent::Finished => "Finished",
            };
            log::info!(
                "keym_diag event=on_page_load state={} label={} url={} thread_id={:?}",
                state,
                webview.label(),
                payload.url(),
                std::thread::current().id()
            );
        })
        .setup(|app| {
            // 0. 主窗口延迟创建（G2-A3：macOS 27 beta 上配置预声明窗口会触发 WebView 崩溃白屏）
            build_main_window(app.handle())?;

            let runtime_health = RuntimeHealth::new();

            // 1. 初始化音频引擎。AudioEngine 是 Send+Sync 门面（AUD-002），
            //    OutputStream 常驻其 worker 线程，CoreAudio IO 线程与主线程 RunLoop 隔离。
            //    AUD-005：初始化失败降级为无声模式，应用与统计仍可用，错误进入健康面板。
            let audio = Arc::new(AudioEngine::new(runtime_health.clone()).unwrap_or_else(
                |error| {
                    log::error!("音频初始化失败，应用以无声模式继续: {error}");
                    AudioEngine::unavailable(runtime_health.clone(), error)
                },
            ));

            // 2. 初始化数据库
            let db = Arc::new(Database::new().expect("数据库初始化失败"));

            // 3. 初始化静音快捷键（第一版保留的独立能力；静音时段/静音应用/WPM/成就已裁掉）
            //    F5/FRB-008：数据根不可解析时绝不回退相对路径——启动日志可见失败，
            //    以内存预设继续运行（不读写磁盘），后续写操作会把错误经 IPC 透出给界面。
            let mute_shortcut = Arc::new(MuteShortcut::load().unwrap_or_else(|error| {
                log::error!("静音快捷键配置路径不可用，以内存预设运行（不读写磁盘）: {error}");
                let in_memory = MuteShortcut::new();
                in_memory.load_presets();
                in_memory
            }));

            // 4. 启动输入 worker（AUD-007：回调不落库/不播音）与键盘+鼠标监听线程
            let input_worker = InputWorker::start(
                audio.clone(),
                db.clone(),
                mute_shortcut.clone(),
                runtime_health.clone(),
            )
            .expect("输入处理线程启动失败");
            let listener = KeyboardListener::start(input_worker, runtime_health.clone())
                .expect("键盘监听启动失败");
            if !listener.is_running() {
                log::error!(
                    "键盘监听未在运行：CGEventTap 创建失败（通常缺少输入监控权限）。请前往 系统设置 → 隐私与安全性 → 输入监控 授权本应用后重启"
                );
            }
            app.manage(listener);

            // 5. 注入全局状态
            app.manage(AppState {
                audio,
                db,
                mute_shortcut,
                runtime_health,
            });

            // 8. 创建菜单栏右键菜单
            //    F3/AUD-024：托盘文案/图标是引擎状态的投影，初始值也取自真实状态。
            let sound_on_icon =
                tauri::image::Image::new(include_bytes!("../icons/sound-on.rgba"), 36, 36);
            let sound_off_icon =
                tauri::image::Image::new(include_bytes!("../icons/sound-off.rgba"), 36, 36);

            let initial_sound_enabled = app.state::<AppState>().audio.is_enabled();
            let toggle_item = IconMenuItem::with_id(
                app,
                "toggle",
                keym_lib::tray::sound_toggle_label(initial_sound_enabled),
                true,
                Some(match keym_lib::tray::sound_toggle_icon(initial_sound_enabled) {
                    keym_lib::tray::SoundToggleIcon::On => sound_on_icon.clone(),
                    keym_lib::tray::SoundToggleIcon::Off => sound_off_icon.clone(),
                }),
                None::<&str>,
            )?;
            let open_main_item =
                MenuItem::with_id(app, "open_main", "打开主界面", true, None::<&str>)?;
            let open_popup_item =
                MenuItem::with_id(app, "open_popup", "打开小弹窗", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(
                app,
                &[&open_main_item, &open_popup_item, &toggle_item, &quit_item],
            )?;

            // F3/AUD-024：单一更新路径——托盘 toggle、主窗口开关、IPC toggle_sound
            // 任何入口改变音效开关后都会广播 sound-state-changed；该监听器读取引擎
            // 真实状态刷新托盘菜单文案与图标，窗口→托盘方向由此闭环。
            {
                let app_for_events = app.handle().clone();
                let toggle_item_for_events = toggle_item.clone();
                let sound_on_for_events = sound_on_icon.clone();
                let sound_off_for_events = sound_off_icon.clone();
                app.listen(keym_lib::tray::SOUND_STATE_CHANGED_EVENT, move |_| {
                    let enabled = app_for_events.state::<AppState>().audio.is_enabled();
                    let icon = match keym_lib::tray::sound_toggle_icon(enabled) {
                        keym_lib::tray::SoundToggleIcon::On => sound_on_for_events.clone(),
                        keym_lib::tray::SoundToggleIcon::Off => sound_off_for_events.clone(),
                    };
                    if let Err(error) =
                        toggle_item_for_events.set_text(keym_lib::tray::sound_toggle_label(enabled))
                    {
                        log::error!("同步托盘音效文案失败: {error}");
                    }
                    if let Err(error) = toggle_item_for_events.set_icon(Some(icon)) {
                        log::error!("同步托盘音效图标失败: {error}");
                    }
                });
            }

            // 9. （弹窗失焦即隐藏，无需保护标志）

            // 10. 配置托盘图标
            let tray_anchor: std::sync::Arc<TrayAnchor> =
                std::sync::Arc::new(std::sync::Mutex::new(None));
            let anchor_for_menu = tray_anchor.clone();
            let anchor_for_events = tray_anchor.clone();

            let _tray = TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .tooltip("键标")
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "toggle" => {
                        let state = app.state::<AppState>();
                        let new_state = !state.audio.is_enabled();
                        // AUD-012/020/024：持久化失败时保持原状并记录错误；
                        // 成功后只广播事件——托盘文案/图标由 sound-state-changed
                        // 监听器统一刷新（F3 单一更新路径），主窗口经 useRefreshEvents 同步。
                        if let Err(error) = state.audio.set_enabled(new_state) {
                            log::error!("托盘切换音效失败: {error}");
                            return;
                        }
                        let _ = app.emit(keym_lib::tray::SOUND_STATE_CHANGED_EVENT, new_state);
                    }
                    "open_main" => {
                        show_main_window(app);
                    }
                    "open_popup" => {
                        if let Some(window) = app.get_webview_window("popup") {
                            position_popup_near_tray(app, &window, &anchor_for_menu);
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(move |tray, event| {
                    match &event {
                        TrayIconEvent::Click {
                            position,
                            button,
                            button_state,
                            ..
                        } => {
                            if let Ok(mut anchor) = anchor_for_events.lock() {
                                *anchor = Some((position.x, position.y));
                            }
                            if matches!(button, MouseButton::Left)
                                && matches!(button_state, MouseButtonState::Up)
                            {
                                show_main_window(tray.app_handle());
                            }
                        }
                        TrayIconEvent::Enter { position, .. }
                        | TrayIconEvent::Move { position, .. } => {
                            if let Ok(mut anchor) = anchor_for_events.lock() {
                                *anchor = Some((position.x, position.y));
                            }
                        }
                        _ => {}
                    }
                })
                .build(app)?;

            // 11. 弹出窗口失焦自动隐藏
            if let Some(popup_window) = app.get_webview_window("popup") {
                let weak_window = popup_window.as_ref().window().clone();
                popup_window.on_window_event(move |event| {
                    if let tauri::WindowEvent::Focused(false) = event {
                        let _ = weak_window.hide();
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            keym_lib::commands::get_sound_enabled,
            keym_lib::commands::toggle_sound,
            keym_lib::commands::set_volume,
            keym_lib::commands::get_volume,
            keym_lib::commands::set_theme,
            keym_lib::commands::get_theme,
            keym_lib::commands::get_stats_overview,
            keym_lib::commands::get_app_stats,
            keym_lib::commands::get_keycode_stats,
            keym_lib::commands::export_data_to_file,
            // Phase 6: 统计类
            keym_lib::commands::get_heatmap_data,
            keym_lib::commands::get_trend_data,
            keym_lib::commands::get_hourly_distribution,
            keym_lib::commands::get_recent_activity,
            keym_lib::commands::get_insights,
            // Phase 6: 音效类
            keym_lib::commands::get_theme_list,
            // 静音快捷键（静音时段/静音应用/自定义音效/WPM/成就已随第一版裁掉）
            keym_lib::commands::get_mute_combos,
            keym_lib::commands::add_mute_combo,
            keym_lib::commands::remove_mute_combo,
            keym_lib::commands::reset_mute_presets,
            // Phase 6: 数据管理
            keym_lib::commands::clear_all_data,
            // 运行时健康（AUD-004）
            keym_lib::commands::get_runtime_health,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        // FRB-005：应用完成启动（RunEvent::Ready）后先显式激活应用、再亮出主窗口。
        // 本应用是 UIElement(LSUIElement=true) 菜单栏应用：经 LaunchServices(open) 启动时
        // 系统不会激活它，未激活应用的窗口在 macOS 27 beta 上收不到 AppKit 的
        // occlusion/visibility 更新，WebKit 将页面视为遮蔽并在数秒后停止合成（白屏），
        // 任何后续激活（设为 frontmost / 再次 open）都会立即恢复渲染——实测验证。
        // 直接二进制启动时 AppKit 会自动激活应用，因此从未白屏。
        // Ready 后 activate + show/focus 是事件驱动的时序修复，不依赖固定 sleep。
        .run(|handle, event| {
            if let tauri::RunEvent::Ready = event {
                if let Some(mtm) = objc2::MainThreadMarker::new() {
                    objc2_app_kit::NSApplication::sharedApplication(mtm).activate();
                }
                show_main_window(handle);
            }
        });
}
