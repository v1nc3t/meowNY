import assert from 'node:assert/strict'
import { test } from 'node:test'
import { changePart, daysInMonth, monthLabel, monthNames, parseDay, parseMonth, parseParts, parseYear, shiftMonth } from './months.ts'

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

test('month names are three lower case letters in the language', () => {
  assert.deepEqual(monthNames('en'), ['jan', 'feb', 'mar', 'apr', 'may', 'jun', 'jul', 'aug', 'sep', 'oct', 'nov', 'dec'])
  assert.equal(monthNames('de')[2], 'mär')
  assert.equal(monthNames('not a locale!!')[10], '11', 'an unusable locale falls back to numbers')
})

test('a typed day is digits within the days of the month', () => {
  assert.equal(parseDay('5', 2026, 11), 5)
  assert.equal(parseDay('05', 2026, 11), 5)
  assert.equal(parseDay(' 30 ', 2026, 11), 30)
  assert.equal(parseDay('31', 2026, 11), null, 'November has 30 days')
  assert.equal(parseDay('31', 2026, 10), 31)
  assert.equal(parseDay('29', 2026, 2), null)
  assert.equal(parseDay('29', 2024, 2), 29)
  for (const bad of ['', '0', '00', 'x', '1x', '-1', '1.5', '32']) assert.equal(parseDay(bad, 2026, 10), null, bad)
})

test('a typed month is a number or the start of a name, in any case', () => {
  for (const text of ['11', '011', 'nov', 'Nov', 'NOV', 'november', 'November']) assert.equal(parseMonth(text, 'en'), 11, text)
  assert.equal(parseMonth('1', 'en'), 1)
  assert.equal(parseMonth('12', 'en'), 12)
  for (const bad of ['', ' ', '0', '13', 'xyz', 'no', 'novx', 'ju', '1x']) assert.equal(parseMonth(bad, 'en'), null, bad)
  assert.equal(parseMonth('jun', 'en'), 6)
  assert.equal(parseMonth('juni', 'de'), 6, 'the language of the app')
  assert.equal(parseMonth('mär', 'de'), 3)
  assert.equal(parseMonth('märz', 'de'), 3)
  assert.equal(parseMonth('novembre', 'fr'), 11)
  assert.equal(parseMonth('dec', 'de'), 12, 'English names always work')
  assert.equal(parseMonth('jui', 'fr'), null, 'juin and juillet both start with it')
  assert.equal(parseMonth('nov', 'not a locale!!'), 11)
})

test('a typed year is four digits from 1900 to 2100', () => {
  assert.equal(parseYear('2026'), 2026)
  assert.equal(parseYear('1900'), 1900)
  assert.equal(parseYear('2100'), 2100)
  for (const bad of ['', '26', '202', '20266', '1899', '2101', 'abcd', '20 6']) assert.equal(parseYear(bad), null, bad)
})
