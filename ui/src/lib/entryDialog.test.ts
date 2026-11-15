import assert from 'node:assert/strict'
import { afterEach, beforeEach, test } from 'node:test'
import type { Account } from '../api/accounts.ts'
import type { Category } from '../api/categories.ts'
import type { Transaction } from '../api/transactions.ts'
import {
  accountField,
  categoryChoices,
  changeTab,
  chooseAccount,
  ENTRY_TABS,
  entryOf,
  entryStart,
  entryTabs,
  inMainText,
  nativeView,
  splitAmount,
  suggestedInMain,
  tabForKey,
  tabOfCategory,
  transactionBody,
  withInMain,
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
const account = (id: string, name: string, more: Partial<Account> = {}) => ({ id, name, kind: 'CHECKING', archived: false, isMain: id === '1', currency: 'EUR', foreign: false, rate: null, ...more }) as Account
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
  nativeAmount: null,
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
  assert.deepEqual(start, {
    tab: 'EXPENSE',
    name: '',
    amount: '',
    date: TODAY,
    text: '',
    categoryId: '',
    account: '1',
    from: '1',
    to: '2',
    split: NOT_SPLIT,
    inMain: '',
    inMainTouched: false,
    arrives: '',
    arrivesTouched: false,
  })
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
  inMain: '',
  inMainTouched: false,
  arrives: '',
  arrivesTouched: false,
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

// -- an expense or an income in an account in another currency (docs/accounts-design.md, section 21.8) -----------------------------------

// The app's currency is the euro; lei at 1 EUR = 5.00 RON and 4.90 RON, dollars at 1.08.
const lei = account('10', 'Lei account', { currency: 'RON', foreign: true, rate: '5.00' })
const lei2 = account('11', 'Lei savings', { currency: 'RON', foreign: true, rate: '4.90' })
const usd = account('12', 'Dollar card', { currency: 'USD', foreign: true, rate: '1.08' })
const oldLei = account('13', 'Old lei', { currency: 'RON', foreign: true, rate: '5.00', archived: true })
const fx = [main, card, cash, lei, lei2, usd, oldLei]
const fxContext = context(fx)
/** A new entry in `accountId`, with what was typed in the first field. */
const typed = (accountId: string, amount: string, more: Partial<EntryState> = {}): EntryState => ({ ...entryStart(fxContext), name: 'Lunch', categoryId: '1', account: accountId, amount, ...more })
const view = (state: EntryState, current: Transaction | null = null, accounts: readonly Account[] = fx) => nativeView(accounts, state, current)
const second = (state: EntryState, current: Transaction | null = null) => inMainText(fx, state, current)

test('with an account in the app\'s currency there is one amount field; with one in another currency there are two', () => {
  assert.equal(view(typed('1', '10')), null)
  assert.equal(view(typed('3', '10')), null)
  assert.deepEqual(view(typed('10', '113,74')), { code: 'RON', mainCode: 'EUR', inMain: '22.75' })
  assert.deepEqual(view(typed('12', '10')), { code: 'USD', mainCode: 'EUR', inMain: '9.26' }, '10 / 1.08 = 9.259')
  assert.deepEqual(view(typed('10', '10', { tab: 'INCOME' })), { code: 'RON', mainCode: 'EUR', inMain: '2.00' }, 'the income tab too')
  assert.equal(view(typed('10', '10', { tab: 'TRANSFER' })), null, 'a transfer has its own second field')
  assert.equal(view(typed('1', '10'), null, [main]), null, 'one account: nothing new')
  // the account field is hidden with one account, which is the main account: never two fields
  assert.equal(accountField([main], 'EXPENSE', null).shown, false)
  assert.equal(view(entryStart(context([main]))), null)
  // an account that is archived and in another currency, which a transaction is still in
  assert.deepEqual(view(typed('13', '10')), { code: 'RON', mainCode: 'EUR', inMain: '2.00' })
  // before the accounts are read, or with an account that is not there, nothing is said
  assert.equal(view(typed('99', '10')), null)
})

test('the second field follows the rate until it is typed in, keeps what was typed, and follows it again when emptied', () => {
  let state = typed('10', '100')
  assert.equal(second(state), '20.00')
  state = { ...state, amount: '125' }
  assert.equal(second(state), '25.00', 'it follows the first field')
  state = withInMain(state, '24,5')
  assert.deepEqual([state.inMain, state.inMainTouched], ['24,5', true])
  assert.equal(second(state), '24,5', 'theirs, as typed')
  assert.equal(second({ ...state, amount: '500' }), '24,5', 'the first field does not move it')
  state = withInMain(state, '')
  assert.deepEqual([state.inMain, state.inMainTouched], ['', false])
  assert.equal(second({ ...state, amount: '500' }), '100.00', 'emptied: the rate again')
  assert.equal(withInMain(state, '  ').inMainTouched, false, 'spaces alone are nothing')
  assert.equal(second({ ...state, amount: '' }), '', 'nothing to work out from nothing')
  assert.equal(second({ ...state, amount: 'abc' }), '')
  assert.equal(second({ ...state, amount: '0' }), '')
  assert.equal(second({ ...state, amount: '-5' }), '')
})

test('the amount in the app\'s currency is the native amount divided by the rate, half up to the cent', () => {
  const inMain = (rate: string, amount: string) => suggestedInMain([main, account('10', 'Lei', { currency: 'RON', foreign: true, rate })], typed('10', amount), null)
  assert.equal(inMain('5.00', '113.74'), '22.75', '22.748')
  assert.equal(inMain('5.00', '0.05'), '0.01', '0.01 exactly')
  assert.equal(inMain('4', '0.02'), '0.01', '0.005 rounds up')
  assert.equal(inMain('4', '0.01'), '0.00', '0.0025 is nothing')
  assert.equal(inMain('0.5', '10'), '20.00', 'a rate below 1 makes more')
  assert.equal(inMain('0.0063', '100'), '15873.02', '100 / 0.0063 = 15873.0158')
  assert.equal(inMain('1', '33.33'), '33.33')
  assert.equal(inMain('160', '1000000'), '6250.00')
  // very large amounts do not lose a cent to floating point
  assert.equal(inMain('3', '9999999999.99'), '3333333333.33')
  assert.equal(inMain('7', '9999999999.99'), '1428571428.57', '1428571428.5700')
  assert.equal(inMain('5.00', '12,5'), '2.50', 'a comma is a point')
  assert.equal(inMain('5.00', ''), null)
  assert.equal(suggestedInMain(fx, typed('1', '10'), null), null, 'an account in the app\'s currency')
  assert.equal(suggestedInMain([main, { ...lei, rate: null }], typed('10', '10'), null), null, 'a rate that is not there')
})

test('the amount to split is the one in the app\'s currency', () => {
  assert.equal(splitAmount(fx, typed('1', '12,5'), null), '12,5', 'one amount: as typed')
  assert.equal(splitAmount(fx, typed('10', '100'), null), '20.00', 'in lei: what the rate gives')
  assert.equal(splitAmount(fx, withInMain(typed('10', '100'), '18'), null), '18', 'what was typed in the second field')
  assert.equal(splitAmount(fx, typed('10', ''), null), '', 'nothing yet')
})

test('a new entry moved from an account in the app\'s currency to one in another keeps the number where it was typed, now in that currency', () => {
  const state = chooseAccount(fx, typed('1', '50'), null, '10')
  assert.deepEqual([state.account, state.amount, state.inMain, state.inMainTouched], ['10', '50', '', false])
  assert.equal(second(state), '10.00', 'and the second field works it out')
  assert.equal(chooseAccount(fx, typed('1', '50'), null, '3').amount, '50', 'between two accounts in the app\'s currency: only the account changes')
  assert.deepEqual(chooseAccount(fx, typed('1', ''), null, '10').amount, '', 'nothing typed, nothing moves')
})

test('a new entry moved from an account in another currency to one in the app\'s currency: the number typed stays, unless the second field was typed', () => {
  const untouched = chooseAccount(fx, typed('10', '100'), null, '1')
  assert.deepEqual([untouched.account, untouched.amount, untouched.inMain, untouched.inMainTouched], ['1', '100', '', false], 'the native number moves into the single field')
  assert.equal(view(untouched), null)
  const touched = chooseAccount(fx, withInMain(typed('10', '100'), '18,5'), null, '1')
  assert.deepEqual([touched.amount, touched.inMain, touched.inMainTouched], ['18,5', '', false], 'what the person said it was in the app\'s currency is the amount')
  assert.equal(chooseAccount(fx, typed('10', '100'), null, '3').amount, '100', 'any account in the app\'s currency')
})

test('between two accounts in another currency the first field stays, and a second field that was not typed follows the new rate', () => {
  const state = typed('10', '100')
  const other = chooseAccount(fx, state, null, '11')
  assert.deepEqual([other.account, other.amount, other.inMainTouched], ['11', '100', false])
  assert.equal(second(state), '20.00')
  assert.equal(second(other), '20.41', '100 / 4.90 = 20.408')
  const dollars = chooseAccount(fx, state, null, '12')
  assert.equal(dollars.amount, '100')
  assert.equal(second(dollars), '92.59')
  // one that was typed is theirs and stays
  const mine = withInMain(typed('10', '100'), '19')
  const moved = chooseAccount(fx, mine, null, '12')
  assert.deepEqual([moved.amount, moved.inMain, moved.inMainTouched], ['100', '19', true])
  assert.equal(second(moved), '19')
})

test('there and back again brings a new entry\'s amount home', () => {
  let state = typed('1', '50')
  for (const id of ['10', '11', '1']) state = chooseAccount(fx, state, null, id)
  assert.deepEqual([state.account, state.amount, state.inMainTouched], ['1', '50', false])
  let theirs = withInMain(typed('10', '100'), '18')
  for (const id of ['1', '10']) theirs = chooseAccount(fx, theirs, null, id)
  assert.deepEqual([theirs.amount, theirs.inMainTouched], ['18', false], 'the second field was lost on the way out: the amount in the app\'s currency became the amount')
})

test('a choice of the account that does not cross currencies changes only the account', () => {
  const state = typed('10', '100')
  assert.deepEqual(chooseAccount(fx, state, null, '10'), state, 'the same account')
  assert.deepEqual(chooseAccount(fx, state, null, '99'), { ...state, account: '99' }, 'an account that is not there counts as one in the app\'s currency')
})

// A transaction being changed
const inLei = (more: Partial<Transaction> = {}) => transaction({ accountId: '10', amount: '22.75', nativeAmount: '113.74', ...more })
const mainRow = (more: Partial<Transaction> = {}) => transaction({ accountId: '1', amount: '47.35', ...more })

test('a transaction in an account in another currency opens with both amounts, the second as the person\'s own', () => {
  const state = entryOf(inLei(), all, fx)
  assert.deepEqual([state.account, state.amount, state.inMain, state.inMainTouched], ['10', '113.74', '22.75', true])
  assert.equal(second(state, inLei()), '22.75', 'what is stored, not worked out again')
  // the stored amount stays whatever the rate says now
  assert.equal(inMainText([main, { ...lei, rate: '4.00' }], state, inLei()), '22.75')
  assert.deepEqual(view(state, inLei()), { code: 'RON', mainCode: 'EUR', inMain: '22.75' })
})

test('a transaction in an account in the app\'s currency opens with one amount, and a native amount left on it means nothing', () => {
  const state = entryOf(mainRow(), all, fx)
  assert.deepEqual([state.amount, state.inMain, state.inMainTouched], ['47.35', '', false])
  const stale = entryOf(mainRow({ nativeAmount: '236.75' }), all, fx)
  assert.deepEqual([stale.amount, stale.inMain, stale.inMainTouched], ['47.35', '', false], 'the app\'s amount counts')
  assert.equal(view(stale, mainRow({ nativeAmount: '236.75' })), null)
})

test('a transaction in an account in another currency with no native amount opens with that field empty, to be typed', () => {
  const state = entryOf(inLei({ nativeAmount: null }), all, fx)
  assert.deepEqual([state.amount, state.inMain, state.inMainTouched], ['', '22.75', true])
  assert.deepEqual(transactionBody(state, fx, inLei({ nativeAmount: null })), { error: 'entry.errorNative', vars: { code: 'RON' } })
})

test('without the accounts, a native amount says the transaction is in another currency', () => {
  const state = entryOf(inLei(), all)
  assert.deepEqual([state.amount, state.inMain, state.inMainTouched], ['113.74', '22.75', true])
  assert.deepEqual([entryOf(mainRow(), all).amount, entryOf(mainRow(), all).inMainTouched], ['47.35', false])
})

test('a transaction in an account that was archived keeps both its amounts', () => {
  const row = inLei({ accountId: '13' })
  const state = entryOf(row, all, fx)
  assert.deepEqual([state.amount, state.inMain], ['113.74', '22.75'])
  assert.deepEqual(view(state, row), { code: 'RON', mainCode: 'EUR', inMain: '22.75' })
})

test('a change moved from an account in the app\'s currency to one in another keeps its amount as the amount in the app\'s currency and asks for the other', () => {
  const row = mainRow()
  const state = chooseAccount(fx, entryOf(row, all, fx), row, '10')
  assert.deepEqual([state.account, state.amount, state.inMain, state.inMainTouched], ['10', '', '47.35', true])
  assert.deepEqual(transactionBody(state, fx, row), { error: 'entry.errorNative', vars: { code: 'RON' } }, 'the native amount is needed')
  const given = { ...state, amount: '236,75' }
  assert.deepEqual(transactionBody(given, fx, row), {
    body: { name: 'Lidl', amount: '47.35', nativeAmount: '236.75', paymentDate: '2026-11-16', categoryId: '1', description: null, accountId: '10' },
    remember: '10',
  })
  // an amount of nothing in the app's currency typed before is kept as it was
  const empty = chooseAccount(fx, { ...entryOf(row, all, fx), amount: '' }, row, '10')
  assert.deepEqual([empty.amount, empty.inMain, empty.inMainTouched], ['', '', false])
})

test('a change moved out of an account in another currency has the amount in the app\'s currency and no native amount', () => {
  const row = inLei()
  const state = chooseAccount(fx, entryOf(row, all, fx), row, '1')
  assert.deepEqual([state.account, state.amount, state.inMain, state.inMainTouched], ['1', '22.75', '', false])
  assert.deepEqual(transactionBody(state, fx, row), {
    body: { name: 'Lidl', amount: '22.75', paymentDate: '2026-11-16', categoryId: '1', description: null, accountId: '1' },
    remember: '1',
  })
  const body = transactionBody(state, fx, row)
  assert.ok('body' in body && !('nativeAmount' in body.body), 'the app clears the one it had')
  // there and back: the amounts come back to the first field of the amount in the app's currency, and the native one is to be typed
  const back = chooseAccount(fx, state, row, '10')
  assert.deepEqual([back.amount, back.inMain, back.inMainTouched], ['', '22.75', true])
})

test('a change moved between two accounts in another currency keeps its two amounts', () => {
  const row = inLei()
  const state = chooseAccount(fx, entryOf(row, all, fx), row, '11')
  assert.deepEqual([state.account, state.amount, state.inMain, state.inMainTouched], ['11', '113.74', '22.75', true])
  assert.deepEqual(transactionBody(state, fx, row), {
    body: { name: 'Lidl', amount: '22.75', nativeAmount: '113.74', paymentDate: '2026-11-16', categoryId: '1', description: null, accountId: '11' },
    remember: '11',
  })
})

test('a change that stays in its account in another currency sends the native amount, and not the account', () => {
  const row = inLei()
  const state = entryOf(row, all, fx)
  assert.deepEqual(transactionBody(state, fx, row), {
    body: { name: 'Lidl', amount: '22.75', nativeAmount: '113.74', paymentDate: '2026-11-16', categoryId: '1', description: null },
    remember: '10',
  })
  const changed = transactionBody({ ...state, amount: '120' }, fx, row)
  assert.ok('body' in changed && changed.body.nativeAmount === '120.00' && changed.body.amount === '22.75', 'the first field changed, the second (theirs) did not')
  const both = transactionBody(withInMain({ ...state, amount: '120' }, '24'), fx, row)
  assert.ok('body' in both && both.body.nativeAmount === '120.00' && both.body.amount === '24.00')
  // emptied: the rate works it out again
  const follows = transactionBody(withInMain({ ...state, amount: '120' }, ''), fx, row)
  assert.ok('body' in follows && follows.body.amount === '24.00')
})

test('a change that stays in its account in the app\'s currency sends no native amount', () => {
  const row = mainRow()
  const built = transactionBody(entryOf(row, all, fx), fx, row)
  assert.ok('body' in built && !('nativeAmount' in built.body) && !('accountId' in built.body))
  assert.equal('body' in built && built.body.amount, '47.35')
})

test('a new expense in an account in another currency is sent with both amounts, the second worked out or typed', () => {
  const state = typed('10', '113,74')
  assert.deepEqual(transactionBody(state, fx, null), {
    body: { name: 'Lunch', amount: '22.75', nativeAmount: '113.74', paymentDate: TODAY, categoryId: '1', description: null, accountId: '10' },
    remember: '10',
  })
  const mine = transactionBody(withInMain(state, '22,7'), fx, null)
  assert.ok('body' in mine && mine.body.amount === '22.70' && mine.body.nativeAmount === '113.74')
  const income = transactionBody({ ...state, tab: 'INCOME', categoryId: '9' }, fx, null)
  assert.ok('body' in income && income.body.amount === '22.75' && income.body.nativeAmount === '113.74' && income.body.accountId === '10', 'the income tab too')
  const dollars = transactionBody(typed('12', '10'), fx, null)
  assert.ok('body' in dollars && dollars.body.amount === '9.26' && dollars.body.nativeAmount === '10.00')
})

test('a new entry in an account in the app\'s currency sends no native amount, with a second field left over from before', () => {
  const left = { ...typed('1', '10'), inMain: '99', inMainTouched: true }
  const built = transactionBody(left, fx, null)
  assert.ok('body' in built && !('nativeAmount' in built.body) && built.body.amount === '10.00')
})

test('both amounts are needed, and each error names its currency', () => {
  assert.deepEqual(transactionBody(typed('10', ''), fx, null), { error: 'entry.errorNative', vars: { code: 'RON' } })
  for (const amount of ['0', '0,00', '-5', '1.234', 'abc', '12 50', '99999999999']) {
    assert.deepEqual(transactionBody(typed('10', amount), fx, null), { error: 'entry.errorNative', vars: { code: 'RON' } }, amount)
  }
  // the second field typed wrong, empty after being typed in (which makes it follow the rate again: fine), or worked out as nothing
  for (const wrong of ['0', '-1', 'abc', '1.234']) {
    assert.deepEqual(transactionBody(withInMain(typed('10', '100'), wrong), fx, null), { error: 'entry.errorNative', vars: { code: 'EUR' } }, wrong)
  }
  assert.deepEqual(transactionBody(typed('10', '0.01'), fx, null), { error: 'entry.errorNative', vars: { code: 'EUR' } }, '0.01 lei is 0.002 euros: nothing, which an amount cannot be')
  assert.ok('body' in transactionBody(withInMain(typed('10', '0.01'), '0.01'), fx, null), 'typed, it goes')
  assert.deepEqual(transactionBody({ ...typed('10', '100'), name: ' ' }, fx, null), { error: 'tx.errorRequired' }, 'the name before the amounts')
  assert.deepEqual(transactionBody({ ...typed('10', 'x'), categoryId: '' }, fx, null), { error: 'tx.errorRequired' })
  assert.deepEqual(transactionBody(typed('1', 'abc'), fx, null), { error: 'tx.errorAmount' }, 'one amount, as before')
})

test('a change of tab with an account in another currency keeps the account and both amounts between an expense and an income', () => {
  const state = withInMain(typed('10', '100', { text: 'note' }), '19')
  const income = changeTab(state, 'INCOME', fxContext)
  assert.deepEqual([income.tab, income.account, income.amount, income.inMain, income.inMainTouched, income.text, income.name], ['INCOME', '10', '100', '19', true, 'note', 'Lunch'])
  assert.deepEqual(view(income), { code: 'RON', mainCode: 'EUR', inMain: '19' })
  const back = changeTab(income, 'EXPENSE', fxContext)
  assert.deepEqual([back.account, back.amount, back.inMain, back.inMainTouched], ['10', '100', '19', true])
  // a second field that was not typed stays untyped, and follows
  const untouched = changeTab(typed('10', '100'), 'INCOME', fxContext)
  assert.deepEqual([untouched.inMain, untouched.inMainTouched], ['', false])
  assert.equal(second(untouched), '20.00')
})

test('to the transfer tab the amount stays and the second field of an expense does not come along; coming back, the account starts again', () => {
  const state = withInMain(typed('10', '100'), '19')
  const move = changeTab(state, 'TRANSFER', fxContext)
  assert.deepEqual([move.tab, move.amount, move.inMain, move.inMainTouched, move.arrives, move.arrivesTouched], ['TRANSFER', '100', '', false, '', false])
  assert.equal(view(move), null)
  const back = changeTab(move, 'EXPENSE', fxContext)
  assert.deepEqual([back.amount, back.inMain, back.inMainTouched, back.account], ['100', '', false, '1'])
  // the second amount of a transfer is not kept on a change of tab either
  const transfer = { ...entryStart(fxContext, 'TRANSFER'), amount: '100', arrives: '495', arrivesTouched: true }
  const toExpense = changeTab(transfer, 'EXPENSE', fxContext)
  assert.deepEqual([toExpense.arrives, toExpense.arrivesTouched, toExpense.amount], ['', false, '100'])
})

test('a bill split on an expense in another currency works on the amount in the app\'s currency', () => {
  const state = { ...typed('10', '100'), split: { on: true, draft: NOT_SPLIT.draft, ready: false, touched: false } }
  assert.equal(splitAmount(fx, state, null), '20.00')
  assert.equal(splitAmount(fx, chooseAccount(fx, state, null, '1'), null), '100', 'back in the app\'s currency: the single amount')
  assert.equal(splitAmount(fx, chooseAccount(fx, state, null, '11'), null), '20.41', 'another rate, another amount to split')
  const row = inLei()
  assert.equal(splitAmount(fx, entryOf(row, all, fx), row), '22.75', 'a shared bill being changed: what the app has')
})
