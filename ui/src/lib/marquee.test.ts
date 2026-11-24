import assert from 'node:assert/strict'
import { test } from 'node:test'
import { boxPlace, HOLD_MS, HOLD_SLOP, isHold, marqueeSeconds } from './marquee.ts'

test('a short way still takes the least time', () => {
  assert.equal(marqueeSeconds(0), 1.5)
  assert.equal(marqueeSeconds(10), 1.5)
})

test('a longer way takes longer, about 40px a second of travel', () => {
  // 280px is 7s of travel, and the text rests for 30% of the leg: 7 / 0.7 = 10s.
  assert.ok(Math.abs(marqueeSeconds(280) - 10) < 1e-9)
  assert.ok(marqueeSeconds(400) > marqueeSeconds(200))
})

const view = { width: 800, height: 600 }
const box = { width: 200, height: 60 }

test('the box goes just above the text, its left edge on the text', () => {
  const at = boxPlace({ left: 100, top: 300, bottom: 320 }, box, view, 6, 8)
  assert.deepEqual(at, { left: 100, top: 300 - 6 - 60 })
})

test('with no room above (a title at the top) the box goes just below', () => {
  const at = boxPlace({ left: 100, top: 20, bottom: 50 }, box, view, 6, 8)
  assert.deepEqual(at, { left: 100, top: 56 })
})

test('the box is kept inside the window at the right and at the left', () => {
  assert.equal(boxPlace({ left: 750, top: 300, bottom: 320 }, box, view, 6, 8).left, 800 - 8 - 200)
  assert.equal(boxPlace({ left: -50, top: 300, bottom: 320 }, box, view, 6, 8).left, 8)
})

test('with room on neither side the larger one is taken and the box stays in the window', () => {
  const tall = { width: 200, height: 500 }
  // 240px above, 340px below, a box of 500: below, pulled up to end at the margin.
  const at = boxPlace({ left: 10, top: 240, bottom: 260 }, tall, view, 6, 8)
  assert.equal(at.top, 600 - 8 - 500)
  // a box bigger than the window starts at the margin
  assert.deepEqual(boxPlace({ left: 10, top: 240, bottom: 260 }, { width: 900, height: 700 }, view, 6, 8), { left: 8, top: 8 })
})

test('a press is a hold only when it lasted and the finger stayed put', () => {
  assert.equal(isHold(HOLD_MS, 0, 0), true)
  assert.equal(isHold(HOLD_MS - 1, 0, 0), false)
  assert.equal(isHold(HOLD_MS, HOLD_SLOP, 0), true)
  assert.equal(isHold(HOLD_MS, HOLD_SLOP + 1, 0), false)
  // the slop is a distance, not a box: 8 across and 8 down is 11.3 away
  assert.equal(isHold(HOLD_MS, 8, 8), false)
  assert.equal(isHold(HOLD_MS, -6, 6), true)
})
