import assert from 'node:assert/strict'
import { test } from 'node:test'
import { monthsOf, previousRange, resolvePeriod } from './dashboardPeriod.ts'

test('this month is the month the dashboard shows, to its last day, also on a leap day', () => {
  assert.deepEqual(resolvePeriod(undefined, 'thisMonth', '2028-02'), { from: '2028-02-01', to: '2028-02-29' })
  assert.deepEqual(resolvePeriod({ kind: 'thisMonth' }, 'thisYear', '2027-02'), { from: '2027-02-01', to: '2027-02-28' })
  assert.deepEqual(resolvePeriod({ kind: 'thisMonth' }, 'thisMonth', '2026-12'), { from: '2026-12-01', to: '2026-12-31' })
})

test('last month crosses the end of the year and the leap day', () => {
  assert.deepEqual(resolvePeriod({ kind: 'lastMonth' }, 'thisMonth', '2026-01'), { from: '2025-12-01', to: '2025-12-31' })
  assert.deepEqual(resolvePeriod({ kind: 'lastMonth' }, 'thisMonth', '2028-03'), { from: '2028-02-01', to: '2028-02-29' })
})

test('years and the last twelve months', () => {
  assert.deepEqual(resolvePeriod({ kind: 'thisYear' }, 'thisMonth', '2026-05'), { from: '2026-01-01', to: '2026-12-31' })
  assert.deepEqual(resolvePeriod({ kind: 'lastYear' }, 'thisMonth', '2026-05'), { from: '2025-01-01', to: '2025-12-31' })
  assert.deepEqual(resolvePeriod({ kind: 'last12' }, 'thisMonth', '2026-01'), { from: '2025-02-01', to: '2026-01-31' })
  assert.equal(monthsOf(resolvePeriod({ kind: 'last12' }, 'thisMonth', '2028-02')).length, 12)
})

test('a custom range is whole months, turned the right way round; a broken one is this month', () => {
  const custom = { kind: 'custom', from: '2026-03-15', to: '2026-05-02' } as const
  assert.deepEqual(resolvePeriod(custom, 'thisMonth', '2026-11'), { from: '2026-03-01', to: '2026-05-31' })
  assert.deepEqual(resolvePeriod({ kind: 'custom', from: '2026-05-31', to: '2026-02-01' }, 'thisMonth', '2026-11'), { from: '2026-02-01', to: '2026-05-31' })
  assert.deepEqual(resolvePeriod({ kind: 'custom', from: '2026-13-01', to: '2026-02-01' }, 'thisMonth', '2028-02'), { from: '2028-02-01', to: '2028-02-29' })
  assert.deepEqual(resolvePeriod({ kind: 'custom' }, 'thisMonth', '2026-11'), { from: '2026-11-01', to: '2026-11-30' })
})

test('the range before is as long, across a year end', () => {
  assert.deepEqual(previousRange({ from: '2026-01-01', to: '2026-01-31' }), { from: '2025-12-01', to: '2025-12-31' })
  assert.deepEqual(previousRange({ from: '2026-01-01', to: '2026-12-31' }), { from: '2025-01-01', to: '2025-12-31' })
  assert.deepEqual(previousRange({ from: '2026-03-01', to: '2026-04-30' }), { from: '2026-01-01', to: '2026-02-28' })
})
