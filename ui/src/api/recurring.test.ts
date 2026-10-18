import assert from 'node:assert/strict'
import { test } from 'node:test'
import { Recurring } from './recurring.ts'

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

test('a bill with no fixed amount has a null amount, and no other amount may be anything but two decimals', () => {
  assert.ok(Recurring.safeParse({ ...row, amount: null }).success)
  assert.ok(!Recurring.safeParse({ ...row, amount: '12' }).success)
  assert.ok(!Recurring.safeParse({ ...row, amount: undefined }).success, 'the amount is always present, as a value or null')
})
