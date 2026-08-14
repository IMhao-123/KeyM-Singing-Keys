// F4/AUD-030：音量滑杆基础规则 outline:none 抹掉了键盘焦点指示，
// 必须有 :focus-visible 替代样式，否则键盘用户无法看到焦点位置。
import { describe, it, expect } from 'vitest'
// @ts-expect-error node:fs 无类型声明
import { readFileSync } from 'node:fs'

const css = readFileSync('src/styles/global.css', 'utf-8')

describe('F4 滑杆焦点可见性', () => {
  it('.slider 提供 :focus-visible 焦点样式（非 none 的 outline）', () => {
    const rule = css.match(/\.slider:focus-visible\s*\{([^}]*)\}/)
    expect(rule, 'global.css 必须存在 .slider:focus-visible 规则').not.toBeNull()
    const body = rule![1]
    expect(body).toMatch(/outline:\s*2px\s+solid/)
    expect(body).toMatch(/outline-offset:/)
  })

  it('基础 .slider 规则的 outline:none 只作用于非键盘焦点场景', () => {
    // focus-visible 规则必须出现在基础规则之后，确保能覆盖 outline:none
    const baseIdx = css.indexOf('.slider {')
    const focusIdx = css.indexOf('.slider:focus-visible')
    expect(baseIdx).toBeGreaterThanOrEqual(0)
    expect(focusIdx).toBeGreaterThan(baseIdx)
  })
})
