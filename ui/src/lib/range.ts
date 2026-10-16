/** How a range is chosen: by day, by month or by year. The same on the transactions page and the export page. */
export type RangeMode = 'date' | 'month' | 'year'

/**
 * The first or last day a bound stands for, as "YYYY-MM-DD" (a month is its first or last day, a year its first or last). Empty when
 * the value is not a complete one of its kind.
 */
export function apiBound(mode: RangeMode, value: string, edge: 'from' | 'to'): string {
  if (mode === 'date') return /^\d{4}-\d{2}-\d{2}$/.test(value) ? value : ''
  if (mode === 'month') {
    if (!/^\d{4}-(0[1-9]|1[0-2])$/.test(value)) return ''
    return edge === 'from' ? `${value}-01` : lastDay(value)
  }
  if (!/^\d{4}$/.test(value)) return ''
  return edge === 'from' ? `${value}-01-01` : `${value}-12-31`
}

export function shiftBound(value: string, fromMode: RangeMode, toMode: RangeMode, edge: 'from' | 'to'): string {
  if (!value || fromMode === toMode) return fromMode === toMode ? value : ''
  if (fromMode === 'date' && /^\d{4}-\d{2}-\d{2}$/.test(value)) {
    if (toMode === 'month') return value.slice(0, 7)
    return value.slice(0, 4)
  }
  if (fromMode === 'month' && /^\d{4}-(0[1-9]|1[0-2])$/.test(value)) {
    if (toMode === 'year') return value.slice(0, 4)
    return edge === 'from' ? `${value}-01` : lastDay(value)
  }
  if (fromMode === 'year' && /^\d{4}$/.test(value)) {
    if (toMode === 'month') return edge === 'from' ? `${value}-01` : `${value}-12`
    return edge === 'from' ? `${value}-01-01` : `${value}-12-31`
  }
  return ''
}

export function lastDay(yearMonth: string): string {
  const year = Number(yearMonth.slice(0, 4))
  const month = Number(yearMonth.slice(5, 7))
  const day = new Date(Date.UTC(year, month, 0)).getUTCDate()
  return `${yearMonth}-${String(day).padStart(2, '0')}`
}

/** The current day, month or year as a bound of that kind. The date comes from the timezone setting, not the browser's clock. */
export function thisPeriod(mode: RangeMode, today: { date: string; month: string }): string {
  if (mode === 'date') return today.date
  if (mode === 'month') return today.month
  return today.date.slice(0, 4)
}
