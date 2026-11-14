import assert from 'node:assert/strict'
import { test } from 'node:test'
import { CHORD_MS, isTypingTarget, KeyMatcher, SHORTCUTS, shortcutKeys } from './shortcuts.ts'

const press = (key: string, mods: Partial<{ ctrlKey: boolean; metaKey: boolean; altKey: boolean }> = {}) => ({
  key,
  ctrlKey: false,
  metaKey: false,
  altKey: false,
  ...mods,
})

test('the second key of a chord is not also a single-key shortcut', () => {
  const m = new KeyMatcher()
  m.handle(press('g'), 0)
  assert.equal(m.handle(press('t'), 50), 'goTransactions', 'not "today"')
  assert.equal(m.handle(press('t'), 60), 'today', 'on its own it is "today"')
})

test('g then a goes to the accounts, and a on its own does nothing', () => {
  const m = new KeyMatcher()
  m.handle(press('g'), 0)
  assert.equal(m.handle(press('a'), 50), 'goAccounts')
  assert.equal(m.handle(press('a'), 60), null, 'the chord is used up')
  m.handle(press('g'), 100)
  assert.equal(m.handle(press('A'), 150), 'goAccounts', 'whatever the caps lock says')
  assert.deepEqual(SHORTCUTS.goAccounts, { keys: ['g', 'a'], group: 'go', label: 'keys.goTo', page: 'nav.accounts' })
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

test('no two shortcuts use the same keys', () => {
  const seen = new Set<string>()
  for (const [id, def] of Object.entries(SHORTCUTS)) {
    const signature = `${def.mod ? 'mod+' : ''}${def.alt ? 'alt+' : ''}${def.keys.join(' ')}`
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

test('the tabs of the add dialog are in the help list as Alt and 1, 2, 3, and the matcher leaves them to the dialog', () => {
  const tabs = ['entryExpense', 'entryIncome', 'entryTransfer'] as const
  assert.deepEqual(tabs.map((id) => shortcutKeys(id, false)), ['Alt+1', 'Alt+2', 'Alt+3'])
  assert.deepEqual(tabs.map((id) => shortcutKeys(id, true)), ['\u23251', '\u23252', '\u23253'])
  for (const id of tabs) assert.equal(SHORTCUTS[id].group, 'actions')
  const m = new KeyMatcher()
  for (const key of ['1', '2', '3']) {
    assert.equal(m.handle(press(key, { altKey: true }), 0), null, `Alt+${key} is the dialog's own`)
    assert.equal(m.handle(press(key), 0), null, `${key} alone does nothing`)
  }
})

test('a shortcut with Alt in the list does not take the plain key of another', () => {
  assert.equal(shortcutKeys('new', false), 'n')
  assert.equal(shortcutKeys('lock', false), 'Ctrl+L')
  assert.equal(shortcutKeys('goTransactions', false), 'g t')
})
