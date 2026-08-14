import { useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { Toggle } from './Toggle'
import { ThemeIcon } from './ThemeIcon'

interface ThemeInfo {
  index: number
  name: string
}

export function Popup() {
  const [enabled, setEnabled] = useState(true)
  const [volume, setVolume] = useState(0.5)
  const [theme, setTheme] = useState(0)
  const [themes, setThemes] = useState<ThemeInfo[]>([])
  const [loading, setLoading] = useState(true)

  const [totalKeys, setTotalKeys] = useState(0)
  const [todayKeys, setTodayKeys] = useState(0)
  const [totalClicks, setTotalClicks] = useState(0)

  useEffect(() => {
    Promise.all([
      invoke<boolean>('get_sound_enabled'),
      invoke<number>('get_volume'),
      invoke<number>('get_theme'),
      invoke<ThemeInfo[]>('get_theme_list'),
      invoke<{total_keys: number, total_clicks: number, today_keys: number}>('get_stats_overview'),
    ]).then(([e, v, t, tl, s]) => {
      setEnabled(e)
      setVolume(v)
      setTheme(t)
      setThemes(tl)
      setTotalKeys(s.total_keys)
      setTodayKeys(s.today_keys)
      setTotalClicks(s.total_clicks)
      setLoading(false)
    })

    // 监听主窗口设置变更，保持两端状态一致
    const unlisteners = [
      listen<boolean>('sound-state-changed', (e) => setEnabled(e.payload)),
      listen<number>('volume-changed', (e) => setVolume(e.payload)),
      listen<number>('theme-changed', (e) => setTheme(e.payload)),
    ]
    return () => {
      unlisteners.forEach((p) => p.then((un) => un()))
    }
  }, [])

  const handleToggle = () => {
    invoke<boolean>('toggle_sound').then(setEnabled)
  }

  const handleVolume = (val: number) => {
    setVolume(val)
    invoke<number>('set_volume', { volume: val })
  }

  const handleTheme = (t: number) => {
    invoke<number>('set_theme', { theme: t }).then(setTheme)
  }

  if (loading) {
    return (
      <div className="popup-scroll">
        <div className="container">
          <div className="state-view">
            <div className="state-spinner" />
          </div>
        </div>
      </div>
    )
  }

  return (
    <div className="popup-scroll">
      <div className="container">
      <div className="header">键标</div>

      {/* 音效控制 */}
      <div className="row">
        <span className="label">音效</span>
        <Toggle active={enabled} onChange={handleToggle} />
      </div>
      <div className="row">
        <span className="label">音量</span>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <input
            type="range"
            className="slider"
            min={0}
            max={1}
            step={0.05}
            value={volume}
            onChange={(e) => handleVolume(parseFloat(e.target.value))}
          />
          <span className="value">{Math.round(volume * 100)}%</span>
        </div>
      </div>

      {/* 主题选择 */}
      <div className="section-title">音效主题</div>
      <div className="theme-grid">
        {themes.map(t => (
          <div
            key={t.index}
            className={`theme-btn ${theme === t.index ? 'active' : ''}`}
            onClick={() => handleTheme(t.index)}
          >
            <ThemeIcon id={t.index} />
            <span className="theme-name">{t.name}</span>
          </div>
        ))}
      </div>

      {/* 统计概览 */}
      <div className="section-title">统计</div>
      <div className="stats-grid">
        <div className="stat-item">
          <span className="stat-value">{totalKeys.toLocaleString()}</span>
          <span className="stat-label">总按键</span>
        </div>
        <div className="stat-item">
          <span className="stat-value">{todayKeys.toLocaleString()}</span>
          <span className="stat-label">今日</span>
        </div>
        <div className="stat-item">
          <span className="stat-value">{totalClicks.toLocaleString()}</span>
          <span className="stat-label">总点击</span>
        </div>
      </div>
      </div>
    </div>
  )
}