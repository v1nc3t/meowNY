import assert from 'node:assert/strict'
import { test } from 'node:test'
import { MONTH, MonthHistory, Overview } from './budgets.ts'

const side = {
  type: 'EXPENSE',
  budgeted: '1000.00',
  allocated: '1100.00',
  unallocated: '-100.00',
  actual: '800.50',
  remaining: '199.50',
  overAllocated: true,
  categories: [{ categoryId: '3', amountLimit: '700.00' }],
}

test('an overview side allows negative amounts and missing budgets', () => {
  assert.ok(Overview.safeParse(side).success)
  assert.ok(Overview.safeParse({ ...side, budgeted: null, unallocated: null, remaining: null }).success)
  assert.ok(!Overview.safeParse({ ...side, actual: 800.5 }).success)
  assert.ok(!Overview.safeParse({ ...side, type: 'OTHER' }).success)
})

test('months are YYYY-MM with a real month number', () => {
  assert.ok(MONTH.test('2026-06'))
  assert.ok(!MONTH.test('2026-13'))
  assert.ok(!MONTH.test('2026-6'))
  assert.ok(!MONTH.test('2026-06-01'))
})

test('a history row has a month and money as strings, budgets may be missing', () => {
  const row = { month: '2026-10', expenseBudget: null, expenseActual: '12.00', incomeBudget: '3000.00', incomeActual: '0.00' }
  assert.ok(MonthHistory.safeParse(row).success)
  assert.ok(!MonthHistory.safeParse({ ...row, expenseActual: 12 }).success)
  assert.ok(!MonthHistory.safeParse({ ...row, month: 'October' }).success)
})
