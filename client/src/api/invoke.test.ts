import assert from 'node:assert/strict'
import { test } from 'node:test'
import { ApiError, MESSAGES, errorText, toErrorCode } from './invoke.ts'

test('known codes pass through and anything else becomes internal', () => {
  assert.equal(toErrorCode('name_exists'), 'name_exists')
  assert.equal(toErrorCode('toString'), 'internal')
  assert.equal(toErrorCode('UNIQUE constraint failed: categories.name'), 'internal')
  assert.equal(toErrorCode(undefined), 'internal')
  assert.equal(toErrorCode({ code: 'locked' }), 'internal')
})

test('error text is a fixed message, never the raw error', () => {
  assert.equal(errorText(new ApiError('not_found')), MESSAGES.not_found)
  assert.equal(errorText(new Error('SELECT * FROM transactions')), MESSAGES.internal)
})
