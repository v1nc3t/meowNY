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
