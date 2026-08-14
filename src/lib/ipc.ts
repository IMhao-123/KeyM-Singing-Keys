import { invoke } from '@tauri-apps/api/core'

// ===== 类型定义 =====

export interface StatsOverview {
  total_keys: number
  total_clicks: number
  today_keys: number
}

export interface DailyStat {
  date: string
  total_keys: number
  total_clicks: number
}

export interface ActivityItem {
  timestamp: number
  keycode: number
  category: string
  app_name: string | null
}

export interface Insights {
  week_total: number
  prev_week_total: number
  week_overview_change_pct: number | null
  top_keys: [number, number][]
}

export interface ThemeInfo {
  index: number
  name: string
}

export interface KeyCombo {
  keycode: number
  cmd: boolean
  shift: boolean
  ctrl: boolean
  opt: boolean
}

// ===== 音效 =====

export const getSoundEnabled = () => invoke<boolean>('get_sound_enabled')
export const toggleSound = () => invoke<boolean>('toggle_sound')
export const setVolume = (volume: number) => invoke<number>('set_volume', { volume })
export const getVolume = () => invoke<number>('get_volume')
export const setTheme = (theme: number) => invoke<number>('set_theme', { theme })
export const getTheme = () => invoke<number>('get_theme')
export const getThemeList = () => invoke<ThemeInfo[]>('get_theme_list')

// ===== 统计 =====

export const getStatsOverview = () => invoke<StatsOverview>('get_stats_overview')
export const getAppStats = () => invoke<[string, number][]>('get_app_stats')
export const getKeycodeStats = () => invoke<[number, number][]>('get_keycode_stats')
export const getHeatmapData = (period: string) =>
  invoke<[number, number][]>('get_heatmap_data', { period })
export const getTrendData = (days: number) => invoke<DailyStat[]>('get_trend_data', { days })
export const getHourlyDistribution = (date: string) =>
  invoke<[number, number][]>('get_hourly_distribution', { date })
export const getRecentActivity = (limit: number) =>
  invoke<ActivityItem[]>('get_recent_activity', { limit })
export const getInsights = () => invoke<Insights>('get_insights')

// ===== 静音快捷键（静音时段/静音应用已随第一版裁掉） =====

export const getMuteCombos = () => invoke<KeyCombo[]>('get_mute_combos')
export const addMuteCombo = (combo: KeyCombo) =>
  invoke<void>('add_mute_combo', { ...combo })
export const removeMuteCombo = (combo: KeyCombo) =>
  invoke<void>('remove_mute_combo', { ...combo })
export const resetMutePresets = () => invoke<void>('reset_mute_presets')

// ===== 数据 =====

export const exportDataToFile = (format: string, path: string) =>
  invoke<void>('export_data_to_file', { format, path })
export const clearAllData = () => invoke<void>('clear_all_data')

// ===== 运行时健康（AUD-004） =====

export type ServiceStatus = 'starting' | 'running' | 'recovering' | 'permission_denied' | 'failed'

export interface ServiceHealth {
  status: ServiceStatus
  message: string | null
}

export interface RuntimeHealthSnapshot {
  input: ServiceHealth
  audio: ServiceHealth
  database: ServiceHealth
  dropped_input_events: number
}

export const getRuntimeHealth = () => invoke<RuntimeHealthSnapshot>('get_runtime_health')
