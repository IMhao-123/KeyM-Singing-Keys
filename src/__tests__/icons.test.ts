// FRB-002：彩色 KeyM 图标的透明角像素级断言。
// 回归根因：旧图标四角为不透明白底（全图 0 透明像素），菜单栏小尺寸显示成白边/白块。
import { describe, it, expect } from 'vitest'
// @ts-expect-error node:fs 无类型声明
import { readFileSync } from 'node:fs'
import { decodePng, extractIcnsPng, extractIcoLargestPng, pixelAt, type RgbaImage } from './helpers/png'

const ICON_DIR = 'src-tauri/icons'

function readIcon(name: string): Uint8Array {
  return new Uint8Array(readFileSync(`${ICON_DIR}/${name}`))
}

/** 彩色主图标必须满足的形状约束 */
function assertTransparentCorners(img: RgbaImage, label: string) {
  const { width: w, height: h } = img
  // 1. 四角完全透明
  const corners: [number, number][] = [
    [0, 0],
    [w - 1, 0],
    [0, h - 1],
    [w - 1, h - 1],
  ]
  for (const [x, y] of corners) {
    const [, , , a] = pixelAt(img, x, y)
    expect(a, `${label} 角像素 (${x},${y}) 应完全透明`).toBe(0)
  }
  // 2. 角尖 3% 见方区域全部透明（该区域必然位于圆角矩形之外；
  //    允许 <= 10 的重采样残留 alpha，视觉上等同全透明）
  const r = Math.max(1, Math.floor(w * 0.03))
  for (const [cx, cy] of [
    [0, 0],
    [w - r, 0],
    [0, h - r],
    [w - r, h - r],
  ]) {
    for (let y = cy; y < cy + r; y++) {
      for (let x = cx; x < cx + r; x++) {
        const [, , , a] = pixelAt(img, x, y)
        expect(a, `${label} 角尖区域 (${x},${y}) 应透明`).toBeLessThanOrEqual(10)
      }
    }
  }
  // 3. 主体仍然不透明（中心与四边中点）
  for (const [x, y] of [
    [w >> 1, h >> 1],
    [w >> 1, Math.floor(h * 0.03)],
    [w >> 1, Math.floor(h * 0.97)],
    [Math.floor(w * 0.03), h >> 1],
    [Math.floor(w * 0.97), h >> 1],
  ]) {
    const [, , , a] = pixelAt(img, x, y)
    expect(a, `${label} 主体 (${x},${y}) 应不透明`).toBe(255)
  }
  // 4. 透明像素必须真实存在且占比合理（旧 bug：全图 0 个透明像素）
  let transparent = 0
  for (let i = 3; i < img.data.length; i += 4) {
    if (img.data[i] === 0) transparent++
  }
  const total = w * h
  expect(transparent / total, `${label} 透明像素占比应 > 1%`).toBeGreaterThan(0.01)
  expect(transparent / total, `${label} 透明像素占比应 < 40%（不应裁掉主体）`).toBeLessThan(0.4)
  // 5. 最外侧 4% 边框环内不允许出现不透明近白像素（白边回归线）
  const ring = Math.max(2, Math.floor(w * 0.04))
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const inRing = x < ring || x >= w - ring || y < ring || y >= h - ring
      if (!inRing) continue
      const [rr, gg, bb, a] = pixelAt(img, x, y)
      const nearWhite = Math.min(rr, gg, bb) >= 245
      expect(
        !(a > 200 && nearWhite),
        `${label} 边框环 (${x},${y}) 存在不透明白底像素 rgba(${rr},${gg},${bb},${a})`,
      ).toBe(true)
    }
  }
}

describe('FRB-002 彩色图标透明角', () => {
  it.each(['icon.png', '32x32.png', '128x128.png', '128x128@2x.png'])(
    '%s 四角透明、无白底、主体完整',
    (name) => {
      assertTransparentCorners(decodePng(readIcon(name)), name)
    },
  )

  it('icon.icns 含高分辨率 PNG 条目且同样四角透明', () => {
    const icns = readIcon('icon.icns')
    // 优先 1024（ic10），其次 512（ic09）
    const payload = extractIcnsPng(icns, 'ic10') ?? extractIcnsPng(icns, 'ic09')
    expect(payload, 'icon.icns 应包含 ic10(1024) 或 ic09(512) PNG 条目').not.toBeNull()
    assertTransparentCorners(decodePng(payload!), 'icon.icns')
  })

  it('icon.ico 最大条目同样四角透明', () => {
    const payload = extractIcoLargestPng(readIcon('icon.ico'))
    expect(payload, 'icon.ico 应包含 PNG 压缩条目').not.toBeNull()
    assertTransparentCorners(decodePng(payload!), 'icon.ico')
  })

  it.each(['sound-on.png', 'sound-off.png'])('托盘菜单项图标 %s 未被破坏', (name) => {
    const img = decodePng(readIcon(name))
    let transparent = 0
    let opaque = 0
    for (let i = 3; i < img.data.length; i += 4) {
      if (img.data[i] === 0) transparent++
      if (img.data[i] === 255) opaque++
    }
    const total = img.width * img.height
    expect(transparent, `${name} 应保留透明背景`).toBeGreaterThan(0)
    expect(opaque / total, `${name} 应保留不透明主体`).toBeGreaterThan(0.1)
  })
})
