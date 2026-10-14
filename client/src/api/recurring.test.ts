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
}

test('a recurring answer keeps money as a two-decimal string and a known unit', () => {
  assert.ok(Recurring.safeParse(row).success)
  assert.ok(!Recurring.safeParse({ ...row, amount: 900 }).success)
  assert.ok(!Recurring.safeParse({ ...row, intervalUnit: 'DECADE' }).success)
  assert.ok(!Recurring.safeParse({ ...row, intervalCount: 0 }).success)
  assert.ok(!Recurring.safeParse({ ...row, nextDueDate: '28/02/2026' }).success)
})

test('dues carry both lists', () => {
  assert.ok(Dues.safeParse({ overdue: [], upcoming: [row] }).success)
  assert.ok(!Dues.safeParse({ overdue: [] }).success)
})
