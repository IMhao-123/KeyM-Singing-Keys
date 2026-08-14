use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

use crate::audio::{AudioEngine, SoundTheme};
use crate::db::Database;
use crate::mute_shortcut::{KeyCombo, MuteShortcut};
use crate::runtime_health::{RuntimeHealth, RuntimeHealthSnapshot};
use serde::Serialize;

/// 应用全局状态
pub struct AppState {
    pub audio: Arc<AudioEngine>,
    pub db: Arc<Database>,
    pub mute_shortcut: Arc<MuteShortcut>,
    pub runtime_health: RuntimeHealth,
}

#[derive(Serialize)]
pub struct StatsOverview {
    pub total_keys: u64,
    pub total_clicks: u64,
    pub today_keys: u64,
}

// --- 音效命令 ---

#[tauri::command]
pub fn get_sound_enabled(state: State<'_, AppState>) -> bool {
    state.audio.is_enabled()
}

#[tauri::command]
pub fn toggle_sound(app: AppHandle, state: State<'_, AppState>) -> Result<bool, String> {
    let current = state.audio.is_enabled();
    state.audio.set_enabled(!current)?;
    // F3/AUD-024：事件广播后由托盘监听器读取引擎真实状态刷新菜单（单一状态源）。
    let _ = app.emit(crate::tray::SOUND_STATE_CHANGED_EVENT, !current);
    Ok(!current)
}

#[tauri::command]
pub fn set_volume(app: AppHandle, state: State<'_, AppState>, volume: f32) -> Result<f32, String> {
    let v = state.audio.set_volume(volume)?;
    let _ = app.emit("volume-changed", v);
    Ok(v)
}

#[tauri::command]
pub fn get_volume(state: State<'_, AppState>) -> f32 {
    state.audio.get_volume()
}

#[tauri::command]
pub fn set_theme(app: AppHandle, state: State<'_, AppState>, theme: u8) -> Result<u8, String> {
    state.audio.set_theme(SoundTheme::from_index(theme))?;
    let t = state.audio.get_theme().as_index();
    let _ = app.emit("theme-changed", t);
    Ok(t)
}

#[tauri::command]
pub fn get_theme(state: State<'_, AppState>) -> u8 {
    state.audio.get_theme().as_index()
}

// --- 统计命令 ---

#[tauri::command]
pub fn get_stats_overview(state: State<'_, AppState>) -> Result<StatsOverview, String> {
    Ok(StatsOverview {
        total_keys: state.db.try_get_total_keys()?,
        total_clicks: state.db.try_get_total_clicks()?,
        today_keys: state.db.try_get_today_keys()?,
    })
}

#[tauri::command]
pub fn get_app_stats(state: State<'_, AppState>) -> Result<Vec<(String, u64)>, String> {
    state.db.try_get_app_stats_today()
}

#[tauri::command]
pub fn get_keycode_stats(state: State<'_, AppState>) -> Result<Vec<(u16, u64)>, String> {
    state.db.try_get_keycode_stats()
}

// --- 导出 ---

/// 导出数据直接写入指定路径（前端 save 对话框拿到路径后调用）
#[tauri::command]
pub fn export_data_to_file(
    state: State<'_, AppState>,
    format: String,
    path: String,
) -> Result<(), String> {
    let content = match format.as_str() {
        "csv" => state.db.export_csv()?,
        "json" => state.db.export_json()?,
        _ => return Err("不支持的格式".to_string()),
    };
    std::fs::write(&path, content).map_err(|e| format!("写入文件失败: {}", e))
}

// ===== Phase 6: 统计类命令 =====

#[derive(Serialize)]
pub struct DailyStat {
    pub date: String,
    pub total_keys: u64,
    pub total_clicks: u64,
}

#[derive(Serialize)]
pub struct ActivityItem {
    pub timestamp: i64,
    pub keycode: u16,
    pub category: String,
    pub app_name: Option<String>,
}

#[derive(Serialize)]
pub struct Insights {
    pub week_total: u64,
    pub prev_week_total: u64,
    pub week_overview_change_pct: Option<f64>,
    pub top_keys: Vec<(u16, u64)>,
}

fn date_days_ago(days: i64) -> String {
    (chrono::Local::now() - chrono::Duration::days(days))
        .format("%Y-%m-%d")
        .to_string()
}

fn today_string() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

#[tauri::command]
pub fn get_heatmap_data(
    state: State<'_, AppState>,
    period: String,
) -> Result<Vec<(u16, u64)>, String> {
    state.db.try_get_heatmap_data(&period)
}

#[tauri::command]
pub fn get_trend_data(state: State<'_, AppState>, days: u32) -> Result<Vec<DailyStat>, String> {
    let end = today_string();
    let start = date_days_ago(days as i64 - 1);
    Ok(state
        .db
        .try_get_daily_stats_range(&start, &end)?
        .into_iter()
        .map(|(date, total_keys, total_clicks)| DailyStat {
            date,
            total_keys,
            total_clicks,
        })
        .collect())
}

#[tauri::command]
pub fn get_hourly_distribution(
    state: State<'_, AppState>,
    date: String,
) -> Result<Vec<(u8, u64)>, String> {
    state.db.try_get_hourly_distribution(&date)
}

#[tauri::command]
pub fn get_recent_activity(
    state: State<'_, AppState>,
    limit: u32,
) -> Result<Vec<ActivityItem>, String> {
    Ok(state
        .db
        .try_get_recent_activity(limit)?
        .into_iter()
        .map(|(timestamp, keycode, category, app_name)| ActivityItem {
            timestamp,
            keycode,
            category,
            app_name,
        })
        .collect())
}

#[tauri::command]
pub fn get_insights(state: State<'_, AppState>) -> Result<Insights, String> {
    let today = today_string();
    let week_start = date_days_ago(6);
    let prev_week_start = date_days_ago(13);
    let prev_week_end = date_days_ago(7);

    let week_data = state.db.try_get_daily_stats_range(&week_start, &today)?;
    let prev_week_data = state
        .db
        .try_get_daily_stats_range(&prev_week_start, &prev_week_end)?;

    let week_total: u64 = week_data.iter().map(|d| d.1).sum();
    let prev_week_total: u64 = prev_week_data.iter().map(|d| d.1).sum();

    let week_overview_change_pct = if prev_week_total > 0 {
        Some(((week_total as f64 - prev_week_total as f64) / prev_week_total as f64) * 100.0)
    } else {
        None
    };

    let top_keys: Vec<(u16, u64)> = state
        .db
        .try_get_heatmap_data("week")?
        .into_iter()
        .take(5)
        .collect();

    Ok(Insights {
        week_total,
        prev_week_total,
        week_overview_change_pct,
        top_keys,
    })
}

// ===== Phase 6: 音效类命令 =====

#[derive(Serialize)]
pub struct ThemeInfo {
    pub index: u8,
    pub name: String,
}

#[tauri::command]
pub fn get_theme_list() -> Vec<ThemeInfo> {
    SoundTheme::all()
        .iter()
        .map(|t| ThemeInfo {
            index: t.as_index(),
            name: t.name().to_string(),
        })
        .collect()
}

// ===== 静音快捷键命令（第一版保留的独立能力，静音时段/静音应用已裁掉） =====

#[tauri::command]
pub fn get_mute_combos(state: State<'_, AppState>) -> Vec<KeyCombo> {
    state.mute_shortcut.get_combos()
}

#[tauri::command]
pub fn add_mute_combo(
    state: State<'_, AppState>,
    keycode: u16,
    cmd: bool,
    shift: bool,
    ctrl: bool,
    opt: bool,
) -> Result<(), String> {
    state.mute_shortcut.add_combo_persisted(KeyCombo {
        keycode,
        cmd,
        shift,
        ctrl,
        opt,
    })
}

#[tauri::command]
pub fn remove_mute_combo(
    state: State<'_, AppState>,
    keycode: u16,
    cmd: bool,
    shift: bool,
    ctrl: bool,
    opt: bool,
) -> Result<(), String> {
    state.mute_shortcut.remove_combo_persisted(&KeyCombo {
        keycode,
        cmd,
        shift,
        ctrl,
        opt,
    })
}

#[tauri::command]
pub fn reset_mute_presets(state: State<'_, AppState>) -> Result<(), String> {
    state.mute_shortcut.reset_presets_persisted()
}

// ===== Phase 6: 数据管理 =====

#[tauri::command]
pub fn clear_all_data(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    state.db.clear_all_data()?;
    let _ = app.emit("data-cleared", ());
    Ok(())
}

// ===== 运行时健康（AUD-004） =====

#[tauri::command]
pub fn get_runtime_health(state: State<'_, AppState>) -> RuntimeHealthSnapshot {
    state.runtime_health.snapshot()
}
