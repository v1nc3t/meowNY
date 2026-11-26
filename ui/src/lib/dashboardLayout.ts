import { z } from 'zod'
import type { Period } from './dashboardPeriod'
import {
  clampSize,
  isWidgetType,
  MAX_WIDGETS,
  PERIOD_KINDS,
  SPECS,
  sizeRules,
  type Profile,
  type SettingsSpec,
  type WidgetType,
} from './dashboardWidgets'

export { halve, toPhoneSpan } from './dashboardWidgets'
export type { Period }

// The arrangement of the dashboard (docs/dashboard-design.md, sections 5 and 6): which widgets, in which order, how big, and what each is set to.
// Plain data and pure functions; the hook that keeps it in localStorage is hooks/useDashboardLayout.ts.

export type WidgetSettings = {
  period?: Period
  accountIds?: string[]
  categoryIds?: string[]
  side?: 'EXPENSE' | 'INCOME'
  count?: number
  title?: string
  stashId?: string
}

export type Widget = { id: string; type: WidgetType; w: number; h: number; settings: WidgetSettings }
export type Layout = { version: 1; widgets: Widget[] }

/** The layout of the app: today's dashboard (section 5). */
const DEFAULT_TYPES: Record<Profile, WidgetType[]> = {
  desktop: ['summary', 'trend', 'networth', 'budgets', 'stashes', 'upcoming', 'debts'],
  phone: ['summary', 'trend', 'networth', 'budgets', 'upcoming', 'stashes'],
}

function widgetOf(type: WidgetType, profile: Profile, id: string): Widget {
  return { id, type, ...sizeRules(type, profile).start, settings: {} }
}

export function defaultLayout(profile: Profile): Layout {
  return { version: 1, widgets: DEFAULT_TYPES[profile].map((type) => widgetOf(type, profile, type)) }
}

// -- reading ------------------------------------------------------------------------------------------------------------------

// Only ids of accounts, categories and stashes are kept, never names or amounts. A field that is not right is left out (the widget then uses its
// own start); the unknown ones are dropped.
const Id = z.string().regex(/^[1-9]\d*$/)
const Day = z.string().regex(/^\d{4}-(0[1-9]|1[0-2])-(0[1-9]|[12]\d|3[01])$/)
const Ids = z.array(Id).max(100).transform((ids) => [...new Set(ids)])

const SettingsSchema = z.object({
  period: z.object({ kind: z.enum(PERIOD_KINDS), from: Day.optional().catch(undefined), to: Day.optional().catch(undefined) }).optional().catch(undefined),
  accountIds: Ids.optional().catch(undefined),
  categoryIds: Ids.optional().catch(undefined),
  side: z.enum(['EXPENSE', 'INCOME']).optional().catch(undefined),
  count: z.number().int().min(1).max(100).optional().catch(undefined),
  title: z.string().optional().catch(undefined),
  stashId: Id.optional().catch(undefined),
})

export const TITLE_MAX = 40

/** A title as it is kept: trimmed, with no control characters, cut to `TITLE_MAX` characters; none when it is empty. */
export function cleanTitle(text: string): string | undefined {
  const title = [...text.replace(/\p{Cc}/gu, ' ').trim()].slice(0, TITLE_MAX).join('').trim()
  return title === '' ? undefined : title
}

/** The settings a widget of this kind has: what it does not have is dropped, and an empty list means "all". */
export function cleanSettings(type: WidgetType, raw: unknown): WidgetSettings {
  const parsed = SettingsSchema.safeParse(raw ?? {})
  if (!parsed.success) return {}
  const has: SettingsSpec = SPECS[type].settings
  const s = parsed.data
  const out: WidgetSettings = {}
  if (has.period && s.period) {
    const { kind, from, to } = s.period
    if (kind !== 'custom') out.period = { kind }
    else if (from && to) out.period = { kind, from, to }
  }
  if (has.account && s.accountIds?.length) out.accountIds = has.account === 'one' ? s.accountIds.slice(0, 1) : s.accountIds
  if (has.category && s.categoryIds?.length) out.categoryIds = has.category === 'one' ? s.categoryIds.slice(0, 1) : s.categoryIds
  if (has.side && s.side) out.side = s.side
  if (has.count && s.count !== undefined && has.count.options.includes(s.count)) out.count = s.count
  if (has.stash && s.stashId) out.stashId = s.stashId
  const title = s.title === undefined ? undefined : cleanTitle(s.title)
  if (title) out.title = title
  return out
}

const WidgetSchema = z.object({ id: z.string().min(1).max(40), type: z.string(), w: z.number().finite(), h: z.number().finite(), settings: z.unknown().optional() })

/**
 * A layout from what was stored. A widget of a type that is not known, or with a value that is not right, is dropped; a size outside what its
 * widget can take is brought into it; a second widget with the same id is dropped. A layout that is not one, or has nothing left, is the
 * default.
 */
export function parseLayout(raw: unknown, profile: Profile): Layout {
  let value = raw
  if (typeof raw === 'string') {
    try {
      value = JSON.parse(raw)
    } catch {
      return defaultLayout(profile)
    }
  }
  const outer = z.object({ version: z.literal(1), widgets: z.array(z.unknown()) }).safeParse(value)
  if (!outer.success) return defaultLayout(profile)
  const seen = new Set<string>()
  const widgets: Widget[] = []
  for (const item of outer.data.widgets) {
    const one = WidgetSchema.safeParse(item)
    if (!one.success || !isWidgetType(one.data.type) || seen.has(one.data.id) || widgets.length >= MAX_WIDGETS) continue
    const { id, type, w, h, settings } = one.data
    seen.add(id)
    widgets.push({ id, type, ...clampSize(type, profile, w, h), settings: cleanSettings(type, settings) })
  }
  return widgets.length === 0 ? defaultLayout(profile) : { version: 1, widgets }
}

// -- changing ----------------------------------------------------------------------------------------------------------------

/** A new widget at the end, at the size it starts with, with its start settings. `id` must not be in the layout. */
export function addWidget(layout: Layout, type: WidgetType, profile: Profile, id: string): Layout {
  if (layout.widgets.length >= MAX_WIDGETS || layout.widgets.some((widget) => widget.id === id)) return layout
  return { ...layout, widgets: [...layout.widgets, widgetOf(type, profile, id)] }
}

export function removeWidget(layout: Layout, id: string): Layout {
  return { ...layout, widgets: layout.widgets.filter((widget) => widget.id !== id) }
}

/** The widget goes to place `to` (counted from 0) of the list; the others keep their order. A place beyond the ends is the end. */
export function moveWidget(layout: Layout, id: string, to: number): Layout {
  const from = layout.widgets.findIndex((widget) => widget.id === id)
  if (from < 0) return layout
  const widgets = [...layout.widgets]
  const [widget] = widgets.splice(from, 1)
  const place = Math.max(0, Math.min(widgets.length, Math.round(to)))
  if (place === from) return layout
  widgets.splice(place, 0, widget)
  return { ...layout, widgets }
}

/** The widget at a new size, brought into what it can take. The same layout when that is the size it has. */
export function resizeWidget(layout: Layout, id: string, w: number, h: number, profile: Profile): Layout {
  const widget = layout.widgets.find((each) => each.id === id)
  if (!widget) return layout
  const size = clampSize(widget.type, profile, w, h)
  if (size.w === widget.w && size.h === widget.h) return layout
  return { ...layout, widgets: layout.widgets.map((each) => (each === widget ? { ...widget, ...size } : each)) }
}

/** The widget with other settings, cleaned for its kind. */
export function setWidgetSettings(layout: Layout, id: string, settings: WidgetSettings): Layout {
  return { ...layout, widgets: layout.widgets.map((widget) => (widget.id === id ? { ...widget, settings: cleanSettings(widget.type, settings) } : widget)) }
}
