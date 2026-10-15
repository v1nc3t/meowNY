import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { Category } from '../api/categories.ts'
import { NAME_MAX, quickName, recentCategories } from './quickAdd.ts'

const cats: Category[] = [
  { id: '1', name: 'Groceries', type: 'EXPENSE' },
  { id: '2', name: 'Rent', type: 'EXPENSE' },
  { id: '3', name: 'Transport', type: 'EXPENSE' },
  { id: '4', name: 'Salary', type: 'INCOME' },
  { id: '5', name: 'Gifts', type: 'EXPENSE' },
  { id: '6', name: 'Health', type: 'EXPENSE' },
]
const ids = (list: Category[]) => list.map((c) => c.id)

test('the categories of the newest transactions come first, each once', () => {
  const newest = [{ categoryId: '3' }, { categoryId: '1' }, { categoryId: '3' }, { categoryId: '1' }, { categoryId: '2' }]
  assert.deepEqual(ids(recentCategories(newest, cats, 'EXPENSE', 6)), ['3', '1', '2', '5', '6'])
})

test('only categories of the chosen type are offered, and the rest follow in their own order', () => {
  assert.deepEqual(ids(recentCategories([{ categoryId: '4' }], cats, 'INCOME')), ['4'])
  assert.deepEqual(ids(recentCategories([], cats, 'EXPENSE', 3)), ['1', '2', '3'])
  assert.deepEqual(ids(recentCategories([{ categoryId: '4' }, { categoryId: '2' }], cats, 'EXPENSE', 2)), ['2', '1'])
})

test('a deleted category and the limit are respected', () => {
  assert.deepEqual(ids(recentCategories([{ categoryId: '99' }, { categoryId: '1' }], cats, 'EXPENSE', 1)), ['1'])
  assert.deepEqual(recentCategories([], [], 'EXPENSE'), [])
})

test('a transaction typed in a hurry is named after its category unless it has a name', () => {
  assert.equal(quickName('  coffee ', 'Dining out'), 'coffee')
  assert.equal(quickName('', 'Dining out'), 'Dining out')
  assert.equal(quickName('   ', ' Dining out '), 'Dining out')
  assert.equal([...quickName('x'.repeat(80), 'y')].length, NAME_MAX)
  assert.equal([...quickName('', '🐈'.repeat(60))].length, NAME_MAX, 'letters are counted, not bytes')
})
