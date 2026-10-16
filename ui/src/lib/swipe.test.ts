import assert from 'node:assert/strict'
import { test } from 'node:test'
import { EDGE, neighbour, step, swipeDirection, SWIPE_MIN } from './swipe.ts'

test('a long sideways move is a swipe, and its direction says where to go', () => {
  assert.equal(swipeDirection(-120, 10, 200, 400), 'next')
  assert.equal(swipeDirection(120, -10, 200, 400), 'prev')
  assert.equal(swipeDirection(-SWIPE_MIN, 0, 200, 400), 'next')
})

test('a short move, a scroll and a tap are not swipes', () => {
  assert.equal(swipeDirection(-SWIPE_MIN + 1, 0, 200, 400), null)
  assert.equal(swipeDirection(-100, 90, 200, 400), null, 'more down than sideways')
  assert.equal(swipeDirection(-100, 70, 200, 400), null, 'less than one and a half times as far sideways')
  assert.equal(swipeDirection(0, 0, 200, 400), null)
})

test('a move that starts at the edge of the screen is left to the system', () => {
  assert.equal(swipeDirection(120, 0, EDGE - 1, 400), null)
  assert.equal(swipeDirection(-120, 0, 400 - EDGE + 1, 400), null)
  assert.equal(swipeDirection(120, 0, EDGE, 400), 'prev')
})

test('the neighbour of a page is the next tab, and there is none past the ends or off the tabs', () => {
  const tabs = ['/app/dashboard', '/app/transactions', '/app/budgets']
  assert.equal(neighbour(tabs, '/app/transactions', 'next'), '/app/budgets')
  assert.equal(neighbour(tabs, '/app/transactions', 'prev'), '/app/dashboard')
  assert.equal(neighbour(tabs, '/app/budgets', 'next'), null)
  assert.equal(neighbour(tabs, '/app/dashboard', 'prev'), null)
  assert.equal(neighbour(tabs, '/app/settings', 'next'), null)
  assert.equal(neighbour(tabs, '/app/budgets/2026', 'prev'), '/app/transactions')
  assert.equal(neighbour(tabs, '/app/budgetsx', 'prev'), null, 'only a whole path segment matches')
})

test('stepping through a list stops at its ends', () => {
  const tabs = ['EXPENSE', 'INCOME', 'HISTORY'] as const
  assert.equal(step(tabs, 'EXPENSE', 'next'), 'INCOME')
  assert.equal(step(tabs, 'HISTORY', 'prev'), 'INCOME')
  assert.equal(step(tabs, 'HISTORY', 'next'), null)
  assert.equal(step(tabs, 'EXPENSE', 'prev'), null)
})
