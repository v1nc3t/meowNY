import assert from 'node:assert/strict'
import test from 'node:test'
import { amountString, formatMoney } from './money.ts'

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
