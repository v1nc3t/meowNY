import assert from 'node:assert/strict'
import { test } from 'node:test'
import { parseTheme } from './theme.ts'

test('a saved light or dark theme is kept, and anything else follows the system', () => {
  assert.equal(parseTheme('light'), 'light')
  assert.equal(parseTheme('dark'), 'dark')
  assert.equal(parseTheme('system'), 'system')
  assert.equal(parseTheme(null), 'system')
  assert.equal(parseTheme(''), 'system')
  assert.equal(parseTheme('Dark'), 'system')
  assert.equal(parseTheme('sepia'), 'system')
})
