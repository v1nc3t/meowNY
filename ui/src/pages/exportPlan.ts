import type { ExportSelection } from '../api/dataExport'

export type ExportFormat = 'json' | 'csv' | 'xlsx' | 'pdf'

export const FORMATS: readonly ExportFormat[] = ['json', 'csv', 'xlsx', 'pdf']

/** A report covers at most this many months (the app refuses more). */
export const MAX_REPORT_MONTHS = 120

/** What the person chose on the export screen, apart from the format. */
export type Choice = {
  /** False means every date. */
  limitDates: boolean
  /** "YYYY-MM-DD". Only used when `limitDates` is on. */
  from: string
  to: string
  /** False means only `categoryIds`. */
  allCategories: boolean
  categoryIds: string[]
}

export type Span = { firstDate: string | null; lastDate: string | null }

export type Problem = 'dates' | 'categories' | 'months'

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
 * The whole months a PDF report covers: the months of the chosen dates, or the span of the data when every
 * date is wanted, or the current month when there is no data.
 */
export function reportMonths(choice: Choice, span: Span, thisMonth: string): { fromMonth: string; toMonth: string } {
  if (choice.limitDates) return { fromMonth: choice.from.slice(0, 7), toMonth: choice.to.slice(0, 7) }
  return { fromMonth: span.firstDate?.slice(0, 7) ?? thisMonth, toMonth: span.lastDate?.slice(0, 7) ?? thisMonth }
}

/** What the file covers, as the app takes it. A PDF only ever covers whole months, so its dates are widened to them. */
export function selectionFor(format: ExportFormat, choice: Choice): ExportSelection {
  const categoryIds = choice.allCategories ? null : choice.categoryIds
  if (!choice.limitDates) return { from: null, to: null, categoryIds }
  if (format === 'pdf') return { from: `${choice.from.slice(0, 7)}-01`, to: monthEnd(choice.to.slice(0, 7)), categoryIds }
  return { from: choice.from, to: choice.to, categoryIds }
}

/** The first thing wrong with the choice, or null when it can be exported. */
export function problemWith(format: ExportFormat, choice: Choice, months: number): Problem | null {
  if (choice.limitDates && choice.from > choice.to) return 'dates'
  if (!choice.allCategories && choice.categoryIds.length === 0) return 'categories'
  if (format === 'pdf' && months > MAX_REPORT_MONTHS) return 'months'
  return null
}
