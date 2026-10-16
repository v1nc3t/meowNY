import { useEffect } from 'react'
import type { ShortcutId } from './shortcuts'

/** Who answers each shortcut right now. A page registers its own (new, search, ...) while it is open. */
const handlers = new Map<ShortcutId, () => void>()

export function registerShortcut(id: ShortcutId, handler: () => void): () => void {
  handlers.set(id, handler)
  return () => {
    // Only remove our own handler: a newer page may already have taken the shortcut over.
    if (handlers.get(id) === handler) handlers.delete(id)
  }
}

/** Runs the handler for a shortcut. Returns false when nothing is listening, so the key keeps its normal meaning. */
export function runShortcut(id: ShortcutId): boolean {
  const handler = handlers.get(id)
  if (!handler) return false
  handler()
  return true
}

/** Answers a shortcut while the component is mounted. */
export function useShortcut(id: ShortcutId, handler: () => void, enabled = true): void {
  useEffect(() => {
    if (!enabled) return
    return registerShortcut(id, handler)
  }, [id, handler, enabled])
}
