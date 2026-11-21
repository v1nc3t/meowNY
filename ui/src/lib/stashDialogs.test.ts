import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { Account } from '../api/accounts.ts'
import type { Deposit, Stash } from '../api/stashes.ts'
import { aMonthOn, monthsOn, stashBlock, stashFields, stashSubmit, termIsFixed, withKind, type StashFields } from './stashDialogs.ts'

const account = (id: string, more: Partial<Account> = {}): Account => ({
  id,
  name: `Account ${id}`,
  kind: 'CHECKING',
  number: null,
  openingBalance: '0.00',
  openingDate: '1970-01-01',
  archived: false,
  isMain: id === '1',
  balance: '1000.00',
  changeThisMonth: '0.00',
  lastActivity: null,
  entries: 0,
  bic: null,
  notes: null,
  group: null,
  includeInNetWorth: true,
  virtualBalance: '0.00',
  available: '1000.00',
  currency: 'EUR',
  foreign: false,
  rate: null,
  balanceInMain: '1000.00',
  setAside: '0.00',
  free: '1000.00',
  ...more,
})
const accounts = [account('1'), account('3', { kind: 'SAVINGS', free: '250.00' }), account('4', { archived: true })]
const stash: Stash = {
  id: '2',
  name: 'Holiday',
  accountId: '3',
  account: 'Account 3',
  currency: 'EUR',
  archived: false,
  accountArchived: false,
  kind: 'GOAL',
  deposit: null,
  target: '1200.00',
  targetDate: '2027-06-30',
  notes: 'Flights in March.',
  saved: '450.00',
  left: '750.00',
  percent: 37,
  reached: false,
  perMonth: '107.15',
  overdue: false,
  free: '250.00',
  entries: 3,
  lastEntry: '2026-11-02',
}
const typed = (more: Partial<StashFields> = {}): StashFields => ({ ...stashFields(stash, accounts), ...more })
const GOAL = { kind: 'GOAL', rate: '', period: 'YEARLY', startDate: '' } as const
const TODAY = '2026-11-18'
const term: Deposit = { interestRate: '3.1', interestPeriod: 'YEARLY', startDate: '2026-03-12', brokenOn: null, returnedOn: null, state: 'RUNNING', expectedReturn: '31.00', valueAtMaturity: '1031.00', daysLeft: 114 }
const deposit = (state: Deposit['state'] = 'RUNNING', more: Partial<Stash> = {}): Stash => ({ ...stash, name: 'Term', kind: 'DEPOSIT', deposit: { ...term, state }, target: null, targetDate: '2027-03-12', perMonth: null, left: null, percent: null, ...more })

test('a new stash starts empty, in the account of the page it was opened from, else in the account a new row starts with', () => {
  assert.deepEqual(stashFields(null, accounts, '3'), { name: '', account: '3', target: '', targetDate: '', notes: '', ...GOAL })
  assert.equal(stashFields(null, accounts).account, '1', 'no account asked for')
  assert.equal(stashFields(null, accounts, '4').account, '1', 'an archived account is not used')
  assert.equal(stashFields(null, accounts, '99').account, '1', 'an unknown one neither')
  assert.equal(stashFields(null, [], '3').account, '', 'no account at all')
})

test('a stash being changed opens with its own fields', () => {
  assert.deepEqual(stashFields(stash, accounts), { name: 'Holiday', account: '3', target: '1200.00', targetDate: '2027-06-30', notes: 'Flights in March.', ...GOAL })
  assert.deepEqual(stashFields({ ...stash, target: null, targetDate: null, notes: null }, accounts), { name: 'Holiday', account: '3', target: '', targetDate: '', notes: '', ...GOAL })
})

test('a new target below what is saved stops the dialog, exactly what is saved does not, and neither does clearing it or leaving it', () => {
  assert.deepEqual(stashBlock(stash, typed({ target: '449.99' }), accounts), { reason: 'targetBelowSaved', saved: '450.00' })
  assert.equal(stashBlock(stash, typed({ target: '450' }), accounts), null, 'exactly saved: allowed, it is reached')
  assert.equal(stashBlock(stash, typed({ target: '' }), accounts), null, 'cleared')
  assert.equal(stashBlock({ ...stash, saved: '2000.00', target: '1200.00' }, typed({ target: '1200' }), accounts), null, 'the target it has, above it after a sync: not a change')
  assert.equal(stashBlock(stash, typed({ target: 'abc' }), accounts), null, 'a wrong text is a toast on submit, not a block')
  assert.equal(stashBlock({ ...stash, saved: '-10.00' }, typed({ target: '1' }), accounts), null, 'a stash below 0')
  assert.equal(stashBlock(null, typed({ target: '1' }), accounts), null, 'a new stash holds nothing')
})

test('a move to an account stops the dialog when that account has too little free for what the stash holds', () => {
  assert.equal(stashBlock(stash, typed({ account: '1' }), accounts), null, '1000 free, 450 held')
  const low = [account('1', { free: '449.99' }), accounts[1]]
  const block = stashBlock(stash, typed({ account: '1' }), low)
  assert.equal(block?.reason, 'moveLacks')
  assert.deepEqual(block && 'free' in block ? [block.free, block.saved, block.account.id] : null, ['449.99', '450.00', '1'])
  assert.equal(stashBlock(stash, typed({ account: '1' }), [account('1', { free: '450.00' }), accounts[1]]), null, 'exactly')
  assert.equal(stashBlock({ ...stash, saved: '0.00' }, typed({ account: '1' }), [account('1', { free: '0.00' })]), null, 'a stash that holds 0 moves to an account with nothing free')
  assert.equal(stashBlock({ ...stash, saved: '-10.00' }, typed({ account: '1' }), [account('1', { free: '0.00' })]), null, 'so does one below 0: it holds nothing')
  // a stash that holds 0 moves even to an account that is over-booked (section 5.4); one that holds money does not
  assert.equal(stashBlock({ ...stash, saved: '0.00' }, typed({ account: '1' }), [account('1', { free: '-5.00' })]), null)
  assert.equal(stashBlock(stash, typed({ account: '1' }), [account('1', { free: '-5.00' }), accounts[1]])?.reason, 'moveLacks')
  assert.equal(stashBlock(stash, typed(), accounts), null, 'its own account is not a move')
})

test('a submit makes a new stash with only what was filled in', () => {
  const fresh = (more: Partial<StashFields> = {}): StashFields => ({ name: '  Laptop  ', account: '1', target: '900', targetDate: '', notes: '', ...GOAL, ...more })
  assert.deepEqual(stashSubmit('EDITING', null, fresh(), null), { do: 'send', request: { create: { name: 'Laptop', accountId: '1', target: '900.00' } } })
  assert.deepEqual(stashSubmit('EDITING', null, fresh({ target: '', notes: '\n ' }), null), { do: 'send', request: { create: { name: 'Laptop', accountId: '1' } } }, 'no target, no notes')
  assert.deepEqual(stashSubmit('EDITING', null, fresh({ targetDate: '2027-06-30', notes: 'for work\nand play', target: '1,5' }), null), {
    do: 'send',
    request: { create: { name: 'Laptop', accountId: '1', target: '1.50', targetDate: '2027-06-30', notes: 'for work\nand play' } },
  })
})

test('a submit refuses a wrong name, account, target, date or notes, and sends nothing', () => {
  const fresh = (more: Partial<StashFields> = {}): StashFields => ({ name: 'Laptop', account: '1', target: '', targetDate: '', notes: '', ...GOAL, ...more })
  assert.deepEqual(stashSubmit('EDITING', null, fresh({ name: '   ' }), null), { do: 'refuse', error: 'stashes.nameError' })
  assert.deepEqual(stashSubmit('EDITING', null, fresh({ name: 'x'.repeat(51) }), null), { do: 'refuse', error: 'stashes.nameError' })
  assert.equal(stashSubmit('EDITING', null, fresh({ name: 'x'.repeat(50) }), null).do, 'send')
  assert.deepEqual(stashSubmit('EDITING', null, fresh({ name: 'a\u0007b' }), null), { do: 'refuse', error: 'stashes.nameError' }, 'a control character')
  assert.deepEqual(stashSubmit('EDITING', null, fresh({ account: '' }), null), { do: 'refuse', error: 'stashes.accountError' })
  for (const target of ['0', '0.00', '-5', '1.234', 'abc', '10000000000']) assert.deepEqual(stashSubmit('EDITING', null, fresh({ target }), null), { do: 'refuse', error: 'stashes.targetError' }, target)
  assert.deepEqual(stashSubmit('EDITING', null, fresh({ targetDate: '30.06.2027' }), null), { do: 'refuse', error: 'common.invalidDate' })
  assert.deepEqual(stashSubmit('EDITING', null, fresh({ notes: 'x'.repeat(1001) }), null), { do: 'refuse', error: 'stashes.notesError' })
  assert.equal(stashSubmit('EDITING', null, fresh({ notes: 'x'.repeat(1000) }), null).do, 'send')
  assert.equal(stashSubmit('EDITING', null, fresh({ targetDate: '2020-01-01' }), null).do, 'send', 'a past target date is allowed: the stash is then overdue')
})

test('a submit does nothing while a request is out, while the dialog is gone, or while it is stopped', () => {
  const fields = typed({ name: 'Trip' })
  assert.deepEqual(stashSubmit('SAVING', stash, fields, null), { do: 'ignore' })
  assert.deepEqual(stashSubmit('CLOSED', stash, fields, null), { do: 'ignore' })
  assert.deepEqual(stashSubmit('EDITING', stash, fields, { reason: 'targetBelowSaved', saved: '450.00' }), { do: 'ignore' })
})

test('a stash being changed sends only what is different, and nothing different just closes', () => {
  assert.deepEqual(stashSubmit('EDITING', stash, typed(), null), { do: 'close' })
  assert.deepEqual(stashSubmit('EDITING', stash, typed({ name: '  Holiday  ', target: '1200' }), null), { do: 'close' }, 'typed the same way')
  assert.deepEqual(stashSubmit('EDITING', stash, typed({ name: 'Trip' }), null), { do: 'send', request: { patch: { name: 'Trip' } } })
  assert.deepEqual(stashSubmit('EDITING', stash, typed({ account: '1' }), null), { do: 'send', request: { patch: { accountId: '1' } } })
  assert.deepEqual(stashSubmit('EDITING', stash, typed({ target: '1500' }), null), { do: 'send', request: { patch: { target: '1500.00' } } })
  assert.deepEqual(stashSubmit('EDITING', stash, typed({ target: '' }), null), { do: 'send', request: { patch: { target: null } } }, 'cleared')
  assert.deepEqual(stashSubmit('EDITING', stash, typed({ targetDate: '' }), null), { do: 'send', request: { patch: { targetDate: null } } })
  assert.deepEqual(stashSubmit('EDITING', stash, typed({ targetDate: '2027-07-31' }), null), { do: 'send', request: { patch: { targetDate: '2027-07-31' } } })
  assert.deepEqual(stashSubmit('EDITING', stash, typed({ notes: '' }), null), { do: 'send', request: { patch: { notes: null } } })
  assert.deepEqual(stashSubmit('EDITING', stash, typed({ notes: 'New notes' }), null), { do: 'send', request: { patch: { notes: 'New notes' } } })
  assert.deepEqual(stashSubmit('EDITING', stash, typed({ name: 'Trip', target: '', notes: '' }), null), { do: 'send', request: { patch: { name: 'Trip', target: null, notes: null } } })
  // a stash that has none of them, and still has none
  const bare = { ...stash, target: null, targetDate: null, notes: null }
  assert.deepEqual(stashSubmit('EDITING', bare, { ...stashFields(bare, accounts), notes: '  ' }, null), { do: 'close' })
})

test('a target date starts a month on, held to the last day of that month', () => {
  assert.equal(aMonthOn('2026-11-18'), '2026-12-18')
  assert.equal(aMonthOn('2026-12-31'), '2027-01-31')
  assert.equal(aMonthOn('2027-01-31'), '2027-02-28')
  assert.equal(aMonthOn('2028-01-31'), '2028-02-29', 'a leap year')
})

// -- a deposit (docs/loans-deposits-design.md, sections 5.3 and 5.6) ---------------------------------------------------------------------------------

test('a new stash starts as a goal, starting today; the switch to a deposit puts a maturity a year on unless a date is there', () => {
  const fresh = stashFields(null, accounts, '3', TODAY)
  assert.deepEqual([fresh.kind, fresh.startDate, fresh.period, fresh.rate], ['GOAL', TODAY, 'YEARLY', ''])
  assert.deepEqual(withKind(fresh, 'DEPOSIT'), { ...fresh, kind: 'DEPOSIT', targetDate: '2027-11-18' })
  assert.equal(withKind({ ...fresh, targetDate: '2027-02-01' }, 'DEPOSIT').targetDate, '2027-02-01', 'a date already chosen stays')
  assert.equal(withKind(withKind(fresh, 'DEPOSIT'), 'GOAL').targetDate, '2027-11-18', 'and goes back with the switch: it is the target date of a goal then')
  assert.equal(withKind({ ...fresh, startDate: '' }, 'DEPOSIT').targetDate, '', 'no start, no guess')
})

test('a deposit being changed opens with its rate, period and start, and a broken one keeps them', () => {
  const fields = stashFields(deposit(), accounts)
  assert.deepEqual([fields.kind, fields.rate, fields.period, fields.startDate, fields.targetDate], ['DEPOSIT', '3.1', 'YEARLY', '2026-03-12', '2027-03-12'])
  assert.equal(termIsFixed(deposit('BROKEN')), true)
  for (const state of ['RUNNING', 'MATURED', 'BOOKED'] as const) assert.equal(termIsFixed(deposit(state)), false, state)
  assert.equal(termIsFixed(stash), false)
  assert.equal(termIsFixed(null), false)
})

test('a deposit is made with its rate, period, start and maturity, and its target stays optional', () => {
  const fresh = (more: Partial<StashFields> = {}): StashFields => ({ name: 'Term', account: '3', target: '', targetDate: '2027-11-18', notes: '', kind: 'DEPOSIT', rate: '3,10', period: 'YEARLY', startDate: TODAY, ...more })
  assert.deepEqual(stashSubmit('EDITING', null, fresh(), null), {
    do: 'send',
    request: { create: { name: 'Term', accountId: '3', kind: 'DEPOSIT', interestRate: '3.1', interestPeriod: 'YEARLY', startDate: TODAY, targetDate: '2027-11-18' } },
  })
  const monthly = stashSubmit('EDITING', null, fresh({ period: 'MONTHLY', rate: '0.25', target: '1000', notes: 'bank' }), null)
  assert.deepEqual(monthly, {
    do: 'send',
    request: { create: { name: 'Term', accountId: '3', kind: 'DEPOSIT', interestRate: '0.25', interestPeriod: 'MONTHLY', startDate: TODAY, target: '1000.00', targetDate: '2027-11-18', notes: 'bank' } },
  })
  // the goal tab sends none of it, whatever was typed in the deposit's fields before
  assert.deepEqual(stashSubmit('EDITING', null, fresh({ kind: 'GOAL', targetDate: '' }), null), { do: 'send', request: { create: { name: 'Term', accountId: '3' } } })
})

test('a deposit is refused without a rate from 0 to 100 with at most three decimals, or with a maturity that is not after its start', () => {
  const fresh = (more: Partial<StashFields> = {}): StashFields => ({ name: 'Term', account: '3', target: '', targetDate: '2027-11-18', notes: '', kind: 'DEPOSIT', rate: '3', period: 'YEARLY', startDate: TODAY, ...more })
  for (const rate of ['', 'abc', '100.001', '3.1415', '-1', '101', '3.']) assert.deepEqual(stashSubmit('EDITING', null, fresh({ rate }), null), { do: 'refuse', error: 'accounts.interestError' }, rate)
  for (const rate of ['0', '100', '100.000', '0.001']) assert.equal(stashSubmit('EDITING', null, fresh({ rate }), null).do, 'send', rate)
  assert.deepEqual(stashSubmit('EDITING', null, fresh({ targetDate: '' }), null), { do: 'refuse', error: 'stashes.maturityError' }, 'a deposit needs its maturity')
  assert.deepEqual(stashSubmit('EDITING', null, fresh({ targetDate: TODAY }), null), { do: 'refuse', error: 'stashes.maturityError' }, 'the same day')
  assert.deepEqual(stashSubmit('EDITING', null, fresh({ targetDate: '2026-11-17' }), null), { do: 'refuse', error: 'stashes.maturityError' }, 'before')
  assert.equal(stashSubmit('EDITING', null, fresh({ targetDate: '2026-11-19' }), null).do, 'send', 'the day after')
  assert.deepEqual(stashSubmit('EDITING', null, fresh({ startDate: '18.11.2026' }), null), { do: 'refuse', error: 'common.invalidDate' })
  assert.equal(stashSubmit('EDITING', null, fresh({ startDate: '2027-01-01', targetDate: '2027-06-01' }), null).do, 'send', 'a start in the future is allowed')
})

test('a deposit being changed sends only the rate, period, start and maturity that are different', () => {
  const same = stashFields(deposit(), accounts)
  assert.deepEqual(stashSubmit('EDITING', deposit(), same, null), { do: 'close' })
  assert.deepEqual(stashSubmit('EDITING', deposit(), { ...same, rate: '3.10' }, null), { do: 'close' }, 'typed the same way')
  assert.deepEqual(stashSubmit('EDITING', deposit(), { ...same, rate: '3.5' }, null), { do: 'send', request: { patch: { interestRate: '3.5' } } })
  assert.deepEqual(stashSubmit('EDITING', deposit(), { ...same, period: 'MONTHLY' }, null), { do: 'send', request: { patch: { interestPeriod: 'MONTHLY' } } })
  assert.deepEqual(stashSubmit('EDITING', deposit(), { ...same, startDate: '2026-04-01' }, null), { do: 'send', request: { patch: { startDate: '2026-04-01' } } })
  // a roll over: the maturity moves and the start is not sent, so the core starts the next term where the last one ended
  assert.deepEqual(stashSubmit('EDITING', deposit('MATURED'), { ...stashFields(deposit('MATURED'), accounts), targetDate: '2028-03-12' }, null), { do: 'send', request: { patch: { targetDate: '2028-03-12' } } })
  assert.deepEqual(stashSubmit('EDITING', deposit(), { ...same, targetDate: '' }, null), { do: 'refuse', error: 'stashes.maturityError' }, 'a maturity is never cleared')
  assert.deepEqual(stashSubmit('EDITING', deposit(), { ...same, targetDate: '2026-03-12' }, null), { do: 'refuse', error: 'stashes.maturityError' })
})

test('a broken deposit sends no rate, period, start or maturity, and checks none of them; its name, notes and target still change', () => {
  const broken = deposit('BROKEN', { saved: '0.00' })
  const same = stashFields(broken, accounts)
  assert.deepEqual(stashSubmit('EDITING', broken, same, null), { do: 'close' })
  assert.deepEqual(stashSubmit('EDITING', broken, { ...same, rate: 'abc', period: 'MONTHLY', startDate: '', name: 'Closed' }, null), { do: 'send', request: { patch: { name: 'Closed' } } })
  assert.deepEqual(stashSubmit('EDITING', broken, { ...same, notes: 'broken in march', target: '500' }, null), { do: 'send', request: { patch: { target: '500.00', notes: 'broken in march' } } })
})

test('months on: held to the last day of the month, across years', () => {
  assert.equal(monthsOn('2026-11-18', 12), '2027-11-18')
  assert.equal(monthsOn('2028-02-29', 12), '2029-02-28', 'a leap day a year on')
  assert.equal(monthsOn('2026-01-31', 1), '2026-02-28')
  assert.equal(monthsOn('2026-12-31', 2), '2027-02-28')
  assert.equal(monthsOn('2026-03-15', 0), '2026-03-15')
})
