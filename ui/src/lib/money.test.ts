import assert from 'node:assert/strict'
import test from 'node:test'
import { amountString, convertAmount, fromMinor, parseRate, toMinor } from './money.ts'

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

test('amounts convert to whole cents and back without loss', () => {
  assert.equal(toMinor('12.50'), 1250n)
  assert.equal(toMinor('-0.05'), -5n)
  assert.equal(toMinor('12.5'), null)
  for (const amount of ['0.00', '0.07', '-1250.99', '9999999999.99']) assert.equal(fromMinor(toMinor(amount) ?? 0n), amount)
  assert.equal(fromMinor(-1600n), '-16.00')
})

test('a comma is a decimal mark like the dot, and nothing else with a separator is an amount', () => {
  assert.equal(amountString('29,5', false), '29.50')
  assert.equal(amountString('29,50', false), '29.50')
  assert.equal(amountString(' 0,05 ', false), '0.05')
  for (const bad of ['29,', '29.', ',5', '1,234.50', '1.234,50', '29,555', '29,5,0', '29;50', '-29,50']) assert.equal(amountString(bad, false), null, bad)
})
