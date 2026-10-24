import { t } from '../i18n/translate'
import { isMac, SHORTCUTS, shortcutKeys, type ShortcutId } from './shortcuts'

const mac = typeof navigator !== 'undefined' && isMac(navigator.platform ?? navigator.userAgent ?? '')

/** The keys for a shortcut as shown on this computer, e.g. "g t" or "Ctrl+L". */
export function keysFor(id: ShortcutId): string {
  return shortcutKeys(id, mac)
}

/** The name of a shortcut in the current language, e.g. "go to transactions". */
export function shortcutName(id: ShortcutId): string {
  const def = SHORTCUTS[id]
  return t(def.label, def.page ? { page: t(def.page) } : undefined)
}

let hintsOn = true

/** A phone has no keyboard to speak of: its screens show no shortcut hints (a hint would show up on every tap). */
export function showShortcutHints(on: boolean): void {
  hintsOn = on
}

/**
 * Props that mark an element with its shortcut for screen readers (aria-keyshortcuts). Spread them onto
 * the element. Nothing on a phone. The name is not used any more; callers may still pass it.
 */
export function tip(id: ShortcutId, _name?: string): { 'aria-keyshortcuts'?: string } {
  if (!hintsOn) return {}
  return {
    'aria-keyshortcuts': SHORTCUTS[id].mod ? `${mac ? 'Meta' : 'Control'}+${SHORTCUTS[id].keys[0].toUpperCase()}` : SHORTCUTS[id].keys.join(' '),
  }
}
