import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { Overview } from '../api/budgets.ts'
import { amountsFor, categoryTabs, earnedClass, shownTab, spentClass, spentFor } from './categoriesView.ts'

test('the categories page has the expenses, and the history once there is some; the income is on the accounts page', () => {
  assert.deepEqual(categoryTabs(false), ['EXPENSE'])
  assert.deepEqual(categoryTabs(true), ['EXPENSE', 'HISTORY'])
  for (const tabs of [categoryTabs(false), categoryTabs(true)]) assert.equal((tabs as readonly string[]).includes('INCOME'), false)
})

test('a tab that is gone falls back to the expenses', () => {
  assert.equal(shownTab(categoryTabs(true), 'INCOME'), 'EXPENSE', 'an old income tab')
  assert.equal(shownTab(categoryTabs(true), 'HISTORY'), 'HISTORY')
  assert.equal(shownTab(categoryTabs(true), 'EXPENSE'), 'EXPENSE')
  assert.equal(shownTab(categoryTabs(false), 'HISTORY'), 'EXPENSE', 'the history went with the months')
  assert.equal(shownTab(categoryTabs(true), ''), 'EXPENSE')
  assert.equal(shownTab(categoryTabs(true), 'income'), 'EXPENSE', 'a word in other letters')
  assert.equal(shownTab([], 'EXPENSE'), 'EXPENSE')
})

const overview: Overview[] = [
  {
    type: 'EXPENSE',
    budgeted: '500.00',
    used: '120.00',
    categories: [{ categoryId: '2', amountLimit: '300.00' }],
    spent: [{ categoryId: '2', amount: '80.00' }, { categoryId: '3', amount: '40.00' }],
  },
  { type: 'INCOME', budgeted: null, used: '3200.00', categories: [], spent: [{ categoryId: '9', amount: '3200.00' }] },
] as Overview[]

test('what a side has budgeted and added up, by category, for the side asked', () => {
  assert.deepEqual([...amountsFor(overview, 'EXPENSE')], [['2', '300.00']])
  assert.deepEqual([...amountsFor(overview, 'INCOME')], [])
  assert.deepEqual([...spentFor(overview, 'EXPENSE')], [['2', '80.00'], ['3', '40.00']])
  assert.deepEqual([...spentFor(overview, 'INCOME')], [['9', '3200.00']])
  assert.equal(amountsFor(null, 'EXPENSE').size, 0, 'not read yet')
  assert.equal(spentFor([], 'INCOME').size, 0)
})

test('spending is red above its budget and green within it, earning green above it', () => {
  assert.equal(spentClass('10.00', null), 'cell-amount')
  assert.equal(spentClass('10.00', '10.00'), 'cell-amount is-income', 'at the budget is within it')
  assert.equal(spentClass('10.01', '10.00'), 'cell-amount is-over')
  assert.equal(earnedClass('10.00', null), 'cell-amount')
  assert.equal(earnedClass('10.00', '10.00'), 'cell-amount')
  assert.equal(earnedClass('10.01', '10.00'), 'cell-amount is-income')
})
