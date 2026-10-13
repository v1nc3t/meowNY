import assert from 'node:assert/strict'
import test from 'node:test'
import { amountString, convertAmount, formatDate, formatMoney, parseRate } from './money.ts'

test('transaction amounts pad to two decimals and reject empty, zero, and negative', () => {
  assert.equal(amountString('10', false), '10.00')
  assert.equal(amountString('10.5', false), '10.50')
  assert.equal(amountString('10.50', false), '10.50')
  assert.equal(amountString(' 0.5 ', false), '0.50')
  assert.equal(amountString('', false), null)
  assert.equal(amountString('0', false), null)
  assert.equal(amountString('0.0', false), null)
  assert.equal(amountString('0.00', false), null)
  assert.equal(amountString('-1', false), null)
  assert.equal(amountString('-1.50', false), null)
  assert.equal(amountString('10.555', false), null)
  assert.equal(amountString('01.00', false), null)
})

test('budget amounts allow zero and still reject negatives', () => {
  assert.equal(amountString('', true), null)
  assert.equal(amountString('0', true), '0.00')
  assert.equal(amountString('0.5', true), '0.50')
  assert.equal(amountString('-2', true), null)
})

test('formatMoney uses the user currency and locale', () => {
  assert.match(formatMoney('10.50', 'EUR', 'en'), /10\.50/)
  assert.match(formatMoney('-100.00', 'EUR', 'en'), /100\.00/)
})

test('rates follow the same rules as the backend', () => {
  for (const good of ['1.08', '160', '0.0063', '0.00000001', '123456789.12345678']) assert.ok(parseRate(good), good)
  for (const bad of ['', '0', '0.0', '-1', '1,5', '1.', '.5', '01.5', '1.123456789', '1e3', ' 1.2', '1234567890']) {
    assert.equal(parseRate(bad), null, bad)
  }
})

test('conversion rounds half up to the cent, matching the backend', () => {
  assert.equal(convertAmount('100.00', '1.08'), '108.00')
  assert.equal(convertAmount('0.05', '0.5'), '0.03', '2.5 cents rounds up')
  assert.equal(convertAmount('0.04', '0.5'), '0.02')
  assert.equal(convertAmount('0.01', '0.4'), '0.00')
  assert.equal(convertAmount('100000.00', '0.0063'), '630.00')
  assert.equal(convertAmount('9999999999.99', '2'), '19999999999.98', 'the preview shows it; the backend refuses it')
  assert.equal(convertAmount('100', '1.08'), null)
  assert.equal(convertAmount('100.00', 'abc'), null)
})

test('dates are written the way the locale writes them, on the same calendar day', () => {
  assert.equal(formatDate('2026-10-12', 'en-US'), '10/12/2026')
  assert.equal(formatDate('2026-10-12', 'en-GB'), '12/10/2026')
  assert.equal(formatDate('2026-10-12', 'de-DE'), '12.10.2026')
  assert.equal(formatDate('2026-01-01', 'en-US'), '01/01/2026', 'no shift across the year start')
  assert.equal(formatDate('nonsense', 'en-US'), 'nonsense')
  assert.equal(formatDate('2026-10-12', 'not a locale!!'), '2026-10-12')
})
