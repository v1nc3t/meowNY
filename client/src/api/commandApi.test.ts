import assert from 'node:assert/strict'
import { test } from 'node:test'
import {
  ApiStatus,
  cleanTokenName,
  CreatedToken,
  DEFAULT_EXPIRY_DAYS,
  EXPIRY_CHOICES,
  TOKEN_NAME_MAX,
  TokenInfo,
} from './commandApi.ts'

// What the Rust side sends, as its own tests show it (tokens.rs, api.rs).
const info = {
  id: '3',
  name: 'budget agent',
  scope: 'write',
  createdAt: '2026-10-14T09:00:00Z',
  expiresAt: '2026-11-13T09:00:00Z',
  lastUsedAt: null,
  expired: false,
}
const secret = `mny_${'0123456789abcdef'.repeat(4)}`

test('a token as the app sends it is accepted', () => {
  assert.deepEqual(TokenInfo.parse(info), info)
  assert.equal(TokenInfo.parse({ ...info, scope: 'read', lastUsedAt: '2026-10-14T10:00:00Z', expired: true }).expired, true)
  assert.equal(CreatedToken.parse({ token: secret, info }).token, secret)
})

test('a token the app would never send is refused', () => {
  for (const bad of [
    { ...info, id: '0' },
    { ...info, id: 'abc' },
    { ...info, scope: 'admin' },
    { ...info, scope: 'READ' },
    { ...info, createdAt: '2026-10-14' },
    { ...info, expiresAt: '2026-11-13T09:00:00+01:00' },
    { ...info, lastUsedAt: 'yesterday' },
    { ...info, expired: 'no' },
    { ...info, name: undefined },
  ]) {
    assert.equal(TokenInfo.safeParse(bad).success, false, JSON.stringify(bad))
  }
})

test('the secret has the exact shape the app makes', () => {
  for (const bad of ['', 'mny_', secret.slice(0, -1), `${secret}0`, secret.toUpperCase(), secret.replace('mny_', 'xxx_'), `mny_${'g'.repeat(64)}`]) {
    assert.equal(CreatedToken.safeParse({ token: bad, info }).success, false, bad)
  }
})

test('the API status is the on/off choice, whether it listens, and the port', () => {
  assert.deepEqual(ApiStatus.parse({ enabled: false, running: false, port: null }), { enabled: false, running: false, port: null })
  assert.equal(ApiStatus.parse({ enabled: true, running: true, port: 39315 }).port, 39315)
  for (const port of [0, 65536, -1, 1.5, '80']) {
    assert.equal(ApiStatus.safeParse({ enabled: true, running: true, port }).success, false, String(port))
  }
  assert.equal(ApiStatus.safeParse({ enabled: true, running: true }).success, false, 'a missing port')
})

test('a token name is trimmed and must be 1 to 50 characters without control characters or hidden direction marks', () => {
  assert.equal(cleanTokenName('  nightly script '), 'nightly script')
  assert.equal(cleanTokenName('x'), 'x')
  assert.equal(cleanTokenName('é'.repeat(TOKEN_NAME_MAX)), 'é'.repeat(TOKEN_NAME_MAX), '50 characters, not 50 bytes')
  assert.equal(cleanTokenName('日本語のスクリプト'), '日本語のスクリプト')
  assert.equal(cleanTokenName('😀'.repeat(TOKEN_NAME_MAX)), '😀'.repeat(TOKEN_NAME_MAX), 'an emoji is one character')
  for (const bad of ['', '   ', '\t\n', 'x'.repeat(TOKEN_NAME_MAX + 1), 'two\nlines', 'bell\u0007', 'null\u0000byte', 'del\u007f', 'a‮b', 'a‏b', 'a⁦b', 'a﻿b', 'a‎b']) {
    assert.equal(cleanTokenName(bad), null, JSON.stringify(bad))
  }
})

test('the lifetimes offered are valid for the app and include the default', () => {
  assert.ok(EXPIRY_CHOICES.includes(DEFAULT_EXPIRY_DAYS as (typeof EXPIRY_CHOICES)[number]))
  for (const days of EXPIRY_CHOICES) assert.ok(Number.isInteger(days) && days >= 1 && days <= 365, String(days))
  assert.deepEqual([...EXPIRY_CHOICES], [...EXPIRY_CHOICES].sort((a, b) => a - b))
})
