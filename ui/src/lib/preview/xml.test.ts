import assert from 'node:assert/strict'
import { test } from 'node:test'
import { PreviewError } from './errors.ts'
import { child, parseXml, textOf } from './xml.ts'

test('named entities, number references and CDATA give their text', () => {
  const root = parseXml('<a>&lt;&gt;&quot;&apos;&#65;&#x42;<![CDATA[<raw & text>]]></a>')
  assert.equal(textOf(root), '<>"\'AB<raw & text>')
  assert.equal(textOf(parseXml('<a>&#0;&#xD800;&#1114112;&unknown;</a>')), '&unknown;', 'impossible characters are dropped and unknown names stay as written')
})

test('a space inside a line is kept, and indentation between elements is not', () => {
  const root = parseXml('<a>\n  <t> </t>\n  <t>x</t>\n</a>')
  assert.equal(root.kids.length, 2)
  assert.equal(textOf(child(root, 't')!), ' ')
})

test('a greater-than sign inside a quoted value does not end the tag', () => {
  assert.equal(parseXml('<a title="1 > 0" b=\'x\'/>').attrs.title, '1 > 0')
})

test('a document type is refused, which leaves no entity of the file\'s own to expand', () => {
  const bomb = '<!DOCTYPE lolz [<!ENTITY lol "lol"><!ENTITY lol2 "&lol;&lol;&lol;&lol;">]><lolz>&lol2;</lolz>'
  assert.throws(() => parseXml(bomb), (failure) => failure instanceof PreviewError && failure.reason === 'unsupported')
})

test('what is not one well-formed tree is not read', () => {
  for (const bad of ['', 'text only', '<a>', '<a></b>', '<a><b></a></b>', '<a', '<a><!-- never closed</a>', '</a>']) {
    assert.throws(() => parseXml(bad), PreviewError, bad)
  }
})

test('a file with too many elements or too deep is refused', () => {
  assert.throws(() => parseXml('<a>'.repeat(200) + '</a>'.repeat(200)), (failure) => failure instanceof PreviewError && failure.reason === 'tooBig')
})
