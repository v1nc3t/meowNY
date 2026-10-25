import assert from 'node:assert/strict'
import { test } from 'node:test'
import { activeIds, applyOrder, moveItem, scrollStep, SCROLL_EDGE, SCROLL_MAX, shifts, targetIndex } from './reorder.ts'

test('moving an item takes it out and puts it back, and leaves the input alone', () => {
  const items = ['a', 'b', 'c', 'd']
  assert.deepEqual(moveItem(items, 0, 2), ['b', 'c', 'a', 'd'])
  assert.deepEqual(moveItem(items, 3, 0), ['d', 'a', 'b', 'c'])
  assert.deepEqual(moveItem(items, 1, 1), items)
  assert.deepEqual(items, ['a', 'b', 'c', 'd'])
})

test('a dragged row passes a neighbour once its middle is past the neighbour middle', () => {
  const mids = [20, 60, 100, 140]
  assert.equal(targetIndex(mids, 0, 20), 0)
  assert.equal(targetIndex(mids, 0, 59), 0, 'not yet')
  assert.equal(targetIndex(mids, 0, 61), 1)
  assert.equal(targetIndex(mids, 0, 500), 3, 'past the end')
  assert.equal(targetIndex(mids, 3, -50), 0, 'past the start')
  assert.equal(targetIndex(mids, 2, 100), 2, 'back where it started')
  assert.equal(targetIndex(mids, 2, 59), 1)
})

test('only the rows that can be moved are named, and read-only rows keep their place', () => {
  const rows = [
    { id: '1', deleted: false },
    { id: '2', deleted: true },
    { id: '3', deleted: false },
    { id: '4', deleted: false },
  ]
  assert.deepEqual(activeIds(rows), ['1', '3', '4'])
  const moved = applyOrder(rows, ['4', '1', '3'])
  assert.deepEqual(moved.map((row) => row.id), ['4', '2', '1', '3'])
  assert.equal(moved[1], rows[1])
  assert.deepEqual(applyOrder(rows, activeIds(rows)), rows)
  assert.deepEqual(applyOrder(rows, ['3', '1']).map((row) => row.id), ['3', '2', '1', '4'], 'rows of the other type are not named')
})

test('the rows move by the height of the rows they pass', () => {
  assert.deepEqual(shifts([40, 40, 40], [0, 1, 2]), [0, 0, 0])
  assert.deepEqual(shifts([40, 40, 40], [1, 2, 0]), [80, -40, -40])
  assert.deepEqual(shifts([30, 50, 40], [1, 0, 2]), [50, -30, 0])
})

test('the list scrolls near its edges only, faster the closer the pointer is', () => {
  assert.equal(scrollStep(300, 100, 500), 0)
  assert.equal(scrollStep(100, 100, 500), -SCROLL_MAX)
  assert.equal(scrollStep(50, 100, 500), -SCROLL_MAX, 'outside the list is as fast as the edge')
  assert.equal(scrollStep(500, 100, 500), SCROLL_MAX)
  assert.ok(scrollStep(100 + SCROLL_EDGE / 2, 100, 500) < 0)
  assert.ok(Math.abs(scrollStep(100 + SCROLL_EDGE / 2, 100, 500)) < SCROLL_MAX)
})
