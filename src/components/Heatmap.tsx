import { useState, useMemo, useRef } from 'react'
import { getHeatmapData } from '../lib/ipc'
import { useAsync } from '../hooks/useAsync'
import { KEYBOARD_LAYOUT, heatColor } from '../lib/keyboardLayout'
import { StateViews } from './StateViews'

type Period = 'today' | 'week' | 'month'

const PERIODS: { id: Period; label: string }[] = [
  { id: 'today', label: '今日' },
  { id: 'week', label: '本周' },
  { id: 'month', label: '本月' },
]

export function Heatmap() {
  const [period, setPeriod] = useState<Period>('today')
  const [debounced, setDebounced] = useState<Period>('today')
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  const { data, error, loading, retry } = useAsync(
    () => getHeatmapData(debounced),
    [debounced]
  )

  const handlePeriod = (p: Period) => {
    setPeriod(p)
    if (timerRef.current) clearTimeout(timerRef.current)
    timerRef.current = setTimeout(() => setDebounced(p), 250)
  }

  const heatMap = useMemo(() => {
    const m = new Map<number, number>()
    let max = 0
    if (data) {
      for (const [kc, count] of data) {
        m.set(kc, count)
        if (count > max) max = count
      }
    }
    return { m, max }
  }, [data])

  return (
    <div className="card">
      <div className="heatmap-header">
        <div className="card-title" style={{ marginBottom: 0 }}>键盘热力图</div>
        <div className="period-switch">
          {PERIODS.map((p) => (
            <button
              key={p.id}
              className={`period-btn ${period === p.id ? 'active' : ''}`}
              onClick={() => handlePeriod(p.id)}
            >
              {p.label}
            </button>
          ))}
        </div>
      </div>
      <StateViews loading={loading} error={error} onRetry={retry}>
        <div className="keyboard">
          {KEYBOARD_LAYOUT.map((row, ri) => (
            <div className="keyboard-row" key={ri}>
              {row.map((k) => {
                const count = heatMap.m.get(k.keycode) ?? 0
                const ratio = heatMap.max > 0 ? count / heatMap.max : 0
                return (
                  <div
                    key={k.keycode}
                    className="keycap"
                    style={{
                      flex: k.width ?? 1,
                      background: heatColor(ratio),
                      transition: 'background 0.3s',
                    }}
                    title={`${k.label}: ${count}`}
                  >
                    {k.label}
                  </div>
                )
              })}
            </div>
          ))}
        </div>
        <div className="heat-legend">
          <span className="legend-label">少</span>
          {Array.from({ length: 8 }, (_, i) => i / 7).map((r) => (
            <span key={r} className="legend-block" style={{ background: heatColor(r) }} />
          ))}
          <span className="legend-label">多</span>
        </div>
      </StateViews>
    </div>
  )
}
