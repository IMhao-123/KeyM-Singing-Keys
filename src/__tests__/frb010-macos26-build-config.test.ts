// FRB-010：锁定 macOS 26 / arm64 最小实现的三处配置改动。
// 回归根因：tauri.conf.json 的 bundle.macOS.minimumSystemVersion 会被 tauri-cli
// 作为 MACOSX_DEPLOYMENT_TARGET 注入 cargo 全局 env，污染 host 过程宏 dylib，
// 触发 ld-27037 的 mis-aligned LINKEDIT 缺陷（E0463）。方案改为：
// 根 .cargo/config.toml 显式默认 target + target-specific link-arg 锁 minOS 26，
// Info.plist 显式 LSMinimumSystemVersion=26.0。
import { describe, it, expect } from 'vitest'
// @ts-expect-error node:fs 无类型声明
import { readFileSync } from 'node:fs'

describe('FRB-010 macOS 26 / arm64 构建配置', () => {
  it('.cargo/config.toml 默认 target 为 aarch64-apple-darwin', () => {
    const cfg = readFileSync('.cargo/config.toml', 'utf-8')
    expect(cfg).toMatch(/\[build\][^\[]*target\s*=\s*"aarch64-apple-darwin"/)
  })

  it('.cargo/config.toml 仅对 target 加 minOS 26 link-arg', () => {
    const cfg = readFileSync('.cargo/config.toml', 'utf-8')
    expect(cfg).toContain('[target.aarch64-apple-darwin]')
    expect(cfg).toContain('link-arg=-mmacosx-version-min=26.0')
    // rustflags 只允许这一条 link-arg，不得出现 wrapper / 绝对路径等其他注入
    const rustflags = cfg.match(/rustflags\s*=\s*\[([^\]]*)\]/)?.[1] ?? ''
    expect(rustflags).toBe('"-C", "link-arg=-mmacosx-version-min=26.0"')
  })

  it('tauri.conf.json 不再含 minimumSystemVersion（避免全局 MDT 污染 host）', () => {
    const conf = JSON.parse(readFileSync('src-tauri/tauri.conf.json', 'utf-8'))
    expect(
      conf.bundle.macOS.minimumSystemVersion,
      'bundle.macOS.minimumSystemVersion 必须删除',
    ).toBeUndefined()
  })

  it('Info.plist 显式 LSMinimumSystemVersion = 26.0', () => {
    const plist = readFileSync('src-tauri/Info.plist', 'utf-8')
    expect(plist).toMatch(
      /<key>LSMinimumSystemVersion<\/key>\s*<string>26\.0<\/string>/,
    )
  })
})
