import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { Toggle } from '../Toggle'

afterEach(cleanup)

describe('AUD-030 保留控件无障碍', () => {
  it('开关是可键盘激活的原生 button（role=switch + aria）', () => {
    const onChange = vi.fn()
    render(<Toggle active={false} onChange={onChange} label="音效" />)
    const toggle = screen.getByRole('switch', { name: '音效' })
    expect(toggle.tagName).toBe('BUTTON')
    expect(toggle.getAttribute('aria-checked')).toBe('false')
    fireEvent.keyDown(toggle, { key: 'Enter' })
    fireEvent.click(toggle)
    expect(onChange).toHaveBeenCalled()
  })

  it('pending 时禁用，防止重入', () => {
    render(<Toggle active={true} onChange={() => {}} label="音效" disabled />)
    expect(screen.getByRole('switch', { name: '音效' })).toHaveProperty('disabled', true)
  })
})
