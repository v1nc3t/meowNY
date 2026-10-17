import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { Doc } from '../api/documents.ts'
import { t } from '../i18n/translate.ts'
import { canOpen, canPreview, canShowImage, dropNotice, dropNote, fileState, fileStateText, KIND_CHOICES, MAX_DRAW_BYTES, MAX_PREVIEW_BYTES, previewKindOf, reportLines, typeText, whereIs } from './documents.ts'

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

test('a kind of file is named the way a person says it', () => {
  assert.equal(typeText('image/png'), 'docs.typeImage')
  assert.equal(typeText('application/pdf'), 'docs.typePdf')
  for (const text of ['text/plain', 'text/csv', 'text/html'] as const) assert.equal(typeText(text), 'docs.typeText')
  assert.equal(typeText('application/octet-stream'), 'docs.typeOther')
})

test('a document is previewed as an image, a PDF, text or a table, or by its name as a Word or Excel file; never as HTML', () => {
  const doc = (name: string, mediaType: Doc['mediaType']) => ({ name, mediaType })
  assert.equal(previewKindOf(doc('a.png', 'image/png')), 'image')
  assert.equal(previewKindOf(doc('a.pdf', 'application/pdf')), 'pdf')
  assert.equal(previewKindOf(doc('a.txt', 'text/plain')), 'text')
  assert.equal(previewKindOf(doc('a.csv', 'text/csv')), 'csv')
  assert.equal(previewKindOf(doc('page.html', 'text/html')), null)
  assert.equal(previewKindOf(doc('Letter.DOCX', 'application/octet-stream')), 'docx')
  assert.equal(previewKindOf(doc('Budget.xlsx', 'application/octet-stream')), 'xlsx')
  assert.equal(previewKindOf(doc('Budget.xlsm', 'application/octet-stream')), 'xlsx')
  assert.equal(previewKindOf(doc('old.doc', 'application/octet-stream')), null)
  assert.equal(previewKindOf(doc('Budget', 'application/octet-stream')), null, 'no extension, no reader')
  assert.equal(previewKindOf(doc('.xlsx', 'application/octet-stream')), 'xlsx')
})

test('the preview button is offered for a file that is here, of a kind that is shown, and not too heavy for that kind', () => {
  const pdf = { hasFile: true, name: 'a.pdf', mediaType: 'application/pdf', sizeBytes: MAX_PREVIEW_BYTES } as const
  assert.equal(canPreview(pdf), true)
  assert.equal(canPreview({ ...pdf, sizeBytes: MAX_PREVIEW_BYTES + 1 }), false)
  assert.equal(canPreview({ ...pdf, hasFile: false }), false, 'a phone that does not keep the file has nothing to show')
  const image = { hasFile: true, name: 'a.png', mediaType: 'image/png', sizeBytes: MAX_DRAW_BYTES } as const
  assert.equal(canPreview(image), true)
  assert.equal(canPreview({ ...image, sizeBytes: MAX_DRAW_BYTES + 1 }), false, 'an image keeps the lower limit')
  assert.equal(canPreview({ ...pdf, name: 'a.html', mediaType: 'text/html' }), false)
})

test('an item that a document is attached to is found on the page of its kind, and a debt entry on its friend\'s page', () => {
  const focus = (kind: 'transaction' | 'recurring' | 'ledger_entry') => ({ kind, id: '7', date: '2026-03-04' })
  assert.deepEqual(whereIs({ kind: 'transaction', id: '7', date: '2026-03-04' }), { path: '/app/transactions', focus: focus('transaction') })
  assert.deepEqual(whereIs({ kind: 'recurring', id: '7', date: '2026-03-04' }), { path: '/app/recurring', focus: focus('recurring') })
  assert.deepEqual(whereIs({ kind: 'ledger_entry', id: '7', date: '2026-03-04', personId: '3' }), { path: '/app/debts/3', focus: focus('ledger_entry') })
  assert.equal(whereIs({ kind: 'ledger_entry', id: '7', date: '2026-03-04' }), null, 'without the friend there is no page to go to')
})
