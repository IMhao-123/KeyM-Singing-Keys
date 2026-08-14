import { getInsights } from '../lib/ipc'
import { useAsync } from '../hooks/useAsync'
import { StateViews } from './StateViews'
import { formatNumber } from '../lib/format'
import { KEYBOARD_LAYOUT } from '../lib/keyboardLayout'
import type { ReactNode } from 'react'

type InsightIconKind = 'chart' | 'keyboard'

const INSIGHT_SHAPES: Record<InsightIconKind, ReactNode> = {
  chart: (
    <>
      <polyline points="3,17 8,11 12,14 21,4" />
      <line x1={3} y1={21} x2={21} y2={21} opacity={0.4} />
    </>
  ),
  keyboard: (
    <>
      <rect x={3} y={7} width={18} height={11} rx={2} />
      <line x1={6.5} y1={11} x2={8.5} y2={11} />
      <line x1={11} y1={11} x2={13} y2={11} />
      <line x1={15.5} y1={11} x2={17.5} y2={11} />
      <line x1={7} y1={14.5} x2={17} y2={14.5} />
    </>
  ),
}

function InsightIcon({ kind }: { kind: InsightIconKind }) {
  return (
    <svg
      className="insight-icon-svg"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.6}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {INSIGHT_SHAPES[kind]}
    </svg>
  )
}

const keyLabelMap = new Map<number, string>()
for (const row of KEYBOARD_LAYOUT) {
  for (const k of row) keyLabelMap.set(k.keycode, k.label)
}

export function InsightPanel() {
  const { data, error, loading, retry } = useAsync(() => getInsights(), [])

  const insights: { icon: InsightIconKind; text: string }[] = []
  if (data) {
    if (data.week_overview_change_pct !== null) {
      const pct = data.week_overview_change_pct
      const trend =
        Math.abs(pct) < 0.05
          ? '与上周持平'
          : pct > 0
            ? `比上周多打了 ${pct.toFixed(1)}%`
            : `比上周少打了 ${Math.abs(pct).toFixed(1)}%`
      insights.push({
        icon: 'chart',
        text: `本周共 ${formatNumber(data.week_total)} 次按键，${trend}`,
      })
    } else if (data.week_total > 0) {
      insights.push({
        icon: 'chart',
        text: `本周共 ${formatNumber(data.week_total)} 次按键（上周数据不足，无法对比）`,
      })
    }
    if (data.top_keys.length > 0) {
      const top3 = data.top_keys
        .slice(0, 3)
        .map(([kc]) => keyLabelMap.get(kc) ?? `键${kc}`)
        .join('、')
      insights.push({ icon: 'keyboard', text: `最常敲击的键：${top3}` })
    }
  }

  return (
    <div className="card">
      <div className="card-title">洞察</div>
      <StateViews loading={loading} error={error} empty={insights.length === 0} emptyText="数据积累中，稍后再来看看" onRetry={retry}>
        <div className="insight-list">
          {insights.map((ins, i) => (
            <div className="insight-item" key={i}>
              <span className="insight-icon">
                <InsightIcon kind={ins.icon} />
              </span>
              <span className="insight-text">{ins.text}</span>
            </div>
          ))}
        </div>
      </StateViews>
    </div>
  )
}
