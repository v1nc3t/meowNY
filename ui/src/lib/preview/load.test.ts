import assert from 'node:assert/strict'
import { test } from 'node:test'
import { MAX_TEXT_CHARS } from './clean.ts'
import { PreviewError } from './errors.ts'
import { fromBase64Url, shownOf } from './load.ts'
import { makeZip } from './zipFixture.ts'

const url = (bytes: Uint8Array) => Buffer.from(bytes).toString('base64url')
const text = (value: string) => url(new TextEncoder().encode(value))

test('base64url text is read into its bytes', () => {
  assert.deepEqual([...fromBase64Url(url(new Uint8Array([251, 255, 254, 0, 1])))], [251, 255, 254, 0, 1])
  assert.deepEqual([...fromBase64Url('')], [])
})

test('an image becomes a data address and nothing else is made of it', async () => {
  const shown = await shownOf({ kind: 'image', mediaType: 'image/png', data: 'iVBO-_' })
  assert.deepEqual(shown, { kind: 'image', source: 'data:image/png;base64,iVBO+/==' })
})

test('text is shown cleaned, and cut when it is very long', async () => {
  assert.deepEqual(await shownOf({ kind: 'text', mediaType: 'text/plain', data: text('milk‮\neggs\n') }), { kind: 'text', text: 'milk\neggs\n', cut: false })
  const long = await shownOf({ kind: 'text', mediaType: 'text/plain', data: text('x'.repeat(MAX_TEXT_CHARS + 5)) })
  assert.ok(long.kind === 'text' && long.cut && long.text.length === MAX_TEXT_CHARS)
})

test('a CSV becomes a table', async () => {
  const shown = await shownOf({ kind: 'csv', mediaType: 'text/csv', data: text('a,b\n1,2\n') })
  assert.deepEqual(shown, { kind: 'table', rows: [['a', 'b'], ['1', '2']], cut: false })
})

test('a PDF is passed on as its bytes, for the viewer', async () => {
  const shown = await shownOf({ kind: 'pdf', mediaType: 'application/pdf', data: text('%PDF-1.7') })
  assert.ok(shown.kind === 'pdf' && new TextDecoder().decode(shown.bytes) === '%PDF-1.7')
})

test('a Word file and an Excel file are read with their own readers, and a file that is neither is an error, not a crash', async () => {
  const word = await makeZip([{ name: 'word/document.xml', data: '<w:document xmlns:w="x"><w:body><w:p><w:r><w:t>hi</w:t></w:r></w:p></w:body></w:document>' }])
  const shown = await shownOf({ kind: 'docx', mediaType: 'application/octet-stream', data: url(word) })
  assert.ok(shown.kind === 'docx' && shown.content.blocks[0]?.type === 'paragraph')
  await assert.rejects(shownOf({ kind: 'xlsx', mediaType: 'application/octet-stream', data: url(word) }), PreviewError)
  await assert.rejects(shownOf({ kind: 'docx', mediaType: 'application/octet-stream', data: text('not a zip') }), PreviewError)
})
