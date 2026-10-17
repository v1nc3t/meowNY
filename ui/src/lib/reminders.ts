import type { Reminders } from '../api/debts'

/** The time of day a reminder rings, on this device: "HH:MM". Each device has its own. */
export const DEFAULT_REMINDER_TIME = '09:00'
const TIME_KEY = 'debt-reminder-time'
const NOTIFIED_KEY = 'debt-reminded'
const SCHEDULED_KEY = 'debt-scheduled'

/** Hours and minutes of "HH:MM", or null when it is not a time. */
export function parseTime(text: string): { hours: number; minutes: number } | null {
  const match = /^([01]\d|2[0-3]):([0-5]\d)$/.exec(text)
  return match ? { hours: Number(match[1]), minutes: Number(match[2]) } : null
}

function read(key: string): string | null {
  try {
    return localStorage.getItem(key)
  } catch {
    return null
  }
}

function write(key: string, value: string): void {
  try {
    localStorage.setItem(key, value)
  } catch {
    // Not kept: the next check asks again.
  }
}

export function reminderTime(): string {
  const saved = read(TIME_KEY)
  return saved !== null && parseTime(saved) ? saved : DEFAULT_REMINDER_TIME
}

export function saveReminderTime(time: string): void {
  if (parseTime(time)) write(TIME_KEY, time)
}

/** Which day of each friend's reminder this device has already shown: friend id to day. */
export function notifiedDays(): Record<string, string> {
  try {
    const parsed: unknown = JSON.parse(read(NOTIFIED_KEY) ?? '{}')
    if (typeof parsed !== 'object' || parsed === null) return {}
    return Object.fromEntries(Object.entries(parsed).filter((entry): entry is [string, string] => typeof entry[1] === 'string'))
  } catch {
    return {}
  }
}

export function saveNotifiedDays(days: Record<string, string>): void {
  write(NOTIFIED_KEY, JSON.stringify(days))
}

const pad = (n: number) => String(n).padStart(2, '0')

/** The day of a date on this device's clock, "YYYY-MM-DD". */
export function localDay(now: Date): string {
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`
}

/**
 * The friends this device should ring about now: the day of their reminder has come, this device has not shown that day yet, and either
 * the day is past (it came while the app was closed) or the time of day has been reached. `today` is the day in the person's time zone,
 * as the app says it; the clock of the device gives the time.
 */
export function ringNow(reminders: Reminders, time: string, now: Date, notified: Record<string, string>): Reminders['due'] {
  const at = parseTime(time)
  if (!at) return []
  const reached = now.getHours() * 60 + now.getMinutes() >= at.hours * 60 + at.minutes
  return reminders.due.filter((friend) => (notified[friend.personId] ?? '') < friend.dueOn && (friend.dueOn < reminders.today || reached))
}

/** One notification the phone hands to the system. */
export type Scheduled = { id: number; at: Date; count: number }

/** The system's id for the notification of a day: the same day always has the same one, so it replaces itself. */
export function notificationId(day: string): number {
  return 900_000_000 + Number(day.replaceAll('-', ''))
}

/**
 * The notifications to schedule for the days that come: each at the chosen time of day on that day, and only those still ahead. One
 * notification for each day, however many friends it is for.
 */
export function schedulePlan(reminders: Reminders, time: string, now: Date): Scheduled[] {
  const at = parseTime(time)
  if (!at) return []
  const plan: Scheduled[] = []
  for (const { date, count } of reminders.upcoming) {
    const [year, month, day] = date.split('-').map(Number)
    if (!year || !month || !day) continue
    const when = new Date(year, month - 1, day, at.hours, at.minutes)
    // `Date` moves 40 October to November; a day that is not a day is left out
    if (when.getFullYear() !== year || when.getMonth() !== month - 1 || when.getDate() !== day) continue
    if (when.getTime() > now.getTime() + 30_000) plan.push({ id: notificationId(date), at: when, count })
  }
  return plan
}

/** The ids this device has scheduled before, to cancel them when it schedules again. */
export function scheduledIds(): number[] {
  try {
    const parsed: unknown = JSON.parse(read(SCHEDULED_KEY) ?? '[]')
    return Array.isArray(parsed) ? parsed.filter((id): id is number => Number.isInteger(id)) : []
  } catch {
    return []
  }
}

export function saveScheduledIds(ids: number[]): void {
  write(SCHEDULED_KEY, JSON.stringify(ids))
}

/** Something that changed a friend's reminder or balance: the phone schedules again. */
export const REMINDERS_CHANGED = 'meowny-reminders-changed'

export function remindersChanged(): void {
  if (typeof window !== 'undefined') window.dispatchEvent(new Event(REMINDERS_CHANGED))
}
