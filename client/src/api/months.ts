/** Month helpers for "YYYY-MM" strings. No time zones involved: a month is just a label. */

export function shiftMonth(month: string, delta: number): string {
  const [year, number] = month.split('-').map(Number)
  const index = year * 12 + (number - 1) + delta
  const nextYear = Math.floor(index / 12)
  return `${nextYear}-${String((index % 12) + 1).padStart(2, '0')}`
}

/** "October 2026" in the person's locale. Falls back to the plain label if the locale is unusable. */
export function monthLabel(month: string, locale: string): string {
  const [year, number] = month.split('-').map(Number)
  try {
    return new Date(Date.UTC(year, number - 1, 1)).toLocaleDateString(locale, {
      month: 'long',
      year: 'numeric',
      timeZone: 'UTC',
    })
  } catch {
    return month
  }
}

/** The twelve months as three-letter abbreviations in the person's locale, January first. */
export function monthNames(locale: string): string[] {
  return Array.from({ length: 12 }, (_, index) => {
    try {
      const short = new Date(Date.UTC(2000, index, 1)).toLocaleDateString(locale, { month: 'short', timeZone: 'UTC' })
      // Some locales write "Sept" or "Sep."; keep three characters.
      return short.replace(/\.$/, '').slice(0, 3)
    } catch {
      return String(index + 1)
    }
  })
}

/** Years to pick from: a window around this year, widened to always include the selected month's year. */
export function yearChoices(selected: string, thisYear: number, before = 10, after = 5): number[] {
  const year = Number(selected.slice(0, 4))
  const first = Math.min(thisYear - before, year)
  const last = Math.max(thisYear + after, year)
  return Array.from({ length: last - first + 1 }, (_, index) => first + index)
}

export function withMonth(month: string, monthNumber: number): string {
  return `${month.slice(0, 4)}-${String(monthNumber).padStart(2, '0')}`
}

export function withYear(month: string, year: number): string {
  return `${year}-${month.slice(5, 7)}`
}

export type DateKind = 'date' | 'month' | 'year'
export type DateParts = { year: number; month: number; day: number }

export function daysInMonth(year: number, month: number): number {
  return new Date(Date.UTC(year, month, 0)).getUTCDate()
}

/**
 * Splits "YYYY-MM-DD", "YYYY-MM" or "YYYY" (by kind) into parts. Anything that does not
 * parse falls back to `fallback`, a full "YYYY-MM-DD" date such as today.
 */
export function parseParts(kind: DateKind, value: string, fallback: string): DateParts {
  const pattern = kind === 'date' ? /^(\d{4})-(\d{2})-(\d{2})$/ : kind === 'month' ? /^(\d{4})-(\d{2})$/ : /^(\d{4})$/
  const match = pattern.exec(value)
  const [year, month, day] = (match ?? /^(\d{4})-(\d{2})-(\d{2})$/.exec(fallback) ?? [])
    .slice(1)
    .map(Number)
  const safeYear = year || 2000
  const safeMonth = Math.min(12, Math.max(1, kind === 'year' && match ? 1 : month || 1))
  const safeDay = Math.min(daysInMonth(safeYear, safeMonth), Math.max(1, kind === 'date' ? day || 1 : 1))
  return { year: safeYear, month: safeMonth, day: safeDay }
}

/** Changes one part, keeping the day inside the month (31 Jan to Feb becomes 28 or 29 Feb). */
export function changePart(parts: DateParts, part: keyof DateParts, value: number): DateParts {
  const next = { ...parts, [part]: value }
  next.day = Math.min(next.day, daysInMonth(next.year, next.month))
  return next
}

export function joinParts(kind: DateKind, parts: DateParts): string {
  const pad = (n: number) => String(n).padStart(2, '0')
  if (kind === 'year') return String(parts.year)
  if (kind === 'month') return `${parts.year}-${pad(parts.month)}`
  return `${parts.year}-${pad(parts.month)}-${pad(parts.day)}`
}
