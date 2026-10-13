import assert from 'node:assert/strict'
import test from 'node:test'
import { visibleCategories } from './categoriesView.ts'

const rent = { id: '10', name: 'Rent', type: 'EXPENSE' }
const pay = { id: '11', name: 'Pay', type: 'INCOME' }
const lunch = { id: '12', name: 'Lunch', type: 'EXPENSE' }

test('all shows every category in order', () => {
  assert.deepEqual(
    visibleCategories([rent, pay, lunch], 'ALL', '').map((category) => category.name),
    ['Rent', 'Pay', 'Lunch'],
  )
})

test('type and search narrow the list, ignoring case and outer spaces', () => {
  assert.deepEqual(
    visibleCategories([rent, pay, lunch], 'EXPENSE', '').map((category) => category.name),
    ['Rent', 'Lunch'],
  )
  assert.deepEqual(
    visibleCategories([rent, pay, lunch], 'ALL', '  LUN ').map((category) => category.name),
    ['Lunch'],
  )
  assert.deepEqual(visibleCategories([rent, pay, lunch], 'INCOME', 'lunch'), [])
})
