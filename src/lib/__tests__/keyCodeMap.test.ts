import { describe, it, expect } from 'vitest'
import { jsKeyCodeToMacKeyCode, KEYBOARD_LAYOUT } from '../keyboardLayout'

describe('jsKeyCodeToMacKeyCode', () => {
  it('映射字母键', () => {
    expect(jsKeyCodeToMacKeyCode(65)).toBe(0) // A
    expect(jsKeyCodeToMacKeyCode(67)).toBe(8) // C
    expect(jsKeyCodeToMacKeyCode(90)).toBe(6) // Z
  })

  it('映射数字键', () => {
    expect(jsKeyCodeToMacKeyCode(49)).toBe(18) // 1
    expect(jsKeyCodeToMacKeyCode(48)).toBe(29) // 0
  })

  it('映射常用功能键', () => {
    expect(jsKeyCodeToMacKeyCode(32)).toBe(49) // Space
    expect(jsKeyCodeToMacKeyCode(13)).toBe(36) // Enter
    expect(jsKeyCodeToMacKeyCode(8)).toBe(51) // Backspace
    expect(jsKeyCodeToMacKeyCode(9)).toBe(48) // Tab
    expect(jsKeyCodeToMacKeyCode(27)).toBe(53) // Esc
  })

  it('映射方向键', () => {
    expect(jsKeyCodeToMacKeyCode(37)).toBe(123) // ←
    expect(jsKeyCodeToMacKeyCode(38)).toBe(126) // ↑
    expect(jsKeyCodeToMacKeyCode(39)).toBe(124) // →
    expect(jsKeyCodeToMacKeyCode(40)).toBe(125) // ↓
  })

  it('映射常见符号键', () => {
    expect(jsKeyCodeToMacKeyCode(189)).toBe(27) // -
    expect(jsKeyCodeToMacKeyCode(187)).toBe(24) // =
    expect(jsKeyCodeToMacKeyCode(219)).toBe(33) // [
    expect(jsKeyCodeToMacKeyCode(221)).toBe(30) // ]
    expect(jsKeyCodeToMacKeyCode(220)).toBe(42) // \
    expect(jsKeyCodeToMacKeyCode(186)).toBe(41) // ;
    expect(jsKeyCodeToMacKeyCode(222)).toBe(39) // '
    expect(jsKeyCodeToMacKeyCode(188)).toBe(43) // ,
    expect(jsKeyCodeToMacKeyCode(190)).toBe(47) // .
    expect(jsKeyCodeToMacKeyCode(191)).toBe(44) // /
  })

  it('未知 keyCode 返回 null', () => {
    expect(jsKeyCodeToMacKeyCode(999)).toBeNull()
    expect(jsKeyCodeToMacKeyCode(0)).toBeNull()
    expect(jsKeyCodeToMacKeyCode(undefined)).toBeNull()
  })

  it('纯修饰键返回 null（不单独录入）', () => {
    expect(jsKeyCodeToMacKeyCode(16)).toBeNull() // Shift
    expect(jsKeyCodeToMacKeyCode(17)).toBeNull() // Ctrl
    expect(jsKeyCodeToMacKeyCode(18)).toBeNull() // Alt
    expect(jsKeyCodeToMacKeyCode(91)).toBeNull() // Meta
  })

  it('覆盖 KEYBOARD_LAYOUT 中全部非修饰键', () => {
    const MAC_MODIFIER_KEYCODES = new Set([54, 55, 56, 58, 59, 60, 61])
    const mapped = new Set<number>()
    for (let js = 0; js < 256; js++) {
      const mac = jsKeyCodeToMacKeyCode(js)
      if (mac !== null) mapped.add(mac)
    }
    for (const row of KEYBOARD_LAYOUT) {
      for (const key of row) {
        if (MAC_MODIFIER_KEYCODES.has(key.keycode)) continue
        expect(mapped.has(key.keycode), `布局键 ${key.label} (keycode ${key.keycode}) 未被映射覆盖`).toBe(true)
      }
    }
  })
})
