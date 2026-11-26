import { daysInMonth, shiftMonth } from '../api/months'
import type { PeriodKind } from './dashboardWidgets'

/** The period a widget shows: a kind, and for "custom" the first and last day. */
export type Period = { kind: PeriodKind; from?: string; to?: string }

/** From the first day to the last day, both included, "YYYY-MM-DD". Always whole months. */
export type Range = { from: string; to: string }

const DAY = /^\d{4}-(0[1-9]|1[0-2])-(0[1-9]|[12]\d|3[01])$/

/** The first and last day of a month ("YYYY-MM"). */
export function monthRange(month: string): Range {
  return { from: `${month}-01`, to: `${month}-${String(daysInMonth(Number(month.slice(0, 4)), Number(month.slice(5, 7)))).padStart(2, '0')}` }
}

/** The first and last day of the months from `from` to `to` ("YYYY-MM"). */
function span(from: string, to: string): Range {
  return { from: monthRange(from).from, to: monthRange(to).to }
}

/**
 * The days a period covers. Everything but "custom" is counted from `month`, the month the dashboard shows (its control steps it; it is
 * this month at first): "this month" is that month, "last month" the one before it, "this year" its year, "last year" the year before and
 * "last 12 months" the twelve that end with it. A custom range is whole months from the month of `from` to the month of `to` (swapped when
 * they are the wrong way round); a custom range that is missing or broken is "this month".
 */
export function resolvePeriod(period: Period | undefined, fallback: PeriodKind, month: string): Range {
  const kind = period?.kind ?? fallback
  const year = Number(month.slice(0, 4))
  switch (kind) {
    case 'thisMonth':
      return monthRange(month)
    case 'lastMonth':
      return monthRange(shiftMonth(month, -1))
    case 'thisYear':
      return span(`${year}-01`, `${year}-12`)
    case 'lastYear':
      return span(`${year - 1}-01`, `${year - 1}-12`)
    case 'last12':
      return span(shiftMonth(month, -11), month)
    case 'custom': {
      const { from, to } = period ?? {}
      if (!from || !to || !DAY.test(from) || !DAY.test(to)) return monthRange(month)
      const [a, b] = from <= to ? [from, to] : [to, from]
      return span(a.slice(0, 7), b.slice(0, 7))
    }
  }
}

/** The months of a range, "YYYY-MM", the first first. */
export function monthsOf(range: Range): string[] {
  const months: string[] = []
  for (let month = range.from.slice(0, 7); month <= range.to.slice(0, 7) && months.length < 600; month = shiftMonth(month, 1)) months.push(month)
  return months
}

/** The range just before this one, as long: what a change is measured against. */
export function previousRange(range: Range): Range {
  const count = monthsOf(range).length
  return span(shiftMonth(range.from.slice(0, 7), -count), shiftMonth(range.to.slice(0, 7), -count))
}
