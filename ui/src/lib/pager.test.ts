import assert from 'node:assert/strict'
import { test } from 'node:test'
import { pageItems } from './pager.ts'

const G = 'gap'

test('a few pages are all shown', () => {
  assert.deepEqual(pageItems(1, 1, 2), [1])
  assert.deepEqual(pageItems(3, 9, 2), [1, 2, 3, 4, 5, 6, 7, 8, 9])
  assert.deepEqual(pageItems(2, 5, 0), [1, 2, 3, 4, 5])
})

test('many pages: the first, the last and the ones around this page', () => {
  assert.deepEqual(pageItems(6, 46, 2), [1, G, 4, 5, 6, 7, 8, G, 46])
  assert.deepEqual(pageItems(23, 46, 1), [1, G, 22, 23, 24, G, 46])
  assert.deepEqual(pageItems(23, 46, 0), [1, G, 23, G, 46])
})

test('near the ends the run grows, so the pager stays as wide as in the middle', () => {
  assert.deepEqual(pageItems(1, 46, 2), [1, 2, 3, 4, 5, 6, 7, G, 46])
  assert.deepEqual(pageItems(5, 46, 2), [1, 2, 3, 4, 5, 6, 7, G, 46])
  assert.deepEqual(pageItems(46, 46, 2), [1, G, 40, 41, 42, 43, 44, 45, 46])
  assert.deepEqual(pageItems(42, 46, 2), [1, G, 40, 41, 42, 43, 44, 45, 46])
  assert.deepEqual(pageItems(1, 46, 0), [1, 2, 3, G, 46])
  assert.deepEqual(pageItems(46, 46, 0), [1, G, 44, 45, 46])
})

test('every size, every page: as many slots as asked, in order, and a gap is never one page', () => {
  for (const around of [0, 1, 2]) {
    for (let count = 1; count <= 60; count++) {
      for (let current = 1; current <= count; current++) {
        const items = pageItems(current, count, around)
        assert.ok(items.length <= Math.max(2 * around + 5, 1), `${around} ${current}/${count}`)
        assert.ok(items.includes(current), `${around} ${current}/${count} shows the current page`)
        assert.equal(items[0], 1)
        assert.equal(items[items.length - 1], count)
        items.forEach((item, index) => {
          if (item !== G) return
          const before = items[index - 1] as number
          const after = items[index + 1] as number
          assert.ok(after - before >= 3, `${around} ${current}/${count} gap between ${before} and ${after}`)
        })
      }
    }
  }
})
