import { useEffect, useState } from 'react'

// What a page was last showing (a month, a date range). Module memory: it survives moving between pages (they unmount) and is gone when
// the app restarts.
const saved = new Map<string, unknown>()

/** The value saved under `key`, or `initial()` when nothing was saved yet. */
export function recall<T>(key: string, initial: () => T): T {
  return saved.has(key) ? (saved.get(key) as T) : initial()
}

/** Saves `value` under `key` whenever it changes. */
export function useRemember(key: string, value: unknown) {
  useEffect(() => {
    saved.set(key, value)
  }, [key, value])
}

/** Like `useState`, but the value is remembered under `key` for the next time the page is shown. */
export function useSticky<T>(key: string, initial: () => T) {
  const [value, setValue] = useState<T>(() => recall(key, initial))
  useRemember(key, value)
  return [value, setValue] as const
}
