import { useMemo, useState } from 'react'
import { DailyStat } from '../lib/ipc'
import { formatDate } from '../lib/format'

interface TrendChartProps {
  data: DailyStat[]
}

const W = 860
const H = 260
const PAD_L = 44
const PAD_R = 44
const PAD_T = 16
const PAD_B = 28

function buildAreaPath(values: number[], max: number): string {
  if (values.length === 0 || max <= 0) return ''
  const iw = W - PAD_L - PAD_R
  const ih = H - PAD_T - PAD_B
  const step = values.length > 1 ? iw / (values.length - 1) : 0
  const pts = values.map((v, i) => {
    const x = PAD_L + i * step
    const y = PAD_T + ih - (v / max) * ih
    return `${x.toFixed(1)},${y.toFixed(1)}`
  })
  const top = pts.map((p, i) => `${i === 0 ? 'M' : 'L'}${p}`).join(' ')
  const lastX = PAD_L + (values.length - 1) * step
  return `${top} L${lastX.toFixed(1)},${(PAD_T + ih).toFixed(1)} L${PAD_L},${(PAD_T + ih).toFixed(1)} Z`
}

function buildLinePath(values: number[], max: number): string {
  if (values.length === 0 || max <= 0) return ''
  const iw = W - PAD_L - PAD_R
  const ih = H - PAD_T - PAD_B
  const step = values.length > 1 ? iw / (values.length - 1) : 0
  return values
    .map((v, i) => {
      const x = PAD_L + i * step
      const y = PAD_T + ih - (v / max) * ih
      return `${i === 0 ? 'M' : 'L'}${x.toFixed(1)},${y.toFixed(1)}`
    })
    .join(' ')
}

export function TrendChart({ data }: TrendChartProps) {
  const [hover, setHover] = useState<number | null>(null)

  const keysValues = useMemo(() => data.map((d) => d.total_keys), [data])
  const maxKeys = useMemo(() => Math.max(1, ...keysValues), [keysValues])

  const areaPath = useMemo(() => buildAreaPath(keysValues, maxKeys), [keysValues, maxKeys])

  const handleMove = (e: React.MouseEvent<SVGSVGElement>) => {
    const rect = e.currentTarget.getBoundingClientRect()
    const x = ((e.clientX - rect.left) / rect.width) * W
    const iw = W - PAD_L - PAD_R
    const step = data.length > 1 ? iw / (data.length - 1) : 1
    const idx = Math.round((x - PAD_L) / step)
    setHover(idx >= 0 && idx < data.length ? idx : null)
  }

  const hoverX = (i: number) => {
    const iw = W - PAD_L - PAD_R
    const step = data.length > 1 ? iw / (data.length - 1) : 0
    return PAD_L + i * step
  }

  return (
    <div className="trend-chart-wrap">
      <svg
        viewBox={`0 0 ${W} ${H}`}
        className="trend-chart"
        onMouseMove={handleMove}
        onMouseLeave={() => setHover(null)}
      >
        {/* 网格线 */}
        {[0.25, 0.5, 0.75, 1].map((r) => {
          const y = PAD_T + (H - PAD_T - PAD_B) * (1 - r)
          return (
            <g key={r}>
              <line x1={PAD_L} y1={y} x2={W - PAD_R} y2={y} stroke="#33334d" strokeDasharray="3,4" />
              <text x={PAD_L - 6} y={y + 4} textAnchor="end" fontSize="10" fill="#8888a0">
                {Math.round(maxKeys * r)}
              </text>
            </g>
          )
        })}
        {/* 面积图 */}
        {areaPath && <path d={areaPath} fill="rgba(59,130,246,0.25)" stroke="none" />}
        {areaPath && (
          <path d={buildLinePath(keysValues, maxKeys)} fill="none" stroke="#3b82f6" strokeWidth="2" />
        )}
        {/* X 轴日期标签 */}
        {data.map((d, i) => {
          const show = data.length <= 10 || i % Math.ceil(data.length / 10) === 0
          if (!show) return null
          return (
            <text
              key={d.date}
              x={hoverX(i)}
              y={H - 8}
              textAnchor="middle"
              fontSize="10"
              fill="#8888a0"
            >
              {formatDate(d.date)}
            </text>
          )
        })}
        {/* 悬停指示 */}
        {hover !== null && data[hover] && (
          <g>
            <line
              x1={hoverX(hover)}
              y1={PAD_T}
              x2={hoverX(hover)}
              y2={H - PAD_B}
              stroke="#8888a0"
              strokeWidth="1"
            />
            <circle
              cx={hoverX(hover)}
              cy={PAD_T + (H - PAD_T - PAD_B) - (data[hover].total_keys / maxKeys) * (H - PAD_T - PAD_B)}
              r="4"
              fill="#3b82f6"
            />
          </g>
        )}
      </svg>
      {hover !== null && data[hover] && (
        <div className="trend-tooltip">
          <div className="tooltip-date">{data[hover].date}</div>
          <div>按键 {data[hover].total_keys.toLocaleString()}</div>
          <div>点击 {data[hover].total_clicks.toLocaleString()}</div>
        </div>
      )}
      <div className="trend-legend">
        <span className="legend-item"><span className="legend-dot" style={{ background: '#3b82f6' }} />按键量</span>
      </div>
    </div>
  )
}
