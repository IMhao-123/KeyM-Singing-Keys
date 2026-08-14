// AUD-033 / SR-02 / FRB-003：窗口生命周期与 Popup 定位的结构化断言。
// - AUD-033：主窗口关闭后托盘必须能恢复主界面。
// - SR-02：首次创建与重建必须共用同一个绑定 CloseRequested→隐藏 的 builder 入口。
// - FRB-003：托盘打开 Popup 前必须先计算并设置位置（贴菜单栏/屏幕顶部，不越屏）。
import { describe, it, expect } from 'vitest'
// @ts-expect-error node:fs 无类型声明
import { readFileSync } from 'node:fs'

const mainSrc = () => readFileSync('src-tauri/src/main.rs', 'utf-8')

/** 截取 main.rs 中某个 fn 的函数体（按花括号配平） */
function extractFnBody(src: string, name: string): string {
  const start = src.indexOf(`fn ${name}`)
  if (start < 0) throw new Error(`main.rs 中找不到 fn ${name}`)
  const open = src.indexOf('{', start)
  let depth = 0
  for (let i = open; i < src.length; i++) {
    if (src[i] === '{') depth++
    if (src[i] === '}') {
      depth--
      if (depth === 0) return src.slice(open, i + 1)
    }
  }
  throw new Error(`fn ${name} 函数体花括号未闭合`)
}

describe('AUD-033/SR-02 主窗口生命周期', () => {
  it('存在统一的 build_main_window 入口，且内部绑定 CloseRequested 隐藏', () => {
    const src = mainSrc()
    const body = extractFnBody(src, 'build_main_window')
    expect(body).toContain('WebviewWindowBuilder::new')
    expect(body).toContain('WindowEvent::CloseRequested')
    expect(body).toContain('prevent_close()')
    expect(body).toContain('.hide()')
  })

  it('CloseRequested 隐藏绑定只有一个出口（首次创建与重建共用）', () => {
    const src = mainSrc()
    expect(src.match(/WindowEvent::CloseRequested/g)?.length).toBe(1)
    expect(src.match(/WebviewWindowBuilder::new/g)?.length).toBe(1)
  })

  it('setup 首次创建走 build_main_window', () => {
    const src = mainSrc()
    const setupStart = src.indexOf('.setup(')
    expect(setupStart).toBeGreaterThan(-1)
    const setupSlice = src.slice(setupStart, setupStart + 3000)
    expect(setupSlice).toContain('build_main_window(')
  })

  it('show_main_window 在窗口不存在时经 build_main_window 重建并 show/focus', () => {
    const src = mainSrc()
    const body = extractFnBody(src, 'show_main_window')
    expect(body).toContain('get_webview_window("main")')
    expect(body).toContain('build_main_window(')
    expect(body).toContain('.show()')
    expect(body).toContain('.set_focus()')
  })
})

describe('FRB-005 LaunchServices 启动白屏', () => {
  // 根因：经 open(LaunchServices) 启动时，setup 阶段以 visible(true) 直接亮窗，
  // AppKit 在应用完成启动前 order-in 的窗口不会向 WebKit 投递 occlusion/visibility
  // 更新（页面 ActivityState 永远停在 IsVisibleAndOccluded 缺 IsVisible/WindowIsActive），
  // 数秒后 WebKit 将图层标记为 volatile 并停止合成 → 白屏（实测 10/10 复现，
  // 激活应用后立即恢复）。修复：窗口隐藏创建，待 RunEvent::Ready（应用完成启动）
  // 后再统一 show/focus，禁止用固定 sleep 掩盖时序问题。
  it('build_main_window 必须隐藏创建窗口（禁止 setup 阶段 visible(true) 亮窗）', () => {
    const src = mainSrc()
    const body = extractFnBody(src, 'build_main_window')
    expect(body).toContain('.visible(false)')
    expect(body).not.toContain('.visible(true)')
  })

  it('首屏显示必须挂在 RunEvent::Ready 之后，且先激活应用（事件驱动，非 sleep）', () => {
    const src = mainSrc()
    expect(src).toContain('RunEvent::Ready')
    const runIdx = src.indexOf('.run(|')
    expect(runIdx, '必须改用 build().run(|handle, event|) 形式挂载 Ready 处理').toBeGreaterThan(-1)
    const runSlice = src.slice(runIdx, runIdx + 600)
    expect(runSlice).toContain('RunEvent::Ready')
    // UIElement 应用经 LaunchServices 启动不会被激活，未激活应用的窗口收不到
    // occlusion 更新 → WebKit 停止合成 → 白屏；必须显式 activate()
    expect(runSlice).toContain('NSApplication')
    expect(runSlice).toContain('.activate()')
    expect(runSlice).toContain('show_main_window(')
    // 激活必须先于亮窗，否则窗口又在非激活状态下 order-in
    expect(runSlice.indexOf('.activate()')).toBeLessThan(runSlice.indexOf('show_main_window('))
    expect(src).not.toMatch(/thread::sleep|std::thread::sleep/)
  })
})

describe('FRB-003 Popup 定位与滚动', () => {
  it('open_popup 分支在 show 之前先定位（经 position_popup_near_tray 封装）', () => {
    const src = mainSrc()
    expect(src).toContain('window_layout')
    const armStart = src.indexOf('"open_popup" =>')
    expect(armStart).toBeGreaterThan(-1)
    const arm = src.slice(armStart, armStart + 1200)
    const posIdx = arm.indexOf('position_popup_near_tray')
    const showIdx = arm.indexOf('.show()')
    expect(posIdx, 'open_popup 分支必须先调用 position_popup_near_tray').toBeGreaterThan(-1)
    expect(showIdx, 'open_popup 分支必须调用 show').toBeGreaterThan(-1)
    expect(posIdx).toBeLessThan(showIdx)
  })

  it('position_popup_near_tray 使用 window_layout 纯函数并最终 set_position', () => {
    const src = mainSrc()
    const body = extractFnBody(src, 'position_popup_near_tray')
    expect(body).toContain('popup_origin(')
    expect(body).toContain('find_work_area(')
    expect(body).toContain('work_area()')
    expect(body).toContain('set_position')
  })

  it('Popup 内容包在 .popup-scroll 滚动容器中，且 CSS 定义了该容器', () => {
    const popup = readFileSync('src/components/Popup.tsx', 'utf-8')
    expect(popup).toContain('popup-scroll')
    const css = readFileSync('src/styles/global.css', 'utf-8')
    expect(css).toMatch(/\.popup-scroll\s*\{[^}]*overflow-y:\s*auto/)
  })
})
