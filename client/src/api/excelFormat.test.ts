import assert from 'node:assert/strict'
import { test } from 'node:test'
import { PLAIN_FORMAT, sheetFormat } from './excelFormat.ts'

test('German: day first, dots, the symbol after the number with a space', () => {
  assert.deepEqual(sheetFormat('de-DE', 'EUR'), {
    dateOrder: 'DMY',
    dateSeparator: '.',
    currencySymbol: '€',
    symbolBefore: false,
    symbolSpace: true,
  })
})

test('US: month first, slashes, the symbol before the number', () => {
  assert.deepEqual(sheetFormat('en-US', 'USD'), {
    dateOrder: 'MDY',
    dateSeparator: '/',
    currencySymbol: '$',
    symbolBefore: true,
    symbolSpace: false,
  })
})

test('Irish English: day first with slashes and the euro before the number', () => {
  const format = sheetFormat('en-IE', 'EUR')
  assert.deepEqual([format.dateOrder, format.dateSeparator, format.symbolBefore], ['DMY', '/', true])
})

test('Swedish and Japanese put the year first', () => {
  assert.equal(sheetFormat('sv-SE', 'SEK').dateOrder, 'YMD')
  assert.equal(sheetFormat('sv-SE', 'SEK').dateSeparator, '-')
  assert.equal(sheetFormat('ja-JP', 'JPY').dateOrder, 'YMD')
  assert.equal(sheetFormat('ja-JP', 'JPY').dateSeparator, '/')
})

test('Dutch uses dashes and French a space before the symbol', () => {
  assert.equal(sheetFormat('nl-NL', 'EUR').dateSeparator, '-')
  const french = sheetFormat('fr-FR', 'EUR')
  assert.deepEqual([french.dateOrder, french.symbolBefore, french.symbolSpace], ['DMY', false, true])
})

test('a locale the browser cannot use gives ISO dates and a plain number', () => {
  assert.deepEqual(sheetFormat('en_US', 'USD'), PLAIN_FORMAT)
  assert.deepEqual(sheetFormat('', 'USD'), PLAIN_FORMAT)
})

test('a currency the browser does not know still gives usable dates', () => {
  const format = sheetFormat('de-DE', 'ZZZ')
  assert.equal(format.dateOrder, 'DMY')
  assert.ok(format.currencySymbol.length <= 8)
})

test('what is sent to the app always fits what the app accepts', () => {
  for (const locale of ['en-US', 'de-DE', 'fr-CH', 'ar-EG', 'hu-HU', 'th-TH', 'ko-KR', 'pt-BR', 'zh-CN', 'ro-RO']) {
    const format = sheetFormat(locale, 'EUR')
    assert.ok(['DMY', 'MDY', 'YMD'].includes(format.dateOrder), locale)
    assert.ok(['/', '.', '-', ' '].includes(format.dateSeparator), locale)
    assert.ok(format.currencySymbol.length <= 8 && !format.currencySymbol.includes('"'), locale)
  }
})
