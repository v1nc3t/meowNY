import assert from 'node:assert/strict'
import { test } from 'node:test'
import { externalHref } from './externalHref.ts'

test('only https links count as external', () => {
  assert.equal(externalHref('https://example.com/a'), 'https://example.com/a')
  assert.equal(externalHref('http://example.com/'), null)
  assert.equal(externalHref('/app/dashboard'), null)
  assert.equal(externalHref('javascript:alert(1)'), null)
  assert.equal(externalHref(null), null)
})
