import type { MessageKey } from '../i18n/en'

// The widgets of the dashboard, as data (docs/dashboard-design.md, sections 1, 3 and 4): their allowed sizes and the settings they have.
// This file is plain data and maths, with no React: the components that draw them are listed in components/dashboard/registry.tsx.
//
// To add a widget:
//   1. add its type to `WIDGET_TYPES` and its spec to `SPECS` here (the compiler asks for the texts and the component next),
//   2. add the two texts `dashboard.w.<type>.name` and `dashboard.w.<type>.about` (and what it shows) to the 7 languages,
//   3. write its component in components/dashboard/widgets/ and give it one line in components/dashboard/registry.tsx.

export type Profile = 'desktop' | 'phone'

/** The columns of the grid: the phone has two; the desktop twelve, and six below 48em (the spans are halved there). */
export const COLUMNS: Record<Profile, number> = { desktop: 12, phone: 2 }
/** The widths a desktop widget can take, in columns. */
export const DESKTOP_WIDTHS = [3, 4, 6, 8, 9, 12] as const
/** The most rows a widget takes. */
export const MAX_ROWS = 4
/** The most widgets on a dashboard. */
export const MAX_WIDGETS = 40

export const WIDGET_TYPES = ['summary', 'trend', 'networth', 'budgets', 'stashes', 'upcoming', 'debts'] as const
export type WidgetType = (typeof WIDGET_TYPES)[number]

export const PERIOD_KINDS = ['thisMonth', 'lastMonth', 'thisYear', 'lastYear', 'last12', 'custom'] as const
export type PeriodKind = (typeof PERIOD_KINDS)[number]

export type Side = 'EXPENSE' | 'INCOME'

/** What a widget lets the person set. `title` every widget has. */
export type SettingsSpec = {
  /** The period the widget starts with (it follows the dashboard's month while it is "this month"). */
  period?: PeriodKind
  /** One account, or several; none chosen means all. */
  account?: 'one' | 'many'
  /** One category, or several; none chosen means all. With `side`, the list shows that side's categories. */
  category?: 'one' | 'many'
  /** A choice between expense and income, starting at this one. */
  side?: Side
  /** How many rows a list shows: the options, and the start (null: all of them). */
  count?: { options: readonly number[]; start: number | null }
  /** A stash to show (the `stash` widget). */
  stash?: true
}

export type WidgetSpec = {
  /** Its name in the catalog and in the title line. */
  name: MessageKey
  /** One line about what it shows (the catalog). */
  about: MessageKey
  /** The widths and heights it can take on the desktop, `[least, most]`, and the size it starts with, `[w, h]`. */
  w: readonly [number, number]
  h: readonly [number, number]
  size: readonly [number, number]
  settings: SettingsSpec
}

export const SPECS: Record<WidgetType, WidgetSpec> = {
  summary: {
    name: 'dashboard.w.summary.name',
    about: 'dashboard.w.summary.about',
    w: [6, 12],
    h: [1, 1],
    size: [12, 1],
    settings: { period: 'thisMonth', account: 'many' },
  },
  trend: {
    name: 'dashboard.w.trend.name',
    about: 'dashboard.w.trend.about',
    w: [6, 12],
    h: [2, 3],
    size: [12, 2],
    settings: { period: 'thisYear', account: 'many', category: 'many' },
  },
  networth: {
    name: 'accounts.netWorth',
    about: 'dashboard.w.networth.about',
    w: [4, 6],
    h: [1, 2],
    size: [6, 2],
    settings: {},
  },
  budgets: {
    name: 'nav.budgets',
    about: 'dashboard.w.budgets.about',
    w: [6, 12],
    h: [2, 4],
    size: [6, 2],
    settings: { period: 'thisMonth', category: 'many', count: { options: [5, 10, 20], start: null } },
  },
  stashes: {
    name: 'nav.stashes',
    about: 'dashboard.w.stashes.about',
    w: [4, 6],
    h: [2, 3],
    size: [6, 2],
    settings: { count: { options: [3, 5, 10, 20], start: 3 } },
  },
  upcoming: {
    name: 'dashboard.w.upcoming.name',
    about: 'dashboard.w.upcoming.about',
    w: [4, 6],
    h: [2, 3],
    size: [6, 2],
    settings: { count: { options: [5, 10, 20], start: 10 } },
  },
  debts: {
    name: 'debts.title',
    about: 'dashboard.w.debts.about',
    w: [4, 6],
    h: [2, 2],
    size: [6, 2],
    settings: { count: { options: [3, 5, 10], start: 3 } },
  },
}

/** A desktop span on the phone: 6 columns and more fill its two columns, less takes one. */
export function toPhoneSpan(w: number): number {
  return w >= 6 ? 2 : 1
}

/** A span on a grid of six columns (the desktop under 48em): halved, rounded up, so a 3 becomes 2 and a 12 becomes 6. */
export function halve(w: number): number {
  return Math.ceil(w / 2)
}

/** What sizes a widget can take on a profile. */
export type SizeRules = { widths: number[]; minH: number; maxH: number; start: { w: number; h: number } }

export function sizeRules(type: WidgetType, profile: Profile): SizeRules {
  const spec = SPECS[type]
  const desktop = DESKTOP_WIDTHS.filter((w) => w >= spec.w[0] && w <= spec.w[1])
  const widths = profile === 'desktop' ? [...desktop] : [...new Set(desktop.map(toPhoneSpan))]
  return {
    widths,
    minH: spec.h[0],
    maxH: Math.min(spec.h[1], MAX_ROWS),
    start: { w: profile === 'desktop' ? spec.size[0] : toPhoneSpan(spec.size[0]), h: spec.size[1] },
  }
}

/** The allowed width closest to `w` (the smaller one when two are as close). */
export function snapWidth(widths: readonly number[], w: number): number {
  return widths.reduce((best, each) => (Math.abs(each - w) < Math.abs(best - w) ? each : best), widths[0] ?? w)
}

/** A size brought into what the widget can take: the width snapped to an allowed one, the height clamped. */
export function clampSize(type: WidgetType, profile: Profile, w: number, h: number): { w: number; h: number } {
  const rules = sizeRules(type, profile)
  const rows = Math.round(h)
  return { w: snapWidth(rules.widths, Math.round(w)), h: Math.min(rules.maxH, Math.max(rules.minH, Number.isFinite(rows) ? rows : rules.start.h)) }
}

export function isWidgetType(value: unknown): value is WidgetType {
  return typeof value === 'string' && (WIDGET_TYPES as readonly string[]).includes(value)
}
