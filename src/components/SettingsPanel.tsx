import { useState } from 'react'
import { save } from '@tauri-apps/plugin-dialog'
import {
  getSoundEnabled,
  toggleSound,
  getVolume,
  setVolume,
  exportDataToFile,
  clearAllData,
} from '../lib/ipc'
import { useAsync, useMutation, useRefreshEvents } from '../hooks/useAsync'
import { Toggle } from './Toggle'

export function SoundSettings() {
  const { data: enabled, retry: retryEnabled } = useAsync(() => getSoundEnabled(), [])
  const { data: volume, retry: retryVolume } = useAsync(() => getVolume(), [])
  // AUD-024：托盘/其他入口改动后同步主窗口显示
  useRefreshEvents(['sound-state-changed'], retryEnabled)
  useRefreshEvents(['volume-changed'], retryVolume)
  // AUD-025：写操作有错误反馈与 pending 防重入
  const toggleMutation = useMutation(toggleSound, retryEnabled)
  const volumeMutation = useMutation((value: number) => setVolume(value), retryVolume)
  const mutationError = toggleMutation.error ?? volumeMutation.error

  return (
    <div className="settings-section">
      <div className="settings-section-header">
        <div className="settings-section-title">音效</div>
        {enabled !== null && (
          <Toggle
            active={enabled}
            label="音效"
            disabled={toggleMutation.pending}
            onChange={toggleMutation.run}
          />
        )}
      </div>

      <div className="settings-row">
        <label className="settings-label" htmlFor="settings-volume">
          音量
        </label>
        <input
          id="settings-volume"
          aria-label="音量"
          type="range"
          className="slider"
          min={0}
          max={1}
          step={0.05}
          value={volume ?? 0.5}
          disabled={volumeMutation.pending}
          onChange={(e) => volumeMutation.run(parseFloat(e.target.value))}
        />
        <span className="settings-value">{Math.round((volume ?? 0.5) * 100)}%</span>
      </div>
      {mutationError && (
        <div className="settings-message error" role="alert">
          设置保存失败：{mutationError}
        </div>
      )}
    </div>
  )
}

export function DataSettings() {
  const [message, setMessage] = useState('')
  const [confirming, setConfirming] = useState(false)

  const handleExport = async (format: 'csv' | 'json') => {
    setMessage('')
    const path = await save({
      defaultPath: `keym-export.${format}`,
      filters: [{ name: format.toUpperCase(), extensions: [format] }],
    })
    if (!path) return
    try {
      await exportDataToFile(format, path)
      setMessage(`已导出到 ${path}`)
    } catch (e) {
      setMessage(`导出失败：${String(e)}`)
    }
  }

  const handleClear = async () => {
    if (!confirming) {
      setConfirming(true)
      return
    }
    try {
      await clearAllData()
      setMessage('已清除全部数据')
    } catch (e) {
      setMessage(`清除失败：${String(e)}`)
    }
    setConfirming(false)
  }

  return (
    <div className="settings-section">
      <div className="settings-section-title">数据管理</div>
      <div className="settings-row">
        <span className="settings-label">导出</span>
        <button className="btn btn-small" onClick={() => handleExport('csv')}>导出 CSV</button>
        <button className="btn btn-small" onClick={() => handleExport('json')}>导出 JSON</button>
      </div>
      <div className="settings-row">
        <span className="settings-label">危险区</span>
        <button
          className={`btn btn-small ${confirming ? 'btn-danger' : ''}`}
          onClick={handleClear}
          onBlur={() => setConfirming(false)}
        >
          {confirming ? '再点一次确认清除' : '清除全部数据'}
        </button>
      </div>
      {message && <div className="settings-message">{message}</div>}
    </div>
  )
}

export function AboutSection() {
  return (
    <div className="settings-section">
      <div className="settings-section-title">关于</div>
      <div className="settings-row">
        <span className="settings-label">版本</span>
        <span className="settings-value">键标 KeyM 1.0.0</span>
      </div>
      <div className="settings-hint">纯本地运行，数据不上传。键盘音效与打字统计工具。</div>
    </div>
  )
}
