import { memoByKey } from '../lib/cache'

/** Month helpers for "YYYY-MM" strings. No time zones involved: a month is just a label. */

export function shiftMonth(month: string, delta: number): string {
  const [year, number] = month.split('-').map(Number)
  const index = year * 12 + (number - 1) + delta
  const nextYear = Math.floor(index / 12)
  return `${nextYear}-${String((index % 12) + 1).padStart(2, '0')}`
}

const labelFormatter = memoByKey(
  (locale: string) => locale,
  (locale: string) => {
    try {
      return new Intl.DateTimeFormat(locale, { month: 'long', year: 'numeric', timeZone: 'UTC' })
    } catch {
      return null
    }
  },
)

/** "October 2026" in the person's locale. Falls back to the plain label if the locale is unusable. */
export function monthLabel(month: string, locale: string): string {
  const [year, number] = month.split('-').map(Number)
  return labelFormatter(locale)?.format(new Date(Date.UTC(year, number - 1, 1))) ?? month
}

/** Lower case in the person's language ("I" is "ı" in Turkish); a locale the engine rejects gets the plain lower case. */
function lower(text: string, locale: string): string {
  try {
    return text.toLocaleLowerCase(locale)
  } catch {
    return text.toLowerCase()
  }
}

const namesFor = memoByKey(
  (locale: string) => locale,
  (locale: string): string[] => {
    try {
      const formatter = new Intl.DateTimeFormat(locale, { month: 'short', timeZone: 'UTC' })
      // Some locales write "Sept" or "Sep."; keep three characters.
      return Array.from({ length: 12 }, (_, index) =>
        lower(formatter.format(new Date(Date.UTC(2000, index, 1))).replace(/\.$/, ''), locale).slice(0, 3),
      )
    } catch {
      return Array.from({ length: 12 }, (_, index) => String(index + 1))
    }
  },
)

/** The twelve months as lower case three-letter abbreviations in the person's locale, January first. The list is shared; do not change it. */
export function monthNames(locale: string): string[] {
  return namesFor(locale)
}

// Every spelling a person may type for each month: the short name, and the full one.
const spellingsFor = memoByKey(
  (locale: string) => locale,
  (locale: string): string[][] => {
    try {
      const short = new Intl.DateTimeFormat(locale, { month: 'short', timeZone: 'UTC' })
      const long = new Intl.DateTimeFormat(locale, { month: 'long', timeZone: 'UTC' })
      return namesFor(locale).map((name, index) => {
        const day = new Date(Date.UTC(2000, index, 1))
        return [name, lower(short.format(day).replace(/\.$/, ''), locale), lower(long.format(day), locale)]
      })
    } catch {
      return Array.from({ length: 12 }, () => [])
    }
  },
)

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

/** A typed day: digits only, from 1 to the days of that month. Null when it is not one. */
export function parseDay(text: string, year: number, month: number): number | null {
  const typed = text.trim()
  if (!/^\d+$/.test(typed)) return null
  const day = Number(typed)
  return day >= 1 && day <= daysInMonth(year, month) ? day : null
}

/**
 * A typed month, 1 to 12. Digits ("11", "011"), or at least three letters that start one month's name in the
 * language or in English, in any case ("nov", "November"). Null when it is none or more than one month.
 */
export function parseMonth(text: string, locale: string): number | null {
  const typed = text.trim().toLowerCase()
  if (/^\d+$/.test(typed)) {
    const month = Number(typed)
    return month >= 1 && month <= 12 ? month : null
  }
  if (typed.length < 3) return null
  const hits = new Set<number>()
  for (const spellings of [spellingsFor(locale), spellingsFor('en')]) {
    spellings.forEach((names, index) => {
      if (names.some((name) => name.startsWith(typed))) hits.add(index + 1)
    })
  }
  return hits.size === 1 ? [...hits][0] : null
}

/** A typed year: four digits, 1900 to 2100. */
export function parseYear(text: string): number | null {
  const typed = text.trim()
  if (!/^\d{4}$/.test(typed)) return null
  const year = Number(typed)
  return year >= 1900 && year <= 2100 ? year : null
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
