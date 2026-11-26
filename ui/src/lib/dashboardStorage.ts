import { defaultLayout, parseLayout, type Layout } from './dashboardLayout'
import type { Profile } from './dashboardWidgets'

// The dashboard's layout is kept on this device, one for the computer and one for the phone (docs/dashboard-design.md, section 6). Storage can be
// missing, blocked or full, so every use of it is guarded: the dashboard is then the default one.
const PREFIX = 'dashboard.layout.v1.'

export const layoutKey = (profile: Profile) => `${PREFIX}${profile}`

export function loadLayout(profile: Profile): Layout {
  try {
    return parseLayout(localStorage.getItem(layoutKey(profile)), profile)
  } catch {
    return defaultLayout(profile)
  }
}

export function saveLayout(profile: Profile, layout: Layout): void {
  try {
    localStorage.setItem(layoutKey(profile), JSON.stringify(layout))
  } catch {
    // Not kept: the dashboard is the default one next time.
  }
}

/** The stored layout of one profile goes: the dashboard is the default one again. */
export function clearLayout(profile: Profile): void {
  try {
    localStorage.removeItem(layoutKey(profile))
  } catch {
    // Nothing was stored.
  }
}

/** Both layouts go: their ids point at accounts, categories and stashes that are gone (all data erased, a new or a restored vault). */
export function forgetLayouts(): void {
  try {
    for (const profile of ['desktop', 'phone'] as const) localStorage.removeItem(layoutKey(profile))
  } catch {
    // Nothing was stored.
  }
}
