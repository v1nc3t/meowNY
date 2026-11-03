import assert from 'node:assert/strict'
import { test } from 'node:test'
import { pickBox, type Box } from './arrowNav.ts'

const box = (left: number, top: number, width = 40, height = 20): Box => ({ left, top, right: left + width, bottom: top + height })

test('nothing lies that way: -1', () => {
  assert.equal(pickBox(box(0, 0), [], 'down'), -1)
  assert.equal(pickBox(box(0, 100), [box(0, 0), box(100, 100)], 'down'), -1)
  assert.equal(pickBox(box(0, 0), [box(100, 0)], 'left'), -1)
})

test('down and up: the nearest in that way', () => {
  const rows = [box(0, 0), box(0, 40), box(0, 80)]
  assert.equal(pickBox(rows[1], rows, 'down'), 2)
  assert.equal(pickBox(rows[1], rows, 'up'), 0)
  assert.equal(pickBox(rows[0], rows, 'up'), -1)
})

test('down: a box straight below beats one that is a little nearer but to the side', () => {
  const from = box(0, 0)
  assert.equal(pickBox(from, [box(300, 30), box(10, 60)], 'down'), 1)
})

test('down: with nothing under it, a box off to the side is still reached', () => {
  assert.equal(pickBox(box(0, 0), [box(300, 40)], 'down'), 0)
})

test('down: of several straight below, the one with the closest middle', () => {
  const from = box(100, 0, 40)
  assert.equal(pickBox(from, [box(0, 40), box(90, 40), box(200, 40)], 'down'), 1)
})

test('right and left stay in the row', () => {
  const from = box(100, 100)
  const near = box(160, 100)
  const lower = box(130, 140)
  assert.equal(pickBox(from, [lower, near, box(300, 100)], 'right'), 1)
  assert.equal(pickBox(from, [lower, near, box(0, 100)], 'left'), 2)
  // Nothing more in the row: stay, rather than jump to another row.
  assert.equal(pickBox(from, [lower], 'right'), -1)
})

test('right: a taller box beside counts as the same row when the middles meet', () => {
  assert.equal(pickBox(box(0, 0, 40, 20), [box(60, -30, 40, 80)], 'right'), 0)
  assert.equal(pickBox(box(0, 0, 40, 80), [box(60, 60, 40, 20)], 'right'), 0)
})
