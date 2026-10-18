import assert from 'node:assert/strict'
import { test } from 'node:test'
import { changePart, daysInMonth, monthLabel, parseParts, shiftMonth } from './months.ts'

test('stepping a month crosses year ends in both directions', () => {
  assert.equal(shiftMonth('2026-10', 1), '2026-11')
  assert.equal(shiftMonth('2026-12', 1), '2027-01')
  assert.equal(shiftMonth('2027-01', -1), '2026-12')
  assert.equal(shiftMonth('2026-03', -3), '2025-12')
  assert.equal(shiftMonth('2026-03', 0), '2026-03')
  assert.equal(shiftMonth('2026-03', 24), '2028-03')
})

test('the month label uses the locale and does not shift with the time zone', () => {
  assert.equal(monthLabel('2026-10', 'en-US'), 'October 2026')
  assert.equal(monthLabel('2026-01', 'en-US'), 'January 2026')
  assert.equal(monthLabel('2026-10', 'not a locale!!'), '2026-10')
})

test('date parts parse each kind and fall back to the given date', () => {
  assert.deepEqual(parseParts('date', '2026-10-15', '2020-01-01'), { year: 2026, month: 10, day: 15 })
  assert.deepEqual(parseParts('month', '2026-10', '2020-01-01'), { year: 2026, month: 10, day: 1 })
  assert.deepEqual(parseParts('year', '2026', '2020-05-05'), { year: 2026, month: 1, day: 1 })
  assert.deepEqual(parseParts('date', '', '2026-10-13'), { year: 2026, month: 10, day: 13 })
  assert.deepEqual(parseParts('month', 'oops', '2026-10-13'), { year: 2026, month: 10, day: 1 })
  assert.deepEqual(parseParts('date', '2026-02-31', '2020-01-01'), { year: 2026, month: 2, day: 28 }, 'an impossible day is pulled into the month')
})

test('changing a part keeps the day valid', () => {
  const jan31 = { year: 2026, month: 1, day: 31 }
  assert.deepEqual(changePart(jan31, 'month', 2), { year: 2026, month: 2, day: 28 })
  assert.deepEqual(changePart({ year: 2024, month: 2, day: 29 }, 'year', 2025), { year: 2025, month: 2, day: 28 })
  assert.deepEqual(changePart(jan31, 'day', 15), { year: 2026, month: 1, day: 15 })
})

test('days in a month handle leap years', () => {
  assert.equal(daysInMonth(2024, 2), 29)
  assert.equal(daysInMonth(2026, 2), 28)
  assert.equal(daysInMonth(2026, 4), 30)
})
