import assert from 'node:assert/strict'
import { test } from 'node:test'
import { MAX_TEXT_CHARS } from './clean.ts'
import { PreviewError } from './errors.ts'
import { shownOf } from './load.ts'
import { makeZip } from './zipFixture.ts'

const url = (bytes: Uint8Array) => Buffer.from(bytes).toString('base64url')
const text = (value: string) => url(new TextEncoder().encode(value))

test('an image becomes a data address and nothing else is made of it', async () => {
  const shown = await shownOf({ kind: 'image', mediaType: 'image/png', data: 'iVBO-_' })
  assert.deepEqual(shown, { kind: 'image', source: 'data:image/png;base64,iVBO+/==' })
})

test('text is shown cleaned, and cut when it is very long', async () => {
  assert.deepEqual(await shownOf({ kind: 'text', mediaType: 'text/plain', data: text('milk‮\neggs\n') }), { kind: 'text', text: 'milk\neggs\n', cut: false })
  const long = await shownOf({ kind: 'text', mediaType: 'text/plain', data: text('x'.repeat(MAX_TEXT_CHARS + 5)) })
  assert.ok(long.kind === 'text' && long.cut && long.text.length === MAX_TEXT_CHARS)
})

test('a Word file and an Excel file are read with their own readers, and a file that is neither is an error, not a crash', async () => {
  const word = await makeZip([{ name: 'word/document.xml', data: '<w:document xmlns:w="x"><w:body><w:p><w:r><w:t>hi</w:t></w:r></w:p></w:body></w:document>' }])
  const shown = await shownOf({ kind: 'docx', mediaType: 'application/octet-stream', data: url(word) })
  assert.ok(shown.kind === 'docx' && shown.content.blocks[0]?.type === 'paragraph')
  await assert.rejects(shownOf({ kind: 'xlsx', mediaType: 'application/octet-stream', data: url(word) }), PreviewError)
  await assert.rejects(shownOf({ kind: 'docx', mediaType: 'application/octet-stream', data: text('not a zip') }), PreviewError)
})
