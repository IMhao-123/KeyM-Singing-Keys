import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useAsync, useMutation, useRefreshEvents } from '../useAsync'

const listen = vi.fn()
vi.mock('@tauri-apps/api/event', () => ({ listen: (...args: unknown[]) => listen(...args) }))

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason: unknown) => void
  const promise = new Promise<T>((res, rej) => { resolve = res; reject = rej })
  return { promise, resolve, reject }
}

describe('useAsync', () => {
  it('ignores an older request that resolves after the latest request', async () => {
    const first = deferred<string>()
    const second = deferred<string>()
    const fn = vi.fn().mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise)
    const { result } = renderHook(() => useAsync(fn, []))
    act(() => result.current.retry())
    await act(async () => second.resolve('latest'))
    expect(result.current.data).toBe('latest')
    await act(async () => first.resolve('stale'))
    expect(result.current.data).toBe('latest')
  })
})

describe('useMutation', () => {
  it('prevents re-entry and exposes an error after failure', async () => {
    const request = deferred<void>()
    const mutate = vi.fn(() => request.promise)
    const { result } = renderHook(() => useMutation(mutate))
    let first!: Promise<void>
    act(() => { first = result.current.run() })
    act(() => { void result.current.run() })
    expect(mutate).toHaveBeenCalledTimes(1)
    await act(async () => request.reject(new Error('save failed')))
    await expect(first).resolves.toBeUndefined()
    expect(result.current.pending).toBe(false)
    expect(result.current.error).toContain('save failed')
  })
})

describe('useRefreshEvents', () => {
  beforeEach(() => listen.mockReset())

  it('registers the event callback and refreshes through it', async () => {
    const refresh = vi.fn()
    listen.mockResolvedValue(vi.fn())
    const { unmount } = renderHook(() => useRefreshEvents(['data-cleared'], refresh))
    expect(listen).toHaveBeenCalledWith('data-cleared', expect.any(Function))
    const callback = listen.mock.calls[0][1] as () => void
    callback()
    expect(refresh).toHaveBeenCalledTimes(1)
    unmount()
  })

  it('unlistens registrations that resolve before and after cleanup', async () => {
    const firstUnlisten = vi.fn()
    const lateUnlisten = vi.fn()
    const late = deferred<() => void>()
    listen.mockResolvedValueOnce(firstUnlisten).mockReturnValueOnce(late.promise)
    const { unmount } = renderHook(() => useRefreshEvents(['first', 'late'], vi.fn()))
    await act(async () => {})
    unmount()
    expect(firstUnlisten).toHaveBeenCalledTimes(1)
    await act(async () => late.resolve(lateUnlisten))
    expect(lateUnlisten).toHaveBeenCalledTimes(1)
  })
})
