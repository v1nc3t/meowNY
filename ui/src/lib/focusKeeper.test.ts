import assert from 'node:assert/strict'
import { test } from 'node:test'
import { findMark, type Mark } from './focusKeeper.ts'

const button = (name: string, item: string | null = null): Mark => ({ item, name, tag: 'BUTTON' })

test('the same control of the same row is found even when it moved', () => {
  const marks = [button('edit', 'transaction:a'), button('edit', 'transaction:c'), button('edit', 'transaction:b')]
  assert.equal(findMark(marks, button('edit', 'transaction:b'), 1), 2)
})

test('a deleted row gives the focus to the control at its place, the next row', () => {
  const rows = [button('delete', 'transaction:a'), button('delete', 'transaction:c'), button('delete', 'transaction:d')]
  assert.equal(findMark(rows, button('delete', 'transaction:b'), 1), 1)
})

test('rows without an item are found by their place among the controls that read alike', () => {
  const marks = [button('add category'), button('edit'), button('edit'), button('edit')]
  assert.equal(findMark(marks, button('edit'), 2), 3)
})

test('the last row deleted gives the focus to the new last row', () => {
  const marks = [button('delete'), button('delete')]
  assert.equal(findMark(marks, button('delete'), 2), 1)
})

test('the name has to be the same, and the kind of element', () => {
  const marks = [button('cancel'), { item: null, name: 'edit', tag: 'A' }]
  assert.equal(findMark(marks, button('edit'), 0), -1)
})

test('a control whose name changed is found in its row', () => {
  const marks = [button('resume', 'recurring:a'), button('edit', 'recurring:a')]
  assert.equal(findMark(marks, button('mark as paid', 'recurring:a'), 0), 0)
})

test('nothing of that row or name gives -1', () => {
  assert.equal(findMark([button('edit', 'transaction:a')], button('delete', 'transaction:b'), 0), -1)
  assert.equal(findMark([], button('edit'), 0), -1)
})
