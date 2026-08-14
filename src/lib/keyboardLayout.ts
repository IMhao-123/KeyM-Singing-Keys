// 60% 配列 5 行键盘布局（macOS virtual keycode）
export interface KeyDef {
  keycode: number
  label: string
  width?: number // 单位宽度倍数，默认 1
}

export const KEYBOARD_LAYOUT: KeyDef[][] = [
  // 数字行
  [
    { keycode: 53, label: 'esc' },
    { keycode: 18, label: '1' },
    { keycode: 19, label: '2' },
    { keycode: 20, label: '3' },
    { keycode: 21, label: '4' },
    { keycode: 23, label: '5' },
    { keycode: 22, label: '6' },
    { keycode: 26, label: '7' },
    { keycode: 28, label: '8' },
    { keycode: 25, label: '9' },
    { keycode: 29, label: '0' },
    { keycode: 27, label: '-' },
    { keycode: 24, label: '=' },
    { keycode: 51, label: '⌫', width: 1.6 },
  ],
  // QWERTY 行
  [
    { keycode: 48, label: '⇥', width: 1.6 },
    { keycode: 12, label: 'Q' },
    { keycode: 13, label: 'W' },
    { keycode: 14, label: 'E' },
    { keycode: 15, label: 'R' },
    { keycode: 17, label: 'T' },
    { keycode: 16, label: 'Y' },
    { keycode: 32, label: 'U' },
    { keycode: 34, label: 'I' },
    { keycode: 31, label: 'O' },
    { keycode: 35, label: 'P' },
    { keycode: 33, label: '[' },
    { keycode: 30, label: ']' },
    { keycode: 42, label: '\\', width: 1.4 },
  ],
  // ASDF 行
  [
    { keycode: 57, label: '⇪', width: 1.8 },
    { keycode: 0, label: 'A' },
    { keycode: 1, label: 'S' },
    { keycode: 2, label: 'D' },
    { keycode: 3, label: 'F' },
    { keycode: 5, label: 'G' },
    { keycode: 4, label: 'H' },
    { keycode: 38, label: 'J' },
    { keycode: 40, label: 'K' },
    { keycode: 37, label: 'L' },
    { keycode: 41, label: ';' },
    { keycode: 39, label: "'" },
    { keycode: 36, label: '↩', width: 1.8 },
  ],
  // ZXCV 行
  [
    { keycode: 56, label: '⇧', width: 2.3 },
    { keycode: 6, label: 'Z' },
    { keycode: 7, label: 'X' },
    { keycode: 8, label: 'C' },
    { keycode: 9, label: 'V' },
    { keycode: 11, label: 'B' },
    { keycode: 45, label: 'N' },
    { keycode: 46, label: 'M' },
    { keycode: 43, label: ',' },
    { keycode: 47, label: '.' },
    { keycode: 44, label: '/' },
    { keycode: 60, label: '⇧', width: 2.3 },
  ],
  // 底行
  [
    { keycode: 59, label: '⌃', width: 1.3 },
    { keycode: 58, label: '⌥', width: 1.3 },
    { keycode: 55, label: '⌘', width: 1.5 },
    { keycode: 49, label: 'space', width: 6 },
    { keycode: 54, label: '⌘', width: 1.5 },
    { keycode: 61, label: '⌥', width: 1.3 },
    { keycode: 123, label: '←' },
    { keycode: 126, label: '↑' },
    { keycode: 125, label: '↓' },
    { keycode: 124, label: '→' },
  ],
]

// JS KeyboardEvent.keyCode（废弃但 Tauri webview 仍提供）→ macOS virtual keycode
// 纯修饰键（Shift/Ctrl/Alt/Meta）不单独映射，返回 null
const JS_TO_MAC_KEYCODE: Record<number, number> = {
  // 字母
  65: 0, // A
  66: 11, // B
  67: 8, // C
  68: 2, // D
  69: 14, // E
  70: 3, // F
  71: 5, // G
  72: 4, // H
  73: 34, // I
  74: 38, // J
  75: 40, // K
  76: 37, // L
  77: 46, // M
  78: 45, // N
  79: 31, // O
  80: 35, // P
  81: 12, // Q
  82: 15, // R
  83: 1, // S
  84: 17, // T
  85: 32, // U
  86: 9, // V
  87: 13, // W
  88: 7, // X
  89: 16, // Y
  90: 6, // Z
  // 数字
  49: 18, // 1
  50: 19, // 2
  51: 20, // 3
  52: 21, // 4
  53: 23, // 5
  54: 22, // 6
  55: 26, // 7
  56: 28, // 8
  57: 25, // 9
  48: 29, // 0
  // 符号
  189: 27, // -
  187: 24, // =
  219: 33, // [
  221: 30, // ]
  220: 42, // \
  186: 41, // ;
  222: 39, // '
  188: 43, // ,
  190: 47, // .
  191: 44, // /
  192: 50, // `
  // 功能键
  13: 36, // Enter
  8: 51, // Backspace
  9: 48, // Tab
  32: 49, // Space
  27: 53, // Esc
  20: 57, // CapsLock
  // 方向键
  37: 123, // ←
  38: 126, // ↑
  39: 124, // →
  40: 125, // ↓
}

export function jsKeyCodeToMacKeyCode(jsKeyCode: number | undefined | null): number | null {
  if (jsKeyCode === undefined || jsKeyCode === null) return null
  return JS_TO_MAC_KEYCODE[jsKeyCode] ?? null
}

// 热力色阶：0..1 → 8 级颜色
export function heatColor(ratio: number): string {
  const r = Math.max(0, Math.min(1, ratio))
  if (r <= 0) return 'rgba(255,255,255,0.04)'
  if (r < 0.15) return 'rgba(59,130,246,0.25)'
  if (r < 0.3) return 'rgba(59,130,246,0.45)'
  if (r < 0.45) return 'rgba(59,130,246,0.65)'
  if (r < 0.6) return 'rgba(99,102,241,0.75)'
  if (r < 0.75) return 'rgba(168,85,247,0.8)'
  if (r < 0.9) return 'rgba(233,69,96,0.85)'
  return 'rgba(233,69,96,1)'
}
