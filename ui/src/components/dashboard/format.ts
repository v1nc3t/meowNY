import { monthNames } from '../../api/months'
import { monthsOf, type Range } from '../../lib/dashboardPeriod'

/** What a range is called in a title line: "nov 2026", "2026" for a whole year, "mar 2026 - may 2026" otherwise. */
export function rangeLabel(range: Range, language: string): string {
  const months = monthsOf(range)
  const names = monthNames(language)
  const label = (month: string) => `${names[Number(month.slice(5, 7)) - 1] ?? month} ${month.slice(0, 4)}`
  const first = months[0]
  const last = months[months.length - 1]
  if (!first || !last) return ''
  if (first === last) return label(first)
  if (months.length === 12 && first.slice(5) === '01') return first.slice(0, 4)
  return `${label(first)} - ${label(last)}`
}
