import assert from 'node:assert/strict'
import { test } from 'node:test'
import { closesMenu, EDGE, opensMenu, swipeDirection, SWIPE_MIN } from './swipe.ts'

test('a short move, a scroll and a tap are not swipes', () => {
  assert.equal(swipeDirection(-SWIPE_MIN + 1, 0, 200, 400), null)
  assert.equal(swipeDirection(-100, 90, 200, 400), null, 'more down than sideways')
  assert.equal(swipeDirection(-100, 70, 200, 400), null, 'less than one and a half times as far sideways')
  assert.equal(swipeDirection(0, 0, 200, 400), null)
})

test('a move that starts at the edge of the screen is not one of the pages swipes', () => {
  assert.equal(swipeDirection(120, 0, EDGE - 1, 400), null)
  assert.equal(swipeDirection(-120, 0, 400 - EDGE + 1, 400), null)
  assert.equal(swipeDirection(120, 0, EDGE, 400), 'prev')
})

test('a swipe to the right from the left edge opens the menu, one from further in does not', () => {
  assert.equal(opensMenu(120, 0, 10), true)
  assert.equal(opensMenu(120, 0, EDGE - 1), true)
  assert.equal(opensMenu(120, 0, 40), false)
  assert.equal(opensMenu(120, 0, EDGE), false, 'the first 24 pixels are 0 to 23')
})

test('from the edge, a short move, a vertical drag and a swipe to the left do not open the menu', () => {
  assert.equal(opensMenu(SWIPE_MIN - 1, 0, 10), false)
  assert.equal(opensMenu(100, 100, 10), false, 'more down than sideways')
  assert.equal(opensMenu(-120, 0, 10), false)
})

test('only a swipe to the left closes the menu', () => {
  assert.equal(closesMenu(-120, 10), true)
  assert.equal(closesMenu(120, 0), false)
  assert.equal(closesMenu(-SWIPE_MIN + 1, 0), false)
  assert.equal(closesMenu(-100, 90), false, 'a scroll')
})
