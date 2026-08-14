import { useState, useRef } from 'react'
import {
  getMuteCombos,
  addMuteCombo,
  removeMuteCombo,
  resetMutePresets,
  KeyCombo,
} from '../lib/ipc'
import { useAsync, useMutation } from '../hooks/useAsync'
import { StateViews } from './StateViews'
import { KEYBOARD_LAYOUT, jsKeyCodeToMacKeyCode } from '../lib/keyboardLayout'

const keyLabelMap = new Map<number, string>()
for (const row of KEYBOARD_LAYOUT) {
  for (const k of row) keyLabelMap.set(k.keycode, k.label)
}

function comboLabel(c: KeyCombo): string {
  const parts: string[] = []
  if (c.ctrl) parts.push('⌃')
  if (c.opt) parts.push('⌥')
  if (c.shift) parts.push('⇧')
  if (c.cmd) parts.push('⌘')
  parts.push(keyLabelMap.get(c.keycode) ?? `键码 ${c.keycode}`)
  return parts.join(' ')
}

export function MuteComboEditor() {
  const { data, error, loading, retry } = useAsync(() => getMuteCombos(), [])
  const [capturing, setCapturing] = useState(false)
  const [hint, setHint] = useState<string | null>(null)
  const ref = useRef<HTMLDivElement>(null)
  // AUD-025：写操作统一走 mutation——pending 防重入、失败有错误反馈
  const mutation = useMutation((action: () => Promise<unknown>) => action(), retry)

  const handleCaptureKeyDown = async (e: React.KeyboardEvent) => {
    if (!capturing) return
    e.preventDefault()
    e.stopPropagation()
    if (e.key === 'Escape') {
      setCapturing(false)
      setHint(null)
      return
    }
    const ne = e.nativeEvent as KeyboardEvent & { keyCode?: number }
    const keycode = jsKeyCodeToMacKeyCode(ne.keyCode)
    if (keycode === null) {
      setHint('无法识别的按键（修饰键不能单独作为组合键），请重新按下组合键')
      return
    }
    const combo: KeyCombo = {
      keycode,
      cmd: e.metaKey,
      shift: e.shiftKey,
      ctrl: e.ctrlKey,
      opt: e.altKey,
    }
    // 至少需要一个修饰键，避免误触普通键
    if (!combo.cmd && !combo.shift && !combo.ctrl && !combo.opt) {
      setHint('需要同时按住 ⌘/⌥/⌃/⇧ 中至少一个修饰键')
      return
    }
    await mutation.run(() => addMuteCombo(combo))
    setCapturing(false)
    setHint(null)
  }

  const handleRemove = async (c: KeyCombo) => {
    await mutation.run(() => removeMuteCombo(c))
  }

  const handleReset = async () => {
    await mutation.run(() => resetMutePresets())
  }

  return (
    <div className="settings-section">
      <div className="settings-section-header">
        <div className="settings-section-title">静音快捷键</div>
        <button className="btn btn-small" disabled={mutation.pending} onClick={handleReset}>
          重置预设
        </button>
      </div>
      <div className="settings-hint">按下这些组合键时不播放音效，统计仍会记录</div>
      <StateViews
        loading={loading}
        error={error}
        empty={!data || data.length === 0}
        emptyText="暂无静音快捷键"
        onRetry={retry}
      >
        <div className="mute-combo-list">
          {(data ?? []).map((c, i) => (
            <div className="mute-combo-row" key={i}>
              <span className="combo-label">{comboLabel(c)}</span>
              <button
                className="btn btn-small btn-danger"
                disabled={mutation.pending}
                onClick={() => handleRemove(c)}
              >
                删除
              </button>
            </div>
          ))}
        </div>
      </StateViews>
      <div
        ref={ref}
        className={`key-capture ${capturing ? 'capturing' : ''}`}
        tabIndex={0}
        onKeyDown={handleCaptureKeyDown}
        onBlur={() => {
          setCapturing(false)
          setHint(null)
        }}
      >
        {capturing ? '请按下组合键（需含 ⌘/⌥/⌃/⇧ 之一），按 Esc 取消…' : '点击此处，然后按下要添加的组合键'}
        {capturing && hint && <span className="settings-hint">{hint}</span>}
        {!capturing && (
          <button
            className="btn btn-small"
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => {
              setCapturing(true)
              ref.current?.focus()
            }}
          >
            添加组合
          </button>
        )}
      </div>
      {mutation.error && (
        <div className="settings-message error" role="alert">
          保存失败：{mutation.error}
        </div>
      )}
    </div>
  )
}
