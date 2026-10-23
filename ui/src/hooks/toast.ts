import { useSyncExternalStore } from 'react'
import { NOTICE_SECONDS } from './useNotice'

export type ToastKind = 'info' | 'success' | 'warning' | 'error'

/** What the pages call to show a toast (see useToast). */
export type Say = (text: string, kind?: ToastKind) => void

export type ToastItem = {
  id: number
  text: string
  kind: ToastKind
  /** How long the toast stays: the bar along its bottom edge fills in this time. */
  seconds: number
  /** Goes up each time the same text is shown again, so the toast and its bar start over. */
  round: number
}

/** The most toasts on screen at once; the oldest leave first. */
export const MAX_TOASTS = 3

/** A warning or an error is read slower than a "saved", so it stays longer. */
export function toastSeconds(kind: ToastKind): number {
  return kind === 'warning' || kind === 'error' ? 8 : NOTICE_SECONDS
}

/**
 * The list after a new toast. The same text that is still on screen is not stacked twice: that toast starts over
 * (and takes the new kind). Beyond MAX_TOASTS the oldest ones are dropped.
 */
export function withToast(list: ToastItem[], text: string, kind: ToastKind, id: number): ToastItem[] {
  const seconds = toastSeconds(kind)
  const same = list.find((item) => item.text === text)
  const next = same
    ? list.map((item) => (item === same ? { ...item, kind, seconds, round: item.round + 1 } : item))
    : [...list, { id, text, kind, seconds, round: 0 }]
  return next.slice(-MAX_TOASTS)
}

let list: ToastItem[] = []
let lastId = 0
const listeners = new Set<() => void>()

function emit(next: ToastItem[]) {
  list = next
  listeners.forEach((listener) => listener())
}

/** Shows a message in a floating pop-up. An empty text shows nothing. Gives back the id, for `dismissToast`. */
export function toast(text: string, kind: ToastKind = 'info'): number {
  if (!text) return 0
  emit(withToast(list, text, kind, ++lastId))
  return list.find((item) => item.text === text)?.id ?? 0
}

/** Takes one toast away at once. */
export function dismissToast(id: number) {
  emit(list.filter((item) => item.id !== id))
}

function subscribe(listener: () => void) {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

export function useToasts(): ToastItem[] {
  return useSyncExternalStore(subscribe, () => list)
}

/** For the pages: `const say = useToast()`, then `say(text)` or `say(text, 'error')`. It is the same function on every render. */
export function useToast(): Say {
  return toast
}
