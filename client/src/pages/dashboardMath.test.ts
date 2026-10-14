import assert from 'node:assert/strict'
import test from 'node:test'
import { changePercent, niceCeil, usedPercent } from './dashboardMath.ts'

test('percent of budget used matches the hand-calculated Food example', () => {
  assert.equal(usedPercent('356.20', '400.00'), 89.1)
  assert.equal(usedPercent('150.00', '100.00'), 150)
  assert.equal(usedPercent('0.00', '400.00'), 0)
  assert.equal(usedPercent('1.00', '3.00'), 33.3)
  assert.equal(usedPercent('2.00', '3.00'), 66.7)
})

test('no budget, or a budget of zero, gives no percentage', () => {
  assert.equal(usedPercent('10.00', null), null)
  assert.equal(usedPercent('10.00', '0.00'), null)
  assert.equal(usedPercent('nonsense', '10.00'), null)
})

test('chart axes top out at a round number', () => {
  assert.equal(niceCeil(0), 1)
  assert.equal(niceCeil(0.7), 1)
  assert.equal(niceCeil(1), 1)
  assert.equal(niceCeil(1.1), 2)
  assert.equal(niceCeil(3), 5)
  assert.equal(niceCeil(5.01), 10)
  assert.equal(niceCeil(900), 1000)
  assert.equal(niceCeil(3600 / 4), 1000)
})

test('change from last month is a percent, or null with nothing to compare', () => {
  assert.equal(changePercent('3600.00', '3200.00'), 12.5)
  assert.equal(changePercent('2380.00', '2500.00'), -4.8)
  assert.equal(changePercent('100.00', '0.00'), null)
  assert.equal(changePercent('100.00', undefined), null)
})
