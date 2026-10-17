import assert from 'node:assert/strict'
import test from 'node:test'
import { categoryOptions, visibleCategories } from './categoriesView.ts'

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

test('the options of a dropdown are in two sections, expense first, each keeping its order', () => {
  const options = categoryOptions([pay, rent, lunch], { expense: 'expense', income: 'income' })
  assert.deepEqual(
    options.map((option) => [option.group, option.label]),
    [['expense', 'Rent'], ['expense', 'Lunch'], ['income', 'Pay']],
  )
  assert.deepEqual(categoryOptions([], { expense: 'expense', income: 'income' }), [])
  assert.deepEqual(categoryOptions([pay], { expense: 'expense', income: 'income' }).map((option) => option.group), ['income'], 'a section with nothing in it has no heading')
})
