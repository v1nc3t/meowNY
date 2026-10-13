export type ScrollKind = 'date' | 'month' | 'year'

export type Ymd = { year: number; month: number; day: number }

const MONTHS = ['jan', 'feb', 'mar', 'apr', 'may', 'jun', 'jul', 'aug', 'sep', 'oct', 'nov', 'dec']

export function monthName(month: number): string {
  return MONTHS[month - 1] ?? ''
}

export function choiceLabel(_kind: ScrollKind, value: string): string {
  const date = /^(\d{4})-(0[1-9]|1[0-2])-(0[1-9]|[12]\d|3[01])$/.exec(value)
  if (date) return `${date[3]} ${date[2]} ${date[1]}`
  const month = /^(\d{4})-(0[1-9]|1[0-2])$/.exec(value)
  if (month) return `01 ${month[2]} ${month[1]}`
  if (/^\d{4}$/.test(value)) return `01 01 ${value}`
  return value
}

export function rangeBounds(now: Date): { start: Ymd; end: Ymd } {
  const endDate = addYears(now, 1)
  return {
    start: { year: now.getFullYear() - 10, month: now.getMonth() + 1, day: 1 },
    end: { year: endDate.getFullYear(), month: endDate.getMonth() + 1, day: endDate.getDate() },
  }
}

export function parseValue(kind: ScrollKind, value: string, now: Date): Ymd {
  const bounds = rangeBounds(now)
  const date = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value)
  if (kind === 'date' && date) return clampYmd({ year: +date[1], month: +date[2], day: +date[3] }, bounds)
  const month = /^(\d{4})-(\d{2})$/.exec(value)
  if (kind === 'month' && month) return clampYmd({ year: +month[1], month: +month[2], day: 1 }, bounds)
  const year = /^(\d{4})$/.exec(value)
  if (kind === 'year' && year) return clampYmd({ year: +year[1], month: 1, day: 1 }, bounds)
  return clampYmd(
    { year: now.getFullYear(), month: now.getMonth() + 1, day: kind === 'date' ? now.getDate() : 1 },
    bounds,
  )
}

export function emitValue(kind: ScrollKind, date: Ymd): string {
  if (kind === 'year') return String(date.year)
  if (kind === 'month') return `${date.year}-${pad(date.month)}`
  return `${date.year}-${pad(date.month)}-${pad(date.day)}`
}

export function yearsIn(bounds: { start: Ymd; end: Ymd }): number[] {
  const years: number[] = []
  for (let year = bounds.start.year; year <= bounds.end.year; year += 1) years.push(year)
  return years
}

export function monthsIn(year: number, bounds: { start: Ymd; end: Ymd }): number[] {
  const first = year === bounds.start.year ? bounds.start.month : 1
  const last = year === bounds.end.year ? bounds.end.month : 12
  const months: number[] = []
  for (let month = first; month <= last; month += 1) months.push(month)
  return months
}

export function daysIn(year: number, month: number, bounds: { start: Ymd; end: Ymd }): number[] {
  let first = 1
  let last = daysInMonth(year, month)
  if (year === bounds.start.year && month === bounds.start.month) first = bounds.start.day
  if (year === bounds.end.year && month === bounds.end.month) last = Math.min(last, bounds.end.day)
  const days: number[] = []
  for (let day = first; day <= last; day += 1) days.push(day)
  return days
}

export function withPart(date: Ymd, part: 'day' | 'month' | 'year', value: number, now: Date): Ymd {
  const next = { ...date, [part]: value }
  if (part !== 'day') next.day = Math.min(next.day, daysInMonth(next.year, next.month))
  return clampYmd(next, rangeBounds(now))
}

function clampYmd(date: Ymd, bounds: { start: Ymd; end: Ymd }): Ymd {
  const month = Math.min(12, Math.max(1, date.month))
  const day = Math.min(daysInMonth(date.year, month), Math.max(1, date.day))
  const next = { year: date.year, month, day }
  if (stamp(next) < stamp(bounds.start)) return bounds.start
  if (stamp(next) > stamp(bounds.end)) return bounds.end
  return next
}

function addYears(date: Date, years: number): Date {
  const next = new Date(date.getFullYear() + years, date.getMonth(), date.getDate())
  if (next.getMonth() !== date.getMonth()) return new Date(date.getFullYear() + years, date.getMonth() + 1, 0)
  return next
}

function daysInMonth(year: number, month: number): number {
  return new Date(Date.UTC(year, month, 0)).getUTCDate()
}

function stamp(date: Ymd): number {
  return date.year * 10000 + date.month * 100 + date.day
}

function pad(value: number): string {
  return String(value).padStart(2, '0')
}
