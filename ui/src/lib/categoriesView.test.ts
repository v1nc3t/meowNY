import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { Overview } from '../api/budgets.ts'
import { amountsFor, categoryTabs, earnedClass, shownTab, spentClass, spentFor, tabOfSearch, tabQuery } from './categoriesView.ts'

test('on a phone the categories page has the tabs expenses and income, and the history once there is some', () => {
  assert.deepEqual(categoryTabs(false), ['EXPENSE', 'INCOME'])
  assert.deepEqual(categoryTabs(true), ['EXPENSE', 'INCOME', 'HISTORY'])
})

test('a tab that is gone falls back to the expenses, and the income is a tab again', () => {
  assert.equal(shownTab(categoryTabs(true), 'INCOME'), 'INCOME', 'an old remembered income tab')
  assert.equal(shownTab(categoryTabs(false), 'INCOME'), 'INCOME')
  assert.equal(shownTab(categoryTabs(true), 'HISTORY'), 'HISTORY')
  assert.equal(shownTab(categoryTabs(true), 'EXPENSE'), 'EXPENSE')
  assert.equal(shownTab(categoryTabs(false), 'HISTORY'), 'EXPENSE', 'the history went with the months')
  assert.equal(shownTab(categoryTabs(true), ''), 'EXPENSE')
  assert.equal(shownTab(categoryTabs(true), 'income'), 'EXPENSE', 'a word in other letters')
  assert.equal(shownTab([], 'EXPENSE'), 'EXPENSE')
})

test('the tab is in the address: ?tab=income and ?tab=history, and every other address is the expenses', () => {
  assert.equal(tabOfSearch('?tab=income'), 'INCOME')
  assert.equal(tabOfSearch('tab=income'), 'INCOME', 'with no question mark too')
  assert.equal(tabOfSearch(new URLSearchParams({ tab: 'income' })), 'INCOME')
  assert.equal(tabOfSearch('?x=1&tab=history'), 'HISTORY', 'other parameters stay out of the way')
  assert.equal(tabOfSearch('?tab=income&tab=history'), 'INCOME', 'the first one counts')
  for (const bad of ['', '?', '?tab=', '?tab=expenses', '?tab=INCOME', '?tab=Income ', '?tab=income2', '?tab=nope', '?other=income', '?tab=%00']) {
    assert.equal(tabOfSearch(bad), 'EXPENSE', bad)
  }
})

test('the query of a tab reads back as it, and the expenses have none', () => {
  assert.deepEqual(tabQuery('EXPENSE'), {})
  assert.deepEqual(tabQuery('INCOME'), { tab: 'income' })
  assert.deepEqual(tabQuery('HISTORY'), { tab: 'history' })
  for (const tab of ['EXPENSE', 'INCOME', 'HISTORY'] as const) assert.equal(tabOfSearch(new URLSearchParams(tabQuery(tab))), tab)
})

test('an address that names a tab the page does not have shows the expenses', () => {
  const tabs = categoryTabs(false)
  assert.equal(shownTab(tabs, tabOfSearch('?tab=history')), 'EXPENSE', 'no history yet')
  assert.equal(shownTab(tabs, tabOfSearch('?tab=income')), 'INCOME')
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
