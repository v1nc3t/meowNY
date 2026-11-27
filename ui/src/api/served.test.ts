import assert from 'node:assert/strict'
import { test } from 'node:test'
import { isServed, readAnswer } from './served.ts'

test('a page not built for the server is not served', () => {
  assert.equal(isServed(), false)
})

test('an answer gives its data, or throws its code', () => {
  assert.deepEqual(readAnswer({ ok: true, data: [1, 2] }), { data: [1, 2], download: null })
  assert.throws(() => readAnswer({ ok: false, error: 'locked' }), (e) => e === 'locked')
  assert.throws(() => readAnswer('<html>bad gateway</html>'), (e) => e === 'internal')
  assert.throws(() => readAnswer(null), (e) => e === 'internal')
})

test('only a download the server could have named is fetched', () => {
  const id = 'a'.repeat(32)
  assert.deepEqual(readAnswer({ ok: true, data: true, download: { id, name: 'meowny.json' } }).download, { id, name: 'meowny.json' })
  assert.equal(readAnswer({ ok: true, data: true, download: { id: '../settings', name: 'x' } }).download, null)
  assert.equal(readAnswer({ ok: true, data: true, download: { id, name: 3 } }).download, null)
})
