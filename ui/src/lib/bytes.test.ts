import assert from 'node:assert/strict'
import { test } from 'node:test'
import { formatSize } from './bytes.ts'

test('sizes are written in bytes, KB and MB with at most one decimal', () => {
  assert.equal(formatSize(0, 'en'), '0 B')
  assert.equal(formatSize(1023, 'en'), '1,023 B')
  assert.equal(formatSize(1024, 'en'), '1 KB')
  assert.equal(formatSize(1536, 'en'), '1.5 KB')
  assert.equal(formatSize(1024 * 1024 - 1, 'en'), '1,024 KB')
  assert.equal(formatSize(1024 * 1024, 'en'), '1 MB')
  assert.equal(formatSize(25 * 1024 * 1024, 'en'), '25 MB')
  assert.equal(formatSize(4_400_000, 'en'), '4.2 MB')
})

test('the decimal mark follows the locale', () => {
  assert.equal(formatSize(1536, 'de-DE'), '1,5 KB')
  assert.equal(formatSize(1536, 'ro'), '1,5 KB')
})

test('a size that is not a size is empty, and a bad locale does not throw', () => {
  assert.equal(formatSize(-1, 'en'), '')
  assert.equal(formatSize(Number.NaN, 'en'), '')
  assert.equal(formatSize(1536, 'not a locale!!'), '1.5 KB')
})
