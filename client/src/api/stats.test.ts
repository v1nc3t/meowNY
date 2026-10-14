import assert from 'node:assert/strict'
import { test } from 'node:test'
import { BudgetVsActual, Commitments, PlannedVsPaid, TopCategory, YearPoint } from './stats.ts'

const food = {
  categoryId: '1',
  name: 'Food',
  type: 'EXPENSE',
  budgeted: '400.00',
  actual: '356.20',
  remaining: '43.80',
  pctUsed: 89.1,
}

test('a category line allows a missing budget but keeps money as strings', () => {
  const answer = { month: '2026-10', categories: [food, { ...food, budgeted: null, remaining: null, pctUsed: null }], totals: [] }
  assert.ok(BudgetVsActual.safeParse(answer).success)
  assert.ok(!BudgetVsActual.safeParse({ ...answer, categories: [{ ...food, actual: 356.2 }] }).success)
  assert.ok(!BudgetVsActual.safeParse({ ...answer, month: '2026-13' }).success)
})

test('a negative amount is fine for savings and remaining', () => {
  const point = { month: '2026-04', income: '0.00', expense: '100.00', savings: '-100.00', savingsRate: null }
  assert.ok(YearPoint.safeParse(point).success)
  assert.ok(!YearPoint.safeParse({ ...point, savings: '-100' }).success)
})

test('the other statistics answers have the expected shape', () => {
  assert.ok(TopCategory.safeParse({ categoryId: '2', name: 'Rent', actual: '850.00', share: 85 }).success)
  assert.ok(Commitments.safeParse({ expense: '943.33', income: '0.00', items: [{ id: '1', name: 'Rent', type: 'EXPENSE', perMonth: '900.00' }] }).success)
  assert.ok(PlannedVsPaid.safeParse({ recurringId: '1', name: 'Power', paidCount: 1, planned: '80.00', paid: '70.00', difference: '-10.00' }).success)
  assert.ok(Commitments.safeParse({ expense: '0.00', income: '0.00', items: [{ id: '2', name: 'Power', type: 'EXPENSE', perMonth: null }] }).success, 'a bill with no fixed amount')
  assert.ok(PlannedVsPaid.safeParse({ recurringId: '2', name: 'Power', paidCount: 2, planned: null, paid: '159.50', difference: null }).success)
  assert.ok(!PlannedVsPaid.safeParse({ recurringId: '1', name: 'Power', paidCount: 0, planned: '80.00', paid: '70.00', difference: '-10.00' }).success)
})
