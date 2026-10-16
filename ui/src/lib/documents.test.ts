import assert from 'node:assert/strict'
import { test } from 'node:test'
import { t } from '../i18n/translate.ts'
import { canOpen, canShowImage, dropNotice, dropNote, fileState, fileStateText, KIND_CHOICES, MAX_DRAW_BYTES, reportLines } from './documents.ts'

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

test('each dropped file has a sentence of its own, and a failure says why in words the screens have', () => {
  assert.equal(dropNote({ name: 'a.pdf', status: 'added', error: null }, t), 'added a.pdf.')
  assert.equal(dropNote({ name: 'a.pdf', status: 'alreadyThere', error: null }, t), 'a.pdf is in the library already.')
  assert.match(dropNote({ name: 'big.bin', status: 'failed', error: 'too_large' }, t), /^big\.bin: .*25/)
  assert.match(dropNote({ name: 'empty.pdf', status: 'failed', error: 'invalid_input' }, t), /^empty\.pdf: /)
  // a code nobody knows is the plain "something went wrong", never the code itself
  const odd = dropNote({ name: 'x', status: 'failed', error: 'something_new' }, t)
  assert.ok(!odd.includes('something_new'), odd)
})

test('a drop is one notice, with how many files were left out when there were too many', () => {
  const outcomes = [
    { name: 'a.pdf', status: 'added' as const, error: null },
    { name: 'b.pdf', status: 'alreadyThere' as const, error: null },
  ]
  assert.equal(dropNotice({ outcomes, ignored: 0 }, t), 'added a.pdf. b.pdf is in the library already.')
  assert.match(dropNotice({ outcomes, ignored: 30 }, t), /added a\.pdf\..*30/)
  assert.equal(dropNotice({ outcomes: [], ignored: 0 }, t), '')
})
