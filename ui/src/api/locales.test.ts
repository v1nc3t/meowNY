import assert from 'node:assert/strict'
import { test } from 'node:test'
import { localeChoices, timeZoneChoices, whenText } from './locales.ts'

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

test('a time that cannot be shown comes back as written', () => {
  assert.equal(whenText('2026-10-14T09:30:00Z', 'en-IE', 'Not/AZone'), '2026-10-14T09:30:00Z')
  assert.equal(whenText('2026-10-14T09:30:00Z', 'not a locale!!', 'UTC'), '2026-10-14T09:30:00Z')
})
