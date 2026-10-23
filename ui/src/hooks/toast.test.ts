import assert from 'node:assert/strict'
import { test } from 'node:test'
import { MAX_TOASTS, toastSeconds, withToast, type ToastItem } from './toast'

test('info and success stay as long as a notice, warning and error longer', () => {
  assert.equal(toastSeconds('info'), 6)
  assert.equal(toastSeconds('success'), 6)
  assert.equal(toastSeconds('warning'), 8)
  assert.equal(toastSeconds('error'), 8)
})

test('a new text is added with the lifetime of its kind', () => {
  const list = withToast([], 'saved', 'success', 1)
  assert.deepEqual(list, [{ id: 1, text: 'saved', kind: 'success', seconds: 6, round: 0 }])
  assert.equal(withToast(list, 'broke', 'error', 2)[1]?.seconds, 8)
})

test('the same text again restarts that toast instead of stacking a copy', () => {
  const first = withToast([], 'saved', 'info', 1)
  const again = withToast(first, 'saved', 'error', 2)
  assert.equal(again.length, 1)
  assert.equal(again[0]?.id, 1)
  assert.equal(again[0]?.round, 1)
  assert.equal(again[0]?.kind, 'error')
  assert.equal(again[0]?.seconds, 8)
})

test('at most three stay: the oldest leave first', () => {
  let list: ToastItem[] = []
  for (let id = 1; id <= 5; id++) list = withToast(list, `text ${id}`, 'info', id)
  assert.equal(list.length, MAX_TOASTS)
  assert.deepEqual(
    list.map((item) => item.text),
    ['text 3', 'text 4', 'text 5'],
  )
})
