import assert from 'node:assert/strict'
import { test } from 'node:test'
import { formatSize } from './bytes.ts'

test('a size that is not a size is empty, and a bad locale does not throw', () => {
  assert.equal(formatSize(-1, 'en'), '')
  assert.equal(formatSize(Number.NaN, 'en'), '')
  assert.equal(formatSize(1536, 'not a locale!!'), '1.5 KB')
})
