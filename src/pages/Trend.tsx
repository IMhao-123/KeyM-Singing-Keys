import { useState } from 'react'
import { getTrendData } from '../lib/ipc'
import { useAsync } from '../hooks/useAsync'
import { StateViews } from '../components/StateViews'
import { TrendChart } from '../components/TrendChart'
import { InsightPanel } from '../components/InsightPanel'

type Range = 7 | 30

export function Trend() {
  const [range, setRange] = useState<Range>(7)
  const { data, error, loading, retry } = useAsync(() => getTrendData(range), [range])

  return (
    <div className="page">
      <div className="card">
        <div className="trend-header">
          <div className="card-title">按键趋势</div>
          <div className="period-switch">
            {([7, 30] as Range[]).map((r) => (
              <button
                key={r}
                className={`period-btn ${range === r ? 'active' : ''}`}
                onClick={() => setRange(r)}
              >
                {r} 天
              </button>
            ))}
          </div>
        </div>
        <StateViews
          loading={loading}
          error={error}
          empty={!data || data.length === 0}
          emptyText="暂无趋势数据，先打一会儿字吧"
          onRetry={retry}
        >
          {data && <TrendChart data={data} />}
        </StateViews>
      </div>
      <InsightPanel />
    </div>
  )
}
