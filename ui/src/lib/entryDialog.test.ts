import assert from 'node:assert/strict'
import { afterEach, beforeEach, test } from 'node:test'
import type { Account } from '../api/accounts.ts'
import type { Category } from '../api/categories.ts'
import type { Transaction } from '../api/transactions.ts'
import {
  accountField,
  categoryChoices,
  changeTab,
  ENTRY_TABS,
  entryOf,
  entryStart,
  entryTabs,
  tabForKey,
  tabOfCategory,
  transactionBody,
  type EntryContext,
  type EntryState,
} from './entryDialog.ts'
import { NOT_SPLIT } from './split.ts'

let data = new Map<string, string>()
beforeEach(() => {
  data = new Map()
  Object.defineProperty(globalThis, 'localStorage', {
    value: { getItem: (key: string) => data.get(key) ?? null, setItem: (key: string, value: string) => void data.set(key, value) },
    configurable: true,
  })
})
afterEach(() => {
  Reflect.deleteProperty(globalThis, 'localStorage')
})

const TODAY = '2026-11-18'
const account = (id: string, name: string, more: Partial<Account> = {}) => ({ id, name, kind: 'CHECKING', archived: false, isMain: id === '1', ...more }) as Account
const main = account('1', 'Main account')
const card = account('2', 'Card')
const cash = account('3', 'Cash')
const old = account('4', 'Old', { archived: true })
const two = [main, card]
const three = [main, card, cash]

const category = (id: string, type: Category['type'], name = `c${id}`): Category => ({ id, name, type })
const groceries = category('1', 'EXPENSE', 'Groceries')
const rent = category('2', 'EXPENSE', 'Rent')
const salary = category('9', 'INCOME', 'Salary')
const gift = category('10', 'INCOME', 'Gift')
const all = [groceries, rent, salary, gift]

const context = (accounts: readonly Account[], more: Partial<EntryContext> = {}): EntryContext => ({ accounts, tabs: entryTabs(accounts, { editing: false, transfer: true }), today: TODAY, ...more })
const transaction = (more: Partial<Transaction> = {}): Transaction => ({
  id: '5',
  categoryId: '1',
  accountId: '2',
  clientUuid: null,
  name: 'Lidl',
  amount: '47.35',
  myShare: '47.35',
  split: [],
  paidBy: null,
  paymentDate: '2026-11-16',
  description: null,
  ...more,
})

// -- the tabs ---------------------------------------------------------------------------------------------------------------------

test('a new entry has expense and income, and transfer too with two accounts that are not archived', () => {
  assert.deepEqual(entryTabs([main], { editing: false, transfer: true }), ['EXPENSE', 'INCOME'])
  assert.deepEqual(entryTabs(two, { editing: false, transfer: true }), ['EXPENSE', 'INCOME', 'TRANSFER'])
  assert.deepEqual(entryTabs(three, { editing: false, transfer: true }), [...ENTRY_TABS])
})

test('archived accounts do not count towards a transfer', () => {
  assert.deepEqual(entryTabs([main, old], { editing: false, transfer: true }), ['EXPENSE', 'INCOME'], 'one active and one archived')
  assert.deepEqual(entryTabs([main, old, account('5', 'Older', { archived: true })], { editing: false, transfer: true }), ['EXPENSE', 'INCOME'])
  assert.deepEqual(entryTabs([old, card, cash], { editing: false, transfer: true }), [...ENTRY_TABS], 'two active')
  assert.deepEqual(entryTabs([], { editing: false, transfer: true }), ['EXPENSE', 'INCOME'], 'no account at all')
  assert.deepEqual(entryTabs(null, { editing: false, transfer: true }), ['EXPENSE', 'INCOME'], 'accounts not read yet')
})

test('the recurring dialog has no transfer tab, whatever the accounts', () => {
  assert.deepEqual(entryTabs(three, { editing: false, transfer: false }), ['EXPENSE', 'INCOME'])
})

test('a row being changed has no tabs', () => {
  assert.deepEqual(entryTabs(three, { editing: true, transfer: true }), [])
  assert.deepEqual(entryTabs([main], { editing: true, transfer: false }), [])
})

test('Alt and a digit choose a tab, by the key and not by the character it types', () => {
  const key = (code: string, more: Partial<Parameters<typeof tabForKey>[1]> = {}) => ({ code, altKey: true, ctrlKey: false, metaKey: false, shiftKey: false, ...more })
  const tabs = ENTRY_TABS
  assert.equal(tabForKey(tabs, key('Digit1')), 'EXPENSE')
  assert.equal(tabForKey(tabs, key('Digit2')), 'INCOME')
  assert.equal(tabForKey(tabs, key('Digit3')), 'TRANSFER')
  assert.equal(tabForKey(['EXPENSE', 'INCOME'], key('Digit3')), null, 'a dialog without a transfer tab')
  assert.equal(tabForKey([], key('Digit1')), null, 'a dialog without tabs')
  assert.equal(tabForKey(tabs, key('Digit4')), null)
  assert.equal(tabForKey(tabs, key('Digit0')), null)
  assert.equal(tabForKey(tabs, key('KeyA')), null)
  assert.equal(tabForKey(tabs, key('Numpad1')), null)
  assert.equal(tabForKey(tabs, key('Digit1', { altKey: false })), null, 'the digit alone types')
  assert.equal(tabForKey(tabs, key('Digit1', { ctrlKey: true })), null)
  assert.equal(tabForKey(tabs, key('Digit1', { metaKey: true })), null)
  assert.equal(tabForKey(tabs, key('Digit1', { shiftKey: true })), null)
})

// -- starting ---------------------------------------------------------------------------------------------------------------------

test('a new entry starts empty, on today, in the account a new row starts with', () => {
  const start = entryStart(context(two))
  assert.deepEqual(start, { tab: 'EXPENSE', name: '', amount: '', date: TODAY, text: '', categoryId: '', account: '1', from: '1', to: '2', split: NOT_SPLIT })
})

test('the account a new row starts with is the last one used', () => {
  data.set('choice.account.last', '3')
  const start = entryStart(context(three))
  assert.deepEqual([start.account, start.from, start.to], ['3', '3', '1'])
})

test('the opener can ask for a tab and an account', () => {
  const start = entryStart(context(three, { accountId: '2' }), 'INCOME')
  assert.equal(start.tab, 'INCOME')
  assert.equal(start.account, '2')
  const move = entryStart(context(three, { accountId: '3' }), 'TRANSFER')
  assert.deepEqual([move.tab, move.from, move.to], ['TRANSFER', '3', '1'], 'a transfer from that account')
})

test('a preset account that is archived or unknown is not used: the default is', () => {
  const accounts = [main, card, old]
  assert.equal(entryStart(context(accounts, { accountId: '4' })).account, '1', 'archived')
  assert.equal(entryStart(context(accounts, { accountId: '99' })).account, '1', 'unknown')
  assert.equal(entryStart(context(accounts, { accountId: '' })).account, '1', 'empty')
  data.set('choice.account.last', '2')
  assert.equal(entryStart(context(accounts, { accountId: '4' })).account, '2', 'the default is the last one used')
  assert.equal(entryStart(context(accounts, { accountId: '1' })).account, '1', 'a good preset wins over the last one used')
})

test('a transfer tab that this dialog does not have falls back to expense', () => {
  assert.equal(entryStart(context([main]), 'TRANSFER').tab, 'EXPENSE', 'one account')
  assert.equal(entryStart(context(three, { tabs: ['EXPENSE', 'INCOME'] }), 'TRANSFER').tab, 'EXPENSE', 'the recurring dialog')
})

test('with one account the account of a new entry is that account', () => {
  const start = entryStart(context([main]), 'INCOME')
  assert.equal(start.account, '1')
  assert.equal(start.to, '', 'and nothing to move money to')
})

// -- a change of tab --------------------------------------------------------------------------------------------------------------

const filled = (tab: EntryState['tab']): EntryState => ({
  tab,
  name: 'Lunch',
  amount: '12,5',
  date: '2026-11-01',
  text: 'with Anna',
  categoryId: '1',
  account: '3',
  from: '3',
  to: '2',
  split: { on: true, draft: NOT_SPLIT.draft, ready: true, touched: true },
})
const ctx = context(three)
const fresh = entryStart(ctx)

test('expense to income keeps the name, the amount, the date, the text and the account, and drops the category and the split', () => {
  assert.deepEqual(changeTab(filled('EXPENSE'), 'INCOME', ctx), { ...fresh, tab: 'INCOME', name: 'Lunch', amount: '12,5', date: '2026-11-01', text: 'with Anna', account: '3' })
})

test('income to expense is the same', () => {
  assert.deepEqual(changeTab(filled('INCOME'), 'EXPENSE', ctx), { ...fresh, tab: 'EXPENSE', name: 'Lunch', amount: '12,5', date: '2026-11-01', text: 'with Anna', account: '3' })
})

test('to a transfer the amount, the date and the text stay as the note; the name, the category, the split and the accounts start again', () => {
  for (const from of ['EXPENSE', 'INCOME'] as const) {
    assert.deepEqual(changeTab(filled(from), 'TRANSFER', ctx), { ...fresh, tab: 'TRANSFER', amount: '12,5', date: '2026-11-01', text: 'with Anna' }, from)
  }
})

test('from a transfer to an expense or an income the same stays, and nothing else comes back', () => {
  for (const to of ['EXPENSE', 'INCOME'] as const) {
    assert.deepEqual(changeTab(filled('TRANSFER'), to, ctx), { ...fresh, tab: to, amount: '12,5', date: '2026-11-01', text: 'with Anna' }, to)
  }
})

test('the amount is kept as it was typed, comma and all', () => {
  for (const amount of ['12,5', '0,', '', ' 7 ', '1.234,5', 'abc']) {
    for (const [from, to] of [['EXPENSE', 'INCOME'], ['INCOME', 'TRANSFER'], ['TRANSFER', 'EXPENSE']] as const) {
      assert.equal(changeTab({ ...filled(from), amount }, to, ctx).amount, amount, `${amount} ${from} ${to}`)
    }
  }
})

test('the same tab, and a tab the dialog does not have, change nothing', () => {
  const state = filled('EXPENSE')
  assert.equal(changeTab(state, 'EXPENSE', ctx), state, 'the same object, so nothing is drawn again')
  assert.equal(changeTab(state, 'TRANSFER', context([main])), state, 'one account: no transfer tab')
  assert.equal(changeTab(state, 'TRANSFER', context(three, { tabs: ['EXPENSE', 'INCOME'] })), state)
})

test('a row being changed never changes tab', () => {
  const editing = context(three, { tabs: entryTabs(three, { editing: true, transfer: true }) })
  const state = entryOf(transaction({ categoryId: '9' }), all)
  for (const tab of ENTRY_TABS) assert.equal(changeTab(state, tab, editing), state, tab)
})

test('a tab change starts again in the account the opener asked for', () => {
  const asked = context(three, { accountId: '2' })
  const changed = changeTab({ ...filled('EXPENSE'), account: '3' }, 'TRANSFER', asked)
  assert.deepEqual([changed.account, changed.from, changed.to], ['2', '2', '1'])
})

test('a round trip through the transfer tab brings back the amount, the date and the text, but not the name', () => {
  let state = filled('EXPENSE')
  for (const tab of ['INCOME', 'TRANSFER', 'EXPENSE'] as const) state = changeTab(state, tab, ctx)
  assert.deepEqual(state, { ...fresh, amount: '12,5', date: '2026-11-01', text: 'with Anna' })
})

// -- a transaction being changed --------------------------------------------------------------------------------------------------

test('a change opens on the tab of its category, with its own fields', () => {
  const expense = entryOf(transaction({ description: 'Weekly shop' }), all)
  assert.deepEqual([expense.tab, expense.name, expense.amount, expense.date, expense.text, expense.categoryId, expense.account], ['EXPENSE', 'Lidl', '47.35', '2026-11-16', 'Weekly shop', '1', '2'])
  assert.equal(entryOf(transaction({ categoryId: '9' }), all).tab, 'INCOME')
  assert.equal(entryOf(transaction({ description: null }), all).text, '', 'no description is an empty text')
})

test('a transaction whose category is gone opens as an expense', () => {
  assert.equal(entryOf(transaction({ categoryId: '77' }), all).tab, 'EXPENSE')
  assert.equal(tabOfCategory(all, '77'), 'EXPENSE')
  assert.equal(tabOfCategory(all, '10'), 'INCOME')
  assert.equal(tabOfCategory([], '10'), 'EXPENSE')
})

test('a shared bill opens with its split on, as exact amounts', () => {
  const shared = entryOf(transaction({ split: [{ personId: '1', person: 'Anna', guest: null, amount: '20.00' }], myShare: '27.35' }), all)
  assert.equal(shared.split.on, true)
  assert.equal(shared.split.draft.mode, 'EXACT')
  assert.equal(shared.split.touched, false)
  assert.equal(entryOf(transaction({ paidBy: { personId: '2', name: 'Marco' } }), all).split.on, true, 'a bill a friend paid')
  assert.equal(entryOf(transaction(), all).split, NOT_SPLIT)
})

// -- the categories of a tab ------------------------------------------------------------------------------------------------------

test('the expense tab lists the expense categories and the income tab the income ones', () => {
  assert.deepEqual(categoryChoices(all, 'EXPENSE').map((o) => o.label), ['Groceries', 'Rent'])
  assert.deepEqual(categoryChoices(all, 'INCOME').map((o) => o.label), ['Salary', 'Gift'])
  assert.deepEqual(categoryChoices([groceries], 'INCOME'), [], 'no income category at all')
  assert.deepEqual(categoryChoices([], 'EXPENSE'), [])
})

test('a change of a transaction whose category was deleted still shows that category, first', () => {
  const choices = categoryChoices(all, 'EXPENSE', { id: '77', label: 'deleted category' })
  assert.deepEqual(choices.map((o) => [o.value, o.label]), [['77', 'deleted category'], ['1', 'Groceries'], ['2', 'Rent']])
  assert.deepEqual(categoryChoices(all, 'EXPENSE', { id: '1', label: 'deleted category' }).map((o) => o.value), ['1', '2'], 'a category that is there is not named twice')
  assert.deepEqual(categoryChoices(all, 'EXPENSE', { id: '9', label: 'deleted category' }).map((o) => o.value), ['1', '2'], 'a category that is there, of the other type, is not added to the list')
})

// -- the account ------------------------------------------------------------------------------------------------------------------

test('an expense has the account field only with more than one account, an income always', () => {
  assert.equal(accountField([main], 'EXPENSE', null).shown, false)
  assert.equal(accountField([main, old], 'EXPENSE', null).shown, false, 'an archived one does not make two')
  assert.equal(accountField(two, 'EXPENSE', null).shown, true)
  assert.equal(accountField([main], 'INCOME', null).shown, true)
  assert.equal(accountField(null, 'INCOME', null).shown, true)
})

test('with one account the default is that account, also for an income', () => {
  const only = accountField([main], 'INCOME', null)
  assert.deepEqual([only.value, only.send, only.missing], ['1', '1', false])
  const expense = accountField([main], 'EXPENSE', null)
  assert.deepEqual([expense.value, expense.send], ['1', undefined], 'an expense says nothing: it goes to the main account')
})

test('an income with no account at all cannot be saved', () => {
  assert.equal(accountField([], 'INCOME', null).missing, true)
  assert.equal(accountField(null, 'INCOME', null).missing, true)
  assert.equal(accountField([], 'INCOME', '').missing, true)
  assert.equal(accountField([], 'EXPENSE', null).missing, false, 'an expense goes to the main account when none is said')
})

test('an account that a row already has is not sent again', () => {
  assert.equal(accountField(two, 'INCOME', '2', '2').send, undefined)
  assert.equal(accountField(two, 'INCOME', '1', '2').send, '1')
  assert.equal(accountField([main, old], 'INCOME', '4', '4').send, undefined, 'an archived one it stays in')
  assert.equal(accountField(two, 'EXPENSE', null, '2').value, '2', 'what it has is what the field shows until another is chosen')
})

// -- what is sent -----------------------------------------------------------------------------------------------------------------

const entry = (more: Partial<EntryState> = {}): EntryState => ({ ...entryStart(context(two)), name: ' Lunch ', amount: '12,5', categoryId: '1', text: '', ...more })

test('an expense is sent trimmed, with the amount made a number text and no description when it is empty', () => {
  const built = transactionBody(entry({ text: '  ' }), two, null)
  assert.deepEqual(built, { body: { name: 'Lunch', amount: '12.50', paymentDate: TODAY, categoryId: '1', description: null, accountId: '1' }, remember: '1' })
  assert.deepEqual('body' in built && built.body.description, null)
  const text = transactionBody(entry({ text: ' with Anna ' }), two, null)
  assert.equal('body' in text && text.body.description, 'with Anna')
})

test('with one account an expense says nothing of the account and remembers none', () => {
  const built = transactionBody(entry(), [main], null)
  assert.deepEqual(built, { body: { name: 'Lunch', amount: '12.50', paymentDate: TODAY, categoryId: '1', description: null }, remember: '' })
})

test('with one account an income names that account', () => {
  const built = transactionBody(entry({ tab: 'INCOME', categoryId: '9' }), [main], null)
  assert.deepEqual(built, { body: { name: 'Lunch', amount: '12.50', paymentDate: TODAY, categoryId: '9', description: null, accountId: '1' }, remember: '1' })
})

test('an income with an empty account is refused, with nothing sent', () => {
  assert.deepEqual(transactionBody(entry({ tab: 'INCOME', categoryId: '9', account: '' }), [], null), { error: 'entry.errorAccount' })
  assert.deepEqual(transactionBody(entry({ tab: 'INCOME', categoryId: '9', account: '' }), two, null), { error: 'entry.errorAccount' })
  assert.ok('body' in transactionBody(entry({ tab: 'EXPENSE', account: '' }), two, null), 'an expense with none goes to the main account')
})

test('a missing name, category or date, and a wrong amount, are refused', () => {
  const required = { error: 'tx.errorRequired' }
  assert.deepEqual(transactionBody(entry({ name: '   ' }), two, null), required)
  assert.deepEqual(transactionBody(entry({ categoryId: '' }), two, null), required)
  assert.deepEqual(transactionBody(entry({ date: '' }), two, null), required)
  for (const amount of ['', '0', '0,00', '-5', '1.234', '1,2,3', 'abc', '12 50', '99999999999']) {
    assert.deepEqual(transactionBody(entry({ amount }), two, null), { error: 'tx.errorAmount' }, amount)
  }
  assert.deepEqual(transactionBody(entry({ amount: '1,5' }), two, null), { body: { name: 'Lunch', amount: '1.50', paymentDate: TODAY, categoryId: '1', description: null, accountId: '1' }, remember: '1' })
})

test('a change that stays in its account says nothing of it, one that moves says where', () => {
  const stays = transactionBody(entry({ account: '2' }), two, transaction())
  assert.ok('body' in stays && !('accountId' in stays.body))
  const moves = transactionBody(entry({ account: '1' }), two, transaction())
  assert.ok('body' in moves && moves.body.accountId === '1')
  const archived = transactionBody(entry({ account: '4' }), [main, old], transaction({ accountId: '4' }))
  assert.ok('body' in archived && !('accountId' in archived.body), 'an archived account it is in')
})
