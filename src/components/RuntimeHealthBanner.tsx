import { getRuntimeHealth } from '../lib/ipc'
import { useAsync, usePolling } from '../hooks/useAsync'

function messages(health: Awaited<ReturnType<typeof getRuntimeHealth>>): string[] {
  const result: string[] = []
  if (health.input.status === 'permission_denied') {
    result.push(health.input.message ?? '缺少输入监控权限。请在系统设置中授权后重新启动应用。')
  } else if (health.input.status === 'failed') {
    result.push(health.input.message ?? '键盘监听不可用。')
  }
  if (health.audio.status === 'failed') {
    result.push(health.audio.message ?? '音频输出不可用。统计功能仍可继续使用。')
  } else if (health.audio.status === 'recovering') {
    result.push(health.audio.message ?? '音频输出正在恢复。')
  }
  if (health.database.status === 'failed') {
    result.push(health.database.message ?? '统计数据保存失败。请保留现场并重试。')
  }
  if (health.dropped_input_events > 0) {
    result.push(`输入处理队列已丢弃 ${health.dropped_input_events} 个事件。`)
  }
  return result
}

export function RuntimeHealthBanner({ compact = false }: { compact?: boolean }) {
  const { data, retry } = useAsync(getRuntimeHealth, [])
  usePolling(retry, 3000)
  if (!data) return null
  const problems = messages(data)
  if (problems.length === 0) return null
  return (
    <div className={`runtime-health ${compact ? 'compact' : ''}`} role="alert">
      {problems.map((message) => <div key={message}>{message}</div>)}
    </div>
  )
}