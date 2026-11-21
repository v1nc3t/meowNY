import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { Account } from '../api/accounts.ts'
import { totalOwed } from './loansView.ts'
import { checkPay, interestPrefill, payAfter, payCategory, payFrom, payFromChoices, paySubmit, payStart, withAmount, withInterest, type PayFields } from './loanPay.ts'

// A mortgage that owes 180000.00 at 585.00 of interest a month, an installment of 950.00; only what the dialog reads is filled in.
const mortgage = {
  id: '7',
  kind: 'MORTGAGE',
  balance: '-180000.00',
  currency: 'EUR',
  foreign: false,
  installment: '950.00',
  liability: { owed: '180000.00', state: 'OWING', interestPerMonth: '585.00', payoff: null },
} as Account
const asset = (id: string, more: Partial<Account> = {}) => ({ id, kind: 'CHECKING', archived: false, isMain: id === '1', currency: 'EUR', foreign: false, ...more }) as Account
const categories = [
  { id: '1', type: 'EXPENSE' as const },
  { id: '9', type: 'INCOME' as const },
]
const TODAY = '2026-11-18'
const fields = (more: Partial<PayFields> = {}): PayFields => ({ from: '1', amount: '950', interest: '585.00', interestTouched: false, category: '1', date: TODAY, note: '', ...more })

test('the interest is at most the amount; all of it is a payment with no principal', () => {
  assert.deepEqual(checkPay(fields({ amount: '500', interest: '585.00' })), { ok: false, reason: 'overAmount' })
  assert.deepEqual(checkPay(fields({ amount: '585', interest: '585.00' })), { ok: true, amount: 58500n, interest: 58500n, principal: 0n })
  assert.deepEqual(checkPay(fields({ amount: '585.01', interest: '585' })), { ok: true, amount: 58501n, interest: 58500n, principal: 1n })
})

test('a category is needed when there is interest, and not otherwise', () => {
  assert.deepEqual(checkPay(fields({ category: '' })), { ok: false, reason: 'category' })
  assert.deepEqual(checkPay(fields({ category: '', interest: '0.00' })), { ok: true, amount: 95000n, interest: 0n, principal: 95000n })
  assert.equal(checkPay(fields({ category: '', interest: '' })).ok, true, 'an empty interest is none')
})

test('no amount, a wrong amount or interest, and no account to pay from, block it', () => {
  assert.deepEqual(checkPay(fields({ amount: '' })), { ok: false, reason: 'amount' })
  assert.deepEqual(checkPay(fields({ amount: '0' })), { ok: false, reason: 'amount' })
  assert.deepEqual(checkPay(fields({ interest: '-5' })), { ok: false, reason: 'interest' })
  assert.deepEqual(checkPay(fields({ from: '' })), { ok: false, reason: 'from' })
})

test('the interest starts as a month of it, never more than the installment, and follows the amount until it is typed', () => {
  assert.equal(interestPrefill(mortgage, '950.00'), '585.00')
  const small = { ...mortgage, installment: '400.00' } as Account
  assert.equal(interestPrefill(small, small.installment ?? ''), '400.00', 'capped at the installment')
  assert.equal(interestPrefill({ liability: { interestPerMonth: null } }, '950.00'), '0.00', 'no rate')
  assert.equal(interestPrefill(mortgage, ''), '0.00', 'no amount yet')
  const opened = payStart(small, [asset('1')], categories, { from: '', category: '' }, TODAY)
  assert.equal(opened.interest, '400.00')
  assert.equal(withAmount(opened, mortgage, '950').interest, '585.00', 'it follows the amount')
  assert.equal(withAmount(withInterest(opened, '10'), mortgage, '950').interest, '10', 'until it is typed')
})

test('no installment: the amount starts empty', () => {
  const none = { ...mortgage, installment: null } as Account
  const opened = payStart(none, [asset('1')], categories, { from: '', category: '' }, TODAY)
  assert.deepEqual([opened.amount, opened.interest], ['', '0.00'])
})

test('a payment above what is owed is allowed: nothing is owed after it, and the state says it was overpaid', () => {
  assert.deepEqual(payAfter('-100.00', 15000n), { owed: 0n, state: 'OVERPAID' })
  assert.deepEqual(payAfter('-100.00', 10000n), { owed: 0n, state: 'PAID_OFF' })
  assert.deepEqual(payAfter('-100.00', 4000n), { owed: 6000n, state: 'OWING' })
  assert.deepEqual(payAfter('5.00', 100n), { owed: 0n, state: 'OVERPAID' }, 'a debt that was overpaid already')
  assert.equal(checkPay(fields({ amount: '999999', interest: '0' })).ok, true)
})

test('the money comes from an asset in the currency of the debt: the last used, else the main account', () => {
  const lei = asset('3', { currency: 'RON', foreign: true })
  const choices = payFromChoices([asset('2'), mortgage, asset('1'), asset('4', { archived: true }), lei, asset('5', { kind: 'LOAN' })], mortgage)
  assert.deepEqual(choices.map((account) => account.id), ['2', '1'])
  assert.equal(payFrom(choices, '2'), '2')
  assert.equal(payFrom(choices, '4'), '1', 'an account that is no choice now: the main account')
  assert.equal(payFrom([asset('2')], ''), '2')
  assert.equal(payFrom([], ''), '')
  assert.equal(payCategory(categories, '9'), '', 'an income category is no choice')
  assert.equal(payCategory(categories, '1'), '1')
})

test('a submit sends the interest and its category only when there is interest, ignores a second one, and refuses what the dialog would not allow', () => {
  assert.deepEqual(paySubmit('EDITING', '7', fields({ note: ' November ' })), {
    do: 'send',
    request: { liabilityId: '7', fromAccountId: '1', amount: '950.00', interest: '585.00', categoryId: '1', date: TODAY, note: 'November' },
  })
  assert.deepEqual(paySubmit('EDITING', '7', fields({ interest: '0', category: '' })), { do: 'send', request: { liabilityId: '7', fromAccountId: '1', amount: '950.00', date: TODAY } })
  assert.deepEqual(paySubmit('SAVING', '7', fields()), { do: 'ignore' })
  assert.deepEqual(paySubmit('EDITING', '7', fields({ amount: '10' })), { do: 'refuse', error: 'loans.interestOver' })
})

test('the total owed counts each debt in the app\'s currency and an overpaid one as nothing', () => {
  const debt = (balanceInMain: string, kind = 'LOAN') => ({ kind, balanceInMain }) as Account
  assert.equal(totalOwed([debt('-180000.00', 'MORTGAGE'), debt('-9000.00'), debt('-20.00')]), '189020.00')
  assert.equal(totalOwed([debt('-100.00'), debt('15.00')]), '100.00', 'an overpaid one does not lower it')
  assert.equal(totalOwed([debt('-24.00'), { kind: 'CHECKING', balanceInMain: '-500.00' } as Account]), '24.00', 'an asset is not a debt')
  assert.equal(totalOwed([]), '0.00')
})
