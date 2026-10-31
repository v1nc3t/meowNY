import assert from 'node:assert/strict'
import { test } from 'node:test'
import { parseTextSize } from './textSize.ts'

test('a saved text size is kept, and anything else is the default size', () => {
  assert.equal(parseTextSize('small'), 'small')
  assert.equal(parseTextSize('large'), 'large')
  assert.equal(parseTextSize('larger'), 'larger')
  assert.equal(parseTextSize(null), 'default')
  assert.equal(parseTextSize(''), 'default')
  assert.equal(parseTextSize('Large'), 'default')
  assert.equal(parseTextSize('toString'), 'default')
})
