// FRB-001：菜单栏只能有一个 KeyM 托盘图标。
// 回归根因：tauri.conf.json 的 app.trayIcon 与 main.rs 的 TrayIconBuilder 双份声明，
// Tauri 2 会各自创建一个托盘（配置创建的以 template 渲染成全黑方块）。
import { describe, it, expect } from 'vitest'
// @ts-expect-error node:fs 无类型声明
import { readFileSync } from 'node:fs'

describe('FRB-001 托盘唯一入口', () => {
  it('tauri.conf.json 不再声明 app.trayIcon', () => {
    const conf = JSON.parse(readFileSync('src-tauri/tauri.conf.json', 'utf-8'))
    expect(conf.app.trayIcon, 'app.trayIcon 必须删除，托盘只由代码创建').toBeUndefined()
  })

  it('main.rs 只保留一个 TrayIconBuilder（代码侧唯一入口）', () => {
    const src = readFileSync('src-tauri/src/main.rs', 'utf-8')
    const builders = src.match(/TrayIconBuilder::with_id/g) ?? []
    expect(builders.length, 'TrayIconBuilder::with_id 应恰好出现一次').toBe(1)
    expect(src).toContain('TrayIconBuilder::with_id("main")')
  })
})
