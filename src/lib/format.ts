export function formatNumber(n: number): string {
  return n.toLocaleString('zh-CN')
}

export function formatCompact(n: number): string {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + 'M'
  if (n >= 10_000) return (n / 1_000).toFixed(1) + 'K'
  return n.toLocaleString('zh-CN')
}

export function formatTime(timestamp: number): string {
  const d = new Date(timestamp)
  return d.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })
}

export function formatDate(dateStr: string): string {
  const parts = dateStr.split('-')
  if (parts.length === 3) return `${parseInt(parts[1])}/${parseInt(parts[2])}`
  return dateStr
}

export function todayString(): string {
  const d = new Date()
  const y = d.getFullYear()
  const m = String(d.getMonth() + 1).padStart(2, '0')
  const day = String(d.getDate()).padStart(2, '0')
  return `${y}-${m}-${day}`
}
