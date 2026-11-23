import assert from 'node:assert/strict'
import { test } from 'node:test'
import { menuTarget } from './menuNav.ts'

test('the arrows go round at the ends', () => {
  assert.equal(menuTarget('ArrowDown', 2, 3), 0)
  assert.equal(menuTarget('ArrowUp', 0, 3), 2)
  assert.equal(menuTarget('ArrowDown', 0, 3), 1)
  assert.equal(menuTarget('ArrowUp', 2, 3), 1)
})

test('with no item focused, down goes to the first and up to the last', () => {
  assert.equal(menuTarget('ArrowDown', -1, 3), 0)
  assert.equal(menuTarget('ArrowUp', -1, 3), 2)
})

test('Home and End go to the ends; other keys and an empty menu go nowhere', () => {
  assert.equal(menuTarget('Home', 2, 4), 0)
  assert.equal(menuTarget('End', 0, 4), 3)
  assert.equal(menuTarget('a', 1, 4), null)
  assert.equal(menuTarget('ArrowDown', -1, 0), null)
  assert.equal(menuTarget('ArrowDown', 0, 1), 0, 'one item: stays')
})
