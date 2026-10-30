import { memoByKey } from './cache'

const dayFormat = memoByKey(
  (locale: string, withYear: boolean) => `${locale}|${withYear}`,
  (locale: string, withYear: boolean) => {
    try {
      return new Intl.DateTimeFormat(locale, { weekday: 'short', day: 'numeric', month: 'short', ...(withYear ? { year: 'numeric' } : {}), timeZone: 'UTC' })
    } catch {
      return null
    }
  },
)

/** "wed 28 oct": the heading of a day in a phone's list, in the words of the locale, with the year when it is not this one. */
export function dayHeading(isoDate: string, locale: string, thisYear: string): string {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(isoDate)
  if (!match) return isoDate
  const date = new Date(Date.UTC(Number(match[1]), Number(match[2]) - 1, Number(match[3])))
  return dayFormat(locale, match[1] !== thisYear)?.format(date).toLowerCase() ?? isoDate
}

/** The rows of a list in the order they come, in a group for each run of the same day. */
export function byDay<T>(rows: readonly T[], dateOf: (row: T) => string): { date: string; rows: T[] }[] {
  const groups: { date: string; rows: T[] }[] = []
  for (const row of rows) {
    const date = dateOf(row)
    const last = groups[groups.length - 1]
    if (last && last.date === date) last.rows.push(row)
    else groups.push({ date, rows: [row] })
  }
  return groups
}
