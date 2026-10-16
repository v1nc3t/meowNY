import type { ExportSelection } from '../api/dataExport'
import { apiBound, shiftBound, thisPeriod, type RangeMode } from '../lib/range'

export type ExportFormat = 'json' | 'csv' | 'xlsx' | 'pdf'

export const FORMATS: readonly ExportFormat[] = ['json', 'csv', 'xlsx', 'pdf']

/** A report covers at most this many months (the app refuses more). */
export const MAX_REPORT_MONTHS = 120

/** What the person chose on the export screen, apart from the format. */
export type Choice = {
  /** False means every date. */
  limitDates: boolean
  /** How the range is chosen, as on the transactions page: by day, by month or by year. */
  mode: RangeMode
  /** In the units of `mode`: "YYYY-MM-DD", "YYYY-MM" or "YYYY". Only used when `limitDates` is on. */
  from: string
  to: string
  /** False means only `categoryIds`. */
  allCategories: boolean
  categoryIds: string[]
}

export type Span = { firstDate: string | null; lastDate: string | null }

export type Problem = 'dates' | 'categories' | 'months'

/** Every date and every category, and a range of the current month ready for when the person limits the dates. */
export function defaultChoice(today: { date: string; month: string }): Choice {
  return { limitDates: false, mode: 'month', from: thisPeriod('month', today), to: thisPeriod('month', today), allCategories: true, categoryIds: [] }
}

/**
 * The ways a range can be chosen for a format. A PDF report is made of whole months (its budgets, its chart and its comparison are
 * monthly), so it has no choice by day: asking for one would only be widened to the months, and quietly.
 */
export function modesFor(format: ExportFormat): readonly RangeMode[] {
  return format === 'pdf' ? ['month', 'year'] : ['date', 'month', 'year']
}

/** The choice with its range chosen in a way the format allows: a range by day becomes the months it touches. */
export function fitFormat(format: ExportFormat, choice: Choice): Choice {
  if (modesFor(format).includes(choice.mode)) return choice
  return {
    ...choice,
    mode: 'month',
    from: shiftBound(choice.from, choice.mode, 'month', 'from'),
    to: shiftBound(choice.to, choice.mode, 'month', 'to'),
  }
}

/** The same choice, chosen by another unit. */
export function withMode(choice: Choice, mode: RangeMode): Choice {
  return { ...choice, mode, from: shiftBound(choice.from, choice.mode, mode, 'from'), to: shiftBound(choice.to, choice.mode, mode, 'to') }
}

/** The first and last day the range covers, "YYYY-MM-DD", or empty for an end that is not a whole value of its unit. */
export function bounds(choice: Choice): { from: string; to: string } {
  return { from: apiBound(choice.mode, choice.from, 'from'), to: apiBound(choice.mode, choice.to, 'to') }
}

/** The last day of "YYYY-MM" as "YYYY-MM-DD". */
export function monthEnd(month: string): string {
  const [year, number] = month.split('-').map(Number)
  return `${month}-${String(new Date(Date.UTC(year, number, 0)).getUTCDate()).padStart(2, '0')}`
}

/** How many months from `fromMonth` to `toMonth`, both included. */
export function monthsBetween(fromMonth: string, toMonth: string): number {
  const [fy, fm] = fromMonth.split('-').map(Number)
  const [ty, tm] = toMonth.split('-').map(Number)
  return (ty - fy) * 12 + (tm - fm) + 1
}

/**
 * The whole months a PDF report covers: the months of the chosen range, or the span of the data when every date is wanted, or the
 * current month when there is no data.
 */
export function reportMonths(choice: Choice, span: Span, thisMonth: string): { fromMonth: string; toMonth: string } {
  if (choice.limitDates) {
    const { from, to } = bounds(choice)
    return { fromMonth: from.slice(0, 7) || thisMonth, toMonth: to.slice(0, 7) || thisMonth }
  }
  return { fromMonth: span.firstDate?.slice(0, 7) ?? thisMonth, toMonth: span.lastDate?.slice(0, 7) ?? thisMonth }
}

/** What the file covers, as the app takes it. A PDF only ever covers whole months, so its dates are widened to them if need be. */
export function selectionFor(format: ExportFormat, choice: Choice): ExportSelection {
  const categoryIds = choice.allCategories ? null : choice.categoryIds
  if (!choice.limitDates) return { from: null, to: null, categoryIds }
  const { from, to } = bounds(choice)
  if (format === 'pdf') return { from: from ? `${from.slice(0, 7)}-01` : from, to: to ? monthEnd(to.slice(0, 7)) : to, categoryIds }
  return { from, to, categoryIds }
}

/** The first thing wrong with the choice, or null when it can be exported. */
export function problemWith(format: ExportFormat, choice: Choice, months: number): Problem | null {
  if (choice.limitDates) {
    const { from, to } = bounds(choice)
    if (!from || !to || from > to) return 'dates'
  }
  if (!choice.allCategories && choice.categoryIds.length === 0) return 'categories'
  if (format === 'pdf' && months > MAX_REPORT_MONTHS) return 'months'
  return null
}
