import { getAppStats } from '../lib/ipc'
import { useAsync } from '../hooks/useAsync'
import { StateViews } from './StateViews'
import { formatNumber } from '../lib/format'

export function AppRanking() {
  const { data, error, loading, retry } = useAsync(() => getAppStats(), [])
  const max = data && data.length > 0 ? data[0][1] : 0

  return (
    <div className="card">
      <div className="card-title">今日应用排行</div>
      <StateViews loading={loading} error={error} empty={!data || data.length === 0} emptyText="今日暂无应用数据" onRetry={retry}>
        <div className="app-ranking">
          {(data ?? []).slice(0, 8).map(([name, count]) => (
            <div className="app-row" key={name}>
              <span className="app-name" title={name}>{name}</span>
              <div className="app-bar-wrap">
                <div
                  className="app-bar"
                  style={{ width: max > 0 ? `${(count / max) * 100}%` : 0 }}
                />
              </div>
              <span className="app-count">{formatNumber(count)}</span>
            </div>
          ))}
        </div>
      </StateViews>
    </div>
  )
}
