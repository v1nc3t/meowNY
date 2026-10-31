import assert from 'node:assert/strict'
import { test } from 'node:test'
import { parseTextSize } from './textSize.ts'

test('a saved text size is kept within the range, and anything else is the normal size', () => {
  assert.equal(parseTextSize('80'), 80)
  assert.equal(parseTextSize('125'), 125)
  assert.equal(parseTextSize('150'), 150)
  assert.equal(parseTextSize('10'), 80)
  assert.equal(parseTextSize('900'), 150)
  assert.equal(parseTextSize(null), 100)
  assert.equal(parseTextSize(''), 100)
  assert.equal(parseTextSize('large'), 100)
})
