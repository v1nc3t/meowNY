import assert from 'node:assert/strict'
import { test } from 'node:test'
import { columnName, looksNumeric } from './grid.ts'

test('columns are lettered as a spreadsheet does', () => {
  assert.deepEqual([0, 1, 25, 26, 27, 51, 52, 701, 702].map(columnName), ['A', 'B', 'Z', 'AA', 'AB', 'AZ', 'BA', 'ZZ', 'AAA'])
})

test('a plain number goes to the right of its cell, and a date, a text or an empty cell does not', () => {
  for (const text of ['0', '900', '-5', '812.4', '900,50', '12%']) assert.equal(looksNumeric(text), true, text)
  for (const text of ['', 'January', '2026-01-01', '12:00', '1.2.3', 'TRUE', '1e5']) assert.equal(looksNumeric(text), false, text)
})
