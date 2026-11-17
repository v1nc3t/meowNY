import assert from 'node:assert/strict'
import { test } from 'node:test'
import { Account, AccountLine, AccountStatement, AccountsSummary, Breakdown, HistoryPoint, Share } from './accounts.ts'
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
  bic: 'COBADEFFXXX',
  notes: 'Pays the streaming plan.\nLimit 1000.',
  group: 'Cards',
  includeInNetWorth: true,
  virtualBalance: '1000.00',
  available: '2520.35',
  // the currency of the account: the app's own for an account in it (docs/accounts-design.md, section 21.4)
  currency: 'EUR',
  foreign: false,
  rate: null,
  balanceInMain: '1520.35',
}
// an account in lei, at 1 EUR = 5.00 RON: its amounts are in lei, and `balanceInMain` is what it is worth in the app's currency
const lei = { ...account, id: '3', name: 'Lei account', currency: 'RON', foreign: true, rate: '5.00', balance: '120.00', balanceInMain: '24.00', virtualBalance: '0.00', available: '120.00' }
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
    ['a virtual balance as a number', { ...account, virtualBalance: 1000 }],
    ['a virtual balance with one decimal', { ...account, virtualBalance: '1000.0' }],
    ['a virtual balance with a plus', { ...account, virtualBalance: '+1.00' }],
    ['available as a number', { ...account, available: 2520.35 }],
    ['net worth as a string', { ...account, includeInNetWorth: 'true' }],
    ['net worth as 1', { ...account, includeInNetWorth: 1 }],
    ['a BIC as a number', { ...account, bic: 123 }],
    ['notes as a list', { ...account, notes: ['a'] }],
    ['a group left undefined', { ...account, group: undefined }],
    ['foreign as a string', { ...account, foreign: 'false' }],
    ['foreign as 1', { ...account, foreign: 1 }],
    ['a rate as a number', { ...lei, rate: 5 }],
    ['a rate with a comma', { ...lei, rate: '5,00' }],
    ['a currency as two letters', { ...account, currency: 'EU' }],
    ['a currency as four letters', { ...account, currency: 'EURO' }],
    ['a currency with a digit', { ...account, currency: 'E1R' }],
    ['a currency as a number', { ...account, currency: 978 }],
    ['the value in the app\'s currency as a number', { ...lei, balanceInMain: 24 }],
    ['the value in the app\'s currency with one decimal', { ...lei, balanceInMain: '24.0' }],
  ]
  for (const [why, value] of bad) assert.equal(ok(Account, value), false, why)
  // the backend always sends the six new fields, so an account without one is refused
  for (const field of ['bic', 'notes', 'group', 'includeInNetWorth', 'virtualBalance', 'available', 'currency', 'foreign', 'rate', 'balanceInMain']) {
    assert.equal(ok(Account, Object.fromEntries(Object.entries(account).filter(([key]) => key !== field))), false, `${field} left out`)
  }
})

test('an account in another currency is accepted, with its rate and its value in the app\'s currency', () => {
  assert.deepEqual(Account.parse(lei), lei)
  assert.ok(ok(Account, { ...lei, rate: '0.0063', balance: '-1.00', balanceInMain: '-158.73' }), 'a rate below 1, a balance below 0')
  assert.ok(ok(Account, { ...lei, rate: '5' }), 'the app writes a rate as it was typed')
  assert.ok(ok(Account, { ...lei, currency: 'ron' }), 'the schema reads the shape; the app stores capitals')
  assert.ok(ok(Account, { ...account, rate: null, foreign: false }), 'an account in the app\'s currency has no rate')
})

test('an account with its new fields is accepted, set or not', () => {
  assert.deepEqual(Account.parse(account), account)
  const plain = { ...account, bic: null, notes: null, group: null, includeInNetWorth: true, virtualBalance: '0.00', available: '1520.35' }
  assert.deepEqual(Account.parse(plain), plain)
  assert.ok(ok(Account, { ...plain, includeInNetWorth: false, virtualBalance: '-250.00', available: '1270.35' }), 'left out of the net worth, a negative virtual balance')
  assert.ok(ok(Account, { ...plain, virtualBalance: '-0.00' }), 'the schema reads the shape; the app never sends this')
  assert.ok(ok(Account, { ...plain, notes: 'a\nb', bic: 'COBADEFFXXX' }))
  assert.ok(ok(Account, { ...plain, balance: '-240.00', virtualBalance: '1000.00', available: '760.00' }))
})

test('the summary, a statement and the history are accepted, and refused with numbers for money', () => {
  const summary = { inAccounts: '5210.00', friends: '50.00', netWorth: '5260.00', accounts: 3, inUse: true }
  assert.ok(ok(AccountsSummary, summary))
  assert.ok(ok(AccountsSummary, { ...summary, friends: '-50.00', netWorth: '-10.00', inUse: false }))
  assert.equal(ok(AccountsSummary, { ...summary, netWorth: 5260 }), false)
  assert.equal(ok(AccountsSummary, { ...summary, inUse: 'yes' }), false)

  const statement = { account, currency: 'EUR', from: '2026-11-01', to: '2026-11-30', balanceBefore: '900.00', lines: [line], balanceAfter: '1520.35' }
  assert.ok(ok(AccountStatement, statement))
  assert.ok(ok(AccountStatement, { ...statement, account: lei, currency: 'RON' }), 'the statement of an account in lei')
  assert.equal(ok(AccountStatement, { ...statement, currency: undefined }), false, 'the statement always says its currency')
  assert.equal(ok(AccountStatement, { ...statement, currency: 'RO' }), false)
  assert.equal(ok(AccountStatement, { ...statement, currency: 642 }), false)
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
  const transfer = { id: '7', fromAccountId: '1', toAccountId: '2', amount: '200.00', toAmount: null, date: '2026-11-03', note: null }
  assert.deepEqual(Transfer.parse(transfer), transfer)
  assert.deepEqual(Transfer.parse({ ...transfer, toAmount: '995.00' }).toAmount, '995.00', 'between two currencies it has the amount that arrived')
  assert.equal(ok(Transfer, { ...transfer, toAmount: 995 }), false)
  assert.equal(ok(Transfer, { ...transfer, toAmount: '995' }), false)
  assert.equal(ok(Transfer, { ...transfer, toAmount: '-995.00' }), false)
  assert.equal(ok(Transfer, (({ toAmount: _toAmount, ...rest }) => rest)(transfer)), false, 'the app always says it, null or not')
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
  const transaction = { id: '1', categoryId: '2', accountId: '3', clientUuid: null, name: 'Lidl', amount: '12.50', nativeAmount: null, myShare: '12.50', split: [], paidBy: null, paymentDate: '2026-11-02', description: null }
  assert.ok(ok(Transaction, transaction))
  // in an account in another currency: what it was there, beside what it was in the app's currency
  assert.equal(Transaction.parse({ ...transaction, nativeAmount: '62.50' }).nativeAmount, '62.50')
  assert.ok(ok(Transaction, { ...transaction, nativeAmount: '0.00' }), 'zero where amount allows it')
  assert.equal(ok(Transaction, { ...transaction, nativeAmount: 62.5 }), false, 'a number where money is text')
  assert.equal(ok(Transaction, { ...transaction, nativeAmount: '62.5' }), false)
  assert.equal(ok(Transaction, { ...transaction, nativeAmount: '-62.50' }), false)
  assert.equal(ok(Transaction, (({ nativeAmount: _nativeAmount, ...rest }) => rest)(transaction)), false, 'the app always says it, null or not')
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
  for (const field of ['name', 'type', 'number', 'openingBalance', 'openingDate', 'archived', 'bic', 'notes', 'group', 'includeNetWorth', 'virtualBalance', 'currency', 'rate']) conflict.fields.push({ field, mine: 'a', theirs: 'b' })
  assert.ok(ok(Conflict, conflict))
  assert.ok(ok(Conflict, { ...conflict, table: 'transfers', fields: ['from', 'to', 'amount', 'toAmount', 'date', 'note'].map((field) => ({ field, mine: '', theirs: 'x' })) }))
  assert.ok(ok(Conflict, { ...conflict, table: 'transactions', fields: [{ field: 'account', mine: 'Cash', theirs: 'Savings' }, { field: 'nativeAmount', mine: '113.74', theirs: '' }] }))
  assert.equal(ok(Conflict, { ...conflict, table: 'piggy_banks' }), false)
  assert.equal(ok(Conflict, { ...conflict, fields: [{ field: 'iban', mine: '', theirs: '' }] }), false)
})

const share = { categoryId: '4', name: 'Groceries', amount: '137.00' }
const breakdown = { currency: 'EUR', from: '2026-11-01', to: '2026-11-30', expenses: [share, { categoryId: '5', name: 'Rent', amount: '0.50' }], income: [{ categoryId: '9', name: 'Salary', amount: '3200.00' }], expenseTotal: '137.50', incomeTotal: '3200.00' }

test('a breakdown as the app sends it is accepted, also an empty one', () => {
  assert.deepEqual(Breakdown.parse(breakdown), breakdown)
  const empty = { currency: 'EUR', from: '2026-11-01', to: '2026-11-30', expenses: [], income: [], expenseTotal: '0.00', incomeTotal: '0.00' }
  assert.deepEqual(Breakdown.parse(empty), empty)
  assert.ok(ok(Breakdown, { ...empty, income: breakdown.income, incomeTotal: '3200.00' }), 'only one side')
  assert.ok(ok(Share, { ...share, name: '' }), 'a name is text; the app may send a deleted category by its name')
  assert.equal(Breakdown.parse({ ...breakdown, currency: 'RON' }).currency, 'RON', 'the breakdown of an account in lei')
})

test('a breakdown with a number for money, or a wrong shape, is refused', () => {
  const bad: [string, unknown][] = [
    ['a total as a number', { ...breakdown, expenseTotal: 137.5 }],
    ['the other total as a number', { ...breakdown, incomeTotal: 3200 }],
    ['a share amount as a number', { ...breakdown, expenses: [{ ...share, amount: 137 }] }],
    ['an amount with one decimal', { ...breakdown, expenses: [{ ...share, amount: '137.0' }] }],
    ['a signed amount', { ...breakdown, expenses: [{ ...share, amount: '-137.00' }] }],
    ['a signed total', { ...breakdown, incomeTotal: '-1.00' }],
    ['a category id as a number', { ...breakdown, income: [{ ...breakdown.income[0], categoryId: 9 }] }],
    ['a category id of zero', { ...breakdown, income: [{ ...breakdown.income[0], categoryId: '0' }] }],
    ['no name', { ...breakdown, expenses: [{ categoryId: '4', amount: '1.00' }] }],
    ['expenses as null', { ...breakdown, expenses: null }],
    ['no income', (({ income: _income, ...rest }) => rest)(breakdown)],
    ['a date with a time', { ...breakdown, from: '2026-11-01T00:00:00Z' }],
    ['no period', (({ from: _from, to: _to, ...rest }) => rest)(breakdown)],
    ['no currency', (({ currency: _currency, ...rest }) => rest)(breakdown)],
    ['a currency of two letters', { ...breakdown, currency: 'EU' }],
    ['a currency as a number', { ...breakdown, currency: 978 }],
    ['nothing', null],
  ]
  for (const [why, value] of bad) assert.equal(ok(Breakdown, value), false, why)
})

test('the preview of an export counts accounts and transfers when the app says them', () => {
  const preview = { transactions: 1, firstDate: null, lastDate: null, income: '0.00', expense: '0.00', categories: 0, budgets: 0, recurring: 0, documents: 0, history: 0, people: 0, ledger: 0, splitBills: 0, duePayments: 0, debtHistory: 0 }
  assert.ok(ok(Preview, preview), 'an app from before the accounts')
  assert.deepEqual(Preview.parse({ ...preview, accounts: 5, transfers: 16 }).transfers, 16)
  assert.equal(ok(Preview, { ...preview, accounts: '5' }), false)
  assert.equal(ok(Preview, { ...preview, transfers: -1 }), false)
})
