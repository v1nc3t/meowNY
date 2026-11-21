import assert from 'node:assert/strict'
import { test } from 'node:test'
import { Account } from './accounts.ts'
import { Preview } from './dataExport.ts'
import { Conflict } from './sync.ts'
import { Detail, Entry, HistoryPoint, Stash, Summary } from './stashes.ts'

// The shapes of docs/stash-design.md, section 6.1, as the app sends them.
const stash = {
  id: '2',
  name: 'Holiday in Lisbon',
  accountId: '3',
  account: 'Savings',
  currency: 'EUR',
  archived: false,
  accountArchived: false,
  target: '1200.00',
  targetDate: '2027-06-30',
  notes: 'Flights in March.\nHotel later.',
  saved: '450.00',
  left: '750.00',
  percent: 37,
  reached: false,
  perMonth: '107.15',
  overdue: false,
  free: '1220.35',
  entries: 12,
  lastEntry: '2026-11-02',
}
// a stash with no target, no date and no notes
const plain = { ...stash, id: '4', name: 'Rainy day', target: null, targetDate: null, notes: null, left: null, percent: null, perMonth: null, entries: 0, lastEntry: null, saved: '0.00' }
const entry = {
  id: '9',
  stashId: '2',
  kind: 'ADD',
  amount: '100.00',
  date: '2026-11-02',
  note: 'november',
  transferId: '7',
  otherAccountId: '1',
  otherAccount: 'Current account',
  savedAfter: '450.00',
}
const ok = (schema: { safeParse: (value: unknown) => { success: boolean } }, value: unknown) => schema.safeParse(value).success

test('a stash as the app sends it is accepted, with and without a target', () => {
  assert.deepEqual(Stash.parse(stash), stash)
  assert.deepEqual(Stash.parse(plain), plain)
  assert.ok(ok(Stash, { ...stash, archived: true, accountArchived: true }))
  assert.ok(ok(Stash, { ...stash, currency: 'RON' }), 'a stash of an account in lei')
  assert.ok(ok(Stash, { ...stash, overdue: true, perMonth: null, targetDate: '2026-06-30' }), 'a past date makes it overdue')
})

test('what a sync may leave is accepted: saved below 0, above the target, free below 0', () => {
  assert.ok(ok(Stash, { ...stash, saved: '-100.00', percent: 0, left: '1300.00', free: '-20.50' }), 'two devices each took out the last 100')
  assert.ok(ok(Stash, { ...stash, saved: '1440.00', percent: 120, left: '0.00', reached: true, perMonth: null }), 'two devices each added 300')
  assert.ok(ok(Stash, { ...stash, percent: 100000 }), 'a percent has no ceiling')
})

test('a stash with a wrong shape is refused', () => {
  const bad: [string, unknown][] = [
    ['saved as a number', { ...stash, saved: 450 }],
    ['saved with one decimal', { ...stash, saved: '450.0' }],
    ['saved with a plus', { ...stash, saved: '+450.00' }],
    ['a target below 0', { ...stash, target: '-1200.00' }],
    ['a target as a number', { ...stash, target: 1200 }],
    ['left below 0', { ...stash, left: '-1.00' }],
    ['per month with one decimal', { ...stash, perMonth: '107.1' }],
    ['free as a number', { ...stash, free: 1220.35 }],
    ['an id as a number', { ...stash, id: 2 }],
    ['an id of zero', { ...stash, id: '0' }],
    ['an account id as a number', { ...stash, accountId: 3 }],
    ['a percent as text', { ...stash, percent: '37' }],
    ['a percent with decimals', { ...stash, percent: 37.5 }],
    ['a percent below 0', { ...stash, percent: -1 }],
    ['archived as 0', { ...stash, archived: 0 }],
    ['the account archived as text', { ...stash, accountArchived: 'no' }],
    ['a date with a time', { ...stash, targetDate: '2027-06-30T00:00:00Z' }],
    ['a currency as two letters', { ...stash, currency: 'EU' }],
    ['entries as text', { ...stash, entries: '12' }],
    ['entries below 0', { ...stash, entries: -1 }],
    ['notes as a list', { ...stash, notes: ['a'] }],
    ['no account name', (({ account: _account, ...rest }) => rest)(stash)],
    ['a target left out', (({ target: _target, ...rest }) => rest)(stash)],
    ['a last entry left out', (({ lastEntry: _lastEntry, ...rest }) => rest)(stash)],
    ['nothing', null],
  ]
  for (const [why, value] of bad) assert.equal(ok(Stash, value), false, why)
})

test('an entry is accepted as an add or a take, with or without a transfer', () => {
  assert.deepEqual(Entry.parse(entry), entry)
  const take = { ...entry, id: '10', kind: 'TAKE', amount: '40.00', note: null, transferId: null, otherAccountId: null, otherAccount: null, savedAfter: '410.00' }
  assert.deepEqual(Entry.parse(take), take)
  assert.ok(ok(Entry, { ...entry, savedAfter: '-60.00' }), 'a stash that went below 0 after a sync')
  assert.ok(ok(Entry, { ...entry, note: 'a\nb' }))
  const bad: [string, unknown][] = [
    ['an unknown kind', { ...entry, kind: 'MOVE' }],
    ['a kind in small letters', { ...entry, kind: 'add' }],
    ['an amount with a minus: the kind says which way', { ...entry, amount: '-100.00' }],
    ['an amount as a number', { ...entry, amount: 100 }],
    ['one decimal', { ...entry, amount: '100.0' }],
    ['a transfer id as a number', { ...entry, transferId: 7 }],
    ['a transfer id of zero', { ...entry, transferId: '0' }],
    ['the other account id as a number', { ...entry, otherAccountId: 1 }],
    ['a date with a time', { ...entry, date: '2026-11-02T10:00:00Z' }],
    ['saved after as a number', { ...entry, savedAfter: 450 }],
    ['no note field', (({ note: _note, ...rest }) => rest)(entry)],
    ['no stash id', (({ stashId: _stashId, ...rest }) => rest)(entry)],
    ['the transfer left out', (({ transferId: _transferId, ...rest }) => rest)(entry)],
  ]
  for (const [why, value] of bad) assert.equal(ok(Entry, value), false, why)
})

test('a detail is a stash and its entries, also with none', () => {
  assert.deepEqual(Detail.parse({ stash, entries: [entry] }), { stash, entries: [entry] })
  assert.ok(ok(Detail, { stash: plain, entries: [] }))
  assert.equal(ok(Detail, { stash, entries: [{ ...entry, amount: 100 }] }), false)
  assert.equal(ok(Detail, { stash, entries: null }), false)
  assert.equal(ok(Detail, { entries: [] }), false)
})

test('the history of a year and the summary are accepted, and refused with numbers for money', () => {
  assert.ok(ok(HistoryPoint, { month: '2026-10', saved: '450.00' }))
  assert.ok(ok(HistoryPoint, { month: '2026-01', saved: '-3.00' }))
  assert.equal(ok(HistoryPoint, { month: '2026-13', saved: '1.00' }), false)
  assert.equal(ok(HistoryPoint, { month: '2026-10-31', saved: '1.00' }), false)
  assert.equal(ok(HistoryPoint, { month: '2026-10', saved: 1 }), false)
  assert.equal(ok(HistoryPoint, { month: '2026-10', balance: '1.00' }), false, 'the field is saved, not balance')

  const summary = { stashes: 3, saved: '1850.00', target: '4200.00', reached: 1 }
  assert.deepEqual(Summary.parse(summary), summary)
  assert.ok(ok(Summary, { stashes: 0, saved: '0.00', target: '0.00', reached: 0 }))
  assert.equal(ok(Summary, { ...summary, saved: '-1.00' }), false, 'what the stashes hold is never below 0')
  assert.equal(ok(Summary, { ...summary, saved: 1850 }), false)
  assert.equal(ok(Summary, { ...summary, stashes: '3' }), false)
  assert.equal(ok(Summary, { ...summary, reached: -1 }), false)
})

test('an account carries what is set aside and what is free', () => {
  const account = {
    id: '3', name: 'Savings', kind: 'SAVINGS', number: null, openingBalance: '4000.00', openingDate: '2026-08-01', archived: false, isMain: false,
    balance: '4000.00', changeThisMonth: '0.00', lastActivity: null, entries: 1, bic: null, notes: null, group: null, includeInNetWorth: true,
    virtualBalance: '0.00', available: '4000.00', currency: 'EUR', foreign: false, rate: null, balanceInMain: '4000.00', setAside: '450.00', free: '3550.00',
    interestRate: null, interestPeriod: null, installment: null, liability: null, returnPerMonth: null,
  }
  assert.deepEqual(Account.parse(account), account)
  assert.ok(ok(Account, { ...account, setAside: '0.00', free: '4000.00' }), 'nothing set aside')
  assert.ok(ok(Account, { ...account, setAside: '900.00', free: '-50.00' }), 'the stashes hold more than the account has')
  assert.equal(ok(Account, { ...account, setAside: '-1.00' }), false, 'what is set aside is never below 0')
  assert.equal(ok(Account, { ...account, setAside: 450 }), false)
  assert.equal(ok(Account, { ...account, free: '3550.0' }), false)
  assert.equal(ok(Account, (({ setAside: _setAside, ...rest }) => rest)(account)), false, 'the app always says it')
  assert.equal(ok(Account, (({ free: _free, ...rest }) => rest)(account)), false)
})

test('the export preview counts stashes and their entries, and an older app leaves them out', () => {
  const preview = {
    transactions: 1, firstDate: null, lastDate: null, income: '0.00', expense: '0.00', categories: 0, budgets: 0, recurring: 0, documents: 0, history: 0,
    people: 0, ledger: 0, splitBills: 0, duePayments: 0, debtHistory: 0,
  }
  assert.ok(ok(Preview, preview), 'an app from before stashes')
  assert.deepEqual(Preview.parse({ ...preview, stashes: 4, stashEntries: 31 }).stashEntries, 31)
  assert.equal(ok(Preview, { ...preview, stashes: -1 }), false)
  assert.equal(ok(Preview, { ...preview, stashEntries: '31' }), false)
})

test('a conflict of a stash or of one of its entries is read', () => {
  const conflict = { table: 'stashes', id: '2', title: 'Holiday', mineDeleted: false, theirsDeleted: false, fields: [] as { field: string; mine: string; theirs: string }[] }
  for (const field of ['name', 'account', 'target', 'targetDate', 'notes', 'archived']) conflict.fields.push({ field, mine: 'a', theirs: 'b' })
  assert.ok(ok(Conflict, conflict))
  const entryConflict = { ...conflict, table: 'stash_entries', fields: ['stash', 'kind', 'amount', 'date', 'note'].map((field) => ({ field, mine: 'a', theirs: 'b' })) }
  assert.ok(ok(Conflict, entryConflict))
  assert.equal(ok(Conflict, { ...conflict, table: 'piggy_banks' }), false)
  assert.equal(ok(Conflict, { ...conflict, fields: [{ field: 'goal', mine: '', theirs: '' }] }), false)
})
