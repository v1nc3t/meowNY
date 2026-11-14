import assert from 'node:assert/strict'
import { test } from 'node:test'
import { Account, AccountLine, AccountStatement, AccountsSummary, HistoryPoint } from './accounts.ts'
import { Preview } from './dataExport.ts'
import { Entry, Line } from './debts.ts'
import { Recurring } from './recurring.ts'
import { Conflict } from './sync.ts'
import { Transaction } from './transactions.ts'
import { Transfer } from './transfers.ts'

// The shapes of docs/accounts-design.md, section 6, as the app sends them.
const account = {
  id: '2',
  name: 'Credit card',
  kind: 'CREDIT_CARD',
  number: null,
  openingBalance: '-120.00',
  openingDate: '2026-01-01',
  archived: false,
  isMain: false,
  balance: '1520.35',
  changeThisMonth: '-80.10',
  lastActivity: '2026-11-02',
  entries: 42,
}
const line = { date: '2026-11-02', source: 'TRANSACTION', id: '9', what: 'Lidl', other: 'Groceries', amount: '-12.50', balance: '887.50' }
const ok = (schema: { safeParse: (value: unknown) => { success: boolean } }, value: unknown) => schema.safeParse(value).success

test('an account as the app sends it is accepted', () => {
  assert.deepEqual(Account.parse(account), account)
  assert.ok(ok(Account, { ...account, number: 'DE00 1234', lastActivity: null, entries: 0, balance: '0.00', isMain: true }))
  for (const kind of ['CHECKING', 'SAVINGS', 'CASH', 'CREDIT_CARD']) assert.ok(ok(Account, { ...account, kind }), kind)
})

test('an account with a wrong shape is refused', () => {
  const bad: [string, unknown][] = [
    ['a number where money is text', { ...account, balance: 1520.35 }],
    ['an opening balance as a number', { ...account, openingBalance: -120 }],
    ['one decimal', { ...account, changeThisMonth: '-80.1' }],
    ['a plus', { ...account, balance: '+1.00' }],
    ['an id as a number', { ...account, id: 2 }],
    ['an id of zero', { ...account, id: '0' }],
    ['an unknown kind', { ...account, kind: 'LOAN' }],
    ['a kind in small letters', { ...account, kind: 'cash' }],
    ['archived as 0', { ...account, archived: 0 }],
    ['a date with a time', { ...account, openingDate: '2026-01-01T00:00:00Z' }],
    ['entries as text', { ...account, entries: '42' }],
    ['entries below zero', { ...account, entries: -1 }],
    ['no main flag', { ...account, isMain: undefined }],
    ['a number left out', (({ number: _number, ...rest }) => rest)(account)],
    ['nothing', null],
  ]
  for (const [why, value] of bad) assert.equal(ok(Account, value), false, why)
})

test('the summary, a statement and the history are accepted, and refused with numbers for money', () => {
  const summary = { inAccounts: '5210.00', friends: '50.00', netWorth: '5260.00', accounts: 3, inUse: true }
  assert.ok(ok(AccountsSummary, summary))
  assert.ok(ok(AccountsSummary, { ...summary, friends: '-50.00', netWorth: '-10.00', inUse: false }))
  assert.equal(ok(AccountsSummary, { ...summary, netWorth: 5260 }), false)
  assert.equal(ok(AccountsSummary, { ...summary, inUse: 'yes' }), false)

  const statement = { account, from: '2026-11-01', to: '2026-11-30', balanceBefore: '900.00', lines: [line], balanceAfter: '1520.35' }
  assert.ok(ok(AccountStatement, statement))
  assert.ok(ok(AccountStatement, { ...statement, lines: [] }))
  for (const source of ['OPENING', 'TRANSACTION', 'TRANSFER_IN', 'TRANSFER_OUT', 'LEDGER']) assert.ok(ok(AccountLine, { ...line, source, other: null, what: '' }), source)
  assert.equal(ok(AccountLine, { ...line, source: 'BILL' }), false)
  assert.equal(ok(AccountLine, { ...line, amount: -12.5 }), false)
  assert.equal(ok(AccountLine, { ...line, id: 9 }), false)
  assert.equal(ok(AccountStatement, { ...statement, balanceBefore: 900 }), false)
  assert.equal(ok(AccountStatement, { ...statement, lines: [{ ...line, balance: null }] }), false)

  assert.ok(ok(HistoryPoint, { month: '2026-10', balance: '1520.35' }))
  assert.ok(ok(HistoryPoint, { month: '2026-01', balance: '-3.00' }))
  assert.equal(ok(HistoryPoint, { month: '2026-13', balance: '1.00' }), false)
  assert.equal(ok(HistoryPoint, { month: '2026-10-31', balance: '1.00' }), false)
  assert.equal(ok(HistoryPoint, { month: '2026-10', balance: 1 }), false)
})

test('a transfer is accepted, and refused when its amount is a number, signed or zero-padded wrong', () => {
  const transfer = { id: '7', fromAccountId: '1', toAccountId: '2', amount: '200.00', date: '2026-11-03', note: null }
  assert.deepEqual(Transfer.parse(transfer), transfer)
  assert.ok(ok(Transfer, { ...transfer, note: 'to savings\nfor the roof' }))
  assert.equal(ok(Transfer, { ...transfer, amount: 200 }), false)
  assert.equal(ok(Transfer, { ...transfer, amount: '-200.00' }), false, 'a transfer has no sign')
  assert.equal(ok(Transfer, { ...transfer, amount: '200' }), false)
  assert.equal(ok(Transfer, { ...transfer, fromAccountId: 1 }), false)
  assert.equal(ok(Transfer, { ...transfer, toAccountId: null }), false)
  assert.equal(ok(Transfer, { ...transfer, date: '03.11.2026' }), false)
  assert.equal(ok(Transfer, (({ note: _note, ...rest }) => rest)(transfer)), false)
})

test('a transaction, a recurring item and a ledger entry carry their account', () => {
  const transaction = { id: '1', categoryId: '2', accountId: '3', clientUuid: null, name: 'Lidl', amount: '12.50', myShare: '12.50', split: [], paidBy: null, paymentDate: '2026-11-02', description: null }
  assert.ok(ok(Transaction, transaction))
  assert.equal(ok(Transaction, { ...transaction, accountId: 3 }), false)
  assert.equal(ok(Transaction, (({ accountId: _accountId, ...rest }) => rest)(transaction)), false, 'the column is never empty')

  const item = { id: '1', categoryId: '2', accountId: '1', name: 'Rent', description: null, amount: '950.00', startDate: '2025-01-01', intervalUnit: 'MONTH', intervalCount: 1, nextDueDate: '2026-12-01', endDate: null, isActive: true, split: null }
  assert.ok(ok(Recurring, item))
  assert.equal(ok(Recurring, { ...item, accountId: null }), false)

  const entry = { id: '1', personId: '2', kind: 'LENT', amount: '50.00', date: '2026-09-12', note: null, transactionId: null, accountId: '1' }
  assert.ok(ok(Entry, entry))
  assert.equal(ok(Entry, { ...entry, accountId: '' }), false)

  // a line of a friend's statement may or may not say the account
  const debtLine = { date: '2026-09-12', kind: 'LENT', what: '', amount: '50.00', effect: '50.00', balance: '50.00', entryId: '1', transactionId: null, transactionDate: null }
  assert.ok(ok(Line, debtLine))
  assert.ok(ok(Line, { ...debtLine, accountId: '3' }))
  assert.ok(ok(Line, { ...debtLine, accountId: null }))
  assert.equal(ok(Line, { ...debtLine, accountId: 3 }), false)
})

test('a conflict of an account or a transfer is read, with its new fields', () => {
  const conflict = { table: 'accounts', id: '2', title: 'Savings', mineDeleted: false, theirsDeleted: false, fields: [] as { field: string; mine: string; theirs: string }[] }
  for (const field of ['name', 'type', 'number', 'openingBalance', 'openingDate', 'archived']) conflict.fields.push({ field, mine: 'a', theirs: 'b' })
  assert.ok(ok(Conflict, conflict))
  assert.ok(ok(Conflict, { ...conflict, table: 'transfers', fields: ['from', 'to', 'amount', 'date', 'note'].map((field) => ({ field, mine: '', theirs: 'x' })) }))
  assert.ok(ok(Conflict, { ...conflict, table: 'transactions', fields: [{ field: 'account', mine: 'Cash', theirs: 'Savings' }] }))
  assert.equal(ok(Conflict, { ...conflict, table: 'piggy_banks' }), false)
  assert.equal(ok(Conflict, { ...conflict, fields: [{ field: 'iban', mine: '', theirs: '' }] }), false)
})

test('the preview of an export counts accounts and transfers when the app says them', () => {
  const preview = { transactions: 1, firstDate: null, lastDate: null, income: '0.00', expense: '0.00', categories: 0, budgets: 0, recurring: 0, documents: 0, history: 0, people: 0, ledger: 0, splitBills: 0, duePayments: 0, debtHistory: 0 }
  assert.ok(ok(Preview, preview), 'an app from before the accounts')
  assert.deepEqual(Preview.parse({ ...preview, accounts: 5, transfers: 16 }).transfers, 16)
  assert.equal(ok(Preview, { ...preview, accounts: '5' }), false)
  assert.equal(ok(Preview, { ...preview, transfers: -1 }), false)
})
