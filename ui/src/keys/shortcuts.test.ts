import assert from 'node:assert/strict'
import { test } from 'node:test'
import { CHORD_MS, isMac, isTypingTarget, KeyMatcher, SHORTCUTS, shortcutKeys, type ShortcutId } from './shortcuts.ts'

const press = (key: string, mods: Partial<{ ctrlKey: boolean; metaKey: boolean; altKey: boolean }> = {}) => ({
  key,
  ctrlKey: false,
  metaKey: false,
  altKey: false,
  ...mods,
})

test('single keys map to their actions', () => {
  const m = new KeyMatcher()
  const expected: [string, ShortcutId][] = [['n', 'new'], ['/', 'search'], ['[', 'prev'], [']', 'next'], ['t', 'today'], ['j', 'rowNext'], ['k', 'rowPrev'], ['?', 'help']]
  for (const [key, id] of expected) assert.equal(m.handle(press(key), 0), id, key)
})

test('capital letters work the same (caps lock)', () => {
  assert.equal(new KeyMatcher().handle(press('N'), 0), 'new')
})

test('g then a letter goes to a page', () => {
  const m = new KeyMatcher()
  const pages: [string, ShortcutId][] = [['d', 'goDashboard'], ['t', 'goTransactions'], ['c', 'goCategories'], ['r', 'goRecurring'], ['o', 'goDocuments'], ['f', 'goDebts'], ['s', 'goSettings'], ['e', 'goExport']]
  for (const [key, id] of pages) {
    assert.equal(m.handle(press('g'), 0), null)
    assert.equal(m.handle(press(key), 100), id, key)
  }
})

test('the second key of a chord is not also a single-key shortcut', () => {
  const m = new KeyMatcher()
  m.handle(press('g'), 0)
  assert.equal(m.handle(press('t'), 50), 'goTransactions', 'not "today"')
  assert.equal(m.handle(press('t'), 60), 'today', 'on its own it is "today"')
})

test('a chord times out and a wrong second key cancels it', () => {
  const m = new KeyMatcher()
  m.handle(press('g'), 0)
  assert.equal(m.handle(press('d'), CHORD_MS + 1), null, 'too slow')
  m.handle(press('g'), 5000)
  assert.equal(m.handle(press('x'), 5100), null, 'no such page')
  assert.equal(m.handle(press('d'), 5200), null, 'the chord is gone')
  m.handle(press('g'), 6000)
  assert.equal(m.handle(press('Escape'), 6100), null)
  assert.equal(m.handle(press('d'), 6200), null, 'escape cancels')
})

test('Ctrl or Cmd shortcuts work and plain letters with modifiers do not', () => {
  const m = new KeyMatcher()
  assert.equal(m.handle(press('l', { ctrlKey: true }), 0), 'lock')
  assert.equal(m.handle(press('b', { metaKey: true }), 0), 'sidebar')
  assert.equal(m.handle(press('n', { ctrlKey: true }), 0), null, 'ctrl+n is the system own')
  assert.equal(m.handle(press('n', { altKey: true }), 0), null)
  assert.equal(m.handle(press('l', { ctrlKey: true, altKey: true }), 0), null)
})

test('keys are shown the way people expect, per platform', () => {
  assert.equal(shortcutKeys('goTransactions', false), 'g t')
  assert.equal(shortcutKeys('new', false), 'n')
  assert.equal(shortcutKeys('lock', false), 'Ctrl+L')
  assert.equal(shortcutKeys('lock', true), '⌘L')
  assert.ok(isMac('MacIntel') && isMac('iPhone') && !isMac('Linux x86_64') && !isMac('Win32'))
})

test('no two shortcuts use the same keys', () => {
  const seen = new Set<string>()
  for (const [id, def] of Object.entries(SHORTCUTS)) {
    const signature = `${def.mod ? 'mod+' : ''}${def.keys.join(' ')}`
    assert.ok(!seen.has(signature), `${id} repeats ${signature}`)
    seen.add(signature)
  }
})

test('shortcuts are ignored while typing, in dropdown lists and in dialogs', () => {
  assert.ok(isTypingTarget({ tagName: 'INPUT' }))
  assert.ok(isTypingTarget({ tagName: 'TEXTAREA' }))
  assert.ok(isTypingTarget({ tagName: 'SELECT' }))
  assert.ok(isTypingTarget({ tagName: 'DIV', isContentEditable: true }))
  assert.ok(isTypingTarget({ tagName: 'BUTTON', closest: (s) => (s.includes('.pick-list') ? {} : null) }))
  assert.ok(!isTypingTarget({ tagName: 'BUTTON', closest: () => null }))
  assert.ok(!isTypingTarget(null))
})
