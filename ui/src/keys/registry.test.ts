import assert from 'node:assert/strict'
import { test } from 'node:test'
import { registerShortcut, runShortcut } from './registry.ts'

test('a registered handler runs, and an unknown one reports that nothing answered', () => {
  let count = 0
  const off = registerShortcut('new', () => count++)
  assert.equal(runShortcut('new'), true)
  assert.equal(count, 1)
  off()
  assert.equal(runShortcut('new'), false)
})

test('a newer page takes a shortcut over, and the older one cannot remove it by leaving later', () => {
  let a = 0
  let b = 0
  const offA = registerShortcut('search', () => a++)
  const offB = registerShortcut('search', () => b++)
  runShortcut('search')
  assert.deepEqual([a, b], [0, 1])
  offA()
  assert.equal(runShortcut('search'), true, 'b is still registered')
  offB()
  assert.equal(runShortcut('search'), false)
})
