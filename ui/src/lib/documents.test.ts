import assert from 'node:assert/strict'
import { test } from 'node:test'
import { canOpen, canShowImage, fileState, fileStateText, KIND_CHOICES, MAX_DRAW_BYTES, reportLines } from './documents.ts'

test('a missing file is a fault on the computer and normal on the phone', () => {
  assert.equal(fileState({ hasFile: true, keepOnPhone: false }, false), 'here')
  assert.equal(fileState({ hasFile: true, keepOnPhone: true }, true), 'here')
  assert.equal(fileState({ hasFile: false, keepOnPhone: false }, false), 'missing')
  assert.equal(fileState({ hasFile: false, keepOnPhone: true }, false), 'missing')
  assert.equal(fileState({ hasFile: false, keepOnPhone: false }, true), 'onComputer')
  assert.equal(fileState({ hasFile: false, keepOnPhone: true }, true), 'waiting')
})

test('there are words for every state but the good one', () => {
  assert.equal(fileStateText('here'), null)
  assert.equal(fileStateText('missing'), 'docs.fileMissing')
  assert.equal(fileStateText('onComputer'), 'docs.fileOnComputer')
  assert.equal(fileStateText('waiting'), 'docs.fileWaiting')
})

test('only a file that is here and of a known kind can be opened', () => {
  assert.equal(canOpen({ hasFile: true, mediaType: 'application/pdf' }), true)
  assert.equal(canOpen({ hasFile: true, mediaType: 'text/html' }), true)
  assert.equal(canOpen({ hasFile: true, mediaType: 'application/octet-stream' }), false, 'a program or anything unknown is only saved')
  assert.equal(canOpen({ hasFile: false, mediaType: 'application/pdf' }), false)
})

test('an image is drawn only when it is here and not too heavy', () => {
  assert.equal(canShowImage({ hasFile: true, mediaType: 'image/png', sizeBytes: 1000 }), true)
  assert.equal(canShowImage({ hasFile: true, mediaType: 'image/jpeg', sizeBytes: MAX_DRAW_BYTES }), true)
  assert.equal(canShowImage({ hasFile: true, mediaType: 'image/jpeg', sizeBytes: MAX_DRAW_BYTES + 1 }), false)
  assert.equal(canShowImage({ hasFile: false, mediaType: 'image/png', sizeBytes: 1000 }), false)
  assert.equal(canShowImage({ hasFile: true, mediaType: 'application/pdf', sizeBytes: 1000 }), false)
})

test('the page says only what is wrong, and a phone calls a missing file one that is waiting', () => {
  assert.deepEqual(reportLines({ missing: 0, damaged: 0, cleaned: 5 }, false), [])
  assert.deepEqual(reportLines({ missing: 3, damaged: 1, cleaned: 0 }, false), [
    { key: 'docs.missing', count: 3 },
    { key: 'docs.damaged', count: 1 },
  ])
  assert.deepEqual(reportLines({ missing: 2, damaged: 0, cleaned: 0 }, true), [{ key: 'docs.waiting', count: 2 }])
})

test('the type filter offers all, then the three groups the app has', () => {
  assert.deepEqual(KIND_CHOICES.map((choice) => choice.value), ['', 'image', 'pdf', 'other'])
})
