import { getRecentActivity } from '../lib/ipc'
import { useAsync } from '../hooks/useAsync'
import { StateViews } from './StateViews'
import { formatTime } from '../lib/format'
import { KEYBOARD_LAYOUT } from '../lib/keyboardLayout'

const keyLabelMap = new Map<number, string>()
for (const row of KEYBOARD_LAYOUT) {
  for (const k of row) keyLabelMap.set(k.keycode, k.label)
}

const CATEGORY_NAMES: Record<string, string> = {
  normal: '普通',
  space: '空格',
  return: '回车',
  backspace: '退格',
  tab: 'Tab',
  escape: 'Esc',
  modifier: '修饰',
  arrow: '方向',
  other: '其他',
}

export function ActivityFeed() {
  const { data, error, loading, retry } = useAsync(() => getRecentActivity(20), [])

  return (
    <div className="card">
      <div className="card-title">最近活动</div>
      <StateViews loading={loading} error={error} empty={!data || data.length === 0} emptyText="暂无活动记录" onRetry={retry}>
        <div className="activity-feed">
          {(data ?? []).map((a, i) => (
            <div className="activity-row" key={`${a.timestamp}-${i}`}>
              <span className="activity-time">{formatTime(a.timestamp)}</span>
              <span className="activity-key">{keyLabelMap.get(a.keycode) ?? `键${a.keycode}`}</span>
              <span className="activity-cat">{CATEGORY_NAMES[a.category] ?? a.category}</span>
              {a.app_name && <span className="activity-app">{a.app_name}</span>}
            </div>
          ))}
        </div>
      </StateViews>
    </div>
  )
}
