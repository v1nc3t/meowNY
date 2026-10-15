import assert from 'node:assert/strict'
import { test } from 'node:test'
import { PLAIN_FORMAT, displayFormat } from './displayFormat.ts'

test('German: day first, dots, the symbol after the number with a space', () => {
  assert.deepEqual(displayFormat('de-DE', 'EUR'), {
    dateOrder: 'DMY',
    dateSeparator: '.',
    currencySymbol: '€',
    symbolBefore: false,
    symbolSpace: true,
    decimalMark: ',',
    groupSeparator: '.',
  })
})

test('US: month first, slashes, the symbol before the number', () => {
  assert.deepEqual(displayFormat('en-US', 'USD'), {
    dateOrder: 'MDY',
    dateSeparator: '/',
    currencySymbol: '$',
    symbolBefore: true,
    symbolSpace: false,
    decimalMark: '.',
    groupSeparator: ',',
  })
})

test('Irish English: day first with slashes and the euro before the number', () => {
  const format = displayFormat('en-IE', 'EUR')
  assert.deepEqual([format.dateOrder, format.dateSeparator, format.symbolBefore], ['DMY', '/', true])
})

test('Swedish and Japanese put the year first', () => {
  assert.equal(displayFormat('sv-SE', 'SEK').dateOrder, 'YMD')
  assert.equal(displayFormat('sv-SE', 'SEK').dateSeparator, '-')
  assert.equal(displayFormat('ja-JP', 'JPY').dateOrder, 'YMD')
  assert.equal(displayFormat('ja-JP', 'JPY').dateSeparator, '/')
})

test('Dutch uses dashes and French a space before the symbol', () => {
  assert.equal(displayFormat('nl-NL', 'EUR').dateSeparator, '-')
  const french = displayFormat('fr-FR', 'EUR')
  assert.deepEqual([french.dateOrder, french.symbolBefore, french.symbolSpace], ['DMY', false, true])
})

test('decimal and group marks follow the locale', () => {
  const swiss = displayFormat('de-CH', 'CHF')
  assert.equal(swiss.decimalMark, '.')
  assert.match(swiss.groupSeparator, /^['\u2019]$/, 'an apostrophe, straight or curly depending on the ICU version')
  const french = displayFormat('fr-FR', 'EUR')
  assert.equal(french.decimalMark, ',')
  assert.match(french.groupSeparator, /^\s$/, 'a space of some kind')
  assert.deepEqual([displayFormat('en-IN', 'INR').decimalMark, displayFormat('ro-RO', 'RON').groupSeparator], ['.', '.'])
})

test('digits are Western even where the locale writes others', () => {
  const arabic = displayFormat('ar-EG', 'EGP')
  assert.ok(!/[\u0660-\u0669]/.test(arabic.decimalMark + arabic.groupSeparator + arabic.currencySymbol.replace(/\D/g, '')))
  assert.equal(arabic.decimalMark.length, 1)
})

test('a locale the browser cannot use gives ISO dates and a plain number', () => {
  assert.deepEqual(displayFormat('en_US', 'USD'), PLAIN_FORMAT)
  assert.deepEqual(displayFormat('', 'USD'), PLAIN_FORMAT)
})

test('a currency the browser does not know still gives usable dates', () => {
  const format = displayFormat('de-DE', 'ZZZ')
  assert.equal(format.dateOrder, 'DMY')
  assert.ok(format.currencySymbol.length <= 8)
})

test('what is sent to the app always fits what the app accepts', () => {
  for (const locale of ['en-US', 'de-DE', 'fr-CH', 'ar-EG', 'hu-HU', 'th-TH', 'ko-KR', 'pt-BR', 'zh-CN', 'ro-RO']) {
    const format = displayFormat(locale, 'EUR')
    assert.ok(['DMY', 'MDY', 'YMD'].includes(format.dateOrder), locale)
    assert.ok(['/', '.', '-', ' '].includes(format.dateSeparator), locale)
    assert.ok(format.currencySymbol.length <= 8 && !format.currencySymbol.includes('"'), locale)
    assert.equal([...format.decimalMark].length, 1, locale)
    assert.ok([...format.groupSeparator].length <= 1 && format.groupSeparator !== format.decimalMark, locale)
    assert.ok(!/[0-9"]/.test(format.decimalMark + format.groupSeparator), locale)
  }
})
