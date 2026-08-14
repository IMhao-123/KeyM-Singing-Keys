// FRB-009：`bun test` 的 DOM 环境预载（由 bunfig.toml 的 [test].preload 引用）。
//
// bun test 原生不带 DOM；Vitest 走自己的 jsdom 环境（vitest.config.ts），
// 本文件只影响 bun test，不影响 bun run test / build / dev。
// 复用仓库已有 jsdom 依赖，不新增任何依赖。
//
// 本文件刻意放在 src/ 之外：tsconfig 只 include src，jsdom 未附带类型，
// 不参与 tsc 类型检查，避免为测试环境新增 @types/jsdom 依赖。
import { JSDOM } from 'jsdom'

const dom = new JSDOM('<!doctype html><html><body></body></html>', {
  url: 'http://localhost/',
  pretendToBeVisual: true, // 提供 requestAnimationFrame
})

const { window } = dom
const windowRecord = window as unknown as Record<string, unknown>

// 把 jsdom window 上的 DOM 全局（document、HTMLElement、KeyboardEvent、
// getComputedStyle……）挂到 globalThis；已存在的 Bun 原生全局
// （fetch、URL、crypto、performance、setTimeout 等）一律跳过，不覆盖。
for (const key of Object.getOwnPropertyNames(window)) {
  if (key in globalThis) continue
  Object.defineProperty(globalThis, key, {
    configurable: true,
    writable: true,
    enumerable: true,
    value: windowRecord[key],
  })
}

// React 18 的 act() 需要显式声明当前是测试环境（@testing-library/react 依赖）
;(globalThis as Record<string, unknown>).IS_REACT_ACT_ENVIRONMENT = true

// 进程退出时关闭 jsdom window，释放其持有的定时器等资源
process.on('exit', () => {
  window.close()
})
