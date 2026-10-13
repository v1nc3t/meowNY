import assert from 'node:assert/strict'
import { test } from 'node:test'
import { CURRENCIES, currencyLabel, currencyName, currencyOptions } from './currencies.ts'

test('every listed code is three capital letters and appears once, in order', () => {
  const codes = CURRENCIES.map(([code]) => code)
  assert.ok(codes.every((code) => /^[A-Z]{3}$/.test(code)))
  assert.equal(new Set(codes).size, codes.length)
  assert.deepEqual(codes, [...codes].sort())
})

test('labels show the code and a lower-case name, and fall back to the bare code', () => {
  assert.equal(currencyName('EUR', 'en'), 'Euro')
  assert.equal(currencyLabel('USD', 'en'), 'USD  us dollar')
  assert.equal(currencyLabel('XYZ', 'en'), 'XYZ')
})

test('currency names follow the language', () => {
  assert.equal(currencyName('EUR', 'de'), 'Euro')
  assert.equal(currencyName('USD', 'de'), 'US-Dollar')
  assert.equal(currencyName('JPY', 'ro'), 'yen japonez')
  assert.equal(currencyOptions('fr').length, CURRENCIES.length)
})
