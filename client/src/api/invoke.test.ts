import assert from 'node:assert/strict'
import { test } from 'node:test'
import '../i18n/languages.ts'
import { setActiveLanguage } from '../i18n/translate.ts'
import { ApiError, errorText, messageFor, toErrorCode } from './invoke.ts'

test('known codes pass through and anything else becomes internal', () => {
  assert.equal(toErrorCode('name_exists'), 'name_exists')
  assert.equal(toErrorCode('toString'), 'internal')
  assert.equal(toErrorCode('UNIQUE constraint failed: categories.name'), 'internal')
  assert.equal(toErrorCode(undefined), 'internal')
  assert.equal(toErrorCode({ code: 'locked' }), 'internal')
})

test('error text is a fixed message, never the raw error', () => {
  setActiveLanguage('en')
  assert.equal(errorText(new ApiError('not_found')), messageFor('not_found'))
  assert.equal(errorText(new Error('SELECT * FROM transactions')), messageFor('internal'))
})

test('error messages follow the language', () => {
  setActiveLanguage('de')
  assert.equal(messageFor('locked'), 'die app ist gesperrt.')
  assert.equal(errorText(new ApiError('locked')), 'die app ist gesperrt.')
  setActiveLanguage('en')
})
