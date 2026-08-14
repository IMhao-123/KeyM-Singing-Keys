// 发布身份一致性：首个公开版本统一为 0.1.0，锁定四处版本声明和界面版本。
import { describe, it, expect } from 'vitest'
// @ts-expect-error node:fs 无类型声明
import { readFileSync } from 'node:fs'

const RELEASE_VERSION = '0.1.0'

describe('发布版本一致性（0.1.0）', () => {
  it('package.json 版本为 0.1.0', () => {
    const pkg = JSON.parse(readFileSync('package.json', 'utf-8'))
    expect(pkg.version).toBe(RELEASE_VERSION)
  })

  it('tauri.conf.json 版本为 0.1.0', () => {
    const conf = JSON.parse(readFileSync('src-tauri/tauri.conf.json', 'utf-8'))
    expect(conf.version).toBe(RELEASE_VERSION)
  })

  it('src-tauri/Cargo.toml 包版本为 0.1.0', () => {
    const toml = readFileSync('src-tauri/Cargo.toml', 'utf-8')
    const pkgSection = toml.match(/\[package\]([\s\S]*?)(?=\n\[|$)/)?.[1] ?? ''
    expect(pkgSection).toContain(`version = "${RELEASE_VERSION}"`)
  })

  it('Cargo.lock 中 keym 自身条目为 0.1.0', () => {
    const lock = readFileSync('src-tauri/Cargo.lock', 'utf-8')
    const keymEntry = lock.match(
      /\[\[package\]\]\nname = "keym"\nversion = "([^"]+)"/,
    )
    expect(keymEntry?.[1]).toBe(RELEASE_VERSION)
  })

  it('设置界面“关于”区展示 0.1.0，且不出现旧版本 1.0.0', () => {
    const panel = readFileSync('src/components/SettingsPanel.tsx', 'utf-8')
    expect(panel).toContain(`键标 KeyM ${RELEASE_VERSION}`)
    expect(panel).not.toContain('1.0.0')
  })
})
