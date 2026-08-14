import { useState, useEffect, useRef, useCallback } from 'react'
import { listen } from '@tauri-apps/api/event'

export interface AsyncState<T> {
  data: T | null
  error: string | null
  loading: boolean
  retry: () => void
}

export function useAsync<T>(fn: () => Promise<T>, deps: unknown[] = []): AsyncState<T> {
  const [data, setData] = useState<T | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)
  const mountedRef = useRef(false)
  const generationRef = useRef(0)
  const hasDataRef = useRef(false)
  const fnRef = useRef(fn)
  fnRef.current = fn

  const run = useCallback(() => {
    const generation = ++generationRef.current
    if (!hasDataRef.current) setLoading(true)
    setError(null)
    Promise.resolve()
      .then(() => fnRef.current())
      .then((nextData) => {
        if (mountedRef.current && generation === generationRef.current) {
          hasDataRef.current = true
          setData(nextData)
          setLoading(false)
        }
      })
      .catch((reason) => {
        if (mountedRef.current && generation === generationRef.current) {
          setError(String(reason))
          setLoading(false)
        }
      })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps)

  useEffect(() => {
    mountedRef.current = true
    run()
    return () => {
      mountedRef.current = false
      generationRef.current += 1
    }
  }, [run])

  return { data, error, loading, retry: run }
}

export interface MutationState<Args extends unknown[]> {
  pending: boolean
  error: string | null
  run: (...args: Args) => Promise<void>
  clearError: () => void
}

export function useMutation<Args extends unknown[]>(
  fn: (...args: Args) => Promise<unknown>,
  onSuccess?: (...args: Args) => void | Promise<void>,
): MutationState<Args> {
  const [pending, setPending] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const pendingRef = useRef(false)
  const mountedRef = useRef(true)
  const fnRef = useRef(fn)
  const successRef = useRef(onSuccess)
  fnRef.current = fn
  successRef.current = onSuccess

  useEffect(() => {
    mountedRef.current = true
    return () => { mountedRef.current = false }
  }, [])

  const run = useCallback(async (...args: Args) => {
    if (pendingRef.current) return
    pendingRef.current = true
    if (mountedRef.current) {
      setPending(true)
      setError(null)
    }
    try {
      await fnRef.current(...args)
      await successRef.current?.(...args)
    } catch (reason) {
      if (mountedRef.current) setError(String(reason))
    } finally {
      pendingRef.current = false
      if (mountedRef.current) setPending(false)
    }
  }, [])

  return { pending, error, run, clearError: () => setError(null) }
}

/** Poll while visible; becoming visible refreshes immediately. */
export function usePolling(fn: () => void, intervalMs: number) {
  const fnRef = useRef(fn)
  fnRef.current = fn

  useEffect(() => {
    let timer: ReturnType<typeof setInterval> | null = null
    const stop = () => {
      if (timer !== null) clearInterval(timer)
      timer = null
    }
    const start = () => {
      if (!document.hidden && timer === null) timer = setInterval(() => fnRef.current(), intervalMs)
    }
    const onVisibility = () => {
      stop()
      if (!document.hidden) {
        fnRef.current()
        start()
      }
    }
    start()
    document.addEventListener('visibilitychange', onVisibility)
    return () => {
      stop()
      document.removeEventListener('visibilitychange', onVisibility)
    }
  }, [intervalMs])
}

/** Frontend half of the shared-window invalidation contract. */
export function useRefreshEvents(events: string[], refresh: () => void) {
  const refreshRef = useRef(refresh)
  refreshRef.current = refresh
  const eventKey = events.join('\n')
  useEffect(() => {
    let disposed = false
    const unlisteners = new Set<() => void>()

    events.forEach((name) => {
      // Tauri's event API can either throw before returning a Promise or reject
      // while registering. Attach the rejection handler immediately so a
      // missing/unavailable runtime never becomes an unhandled rejection.
      try {
        listen(name, () => refreshRef.current()).then((unlisten) => {
          if (disposed) unlisten()
          else unlisteners.add(unlisten)
        }, () => {
          // Refresh events are an enhancement; polling/manual retry remain
          // available when this window cannot subscribe.
        })
      } catch {
        // Some event API implementations can throw synchronously during setup.
      }
    })

    return () => {
      disposed = true
      unlisteners.forEach((unlisten) => unlisten())
      unlisteners.clear()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [eventKey])
}
