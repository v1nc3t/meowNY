import assert from 'node:assert/strict'
import { test } from 'node:test'
import { apiBound, lastDay, shiftBound, thisPeriod } from './range.ts'

test('a day, a month and a year each stand for their first and last day', () => {
  assert.equal(apiBound('date', '2026-03-10', 'from'), '2026-03-10')
  assert.equal(apiBound('date', '2026-03-10', 'to'), '2026-03-10')
  assert.equal(apiBound('month', '2026-03', 'from'), '2026-03-01')
  assert.equal(apiBound('month', '2026-03', 'to'), '2026-03-31')
  assert.equal(apiBound('year', '2026', 'from'), '2026-01-01')
  assert.equal(apiBound('year', '2026', 'to'), '2026-12-31')
})

test('a value that is not a whole one of its kind is no bound', () => {
  for (const [mode, value] of [['date', '2026-03'], ['date', ''], ['month', '2026-13'], ['month', '2026-3'], ['month', '2026-03-01'], ['year', '26'], ['year', '2026-03']] as const) {
    assert.equal(apiBound(mode, value, 'from'), '', `${mode} ${value}`)
  }
})

test('the last day of a month knows the lengths and the leap years', () => {
  assert.equal(lastDay('2026-02'), '2026-02-28')
  assert.equal(lastDay('2024-02'), '2024-02-29')
  assert.equal(lastDay('2100-02'), '2100-02-28', 'a century is not a leap year')
  assert.equal(lastDay('2000-02'), '2000-02-29')
  assert.equal(lastDay('2026-04'), '2026-04-30')
  assert.equal(lastDay('2026-12'), '2026-12-31')
})

test('changing how a range is chosen keeps what it covers as far as it can', () => {
  // coarser: the month or year the day or month is in, for both ends
  assert.equal(shiftBound('2026-03-10', 'date', 'month', 'from'), '2026-03')
  assert.equal(shiftBound('2026-03-10', 'date', 'year', 'to'), '2026')
  assert.equal(shiftBound('2026-03', 'month', 'year', 'from'), '2026')
  // finer: the first day for a start, the last for an end
  assert.equal(shiftBound('2026-03', 'month', 'date', 'from'), '2026-03-01')
  assert.equal(shiftBound('2026-03', 'month', 'date', 'to'), '2026-03-31')
  assert.equal(shiftBound('2026', 'year', 'month', 'from'), '2026-01')
  assert.equal(shiftBound('2026', 'year', 'month', 'to'), '2026-12')
  assert.equal(shiftBound('2026', 'year', 'date', 'to'), '2026-12-31')
  // nothing to keep, or nothing changed
  assert.equal(shiftBound('', 'date', 'month', 'from'), '')
  assert.equal(shiftBound('2026-03', 'month', 'month', 'from'), '2026-03')
  assert.equal(shiftBound('garbage', 'month', 'date', 'from'), '')
})

test('this period is the current day, month or year', () => {
  const today = { date: '2026-10-16', month: '2026-10' }
  assert.equal(thisPeriod('date', today), '2026-10-16')
  assert.equal(thisPeriod('month', today), '2026-10')
  assert.equal(thisPeriod('year', today), '2026')
})
