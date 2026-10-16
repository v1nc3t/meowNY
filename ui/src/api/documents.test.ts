import assert from 'node:assert/strict'
import { test } from 'node:test'
import { ApiError } from './invoke.ts'
import {
  AddOutcome,
  checkFileSize,
  Doc,
  DocPage,
  imageSource,
  isImage,
  isOpenable,
  LibrarySize,
  LinkView,
  MAX_DOCUMENT_BYTES,
  PIECE_BYTES,
  toBase64Url,
  toStandardBase64,
  upload,
  type NewDoc,
  type UploadTransport,
} from './documents.ts'

const doc = {
  id: '4',
  name: 'Receipt.pdf',
  mediaType: 'application/pdf',
  sizeBytes: 1234,
  sha256: 'a'.repeat(64),
  documentDate: '2026-10-10',
  keepOnPhone: false,
  createdAt: '2026-10-10T10:00:00.000Z',
  linkCount: 1,
  hasFile: true,
}

test('what the app answers for a document is checked', () => {
  assert.ok(Doc.safeParse(doc).success)
  assert.ok(DocPage.safeParse({ documents: [doc], total: 1, limit: 50, offset: 0 }).success)
  assert.ok(AddOutcome.safeParse({ document: doc, alreadyThere: false }).success)
  assert.ok(!Doc.safeParse({ ...doc, mediaType: 'image/svg+xml' }).success, 'the list of kinds is closed')
  assert.ok(!Doc.safeParse({ ...doc, sizeBytes: 0 }).success)
  assert.ok(!Doc.safeParse({ ...doc, sha256: 'A'.repeat(64) }).success)
  assert.ok(!Doc.safeParse({ ...doc, id: '0' }).success)
  assert.ok(!Doc.safeParse({ ...doc, documentDate: '10/10/2026' }).success)
  assert.ok(!Doc.safeParse({ ...doc, name: '' }).success)
  assert.ok(!Doc.safeParse({ ...doc, hasFile: undefined }).success)
  assert.ok(LinkView.safeParse({ kind: 'transaction', id: '3', name: 'Shop', date: '2026-10-10' }).success)
  assert.ok(!LinkView.safeParse({ kind: 'category', id: '3', name: 'Shop', date: '2026-10-10' }).success)
  assert.ok(LibrarySize.safeParse({ count: 2, bytes: 30, heldCount: 1, heldBytes: 10 }).success)
  assert.ok(!LibrarySize.safeParse({ count: 2, bytes: 30 }).success)
})

test('bytes become the same base64url text that Node writes, for every length', () => {
  for (let length = 0; length <= 70; length += 1) {
    const bytes = Uint8Array.from({ length }, (_, i) => (i * 37 + 11) & 0xff)
    assert.equal(toBase64Url(bytes), Buffer.from(bytes).toString('base64url'), `${length} bytes`)
  }
  const every = Uint8Array.from({ length: 256 }, (_, i) => i)
  assert.equal(toBase64Url(every), Buffer.from(every).toString('base64url'))
  assert.match(toBase64Url(Uint8Array.of(0xfb, 0xff, 0xfe)), /^[A-Za-z0-9_-]+$/, 'nothing that is not in the url alphabet')
})

test('a long array of bytes does not overflow the call stack', () => {
  const big = new Uint8Array(PIECE_BYTES).fill(0xab)
  assert.equal(toBase64Url(big), Buffer.from(big).toString('base64url'))
})

test('the text of a data address is the standard base64 of the same bytes', () => {
  for (let length = 0; length <= 40; length += 1) {
    const bytes = Uint8Array.from({ length }, (_, i) => 255 - ((i * 53) & 0xff))
    assert.equal(toStandardBase64(Buffer.from(bytes).toString('base64url')), Buffer.from(bytes).toString('base64'), `${length} bytes`)
  }
  const png = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0xfb, 0xff]).toString('base64url')
  assert.equal(imageSource({ mediaType: 'image/png', data: png }), `data:image/png;base64,${Buffer.from([0x89, 0x50, 0x4e, 0x47, 0xfb, 0xff]).toString('base64')}`)
})

test('the pieces are what the Rust side takes: at most half a megabyte of text', () => {
  assert.ok(toBase64Url(new Uint8Array(PIECE_BYTES)).length <= 512 * 1024)
  assert.equal(MAX_DOCUMENT_BYTES, 26_214_400)
})

test('a file that is empty or too large is refused before it is read', () => {
  assert.throws(() => checkFileSize(0), (e) => e instanceof ApiError && e.code === 'invalid_input')
  assert.throws(() => checkFileSize(MAX_DOCUMENT_BYTES + 1), (e) => e instanceof ApiError && e.code === 'too_large')
  checkFileSize(1)
  checkFileSize(MAX_DOCUMENT_BYTES)
})

/** An app that gathers what it is sent, the way the real one does, and says what it was asked. */
function fakeApp(options: { failOnChunk?: number } = {}) {
  const log: string[] = []
  let size = 0
  let gathered: number[] = []
  let chunks = 0
  const transport: UploadTransport = {
    begin: async (input, total) => {
      log.push(`begin ${input.name} ${total}`)
      size = total
      gathered = []
    },
    chunk: async (offset, data) => {
      chunks += 1
      if (options.failOnChunk === chunks) throw new ApiError('internal')
      assert.equal(offset, gathered.length, 'each piece starts where the last one ended')
      assert.ok(data.length <= 512 * 1024)
      for (const byte of Buffer.from(data, 'base64url')) gathered.push(byte)
      return gathered.length
    },
    finish: async () => {
      log.push('finish')
      assert.equal(gathered.length, size)
      return { document: { ...doc, sizeBytes: size }, alreadyThere: false }
    },
    cancel: async () => {
      log.push('cancel')
    },
  }
  return { transport, log, bytes: () => Uint8Array.from(gathered) }
}

test('a file goes in piece by piece and arrives whole', async () => {
  for (const size of [1, 100, PIECE_BYTES - 1, PIECE_BYTES, PIECE_BYTES + 1, 3 * PIECE_BYTES + 17]) {
    const source = Uint8Array.from({ length: size }, (_, i) => (i * 7) & 0xff)
    const app = fakeApp()
    const seen: number[] = []
    const outcome = await upload(new Blob([source]), { name: 'a.pdf' } satisfies NewDoc, app.transport, (sent) => seen.push(sent))
    assert.equal(outcome.document.sizeBytes, size)
    assert.deepEqual(app.bytes(), source, `${size} bytes`)
    assert.deepEqual(app.log, [`begin a.pdf ${size}`, 'finish'])
    assert.equal(seen.length, Math.ceil(size / PIECE_BYTES))
    assert.equal(seen[seen.length - 1], size, 'progress ends at the whole')
  }
})

test('a failure in the middle cancels the upload and is passed on', async () => {
  const app = fakeApp({ failOnChunk: 2 })
  await assert.rejects(upload(new Blob([new Uint8Array(3 * PIECE_BYTES)]), { name: 'a.pdf' }, app.transport), (e) => e instanceof ApiError && e.code === 'internal')
  assert.deepEqual(app.log, [`begin a.pdf ${3 * PIECE_BYTES}`, 'cancel'])
})

test('a file the app would refuse is not sent at all', async () => {
  const app = fakeApp()
  await assert.rejects(upload(new Blob([]), { name: 'empty' }, app.transport), (e) => e instanceof ApiError && e.code === 'invalid_input')
  await assert.rejects(upload(new Blob([new Uint8Array(MAX_DOCUMENT_BYTES + 1)]), { name: 'big' }, app.transport), (e) => e instanceof ApiError && e.code === 'too_large')
  assert.deepEqual(app.log, [], 'nothing went to the app')
})

test('a failure at the end is cancelled too, and a failed cancel does not hide the cause', async () => {
  const app = fakeApp()
  const transport: UploadTransport = {
    ...app.transport,
    finish: async () => {
      throw new ApiError('file_damaged')
    },
    cancel: async () => {
      throw new ApiError('internal')
    },
  }
  await assert.rejects(upload(new Blob([new Uint8Array(10)]), { name: 'a' }, transport), (e) => e instanceof ApiError && e.code === 'file_damaged')
})

test('which kinds are opened and which are drawn', () => {
  assert.ok(isOpenable('application/pdf') && isOpenable('text/html') && isOpenable('image/png'))
  assert.ok(!isOpenable('application/octet-stream'))
  assert.ok(isImage('image/webp') && !isImage('application/pdf') && !isImage('text/plain'))
})

