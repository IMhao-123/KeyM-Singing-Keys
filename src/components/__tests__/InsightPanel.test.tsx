import { describe, it, expect, vi, afterEach } from 'vitest'
import { render, screen, cleanup } from '@testing-library/react'

afterEach(cleanup)

const getInsights = vi.fn()

vi.mock('../../lib/ipc', () => ({
  getInsights: (...args: unknown[]) => getInsights(...args),
}))

import { InsightPanel } from '../InsightPanel'

const base = {
  week_total: 12345,
  prev_week_total: 10000,
  week_overview_change_pct: null as number | null,
  top_keys: [] as [number, number][],
}

describe('InsightPanel 洞察文案', () => {
  it('pct 为正：显示「比上周多打了 X.X%」（无多余正号）', async () => {
    getInsights.mockResolvedValue({ ...base, week_overview_change_pct: 12.34 })
    render(<InsightPanel />)
    await screen.findByText(/比上周多打了 12\.3%/)
    expect(screen.queryByText(/\+/)).toBeNull()
  })

  it('pct 为负：显示「比上周少打了 X.X%」', async () => {
    getInsights.mockResolvedValue({ ...base, week_overview_change_pct: -8 })
    render(<InsightPanel />)
    await screen.findByText(/比上周少打了 8\.0%/)
  })

  it('pct 接近 0：显示「与上周持平」且不带数字', async () => {
    getInsights.mockResolvedValue({ ...base, week_overview_change_pct: 0.01 })
    render(<InsightPanel />)
    const el = await screen.findByText(/与上周持平/)
    expect(el.textContent).not.toMatch(/%/)
  })

  it('pct 为 null 且本周有数据：显示数据不足说明', async () => {
    getInsights.mockResolvedValue({ ...base })
    render(<InsightPanel />)
    await screen.findByText(/上周数据不足，无法对比/)
  })

  it('top_keys 正常渲染', async () => {
    getInsights.mockResolvedValue({
      ...base,
      top_keys: [
        [0, 500],
        [49, 300],
      ],
    })
    render(<InsightPanel />)
    await screen.findByText(/最常敲击的键：A、space/)
  })
})
