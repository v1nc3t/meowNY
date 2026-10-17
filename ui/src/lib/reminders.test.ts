import assert from 'node:assert/strict'
import { test } from 'node:test'
import { en } from '../i18n/en.ts'
import { LANGUAGES } from '../i18n/languages.ts'
import { DEFAULT_REMINDER_EVERY, DEFAULT_REMINDER_TIME, localDay, notificationId, notifiedDays, parseTime, REMINDER_PERIODS, reminderEvery, reminderTime, ringNow, saveReminderEvery, schedulePlan } from './reminders.ts'

const reminders = (today: string, due: [string, string, string][] = [], upcoming: [string, number][] = []) => ({
  today,
  due: due.map(([personId, name, dueOn]) => ({ personId, name, dueOn })),
  upcoming: upcoming.map(([date, count]) => ({ date, count })),
})

test('a time of day is HH:MM on a 24-hour clock and nothing else', () => {
  assert.deepEqual(parseTime('09:00'), { hours: 9, minutes: 0 })
  assert.deepEqual(parseTime('23:59'), { hours: 23, minutes: 59 })
  for (const bad of ['', '9:00', '24:00', '12:60', '12:5', '12-30', '1200', 'ab:cd', '12:30:00']) assert.equal(parseTime(bad), null, bad)
})

test('with nothing saved a device rings at nine and remembers nothing', () => {
  assert.equal(reminderTime(), DEFAULT_REMINDER_TIME)
  assert.deepEqual(notifiedDays(), {})
})

test('a device reminds every week unless told otherwise, and only the periods on offer are kept', () => {
  assert.equal(reminderEvery(), DEFAULT_REMINDER_EVERY, 'nothing saved')
  assert.ok(REMINDER_PERIODS.includes(DEFAULT_REMINDER_EVERY))
  const store = new Map<string, string>()
  Object.defineProperty(globalThis, 'localStorage', {
    configurable: true,
    value: { getItem: (key: string) => store.get(key) ?? null, setItem: (key: string, value: string) => void store.set(key, value) },
  })
  try {
    for (const days of REMINDER_PERIODS) {
      saveReminderEvery(days)
      assert.equal(reminderEvery(), days)
    }
    saveReminderEvery(14)
    saveReminderEvery(5)
    saveReminderEvery(-7)
    assert.equal(reminderEvery(), 14, 'a period that is not offered is not kept')
    store.set('debt-reminder-every', 'soon')
    assert.equal(reminderEvery(), DEFAULT_REMINDER_EVERY, 'a damaged value is the default')
    store.set('debt-reminder-every', '9')
    assert.equal(reminderEvery(), DEFAULT_REMINDER_EVERY)
  } finally {
    Reflect.deleteProperty(globalThis, 'localStorage')
  }
})

test('a reminder rings when its day has come and the time of day is reached, once for each day', () => {
  const morning = new Date(2026, 9, 20, 8, 59)
  const nine = new Date(2026, 9, 20, 9, 0)
  const today = reminders('2026-10-20', [['1', 'Anna', '2026-10-20']])
  assert.deepEqual(ringNow(today, '09:00', morning, {}), [], 'not before the time')
  assert.deepEqual(ringNow(today, '09:00', nine, {}).map((f) => f.personId), ['1'])
  assert.deepEqual(ringNow(today, '09:00', nine, { '1': '2026-10-20' }), [], 'this day was shown already')
  assert.equal(ringNow(today, '09:00', nine, { '1': '2026-10-13' }).length, 1, 'an older day does not count: this one is new')
})

test('a reminder that came while the app was closed rings at once, whatever the time of day', () => {
  const early = new Date(2026, 9, 20, 6, 0)
  const missed = reminders('2026-10-20', [['1', 'Anna', '2026-10-18'], ['2', 'Bob', '2026-10-20']])
  assert.deepEqual(ringNow(missed, '09:00', early, {}).map((f) => f.personId), ['1'], "Bob's is today and waits for nine")
})

test('a bad time rings nothing', () => {
  assert.deepEqual(ringNow(reminders('2026-10-20', [['1', 'Anna', '2026-10-01']]), '25:00', new Date(2026, 9, 20, 12, 0), {}), [])
})

test('the phone schedules each coming day once, at the time of day, and only what is still ahead', () => {
  const now = new Date(2026, 9, 20, 10, 0)
  const upcoming = reminders('2026-10-20', [], [['2026-10-20', 2], ['2026-10-21', 1], ['2026-10-28', 1]])
  const plan = schedulePlan(upcoming, '09:00', now)
  assert.deepEqual(plan.map((p) => [p.id, localDay(p.at), p.at.getHours(), p.count]), [[notificationId('2026-10-21'), '2026-10-21', 9, 1], [notificationId('2026-10-28'), '2026-10-28', 9, 1]], 'today at nine has gone')
  assert.deepEqual(schedulePlan(upcoming, '11:00', now).map((p) => localDay(p.at)), ['2026-10-20', '2026-10-21', '2026-10-28'])
  assert.deepEqual(schedulePlan(upcoming, 'x', now), [])
  assert.deepEqual(schedulePlan(reminders('2026-10-20', [], [['2026-13-40', 1]]), '09:00', now).length, 0, 'a day that is not a day is skipped')
})

test('the id of a day is the same every time, different for another day and fits the 32 bits the system gives', () => {
  assert.equal(notificationId('2026-10-21'), notificationId('2026-10-21'))
  assert.notEqual(notificationId('2026-10-21'), notificationId('2026-10-22'))
  assert.ok(notificationId('2099-12-31') < 2 ** 31)
})

test('the notification never holds a name or an amount, in any language, only a count', () => {
  for (const language of [{ code: 'en', messages: en }, ...LANGUAGES]) {
    for (const key of ['notify.debtTitle', 'notify.debtBody'] as const) {
      const text = language.messages[key]
      assert.deepEqual([...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).filter((name) => name !== 'count'), [], `${language.code} ${key}`)
      assert.ok(!/\d/.test(text), `${language.code} ${key} has a digit`)
    }
  }
})
