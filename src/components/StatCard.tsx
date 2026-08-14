import { useState, useEffect, useRef, useCallback } from 'react'
import { getStatsOverview } from '../lib/ipc'
import { usePolling, useRefreshEvents } from '../hooks/useAsync'
import { formatCompact } from '../lib/format'

function useAnimatedNumber(target: number): number {
  const [display, setDisplay] = useState(target)
  const rafRef = useRef<number>(0)
  const fromRef = useRef(target)

  useEffect(() => {
    const from = fromRef.current
    if (from === target) return
    const start = performance.now()
    const duration = 400
    const step = (now: number) => {
      const t = Math.min(1, (now - start) / duration)
      const eased = 1 - Math.pow(1 - t, 3)
      const val = from + (target - from) * eased
      setDisplay(val)
      if (t < 1) rafRef.current = requestAnimationFrame(step)
      else fromRef.current = target
    }
    rafRef.current = requestAnimationFrame(step)
    return () => cancelAnimationFrame(rafRef.current)
  }, [target])

  return display
}

function StatCard({ label, value, suffix }: { label: string; value: number; suffix?: string }) {
  const animated = useAnimatedNumber(value)
  return (
    <div className="card stat-card">
      <div className="stat-card-value">
        {formatCompact(Math.round(animated))}
        {suffix && <span className="stat-card-suffix">{suffix}</span>}
      </div>
      <div className="stat-card-label">{label}</div>
    </div>
  )
}

export function StatCards() {
  const [todayKeys, setTodayKeys] = useState(0)
  const [totalKeys, setTotalKeys] = useState(0)
  const [totalClicks, setTotalClicks] = useState(0)

  const refresh = useCallback(() => {
    getStatsOverview()
      .then((s) => {
        setTodayKeys(s.today_keys)
        setTotalKeys(s.total_keys)
        setTotalClicks(s.total_clicks)
      })
      .catch(() => {})
  }, [])

  useEffect(refresh, [refresh])
  usePolling(refresh, 5000)
  useRefreshEvents(['data-cleared'], refresh)

  return (
    <div className="stat-cards">
      <StatCard label="今日按键" value={todayKeys} />
      <StatCard label="累计按键" value={totalKeys} />
      <StatCard label="累计点击" value={totalClicks} />
    </div>
  )
}
