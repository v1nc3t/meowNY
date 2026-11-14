import assert from 'node:assert/strict'
import { test } from 'node:test'
import { Recurring } from '../api/recurring.ts'
import type { Category } from '../api/categories.ts'
import { groupByType, sharedItems } from './recurringView.ts'

const item = (id: string, nextDueDate: string, o: Partial<Recurring> = {}): Recurring => ({
  id,
  categoryId: '1',
  name: `item ${id}`,
  description: null,
  amount: '10.00',
  startDate: '2026-01-01',
  intervalUnit: 'MONTH',
  intervalCount: 1,
  nextDueDate,
  endDate: null,
  isActive: true,
  split: null,
  ...o,
})
const split = (...names: [string | null, string | null][]) => ({
  mine: 1,
  parts: names.map(([person, guest], at) => ({ personId: person ? String(at + 1) : null, person, guest, weight: 1 })),
})

test('the shared items are the running ones with a split, the next to fall due first', () => {
  const list = [
    item('1', '2026-10-30', { split: split(['Anna', null]) }),
    item('2', '2026-10-20'),
    item('3', '2026-10-25', { split: split([null, 'Dave']), isActive: false }),
    item('4', '2026-10-20', { split: split(['Bob', null]) }),
    item('5', '2026-10-20', { split: split(['Cleo', null]) }),
  ]
  assert.deepEqual(sharedItems(list).map((i) => i.id), ['4', '5', '1'], 'no plain item, no paused one; by day, then by id')
  assert.deepEqual(sharedItems([]), [])
})

const categories: Category[] = [
  { id: '1', name: 'Rent', type: 'EXPENSE' },
  { id: '9', name: 'Salary', type: 'INCOME' },
]

test('the items are two lists by the type of their category, each in the order it came in', () => {
  const list = [item('1', '2026-10-30', { categoryId: '9' }), item('2', '2026-10-20'), item('3', '2026-10-25', { categoryId: '9' }), item('4', '2026-10-20')]
  const { expenses, income } = groupByType(list, categories)
  assert.deepEqual(expenses.map((i) => i.id), ['2', '4'])
  assert.deepEqual(income.map((i) => i.id), ['1', '3'])
})

test('an item whose category is gone is an expense', () => {
  const { expenses, income } = groupByType([item('1', '2026-10-30', { categoryId: '77' }), item('2', '2026-10-30', { categoryId: '9' })], categories)
  assert.deepEqual(expenses.map((i) => i.id), ['1'])
  assert.deepEqual(income.map((i) => i.id), ['2'])
  assert.deepEqual(groupByType([item('1', '2026-10-30', { categoryId: '9' })], []).income, [], 'with no categories read yet every item is an expense')
})

test('a paused or ended item stays in the list of its category, and no item is in both lists', () => {
  const list = [
    item('1', '2026-10-30', { categoryId: '9', isActive: false }),
    item('2', '2026-10-20', { isActive: false, endDate: '2026-09-30' }),
    item('3', '2026-10-25'),
  ]
  const { expenses, income } = groupByType(list, categories)
  assert.deepEqual(income.map((i) => i.id), ['1'])
  assert.deepEqual(expenses.map((i) => i.id), ['2', '3'])
  assert.equal(expenses.length + income.length, list.length)
})

test('no items are two empty lists', () => {
  assert.deepEqual(groupByType([], categories), { expenses: [], income: [] })
  assert.deepEqual(groupByType([item('1', '2026-10-30')], categories).income, [], 'an empty income list')
})
