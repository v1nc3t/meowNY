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
 * Props that show a shortcut when hovering or focusing an element: a small label such as
 * "add new · n". Spread them onto the element. The element is also marked for screen readers.
 * Nothing on a phone.
 */
export function tip(id: ShortcutId, name?: string): { 'data-tip'?: string; 'aria-keyshortcuts'?: string } {
  if (!hintsOn) return {}
  return {
    'data-tip': `${name ?? shortcutName(id)} \u00b7 ${keysFor(id)}`,
    'aria-keyshortcuts': SHORTCUTS[id].mod ? `${mac ? 'Meta' : 'Control'}+${SHORTCUTS[id].keys[0].toUpperCase()}` : SHORTCUTS[id].keys.join(' '),
  }
}
