import { ReactNode } from 'react'

function WarningIcon() {
  return (
    <svg
      className="state-icon-svg"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.6}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M12 4 L21 19.5 H3 Z" />
      <line x1={12} y1={10} x2={12} y2={14.5} />
      <circle cx={12} cy={17} r={0.4} fill="currentColor" />
    </svg>
  )
}

function EmptyIcon() {
  return (
    <svg
      className="state-icon-svg"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.6}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <rect x={4} y={6} width={16} height={13} rx={2} strokeDasharray="4 3" />
      <line x1={9} y1={12.5} x2={15} y2={12.5} opacity={0.5} />
    </svg>
  )
}

interface StateViewsProps {
  loading: boolean
  error: string | null
  empty?: boolean
  emptyText?: string
  onRetry?: () => void
  children: ReactNode
}

export function StateViews({
  loading,
  error,
  empty = false,
  emptyText = '暂无数据',
  onRetry,
  children,
}: StateViewsProps) {
  if (loading) {
    return (
      <div className="state-view">
        <div className="state-spinner" />
        <div className="state-text">加载中…</div>
      </div>
    )
  }
  if (error) {
    return (
      <div className="state-view">
        <div className="state-icon">
          <WarningIcon />
        </div>
        <div className="state-text">加载失败：{error}</div>
        {onRetry && (
          <button className="btn" onClick={onRetry}>
            重试
          </button>
        )}
      </div>
    )
  }
  if (empty) {
    return (
      <div className="state-view">
        <div className="state-icon">
          <EmptyIcon />
        </div>
        <div className="state-text">{emptyText}</div>
      </div>
    )
  }
  return <>{children}</>
}
