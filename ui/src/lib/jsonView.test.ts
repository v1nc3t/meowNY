import assert from 'node:assert/strict'
import { test } from 'node:test'
import { tokenizeJson } from './jsonView.ts'

const kinds = (text: string) => tokenizeJson(text).map((token) => `${token.kind}:${token.text}`)

test('a string with a quote or a colon inside is one string', () => {
  assert.deepEqual(kinds('["say \\"hi\\": ok", "a:b"]'), ['plain:[', 'string:"say \\"hi\\": ok"', 'plain:, ', 'string:"a:b"', 'plain:]'])
})

test('a text that is cut in the middle of a string still comes out whole', () => {
  const text = '{\n  "name": "Boiler rep'
  assert.equal(tokenizeJson(text).map((token) => token.text).join(''), text)
  assert.equal(tokenizeJson(text).at(-1)?.kind, 'string')
})

test('the pieces always put back together the text, whatever it is', () => {
  for (const text of ['', '   ', 'not json at all', '{"a":1}\n', '"\\', '-', 'tru', '{"é€": "日本"}']) {
    assert.equal(tokenizeJson(text).map((token) => token.text).join(''), text, JSON.stringify(text))
  }
})

test('words inside a longer word are not words', () => {
  assert.equal(tokenizeJson('nullable').filter((token) => token.kind === 'literal').length, 0)
  assert.equal(tokenizeJson('null,').filter((token) => token.kind === 'literal').length, 1)
  assert.deepEqual(kinds('x'), ['plain:x'])
})
