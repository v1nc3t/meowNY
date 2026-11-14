import assert from 'node:assert/strict'
import { test } from 'node:test'
import { entryTabOf, kindOf, offsetOf, txTabOf, txTabs, withTab, type TxTab } from './txTabs.ts'

test('the page has four tabs with more than one account, and no transfers tab with one', () => {
  assert.deepEqual(txTabs(true), ['all', 'expenses', 'income', 'transfers'])
  assert.deepEqual(txTabs(false), ['all', 'expenses', 'income'])
})

test('the address names the tab', () => {
  assert.equal(txTabOf('?tab=expenses', true), 'expenses')
  assert.equal(txTabOf('tab=income', false), 'income', 'with no question mark too')
  assert.equal(txTabOf(new URLSearchParams({ tab: 'transfers' }), true), 'transfers')
  assert.equal(txTabOf('?tab=income&x=1', true), 'income', 'other parameters stay out of the way')
})

test('no tab, an unknown word and an empty one are "all"', () => {
  for (const search of ['', '?', '?tab=', '?tab=all', '?tab=ALL', '?tab=Income', '?tab=income%20', '?tab=nonsense', '?tab=expense', '?tab=%00', '?other=income']) {
    assert.equal(txTabOf(search, true), 'all', search)
  }
})

test('transfers where the page has no transfers tab is "all"', () => {
  assert.equal(txTabOf('?tab=transfers', false), 'all')
  assert.equal(txTabOf('?tab=transfers', true), 'transfers')
})

test('with the tab said twice the first counts', () => {
  assert.equal(txTabOf('?tab=income&tab=expenses', true), 'income')
})

test('"all" is no parameter, and the other parameters stay', () => {
  assert.equal(withTab('', 'all').toString(), '')
  assert.equal(withTab('', 'income').toString(), 'tab=income')
  assert.equal(withTab('tab=income', 'all').toString(), '')
  assert.equal(withTab('tab=income', 'transfers').toString(), 'tab=transfers')
  assert.equal(withTab('a=1&tab=income&b=2', 'expenses').toString(), 'a=1&tab=expenses&b=2')
  assert.equal(withTab('a=1&tab=income', 'all').toString(), 'a=1')
  const params = new URLSearchParams('tab=income')
  withTab(params, 'all')
  assert.equal(params.toString(), 'tab=income', 'the address given is not changed')
})

test('the address of a tab names that tab again', () => {
  for (const tab of txTabs(true)) assert.equal(txTabOf(withTab('', tab), true), tab)
})

test('the list is asked for a kind on the expenses and income tabs, and for none on "all"', () => {
  assert.equal(kindOf('all'), undefined)
  assert.equal(kindOf('expenses'), 'EXPENSE')
  assert.equal(kindOf('income'), 'INCOME')
  assert.equal(kindOf('transfers'), undefined, 'the transfers are another list')
})

test('the add button opens on the tab that matches the list', () => {
  assert.equal(entryTabOf('all'), 'EXPENSE')
  assert.equal(entryTabOf('expenses'), 'EXPENSE')
  assert.equal(entryTabOf('income'), 'INCOME')
  assert.equal(entryTabOf('transfers'), 'TRANSFER')
})

test('a tab starts at its first page, and keeps the place it has while it is shown', () => {
  const at = { tab: 'income' as TxTab, offset: 100 }
  assert.equal(offsetOf(at, 'income'), 100)
  for (const tab of ['all', 'expenses', 'transfers'] as const) assert.equal(offsetOf(at, tab), 0, tab)
  assert.equal(offsetOf({ tab: 'all', offset: 0 }, 'all'), 0)
})

test('going to another tab and back with the browser starts at the first page again', () => {
  // the place is kept for one tab only: it is replaced when the person pages on the next one
  let at = { tab: 'all' as TxTab, offset: 50 }
  assert.equal(offsetOf(at, 'income'), 0, 'the income tab has its own first page')
  at = { tab: 'income', offset: 0 }
  assert.equal(offsetOf(at, 'all'), 0, 'and back on "all" the place of before is not remembered')
})
