import assert from 'node:assert/strict'
import { test } from 'node:test'
import { marqueeSeconds } from './marquee.ts'

test('a short way still takes the least time', () => {
  assert.equal(marqueeSeconds(0), 1.5)
  assert.equal(marqueeSeconds(10), 1.5)
})

test('a longer way takes longer, about 40px a second of travel', () => {
  // 280px is 7s of travel, and the text rests for 30% of the leg: 7 / 0.7 = 10s.
  assert.ok(Math.abs(marqueeSeconds(280) - 10) < 1e-9)
  assert.ok(marqueeSeconds(400) > marqueeSeconds(200))
})
