import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { render, screen, fireEvent, cleanup } from '@testing-library/react'
// vitest 运行于 Node，但仓库未安装 @types/node，这里压制类型错误仅取运行时 fs
// @ts-expect-error node:fs 无类型声明
import { readFileSync } from 'node:fs'

afterEach(cleanup)

const addMuteCombo = vi.fn().mockResolvedValue(undefined)
const getMuteCombos = vi.fn().mockResolvedValue([])

vi.mock('../../lib/ipc', () => ({
  getMuteCombos: (...args: unknown[]) => getMuteCombos(...args),
  addMuteCombo: (...args: unknown[]) => addMuteCombo(...args),
  removeMuteCombo: vi.fn().mockResolvedValue(undefined),
  resetMutePresets: vi.fn().mockResolvedValue(undefined),
}))

import { MuteComboEditor } from '../MuteComboEditor'

async function renderLoaded() {
  render(<MuteComboEditor />)
  await screen.findByText('暂无静音快捷键')
}

describe('MuteComboEditor 捕获交互', () => {
  beforeEach(() => {
    addMuteCombo.mockClear()
    getMuteCombos.mockResolvedValue([])
  })

  it('点击「添加组合」后捕获态保持开启，且焦点落在捕获框上', async () => {
    await renderLoaded()
    fireEvent.click(screen.getByText('添加组合'))
    const capture = screen.getByText(/请按下组合键/).closest('.key-capture') as HTMLElement
    expect(capture).not.toBeNull()
    expect(capture.className).toContain('capturing')
    expect(document.activeElement).toBe(capture)
  })

  it('捕获态下按 Esc 退出捕获', async () => {
    await renderLoaded()
    fireEvent.click(screen.getByText('添加组合'))
    const capture = screen.getByText(/请按下组合键/).closest('.key-capture') as HTMLElement
    fireEvent.keyDown(capture, { key: 'Escape' })
    expect(screen.getByText('添加组合')).toBeTruthy()
  })

  it('捕获态下捕获框失焦退出捕获', async () => {
    await renderLoaded()
    fireEvent.click(screen.getByText('添加组合'))
    const capture = screen.getByText(/请按下组合键/).closest('.key-capture') as HTMLElement
    fireEvent.blur(capture)
    expect(screen.getByText('添加组合')).toBeTruthy()
  })

  it('无修饰键的普通按键不会添加组合，捕获保持', async () => {
    await renderLoaded()
    fireEvent.click(screen.getByText('添加组合'))
    const capture = screen.getByText(/请按下组合键/).closest('.key-capture') as HTMLElement
    fireEvent.keyDown(capture, { key: 'c', keyCode: 67 })
    expect(addMuteCombo).not.toHaveBeenCalled()
    expect(capture.className).toContain('capturing')
    expect(screen.getByText(/需要同时按住/)).toBeTruthy()
  })

  it('按下 Cmd+C（JS keyCode 67）发送 macOS keycode 8 并退出捕获', async () => {
    await renderLoaded()
    fireEvent.click(screen.getByText('添加组合'))
    const capture = screen.getByText(/请按下组合键/).closest('.key-capture') as HTMLElement
    fireEvent.keyDown(capture, { key: 'c', keyCode: 67, metaKey: true })
    expect(addMuteCombo).toHaveBeenCalledWith({
      keycode: 8,
      cmd: true,
      shift: false,
      ctrl: false,
      opt: false,
    })
  })

  it('未知按键不调用 IPC 并提示', async () => {
    await renderLoaded()
    fireEvent.click(screen.getByText('添加组合'))
    const capture = screen.getByText(/请按下组合键/).closest('.key-capture') as HTMLElement
    fireEvent.keyDown(capture, { key: 'Unidentified', keyCode: 999, metaKey: true })
    expect(addMuteCombo).not.toHaveBeenCalled()
    expect(capture.className).toContain('capturing')
    expect(screen.getByText(/无法识别的按键/)).toBeTruthy()
  })

  it('纯修饰键按下不调用 IPC 并提示', async () => {
    await renderLoaded()
    fireEvent.click(screen.getByText('添加组合'))
    const capture = screen.getByText(/请按下组合键/).closest('.key-capture') as HTMLElement
    fireEvent.keyDown(capture, { key: 'Meta', keyCode: 91, metaKey: true })
    expect(addMuteCombo).not.toHaveBeenCalled()
    expect(capture.className).toContain('capturing')
    expect(screen.getByText(/无法识别的按键/)).toBeTruthy()
  })
})

// FRB-004：列表/行使用静音快捷键语义独立的类名，且 global.css 提供对应样式，
// 不再依赖已裁掉的 custom-sound-* 语义
describe('MuteComboEditor 样式类（FRB-004）', () => {
  it('列表使用 mute-combo-list/mute-combo-row，且无 custom-sound-* 残留', async () => {
    getMuteCombos.mockResolvedValue([
      { keycode: 8, cmd: true, shift: false, ctrl: false, opt: false },
    ])
    const { container } = render(<MuteComboEditor />)
    await screen.findByText('⌘ C')
    expect(container.querySelector('.mute-combo-list')).not.toBeNull()
    expect(container.querySelector('.mute-combo-row')).not.toBeNull()
    expect(container.querySelector('[class*="custom-sound"]')).toBeNull()
  })

  it('global.css 定义了 .mute-combo-list/.mute-combo-row', () => {
    const css = readFileSync('src/styles/global.css', 'utf-8')
    expect(css).toContain('.mute-combo-list')
    expect(css).toContain('.mute-combo-row')
  })

  it('组件源码不再引用 custom-sound 类名', () => {
    const src = readFileSync('src/components/MuteComboEditor.tsx', 'utf-8')
    expect(src).not.toContain('custom-sound')
  })
})
