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

// Choices in a list's dropdowns and switches (a category, a kind of period, rows per page) are also kept on this device, so they are there
// after the app is restarted. Only ids, kinds and numbers go here, never text the person typed. Storage can be missing or full, so every
// use of it is guarded, and a stored value that is not one of the known ones is ignored.
const CHOICE = 'choice.'

/** The stored choice under `key` when it is one of `allowed` (text, numbers or true/false), and `fallback` otherwise. */
export function storedChoice<T extends string | number | boolean>(key: string, allowed: readonly T[], fallback: T): T {
  try {
    const text = localStorage.getItem(CHOICE + key)
    return allowed.find((value) => String(value) === text) ?? fallback
  } catch {
    return fallback
  }
}

/** The stored id under `key`, or '' when there is none. The caller checks that it is still an id of something. */
export function storedId(key: string): string {
  try {
    return localStorage.getItem(CHOICE + key) ?? ''
  } catch {
    return ''
  }
}

/** Keeps what the person chose under `key`. */
export function saveChoice(key: string, value: string | number | boolean): void {
  try {
    localStorage.setItem(CHOICE + key, String(value))
  } catch {
    // Not kept: the page starts from its defaults next time.
  }
}

/** Forgets every stored choice: the data they were made on is gone (all data erased, a new or a restored vault). */
export function forgetChoices(): void {
  try {
    const keys = Array.from({ length: localStorage.length }, (_, index) => localStorage.key(index))
    for (const key of keys) if (key?.startsWith(CHOICE)) localStorage.removeItem(key)
  } catch {
    // Nothing was stored.
  }
}
