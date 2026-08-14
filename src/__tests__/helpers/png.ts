// 极简 PNG / ICNS / ICO 解码辅助，仅用于图标像素级断言测试。
// 支持 8-bit、非交错 PNG，颜色类型 6(RGBA) / 2(RGB) / 3(调色板+tRNS)。
// @ts-expect-error node:zlib 无类型声明
import { inflateSync } from 'node:zlib'

export interface RgbaImage {
  width: number
  height: number
  /** RGBA 交错像素，每像素 4 字节 */
  data: Uint8Array
}

export function decodePng(buf: Uint8Array): RgbaImage {
  const sig = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]
  for (let i = 0; i < 8; i++) {
    if (buf[i] !== sig[i]) throw new Error('不是 PNG 文件')
  }
  let offset = 8
  let width = 0
  let height = 0
  let bitDepth = 0
  let colorType = 0
  let interlace = 0
  let palette: Uint8Array | null = null
  let paletteAlpha: Uint8Array | null = null
  const idat: Uint8Array[] = []

  while (offset + 8 <= buf.length) {
    const length =
      (buf[offset] << 24) | (buf[offset + 1] << 16) | (buf[offset + 2] << 8) | buf[offset + 3]
    const type = String.fromCharCode(buf[offset + 4], buf[offset + 5], buf[offset + 6], buf[offset + 7])
    const body = buf.subarray(offset + 8, offset + 8 + length)
    if (type === 'IHDR') {
      width = (body[0] << 24) | (body[1] << 16) | (body[2] << 8) | body[3]
      height = (body[4] << 24) | (body[5] << 16) | (body[6] << 8) | body[7]
      bitDepth = body[8]
      colorType = body[9]
      interlace = body[12]
    } else if (type === 'PLTE') {
      palette = body
    } else if (type === 'tRNS') {
      paletteAlpha = body
    } else if (type === 'IDAT') {
      idat.push(body)
    } else if (type === 'IEND') {
      break
    }
    offset += 8 + length + 4
  }

  if (bitDepth !== 8) throw new Error(`仅支持 8-bit PNG，实际 bitDepth=${bitDepth}`)
  if (interlace !== 0) throw new Error('不支持交错 PNG')
  const channels = colorType === 6 ? 4 : colorType === 2 ? 3 : colorType === 3 ? 1 : 0
  if (channels === 0) throw new Error(`不支持的 PNG 颜色类型 ${colorType}`)
  if (colorType === 3 && !palette) throw new Error('调色板 PNG 缺少 PLTE')

  const idatAll = new Uint8Array(idat.reduce((n, c) => n + c.length, 0))
  let p = 0
  for (const c of idat) {
    idatAll.set(c, p)
    p += c.length
  }
  const raw = inflateSync(idatAll)

  const stride = width * channels
  const out = new Uint8Array(width * height * 4)
  const prev = new Uint8Array(stride)
  const cur = new Uint8Array(stride)
  let src = 0
  for (let y = 0; y < height; y++) {
    const filter = raw[src++]
    for (let x = 0; x < stride; x++) {
      const v = raw[src + x]
      const a = x >= channels ? cur[x - channels] : 0
      const b = prev[x]
      const c = x >= channels ? prev[x - channels] : 0
      let val: number
      switch (filter) {
        case 0: val = v; break
        case 1: val = v + a; break
        case 2: val = v + b; break
        case 3: val = v + ((a + b) >> 1); break
        case 4: {
          const pp = a + b - c
          const pa = Math.abs(pp - a)
          const pb = Math.abs(pp - b)
          const pc = Math.abs(pp - c)
          val = v + (pa <= pb && pa <= pc ? a : pb <= pc ? b : c)
          break
        }
        default:
          throw new Error(`未知 PNG filter ${filter}`)
      }
      cur[x] = val & 0xff
    }
    src += stride
    for (let x = 0; x < width; x++) {
      const di = (y * width + x) * 4
      const si = x * channels
      if (colorType === 6) {
        out[di] = cur[si]
        out[di + 1] = cur[si + 1]
        out[di + 2] = cur[si + 2]
        out[di + 3] = cur[si + 3]
      } else if (colorType === 2) {
        out[di] = cur[si]
        out[di + 1] = cur[si + 1]
        out[di + 2] = cur[si + 2]
        out[di + 3] = 255
      } else {
        const idx = cur[si]
        out[di] = palette![idx * 3]
        out[di + 1] = palette![idx * 3 + 1]
        out[di + 2] = palette![idx * 3 + 2]
        out[di + 3] = paletteAlpha && idx < paletteAlpha.length ? paletteAlpha[idx] : 255
      }
    }
    prev.set(cur)
  }
  return { width, height, data: out }
}

export function pixelAt(img: RgbaImage, x: number, y: number): [number, number, number, number] {
  const i = (y * img.width + x) * 4
  return [img.data[i], img.data[i + 1], img.data[i + 2], img.data[i + 3]]
}

/** 从 ICNS 中提取指定类型（如 'ic10'）的 PNG 载荷；不存在返回 null */
export function extractIcnsPng(buf: Uint8Array, wanted: string): Uint8Array | null {
  if (String.fromCharCode(buf[0], buf[1], buf[2], buf[3]) !== 'icns') {
    throw new Error('不是 ICNS 文件')
  }
  let offset = 8
  while (offset + 8 <= buf.length) {
    const type = String.fromCharCode(buf[offset], buf[offset + 1], buf[offset + 2], buf[offset + 3])
    const size =
      (buf[offset + 4] << 24) | (buf[offset + 5] << 16) | (buf[offset + 6] << 8) | buf[offset + 7]
    if (type === wanted) return buf.subarray(offset + 8, offset + size)
    offset += size
  }
  return null
}

/** 从 ICO 中提取最大尺寸的 PNG 载荷（Vista+ PNG 压缩条目）；不存在返回 null */
export function extractIcoLargestPng(buf: Uint8Array): Uint8Array | null {
  const count = buf[4] | (buf[5] << 8)
  let best: { size: number; offset: number; length: number } | null = null
  for (let i = 0; i < count; i++) {
    const base = 6 + i * 16
    const w = buf[base] === 0 ? 256 : buf[base]
    const length = buf[base + 8] | (buf[base + 9] << 8) | (buf[base + 10] << 16) | (buf[base + 11] << 24)
    const offset = buf[base + 12] | (buf[base + 13] << 8) | (buf[base + 14] << 16) | (buf[base + 15] << 24)
    if (!best || w > best.size) best = { size: w, offset, length }
  }
  if (!best) return null
  const payload = buf.subarray(best.offset, best.offset + best.length)
  const isPng = payload[0] === 0x89 && payload[1] === 0x50
  return isPng ? payload : null
}
