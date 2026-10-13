import assert from 'node:assert/strict'
import test from 'node:test'
import { choiceLabel, daysIn, emitValue, monthName, monthsIn, parseValue, rangeBounds, withPart, yearsIn } from './dateScrollModel.ts'

test('date parts stop one year ahead and read dd mm yyyy', () => {
  const now = new Date(2026, 9, 11)
  const bounds = rangeBounds(now)

  assert.deepEqual(bounds.start, { year: 2016, month: 10, day: 1 })
  assert.deepEqual(bounds.end, { year: 2027, month: 10, day: 11 })
  assert.deepEqual(yearsIn(bounds), [2016, 2017, 2018, 2019, 2020, 2021, 2022, 2023, 2024, 2025, 2026, 2027])
  assert.deepEqual(monthsIn(2016, bounds), [10, 11, 12])
  assert.deepEqual(monthsIn(2026, bounds), [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12])
  assert.deepEqual(monthsIn(2027, bounds), [1, 2, 3, 4, 5, 6, 7, 8, 9, 10])
  assert.equal(daysIn(2027, 10, bounds).at(-1), 11)
  assert.equal(daysIn(2024, 2, bounds).at(-1), 29)
  assert.equal(daysIn(2023, 2, bounds).at(-1), 28)

  assert.equal(monthName(10), 'oct')
  assert.equal(choiceLabel('date', '2026-10-11'), '11 10 2026')
  assert.equal(choiceLabel('month', '2026-10'), '01 10 2026')
  assert.equal(choiceLabel('year', '2026'), '01 01 2026')

  const picked = withPart(parseValue('date', '2026-10-11', now), 'year', 2028, now)
  assert.deepEqual(picked, bounds.end)
  assert.equal(emitValue('date', picked), '2027-10-11')
  assert.equal(emitValue('month', parseValue('month', '2026-10', now)), '2026-10')
  assert.equal(emitValue('year', parseValue('year', '2026', now)), '2026')
})
