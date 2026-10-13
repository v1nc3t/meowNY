import type { MessageKey } from '../i18n/en'

export type ShortcutId =
  | 'goDashboard'
  | 'goTransactions'
  | 'goCategories'
  | 'goBudgets'
  | 'goSettings'
  | 'new'
  | 'search'
  | 'prev'
  | 'next'
  | 'today'
  | 'rowNext'
  | 'rowPrev'
  | 'help'
  | 'lock'
  | 'sidebar'

export type ShortcutGroup = 'go' | 'actions' | 'lists' | 'general'

export type ShortcutDef = {
  /** The keys, pressed one after another. For a `mod` shortcut it is one key held with Ctrl (Cmd on a Mac). */
  keys: readonly string[]
  mod?: boolean
  group: ShortcutGroup
  label: MessageKey
  /** For "go to {page}": the name of the page. */
  page?: MessageKey
}

/** Every keyboard shortcut. The help dialog, the hover hints and the key matcher all read this one table. */
export const SHORTCUTS: Record<ShortcutId, ShortcutDef> = {
  goDashboard: { keys: ['g', 'd'], group: 'go', label: 'keys.goTo', page: 'nav.dashboard' },
  goTransactions: { keys: ['g', 't'], group: 'go', label: 'keys.goTo', page: 'nav.transactions' },
  goCategories: { keys: ['g', 'c'], group: 'go', label: 'keys.goTo', page: 'nav.categories' },
  goBudgets: { keys: ['g', 'b'], group: 'go', label: 'keys.goTo', page: 'nav.budgets' },
  goSettings: { keys: ['g', 's'], group: 'go', label: 'keys.goTo', page: 'nav.settings' },
  new: { keys: ['n'], group: 'actions', label: 'keys.new' },
  search: { keys: ['/'], group: 'actions', label: 'keys.search' },
  prev: { keys: ['['], group: 'actions', label: 'keys.prev' },
  next: { keys: [']'], group: 'actions', label: 'keys.next' },
  today: { keys: ['t'], group: 'actions', label: 'keys.today' },
  rowNext: { keys: ['j'], group: 'lists', label: 'keys.rowNext' },
  rowPrev: { keys: ['k'], group: 'lists', label: 'keys.rowPrev' },
  help: { keys: ['?'], group: 'general', label: 'keys.help' },
  lock: { keys: ['l'], mod: true, group: 'general', label: 'keys.lock' },
  sidebar: { keys: ['b'], mod: true, group: 'general', label: 'keys.sidebar' },
}

const GO_SECOND_KEY: Record<string, ShortcutId> = {
  d: 'goDashboard',
  t: 'goTransactions',
  c: 'goCategories',
  b: 'goBudgets',
  s: 'goSettings',
}

const SINGLE_KEY: Record<string, ShortcutId> = {
  n: 'new',
  '/': 'search',
  '[': 'prev',
  ']': 'next',
  t: 'today',
  j: 'rowNext',
  k: 'rowPrev',
  '?': 'help',
}

const MOD_KEY: Record<string, ShortcutId> = { l: 'lock', b: 'sidebar' }

/** How long after "g" the second key still counts. */
export const CHORD_MS = 1200

export type KeyInput = { key: string; ctrlKey: boolean; metaKey: boolean; altKey: boolean }

/** Turns key presses into shortcut ids. It remembers a pending "g" between presses; time is passed in so it can be tested. */
export class KeyMatcher {
  private pendingSince: number | null = null

  handle(event: KeyInput, now: number): ShortcutId | null {
    const key = event.key.length === 1 ? event.key.toLowerCase() : event.key
    const mod = event.ctrlKey || event.metaKey

    if (mod && !event.altKey) {
      this.pendingSince = null
      return MOD_KEY[key] ?? null
    }
    if (event.altKey || mod) {
      this.pendingSince = null
      return null
    }
    if (key === 'Escape') {
      this.pendingSince = null
      return null
    }

    if (this.pendingSince !== null) {
      const waited = now - this.pendingSince
      this.pendingSince = null
      if (waited <= CHORD_MS) return GO_SECOND_KEY[key] ?? null
    }

    if (key === 'g') {
      this.pendingSince = now
      return null
    }
    return SINGLE_KEY[key] ?? null
  }
}

export function isMac(platform: string): boolean {
  return /mac|iphone|ipad/i.test(platform)
}

/** The keys as shown to people: "g d", "n", "Ctrl+L" (or "⌘L" on a Mac). */
export function shortcutKeys(id: ShortcutId, mac: boolean): string {
  const def = SHORTCUTS[id]
  if (def.mod) return mac ? `⌘${def.keys[0].toUpperCase()}` : `Ctrl+${def.keys[0].toUpperCase()}`
  return def.keys.join(' ')
}

/** True when the key press happened somewhere people type or pick, where letters must not trigger shortcuts. */
export function isTypingTarget(target: { tagName?: string; isContentEditable?: boolean; closest?: (selector: string) => unknown } | null): boolean {
  if (!target) return false
  const tag = (target.tagName ?? '').toLowerCase()
  if (tag === 'input' || tag === 'textarea' || tag === 'select') return true
  if (target.isContentEditable) return true
  // Inside a dropdown list (which has its own type-to-jump) or any open dialog.
  return Boolean(target.closest?.('.pick-list, dialog'))
}
