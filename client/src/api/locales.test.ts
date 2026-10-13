import assert from 'node:assert/strict'
import { test } from 'node:test'
import { formatExample, localeChoices, timeZoneChoices } from './locales.ts'

test('the locale in use comes first, then the system preference, then common ones, without repeats', () => {
  const choices = localeChoices('en-IE', ['fr-FR', 'en-IE', 'de-DE'], () => true)
  assert.deepEqual(choices.slice(0, 3), ['en-IE', 'fr-FR', 'de-DE'])
  assert.equal(new Set(choices).size, choices.length)
  assert.ok(choices.includes('ja-JP'), 'common locales follow')
})

test('formats the engine cannot handle are dropped, but the one in use always stays', () => {
  const choices = localeChoices('xx-YY', ['yy-ZZ', 'en-GB'], (tag) => tag === 'en-GB' || tag === 'en-US')
  assert.deepEqual(choices, ['xx-YY', 'en-GB', 'en-US'])
})

test('time zones: current and system first, UTC always available, the rest sorted', () => {
  const zones = timeZoneChoices(['Europe/Paris', 'Asia/Tokyo', 'Europe/Dublin'], 'Asia/Tokyo', 'Europe/Dublin')
  assert.deepEqual(zones, ['Europe/Dublin', 'Asia/Tokyo', 'Europe/Paris', 'UTC'])
  assert.equal(new Set(zones).size, zones.length)
})

test('time zones fall back to a common list when the system cannot list them', () => {
  const zones = timeZoneChoices(null, 'Europe/Dublin', 'Europe/Dublin')
  assert.equal(zones[0], 'Europe/Dublin')
  assert.ok(zones.includes('UTC') && zones.includes('America/New_York'))
})

test('the example shows the number and date style of the locale', () => {
  assert.match(formatExample('en-US', 'USD'), /\$1,234\.56\u00a0\u00a0\|\u00a0\u00a010\/13\/2026/)
  assert.match(formatExample('de-DE', 'EUR'), /1\.234,56\s+€\u00a0\u00a0\|\u00a0\u00a013\.10\.2026/)
  assert.equal(formatExample('not a locale!!', 'EUR'), '')
})
