import assert from 'node:assert/strict'
import { test } from 'node:test'
import { showShortcutHints, tip } from './tip.ts'

test('a hint names the shortcut, and a phone gets none', () => {
  showShortcutHints(true)
  assert.match(tip('new', 'add new')['data-tip'] ?? '', /^add new · n$/)
  assert.equal(tip('goCategories')['aria-keyshortcuts'], 'g c')
  showShortcutHints(false)
  assert.deepEqual(tip('new', 'add new'), {})
  showShortcutHints(true)
  assert.ok(tip('lock')['data-tip'])
})
