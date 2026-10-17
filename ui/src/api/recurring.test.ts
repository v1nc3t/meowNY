import assert from 'node:assert/strict'
import { test } from 'node:test'
import { Dues, Recurring } from './recurring.ts'

const row = {
  id: '3',
  categoryId: '2',
  name: 'Rent',
  description: null,
  amount: '900.00',
  startDate: '2026-01-31',
  intervalUnit: 'MONTH',
  intervalCount: 1,
  nextDueDate: '2026-02-28',
  endDate: null,
  isActive: true,
  split: null,
}

test('a recurring answer keeps money as a two-decimal string and a known unit', () => {
  assert.ok(Recurring.safeParse(row).success)
  assert.ok(!Recurring.safeParse({ ...row, amount: 900 }).success)
  assert.ok(!Recurring.safeParse({ ...row, intervalUnit: 'DECADE' }).success)
  assert.ok(!Recurring.safeParse({ ...row, intervalCount: 0 }).success)
  assert.ok(!Recurring.safeParse({ ...row, nextDueDate: '28/02/2026' }).success)
})

test('a bill with no fixed amount has a null amount, and no other amount may be anything but two decimals', () => {
  assert.ok(Recurring.safeParse({ ...row, amount: null }).success)
  assert.ok(!Recurring.safeParse({ ...row, amount: '12' }).success)
  assert.ok(!Recurring.safeParse({ ...row, amount: undefined }).success, 'the amount is always present, as a value or null')
})

test('an item may carry the way its payments are shared, as weights', () => {
  const split = { mine: 2, parts: [{ personId: '4', person: 'Anna', guest: null, weight: 1 }, { personId: null, person: null, guest: 'Bob', weight: 1 }] }
  assert.ok(Recurring.safeParse({ ...row, split }).success)
  assert.ok(!Recurring.safeParse({ ...row, split: { ...split, mine: -1 } }).success)
  assert.ok(!Recurring.safeParse({ ...row, split: undefined }).success, 'the split is always present, as a value or null')
})

test('dues carry both lists', () => {
  assert.ok(Dues.safeParse({ overdue: [], upcoming: [row] }).success)
  assert.ok(!Dues.safeParse({ overdue: [] }).success)
})
