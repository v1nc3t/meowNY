import assert from 'node:assert/strict'
import test from 'node:test'
import { changePercent, DONUT, savingsRateText, slicePath, usedPercent } from './dashboardMath.ts'

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

test('change from last month is a percent, or null with nothing to compare', () => {
  assert.equal(changePercent('3600.00', '3200.00'), 12.5)
  assert.equal(changePercent('2380.00', '2500.00'), -4.8)
  assert.equal(changePercent('100.00', '0.00'), null)
  assert.equal(changePercent('100.00', undefined), null)
})

test('the savings rate reads 0% without a rate, and -x% when more was spent than earned', () => {
  assert.equal(savingsRateText(null, 'en-IE'), '0%', 'no income, no rate')
  assert.equal(savingsRateText(0, 'en-IE'), '0%')
  assert.equal(savingsRateText(-0, 'en-IE'), '0%', 'never "-0%"')
  assert.equal(savingsRateText(12.5, 'en-IE'), '12.5%')
  assert.equal(savingsRateText(100, 'en-IE'), '100%')
  assert.equal(savingsRateText(-8.3, 'en-IE'), '-8.3%')
  assert.equal(savingsRateText(-250, 'en-IE'), '-250%')
})

// the numbers of "M x1 y1 A r r rot large sweep x2 y2 L x3 y3 A r r rot large sweep x4 y4 Z", in order
const numbers = (path: string) => path.match(/-?\d+(?:\.\d+)?/g)!.map(Number)

test('a slice has two straight edges that point at the centre, however thin it is', () => {
  for (const [from, to] of [[0, 0.011], [0.3, 0.311], [0.9, 0.911], [0.5, 0.5011], [0.2, 0.7], [0.99, 0.9999]] as const) {
    const n = numbers(slicePath(from, to))
    const outerStart = [n[0]!, n[1]!], outerEnd = [n[7]!, n[8]!], innerEnd = [n[9]!, n[10]!], innerStart = [n[16]!, n[17]!]
    for (const [outside, inside] of [[outerStart, innerStart], [outerEnd, innerEnd]] as const) {
      const [dx, dy] = [outside[0]! - DONUT.center, outside[1]! - DONUT.center]
      const [ex, ey] = [inside[0]! - DONUT.center, inside[1]! - DONUT.center]
      assert.ok(Math.abs(Math.hypot(dx, dy) - DONUT.outer) < 0.01, `${from}-${to}: the outside is on the outer circle`)
      assert.ok(Math.abs(Math.hypot(ex, ey) - DONUT.inner) < 0.01, `${from}-${to}: the inside is on the inner circle`)
      // the outer and the inner end of an edge are on one line through the centre: the cross product of the two is (all but) nothing
      assert.ok(Math.abs(dx * ey - dy * ex) < 0.1, `${from}-${to}: an edge is not straight towards the centre`)
    }
  }
})

test('a slice of more than half the ring takes the long way round, and the whole ring is two circles', () => {
  assert.match(slicePath(0, 0.4), /A47 47 0 0 1/)
  assert.match(slicePath(0, 0.4), /A33 33 0 0 0/)
  assert.match(slicePath(0, 0.6), /A47 47 0 1 1/)
  assert.match(slicePath(0, 0.6), /A33 33 0 1 0/)
  const whole = slicePath(0, 1)
  assert.equal((whole.match(/Z/g) ?? []).length, 2)
  assert.equal(slicePath(0, 0.99995), whole, 'a hair short of whole is whole')
})
