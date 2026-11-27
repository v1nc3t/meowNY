import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { Account } from '../api/accounts.ts'
import { shiftMonth } from '../api/months.ts'
import { biggest, loanFigures, netWorthByMonth, newest, sparkMonths, sparkPoints, topShares } from './dashboardData.ts'

const balances = (...pairs: [string, string][]) => new Map(pairs)

test('net worth by month adds the accounts, each at its own rate', () => {
  const rows = netWorthByMonth(
    [
      { rate: null, balances: balances(['2026-01', '100.00'], ['2026-02', '150.50']) },
      { rate: '5.00', balances: balances(['2026-01', '50.00'], ['2026-02', '25.00']) },
      { rate: null, balances: balances(['2026-01', '-30.00'], ['2026-02', '-30.00']) },
    ],
    ['2026-01', '2026-02'],
  )
  assert.deepEqual(rows, [
    { month: '2026-01', value: '80.00' },
    { month: '2026-02', value: '125.50' },
  ])
})

test('net worth: a debt in another currency stays below 0, a month with no balance counts 0, no accounts is 0', () => {
  assert.deepEqual(netWorthByMonth([{ rate: '2.00', balances: balances(['2026-01', '-10.01']) }], ['2026-01', '2026-02']), [
    { month: '2026-01', value: '-5.01' },
    { month: '2026-02', value: '0.00' },
  ])
  assert.deepEqual(netWorthByMonth([], ['2026-03']), [{ month: '2026-03', value: '0.00' }])
  assert.deepEqual(netWorthByMonth([{ rate: null, balances: balances() }], []), [])
})

test('top shares: the same category in two accounts is added, 0 and below is dropped, the name breaks a tie, the count cuts', () => {
  const shares = [
    { id: '1', name: 'rent', amount: '400.00' },
    { id: '2', name: 'food', amount: '90.00' },
    { id: '2', name: 'food', amount: '10.00' },
    { id: '3', name: 'fun', amount: '100.00' },
    { id: '4', name: 'zero', amount: '0.00' },
    { id: '5', name: 'back', amount: '-5.00' },
  ]
  assert.deepEqual(topShares(shares, 5).map((s) => `${s.name} ${s.amount}`), ['rent 400.00', 'food 100.00', 'fun 100.00'])
  assert.deepEqual(topShares(shares, 1).map((s) => s.id), ['1'])
  assert.deepEqual(topShares(shares, 0), [])
  assert.deepEqual(topShares([], 3), [])
})

const row = (id: string, paymentDate: string, myShare: string) => ({ id, paymentDate, myShare })

test('biggest: the largest cost first, the newest first among equals, a repeated id once', () => {
  const rows = [row('1', '2026-05-01', '10.00'), row('2', '2026-05-03', '10.00'), row('3', '2026-05-02', '99.99'), row('2', '2026-05-03', '10.00'), row('4', '2026-05-03', '10.00')]
  assert.deepEqual(biggest(rows, 10).map((r) => r.id), ['3', '4', '2', '1'])
  assert.deepEqual(biggest(rows, 2).map((r) => r.id), ['3', '4'])
  assert.deepEqual(biggest(rows, -1), [])
  assert.deepEqual(biggest([], 3), [])
})

test('newest: by day, then the one made last, a repeated id once', () => {
  const rows = [row('5', '2026-05-01', '1.00'), row('9', '2026-05-02', '1.00'), row('7', '2026-05-02', '1.00'), row('9', '2026-05-02', '1.00'), row('1', '2025-12-31', '1.00')]
  assert.deepEqual(newest(rows, 10).map((r) => r.id), ['9', '7', '5', '1'])
  assert.deepEqual(newest(rows, 3).map((r) => r.id), ['9', '7', '5'])
  assert.deepEqual(newest([], 3), [])
})

test('sparkline months end with the range, never after today, across a year end', () => {
  assert.deepEqual(sparkMonths('2026-02', '2026-11', 4, shiftMonth), ['2025-11', '2025-12', '2026-01', '2026-02'])
  assert.deepEqual(sparkMonths('2026-12', '2026-11', 3, shiftMonth), ['2026-09', '2026-10', '2026-11'])
})

test('sparkline points: first at the left, last at the right, highest at the top; flat and single runs sit in the middle', () => {
  assert.deepEqual(sparkPoints([0, 5, 10], 104, 24, 2), [[2, 22], [52, 12], [102, 2]])
  assert.deepEqual(sparkPoints([7, 7, 7], 100, 20, 0), [[0, 10], [50, 10], [100, 10]])
  assert.deepEqual(sparkPoints([3], 100, 20), [[50, 10]])
  assert.deepEqual(sparkPoints([], 100, 20), [])
  for (const [x, y] of sparkPoints([-5, 3, -1], 2, 2, 5)) assert.ok(Number.isFinite(x) && Number.isFinite(y))
})

const account = (over: object): Account =>
  ({ kind: 'LOAN', archived: false, rate: null, installment: null, liability: { owed: '100.00', state: 'OWING', interestPerMonth: null, payoff: null }, ...over }) as Account

test('loan figures: the installments add up in the app currency, the payoff is the latest month, "never" wins, loans not owing or archived are left out', () => {
  const owing = (installment: string | null, payoff: object | null, extra: object = {}) =>
    account({ installment, liability: { owed: '100.00', state: 'OWING', interestPerMonth: null, payoff }, ...extra })
  const both = loanFigures([owing('100.00', { months: 5, interestLeft: '1.00', month: '2027-04' }), owing('50.00', { months: 9, interestLeft: '1.00', month: '2027-08' }, { rate: '5.00' })])
  assert.deepEqual(both, { installment: '110.00', payoff: { month: '2027-08' } })
  assert.deepEqual(loanFigures([owing('10.00', { never: true }), owing('10.00', { months: 1, interestLeft: '0.00', month: '2030-01' })]).payoff, { never: true })
  assert.deepEqual(loanFigures([owing('10.00', { months: 1, interestLeft: '0.00', month: '2030-01' }), owing('10.00', { never: true })]).payoff, { never: true })
  const paid = account({ liability: { owed: '0.00', state: 'PAID_OFF', interestPerMonth: null, payoff: null }, installment: '40.00' })
  const away = owing('40.00', null, { archived: true })
  const asset = account({ kind: 'CHECKING', installment: null, liability: null })
  assert.deepEqual(loanFigures([paid, away, asset]), { installment: '0.00', payoff: null })
  assert.deepEqual(loanFigures([owing(null, null)]), { installment: '0.00', payoff: null })
})
