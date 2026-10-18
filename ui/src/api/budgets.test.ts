import assert from 'node:assert/strict'
import { test } from 'node:test'
import { MONTH } from './budgets.ts'

test('months are YYYY-MM with a real month number', () => {
  assert.ok(MONTH.test('2026-06'))
  assert.ok(!MONTH.test('2026-13'))
  assert.ok(!MONTH.test('2026-6'))
  assert.ok(!MONTH.test('2026-06-01'))
})
