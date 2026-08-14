import type { ReactNode } from 'react'

const ICONS: Record<number, ReactNode> = {
  0: (
    <>
      <rect x="6" y="3.5" width="12" height="17" rx="2.5" />
      <circle cx="12" cy="17.2" r="0.9" fill="currentColor" stroke="none" />
    </>
  ),
  1: (
    <>
      <rect x="3.5" y="3.5" width="17" height="17" rx="3.5" />
      <rect x="8" y="8" width="8" height="8" rx="1.5" />
    </>
  ),
  2: (
    <>
      <rect x="7" y="3" width="10" height="5" rx="1" />
      <path d="M4.5 9h15v5.5a3 3 0 0 1-3 3h-9a3 3 0 0 1-3-3V9z" />
      <path d="M8.5 13h7" />
    </>
  ),
  3: (
    <>
      <circle cx="9.5" cy="12" r="5" />
      <path d="M15.64 7.7A7.5 7.5 0 0 1 15.64 16.3" />
    </>
  ),
}

// id ≥ 4 的主题用参数化图形（圆形/三角/菱形/星形等）区分
function GenericIcon({ id }: { id: number }) {
  const shapes: ReactNode[] = [
    <circle key="c" cx="12" cy="12" r="7" />,
    <path key="t" d="M12 4l8 16H4z" />,
    <path key="d" d="M12 3l7 9-7 9-7-9z" />,
    <path key="s" d="M12 3l2.5 6.5H21l-5 4.5 2 7-6-4-6 4 2-7-5-4.5h6.5z" />,
    <rect key="r" x="5" y="5" width="14" height="14" rx="2" transform="rotate(45 12 12)" />,
    <path key="w" d="M4 12c2-4 4-4 6 0s4 4 6 0 3-3 4-2" />,
    <path key="b" d="M6 18V8a6 6 0 0 1 12 0v10l-3-3-3 3-3-3z" />,
  ]
  return <>{shapes[id % shapes.length]}</>
}

export function ThemeIcon({ id }: { id: number }) {
  return (
    <svg
      className="theme-icon"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.6}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {ICONS[id] ?? <GenericIcon id={id} />}
    </svg>
  )
}
