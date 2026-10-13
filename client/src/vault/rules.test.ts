import assert from 'node:assert/strict'
import { test } from 'node:test'
import { isCurrency, nearestLockChoice, normalizeCurrency, passphraseProblem } from './rules.ts'

test('currency codes are three letters, any case', () => {
  assert.equal(normalizeCurrency(' eur '), 'EUR')
  assert.ok(isCurrency('usd'))
  assert.ok(!isCurrency('us'))
  assert.ok(!isCurrency('u5d'))
  assert.ok(!isCurrency(''))
})

test('an empty passphrase means keychain only', () => {
  assert.equal(passphraseProblem('', ''), '')
})

test('a passphrase must be long enough and confirmed', () => {
  assert.match(passphraseProblem('short', 'short'), /at least 8/)
  assert.match(passphraseProblem('long enough', 'different'), /do not match/)
  assert.equal(passphraseProblem('long enough', 'long enough'), '')
})

test('passphrase length counts characters, not bytes', () => {
  assert.match(passphraseProblem('ééééééé', 'ééééééé'), /at least 8/)
  assert.equal(passphraseProblem('éééééééé', 'éééééééé'), '')
})

test('a stored lock time outside the list maps to the nearest choice', () => {
  assert.equal(nearestLockChoice(300), 300)
  assert.equal(nearestLockChoice(0), 0)
  assert.equal(nearestLockChoice(400), 300)
  assert.equal(nearestLockChoice(5000), 3600)
})
