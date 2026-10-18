import assert from 'node:assert/strict'
import { test } from 'node:test'
import { PLAIN_FORMAT, displayFormat } from './displayFormat.ts'

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
