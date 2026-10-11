import assert from 'node:assert/strict'
import test from 'node:test'
import { visibleGroups, visibleUngrouped } from './categoriesView.ts'

const housing = { id: '1', name: 'Housing' }
const food = { id: '2', name: 'Food' }
const rent = { id: '10', name: 'Rent', type: 'EXPENSE', categoryGroupId: '1' }
const pay = { id: '11', name: 'Pay', type: 'INCOME', categoryGroupId: '1' }
const lunch = { id: '12', name: 'Lunch', type: 'EXPENSE', categoryGroupId: null }
const lost = { id: '13', name: 'Lost', type: 'EXPENSE', categoryGroupId: '99' }

test('groups keep their categories and a name match keeps the whole group', () => {
  const groups = visibleGroups([housing, food], [rent, pay, lunch], 'ALL', 'hous')
  assert.deepEqual(
    groups.map((block) => [block.group.name, block.categories.map((category) => category.name)]),
    [['Housing', ['Rent', 'Pay']]],
  )
  assert.deepEqual(
    visibleUngrouped([housing], [rent, lunch, lost], 'ALL', '').map((category) => category.name),
    ['Lunch', 'Lost'],
  )
})

test('search and type narrow categories inside a group', () => {
  const byCategory = visibleGroups([housing], [rent, pay], 'ALL', 'pay')
  assert.deepEqual(byCategory[0]?.categories.map((category) => category.name), ['Pay'])
  const expenses = visibleGroups([housing, food], [rent, pay], 'EXPENSE', '')
  assert.deepEqual(
    expenses.map((block) => block.group.name),
    ['Housing'],
  )
  assert.deepEqual(visibleUngrouped([housing], [rent, pay, lunch], 'INCOME', 'lunch'), [])
})
