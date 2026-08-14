// F3/AUD-024：托盘菜单是引擎状态的投影——任何入口（托盘 toggle、主窗口、IPC）
// 切换音效后，必须由唯一的 sound-state-changed 监听器读取引擎真实状态刷新
// 托盘文案/图标，窗口→托盘方向闭环。
import { describe, it, expect } from 'vitest'
// @ts-expect-error node:fs 无类型声明
import { readFileSync } from 'node:fs'

const mainRs = readFileSync('src-tauri/src/main.rs', 'utf-8')
const commandsRs = readFileSync('src-tauri/src/commands.rs', 'utf-8')
const trayRs = readFileSync('src-tauri/src/tray.rs', 'utf-8')

describe('F3 托盘音效状态同步', () => {
  it('main.rs 注册 sound-state-changed 监听器刷新托盘（单一更新路径）', () => {
    expect(mainRs).toContain('app.listen(keym_lib::tray::SOUND_STATE_CHANGED_EVENT')
    // 监听器读取引擎真实状态（单一状态源），而不是只信事件负载
    const listenerBlock = mainRs.slice(mainRs.indexOf('app.listen(keym_lib::tray::SOUND_STATE_CHANGED_EVENT'))
    expect(listenerBlock).toContain('audio.is_enabled()')
    expect(listenerBlock).toContain('set_text(keym_lib::tray::sound_toggle_label(enabled))')
    expect(listenerBlock).toContain('set_icon(Some(icon))')
  })

  it('托盘 toggle 分支只切换状态并广播事件，不再内联更新文案/图标', () => {
    const toggleBranch = mainRs.slice(
      mainRs.indexOf('"toggle" => {'),
      mainRs.indexOf('"open_main" =>'),
    )
    expect(toggleBranch).toContain('set_enabled(new_state)')
    expect(toggleBranch).toContain('app.emit(keym_lib::tray::SOUND_STATE_CHANGED_EVENT, new_state)')
    expect(toggleBranch).not.toContain('set_text')
    expect(toggleBranch).not.toContain('set_icon')
  })

  it('托盘文案/图标只由 tray.rs 纯函数决定，main.rs 无硬编码初值', () => {
    expect(mainRs).not.toContain('"音效: 开启"')
    expect(mainRs).not.toContain('"音效: 关闭"')
    // 初始菜单项同样取自引擎真实状态
    expect(mainRs).toContain('sound_toggle_label(initial_sound_enabled)')
    expect(mainRs).toContain('sound_toggle_icon(initial_sound_enabled)')
    expect(trayRs).toContain('pub fn sound_toggle_label(enabled: bool)')
    expect(trayRs).toContain('pub fn sound_toggle_icon(enabled: bool)')
  })

  it('IPC toggle_sound 与托盘走同一事件（事件与托盘更新闭合）', () => {
    expect(commandsRs).toContain('app.emit(crate::tray::SOUND_STATE_CHANGED_EVENT, !current)')
    // 事件名常量唯一定义，避免两端字符串漂移
    expect(trayRs).toContain('pub const SOUND_STATE_CHANGED_EVENT: &str = "sound-state-changed"')
  })
})
