import assert from 'node:assert/strict'
import { test } from 'node:test'
import { PreviewError } from './errors.ts'
import { openZip } from './zip.ts'
import { makeZip } from './zipFixture.ts'

const text = (bytes: Uint8Array | null) => new TextDecoder().decode(bytes ?? new Uint8Array())

test('a part is found by its name, stored or deflated', async () => {
  const zip = openZip(await makeZip([
    { name: 'a.txt', data: 'stored part' },
    { name: 'dir/b.xml', data: '<x>'.repeat(100), deflate: true },
  ]))
  assert.equal(zip.has('a.txt'), true)
  assert.equal(zip.has('missing'), false)
  assert.equal(text(await zip.read('a.txt')), 'stored part')
  assert.equal(text(await zip.read('dir/b.xml')), '<x>'.repeat(100))
  assert.equal(await zip.read('missing'), null)
})

test('a name that tries to leave a folder is only a name: nothing is written anywhere', async () => {
  const zip = openZip(await makeZip([{ name: '../../evil', data: 'x' }]))
  assert.equal(text(await zip.read('../../evil')), 'x')
  assert.equal(await zip.read('evil'), null)
})

test('what is not a zip, or is cut, is not read', async () => {
  assert.throws(() => openZip(new Uint8Array([1, 2, 3])), PreviewError)
  assert.throws(() => openZip(new TextEncoder().encode('PK\u0003\u0004 not a real zip, just the first bytes of one')), PreviewError)
  const whole = await makeZip([{ name: 'a', data: 'hello' }])
  assert.throws(() => openZip(whole.subarray(0, whole.length - 10)), PreviewError)
})

test('a part that claims less than it holds is stopped at the limit, not at the claim', async () => {
  // a deflated part of 20 MiB of zeros is about 20 KiB, and its directory entry is changed to say it is 100 bytes
  const bomb = await makeZip([{ name: 'big.xml', data: new Uint8Array(20 * 1024 * 1024), deflate: true }])
  const honest = openZip(bomb)
  await assert.rejects(honest.read('big.xml'), (failure) => failure instanceof PreviewError && failure.reason === 'tooBig')

  const view = new DataView(bomb.buffer, bomb.byteOffset, bomb.byteLength)
  let end = bomb.length - 22
  while (view.getUint32(end, true) !== 0x06054b50) end -= 1
  const entryAt = view.getUint32(end + 16, true)
  view.setUint32(entryAt + 24, 100, true)
  await assert.rejects(openZip(bomb).read('big.xml'), (failure) => failure instanceof PreviewError && failure.reason === 'tooBig')
})

test('an encrypted part and a method that is not known are not read', async () => {
  const stored = await makeZip([{ name: 'a', data: 'hello' }])
  const view = new DataView(stored.buffer, stored.byteOffset, stored.byteLength)
  let end = stored.length - 22
  while (view.getUint32(end, true) !== 0x06054b50) end -= 1
  const entryAt = view.getUint32(end + 16, true)
  view.setUint16(entryAt + 8, 1, true)
  await assert.rejects(openZip(stored).read('a'), (failure) => failure instanceof PreviewError && failure.reason === 'unsupported')
  view.setUint16(entryAt + 8, 0, true)
  view.setUint16(entryAt + 10, 12, true)
  await assert.rejects(openZip(stored).read('a'), (failure) => failure instanceof PreviewError && failure.reason === 'unsupported')
})
