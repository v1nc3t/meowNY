import assert from 'node:assert/strict'
import { afterEach, beforeEach, test } from 'node:test'
import type { Account } from '../api/accounts.ts'
import type { Transfer } from '../api/transfers.ts'
import {
  accountFields as fieldsOf,
  accountSubmit as submitAccount,
  arrivalText,
  balanceTodayFilled,
  chooseSide,
  cleanCurrency,
  currencyLock,
  dialogNext,
  foreignCode,
  hasDetails,
  isMainCurrency,
  rateRow,
  suggestedArrival,
  transferCurrencies,
  transferFieldsOf,
  transferFieldsOfRow,
  transferStart,
  transferSubmit as submitTransfer,
  withArrival,
  withBalanceToday,
  type AccountFields,
  type DialogState,
  type TransferFields,
} from './accountDialogs.ts'

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

const TODAY = '2026-11-13'
const account = (id: string, name: string, more: Partial<Account> = {}): Account => ({
  id,
  name,
  kind: 'CHECKING',
  number: null,
  openingBalance: '0.00',
  openingDate: '1970-01-01',
  archived: false,
  isMain: id === '1',
  balance: '0.00',
  changeThisMonth: '0.00',
  lastActivity: null,
  entries: 0,
  bic: null,
  notes: null,
  group: null,
  includeInNetWorth: true,
  virtualBalance: '0.00',
  available: '0.00',
  currency: 'EUR',
  foreign: false,
  rate: null,
  balanceInMain: '0.00',
  setAside: '0.00',
  free: '0.00',
  ...more,
})
const main = account('1', 'Main account')
const card = account('2', 'Card', { kind: 'CREDIT_CARD', number: 'DE00 1234', openingBalance: '-120.00', openingDate: '2026-01-01' })
const cash = account('3', 'Cash', { kind: 'CASH' })
const old = account('4', 'Old', { archived: true })
const detailed = account('5', 'Card', { kind: 'CREDIT_CARD', bic: 'COBADEFF', group: 'Cards', virtualBalance: '-5.50', includeInNetWorth: false, notes: 'line 1\nline 2' })

// The app's currency is the euro. Lei and dollars are other currencies, each with the rate typed for it: 1 EUR = 5.00 RON, 1 EUR = 1.08 USD.
const MAIN = 'EUR'
const lei = account('10', 'Lei account', { currency: 'RON', foreign: true, rate: '5.00', openingBalance: '800.00', openingDate: '2026-08-01', balance: '1662.90' })
const lei2 = account('11', 'Lei savings', { currency: 'RON', foreign: true, rate: '4.90' })
const usd = account('12', 'Dollar card', { currency: 'USD', foreign: true, rate: '1.08' })
const everyone = [main, card, cash, old, lei, lei2, usd]

// the dialogs know the app's currency and the accounts; these tests ask the rest
const accountFields = (a: Account | null, shown: string, today: string) => fieldsOf(a, shown, today, MAIN)
const accountSubmit = (state: DialogState, a: Account | null, start: AccountFields | null, typed: AccountFields) => submitAccount(state, a, start, typed, MAIN)
const transferSubmit = (state: DialogState, transfer: Transfer | null, typed: TransferFields, accounts: readonly Account[] = everyone) => submitTransfer(state, transfer, typed, accounts)

// -- the states (section 5.4) -----------------------------------------------------------------------------------------------------

test('a dialog goes from editing to saving and back on an error, or closes on an answer', () => {
  assert.equal(dialogNext('EDITING', 'send'), 'SAVING')
  assert.equal(dialogNext('SAVING', 'ok'), 'CLOSED')
  assert.equal(dialogNext('SAVING', 'error'), 'EDITING', 'the fields are given back')
})

test('a second submit while saving changes nothing', () => {
  assert.equal(dialogNext('SAVING', 'send'), 'SAVING')
  assert.deepEqual(accountSubmit('SAVING', null, null, accountFields(null, '', TODAY)), { do: 'ignore' }, 'not even a wrong field is looked at')
  assert.deepEqual(transferSubmit('SAVING', null, { from: '1', to: '1', amount: '', date: TODAY, note: '', arrives: '', arrivesTouched: false }), { do: 'ignore' })
  assert.deepEqual(accountSubmit('CLOSED', null, null, { ...accountFields(null, '', TODAY), name: 'Savings' }), { do: 'ignore' })
})

test('cancel closes from editing and from saving, and a closed dialog stays closed', () => {
  assert.equal(dialogNext('EDITING', 'cancel'), 'CLOSED')
  assert.equal(dialogNext('SAVING', 'cancel'), 'CLOSED')
  for (const event of ['send', 'ok', 'error', 'cancel'] as const) assert.equal(dialogNext('CLOSED', event), 'CLOSED', event)
  // an answer that comes with nothing sent is not a reason to close or to lock the fields
  assert.equal(dialogNext('EDITING', 'ok'), 'EDITING')
  assert.equal(dialogNext('EDITING', 'error'), 'EDITING')
})

test('a failed save can be tried again with the same fields', () => {
  const fields: AccountFields = { ...accountFields(null, '', TODAY), name: 'Savings' }
  let state: DialogState = 'EDITING'
  const first = accountSubmit(state, null, null, fields)
  assert.equal(first.do, 'send')
  state = dialogNext(state, 'send')
  assert.equal(accountSubmit(state, null, null, fields).do, 'ignore')
  state = dialogNext(state, 'error')
  assert.deepEqual(accountSubmit(state, null, null, fields), first, 'the same request, from fields nobody cleared')
})

// -- the account dialog -----------------------------------------------------------------------------------------------------------

test('a new account opens empty, as checking, dated today', () => {
  assert.deepEqual(accountFields(null, '', TODAY), {
    name: '',
    kind: 'CHECKING',
    number: '',
    balance: '',
    date: TODAY,
    bic: '',
    group: '',
    virtualBalance: '',
    includeInNetWorth: true,
    notes: '',
    currency: 'EUR',
    rate: '',
    balanceToday: '',
  })
})

test('an account opens with its row; no opening balance is an empty field dated today', () => {
  assert.deepEqual(accountFields(card, 'Card', TODAY), {
    name: 'Card',
    kind: 'CREDIT_CARD',
    number: 'DE00 1234',
    balance: '-120.00',
    date: '2026-01-01',
    bic: '',
    group: '',
    virtualBalance: '',
    includeInNetWorth: true,
    notes: '',
    currency: 'EUR',
    rate: '',
    balanceToday: '',
  })
  assert.deepEqual(accountFields(main, 'hauptkonto', TODAY), { ...accountFields(null, '', TODAY), name: 'hauptkonto' })
  // what is behind "more" comes from the row; a virtual balance of 0.00 is an empty field
  assert.deepEqual(accountFields(detailed, 'Card', TODAY), {
    name: 'Card',
    kind: 'CREDIT_CARD',
    number: '',
    balance: '',
    date: TODAY,
    bic: 'COBADEFF',
    group: 'Cards',
    virtualBalance: '-5.50',
    includeInNetWorth: false,
    notes: 'line 1\nline 2',
    currency: 'EUR',
    rate: '',
    balanceToday: '',
  })
})

test('a new account is sent with what was typed, and the opening balance only when there is one', () => {
  const empty = accountFields(null, '', TODAY)
  assert.deepEqual(accountSubmit('EDITING', null, null, { ...empty, name: '  Savings ', kind: 'SAVINGS' }), {
    do: 'send',
    request: { create: { name: 'Savings', kind: 'SAVINGS' } },
  })
  assert.deepEqual(accountSubmit('EDITING', null, null, { ...empty, name: 'Card', kind: 'CREDIT_CARD', number: ' DE00 ', balance: '-120,5', date: '2026-01-01' }), {
    do: 'send',
    request: { create: { name: 'Card', kind: 'CREDIT_CARD', number: 'DE00', openingBalance: '-120.50', openingDate: '2026-01-01' } },
  })
  assert.deepEqual(accountSubmit('EDITING', null, null, { ...empty, name: 'Cash', balance: ' 0 ', date: '2020-05-05' }), {
    do: 'send',
    request: { create: { name: 'Cash', kind: 'CHECKING' } },
  })
})

test('a wrong field sends nothing and says which', () => {
  const empty = accountFields(null, '', TODAY)
  for (const name of ['', '   ', 'x'.repeat(51), 'a\nb']) {
    assert.deepEqual(accountSubmit('EDITING', null, null, { ...empty, name }), { do: 'refuse', error: 'accounts.nameError' }, JSON.stringify(name))
  }
  assert.equal(accountSubmit('EDITING', null, null, { ...empty, name: 'é'.repeat(50) }).do, 'send', 'fifty characters, not bytes')
  for (const balance of ['-', '-0.00', '+1', '1.555', 'abc', '10000000000', '- 5']) {
    assert.deepEqual(accountSubmit('EDITING', null, null, { ...empty, name: 'Savings', balance }), { do: 'refuse', error: 'accounts.balanceError' }, balance)
  }
  assert.deepEqual(accountSubmit('EDITING', card, accountFields(card, 'Card', TODAY), { ...accountFields(card, 'Card', TODAY), name: ' ' }), { do: 'refuse', error: 'accounts.nameError' })
})

test('an edit with nothing changed closes and sends nothing', () => {
  const start = accountFields(card, 'Card', TODAY)
  assert.deepEqual(accountSubmit('EDITING', card, start, start), { do: 'close' })
  assert.deepEqual(accountSubmit('EDITING', card, start, { ...start, name: ' Card ', number: 'DE00 1234 ', balance: '-120' }), { do: 'close' }, 'the same after cleaning')
  // the main account shown in the person's language is not renamed to its translation, and its date of today is not a change
  const shown = accountFields(main, 'hauptkonto', TODAY)
  assert.deepEqual(accountSubmit('EDITING', main, shown, shown), { do: 'close' })
  assert.deepEqual(accountSubmit('EDITING', main, shown, { ...shown, date: '2026-02-02' }), { do: 'close' }, 'a date with no balance says nothing')
})

test('an edit sends only the fields that changed', () => {
  const start = accountFields(card, 'Card', TODAY)
  const patch = (fields: Partial<AccountFields>) => accountSubmit('EDITING', card, start, { ...start, ...fields })
  assert.deepEqual(patch({ name: 'Visa' }), { do: 'send', request: { patch: { name: 'Visa' } } })
  assert.deepEqual(patch({ kind: 'SAVINGS' }), { do: 'send', request: { patch: { kind: 'SAVINGS' } } })
  assert.deepEqual(patch({ number: '' }), { do: 'send', request: { patch: { number: null } } }, 'an emptied number is cleared')
  assert.deepEqual(patch({ number: 'DE99' }), { do: 'send', request: { patch: { number: 'DE99' } } })
  assert.deepEqual(patch({ balance: '50' }), { do: 'send', request: { patch: { openingBalance: '50.00' } } })
  assert.deepEqual(patch({ date: '2026-03-01' }), { do: 'send', request: { patch: { openingDate: '2026-03-01' } } })
  assert.deepEqual(patch({ balance: '' }), { do: 'send', request: { patch: { openingBalance: '0.00' } } }, 'emptied: 0.00, and its date is left')
  assert.deepEqual(patch({ name: 'Visa', balance: '-1', date: '2026-03-01' }), {
    do: 'send',
    request: { patch: { name: 'Visa', openingBalance: '-1.00', openingDate: '2026-03-01' } },
  })
})

test('a first opening balance is sent with its date, and a rename of the main account as typed', () => {
  const shown = accountFields(main, 'main account', TODAY)
  assert.deepEqual(accountSubmit('EDITING', main, shown, { ...shown, balance: '900' }), {
    do: 'send',
    request: { patch: { openingBalance: '900.00', openingDate: TODAY } },
  })
  assert.deepEqual(accountSubmit('EDITING', main, shown, { ...shown, name: 'Current account' }), { do: 'send', request: { patch: { name: 'Current account' } } })
})

// -- what is behind "more" ---------------------------------------------------------------------------------------------------------

const empty = accountFields(null, '', TODAY)
const send = (more: Partial<AccountFields>) => accountSubmit('EDITING', null, null, { ...empty, name: 'Savings', ...more })
const refused = (more: Partial<AccountFields>) => (send(more) as { error?: string }).error

test('"more" opens by default exactly when the account being changed has any of the five set', () => {
  assert.equal(hasDetails(null), false, 'a new account has none')
  assert.equal(hasDetails(main), false)
  assert.equal(hasDetails(card), false, 'a number and an opening balance are not behind "more"')
  assert.equal(hasDetails(account('6', 'A', { bic: 'X' })), true)
  assert.equal(hasDetails(account('6', 'A', { notes: 'x' })), true)
  assert.equal(hasDetails(account('6', 'A', { group: 'x' })), true)
  assert.equal(hasDetails(account('6', 'A', { includeInNetWorth: false })), true, 'left out of the net worth is a setting')
  assert.equal(hasDetails(account('6', 'A', { virtualBalance: '1000.00' })), true)
  assert.equal(hasDetails(account('6', 'A', { virtualBalance: '-0.01' })), true)
  assert.equal(hasDetails(account('6', 'A', { virtualBalance: '0.00' })), false)
  assert.equal(hasDetails(detailed), true)
})

test('a BIC is 20 characters at most, a group 50, the notes 1000', () => {
  assert.equal(send({ bic: 'B'.repeat(20) }).do, 'send')
  assert.equal(refused({ bic: 'B'.repeat(21) }), 'accounts.bicError')
  assert.equal(send({ bic: 'é'.repeat(20) }).do, 'send', 'characters, not bytes')
  assert.equal(refused({ bic: 'a\tb' }), 'accounts.bicError', 'no control character')
  assert.equal(send({ group: 'G'.repeat(50) }).do, 'send')
  assert.equal(refused({ group: 'G'.repeat(51) }), 'accounts.groupError')
  assert.equal(refused({ group: 'a\nb' }), 'accounts.groupError', 'a group is one line')
  assert.equal(send({ notes: 'n'.repeat(1000) }).do, 'send')
  assert.equal(refused({ notes: 'n'.repeat(1001) }), 'accounts.notesError')
  assert.equal(refused({ notes: 'a\u0000b' }), 'accounts.notesError', 'no control character but the line break')
  assert.equal(refused({ notes: 'a\tb' }), 'accounts.notesError')
  // the spaces around are not counted: they are cut
  assert.equal(send({ bic: ` ${'B'.repeat(20)} `, notes: ` \n${'n'.repeat(1000)}\n ` }).do, 'send')
})

test('the notes keep their line breaks', () => {
  assert.deepEqual(send({ notes: ' first\nsecond\n\nthird ' }), { do: 'send', request: { create: { name: 'Savings', kind: 'CHECKING', notes: 'first\nsecond\n\nthird' } } })
})

test('a virtual balance follows the rules of the opening balance', () => {
  const virtual = (virtualBalance: string) => (send({ virtualBalance }) as { request?: { create: { virtualBalance?: string } } }).request?.create.virtualBalance
  assert.equal(virtual('1000'), '1000.00')
  assert.equal(virtual('1.5'), '1.50')
  assert.equal(virtual('1,50'), '1.50', 'a comma is a point')
  assert.equal(virtual('-5.5'), '-5.50')
  assert.equal(virtual('9999999999.99'), '9999999999.99')
  assert.equal(virtual('-9999999999.99'), '-9999999999.99')
  assert.equal(virtual(''), undefined, 'empty is 0.00 and is not sent')
  assert.equal(virtual('0'), undefined)
  assert.equal(virtual(' 0.00 '), undefined)
  for (const bad of ['-0.00', '-0', '+1', '+1.00', '1.555', 'abc', '-', '10000000000', '-10000000000', '- 5', '1 000']) {
    assert.equal(refused({ virtualBalance: bad }), 'accounts.balanceError', bad)
  }
})

test('a new account is sent with only the details that were filled in', () => {
  assert.deepEqual(send({}), { do: 'send', request: { create: { name: 'Savings', kind: 'CHECKING' } } }, 'the defaults are not sent')
  assert.deepEqual(send({ bic: '  ', group: '  ', notes: ' \n ', virtualBalance: '  ' }), { do: 'send', request: { create: { name: 'Savings', kind: 'CHECKING' } } }, 'spaces are nothing')
  assert.deepEqual(send({ bic: ' COBADEFF ', group: ' Cards ', notes: 'n', virtualBalance: '1000', includeInNetWorth: false }), {
    do: 'send',
    request: { create: { name: 'Savings', kind: 'CHECKING', bic: 'COBADEFF', group: 'Cards', notes: 'n', includeInNetWorth: false, virtualBalance: '1000.00' } },
  })
})

test('an edit sends only the details that changed, and null for one that was cleared', () => {
  const start = accountFields(detailed, 'Card', TODAY)
  const patch = (more: Partial<AccountFields>) => accountSubmit('EDITING', detailed, start, { ...start, ...more })
  assert.deepEqual(patch({}), { do: 'close' }, 'nothing changed')
  assert.deepEqual(patch({ bic: 'NEW' }), { do: 'send', request: { patch: { bic: 'NEW' } } })
  assert.deepEqual(patch({ bic: '' }), { do: 'send', request: { patch: { bic: null } } })
  assert.deepEqual(patch({ bic: '   ' }), { do: 'send', request: { patch: { bic: null } } }, 'spaces alone clear it')
  assert.deepEqual(patch({ group: ' ' }), { do: 'send', request: { patch: { group: null } } })
  assert.deepEqual(patch({ group: 'Other' }), { do: 'send', request: { patch: { group: 'Other' } } })
  assert.deepEqual(patch({ notes: '\n\n' }), { do: 'send', request: { patch: { notes: null } } })
  assert.deepEqual(patch({ notes: 'line 1\nline 2\nline 3' }), { do: 'send', request: { patch: { notes: 'line 1\nline 2\nline 3' } } })
  assert.deepEqual(patch({ virtualBalance: '' }), { do: 'send', request: { patch: { virtualBalance: '0.00' } } }, 'a virtual balance is cleared to 0.00, not to null')
  assert.deepEqual(patch({ virtualBalance: '-5.5' }), { do: 'close' }, 'the same amount written another way')
  assert.deepEqual(patch({ virtualBalance: '7' }), { do: 'send', request: { patch: { virtualBalance: '7.00' } } })
  assert.deepEqual(patch({ includeInNetWorth: true }), { do: 'send', request: { patch: { includeInNetWorth: true } } })
  assert.deepEqual(patch({ bic: 'NEW', group: ' ', includeInNetWorth: true }), { do: 'send', request: { patch: { bic: 'NEW', group: null, includeInNetWorth: true } } })
})

test('a switch toggled and toggled back is not a change', () => {
  const start = accountFields(main, 'Main', TODAY)
  const toggled = { ...start, includeInNetWorth: !start.includeInNetWorth }
  assert.deepEqual(accountSubmit('EDITING', main, start, toggled), { do: 'send', request: { patch: { includeInNetWorth: false } } })
  assert.deepEqual(accountSubmit('EDITING', main, start, { ...toggled, includeInNetWorth: !toggled.includeInNetWorth }), { do: 'close' })
})

test('details that were set before are not asked again, and one that is wrong beyond the dialog is refused whole', () => {
  const start = accountFields(detailed, 'Card', TODAY)
  assert.deepEqual(accountSubmit('EDITING', detailed, start, { ...start, name: 'Visa' }), { do: 'send', request: { patch: { name: 'Visa' } } })
  assert.deepEqual(accountSubmit('EDITING', detailed, start, { ...start, name: 'Visa', notes: 'n'.repeat(1001) }), { do: 'refuse', error: 'accounts.notesError' })
})

// -- the transfer dialog ----------------------------------------------------------------------------------------------------------

test('a new transfer starts from the account of the page, else the last one used, to the first other one', () => {
  const list = [main, card, cash, old]
  assert.deepEqual(transferStart(list), { from: '1', to: '2' })
  assert.deepEqual(transferStart(list, '3'), { from: '3', to: '1' })
  assert.deepEqual(transferStart(list, '4'), { from: '1', to: '2' }, 'not from an archived account')
  assert.deepEqual(transferStart(list, '99'), { from: '1', to: '2' })
  data.set('choice.account.last', '2')
  assert.deepEqual(transferStart(list), { from: '2', to: '1' })
  assert.deepEqual(transferStart(list, '3'), { from: '3', to: '1' }, 'the page wins over the last one used')
  assert.deepEqual(transferStart([main, old]), { from: '1', to: '' }, 'nothing to move to')
  assert.deepEqual(transferStart([]), { from: '', to: '' })
})

test('choosing the account the other side holds swaps the two', () => {
  assert.deepEqual(chooseSide({ from: '1', to: '2' }, 'from', '2'), { from: '2', to: '1' })
  assert.deepEqual(chooseSide({ from: '1', to: '2' }, 'to', '1'), { from: '2', to: '1' })
  assert.deepEqual(chooseSide({ from: '1', to: '2' }, 'from', '3'), { from: '3', to: '2' })
  assert.deepEqual(chooseSide({ from: '1', to: '2' }, 'to', '3'), { from: '1', to: '3' })
  assert.deepEqual(chooseSide({ from: '1', to: '2' }, 'from', '1'), { from: '1', to: '2' }, 'the one it has')
  assert.deepEqual(chooseSide({ from: '1', to: '' }, 'to', '1'), { from: '', to: '1' }, 'with an empty side the account moves over')
  // whatever is chosen, in whatever order, the two sides never hold the same account
  let sides = { from: '1', to: '2' }
  for (const [side, id] of [['from', '2'], ['to', '2'], ['to', '3'], ['from', '3'], ['from', '1'], ['to', '1']] as const) {
    sides = chooseSide(sides, side, id)
    assert.notEqual(sides.from, sides.to, `${side} ${id}`)
  }
})

const fields = (more: Partial<TransferFields> = {}): TransferFields => ({ from: '1', to: '2', amount: '200', date: TODAY, note: '', arrives: '', arrivesTouched: false, ...more })
const saved: Transfer = { id: '7', fromAccountId: '1', toAccountId: '2', amount: '200.00', toAmount: null, date: '2026-11-03', note: 'Rainy day' }
const asSaved = fields({ date: '2026-11-03', note: 'Rainy day' })

test('a new transfer is sent whole, a note only when there is one', () => {
  assert.deepEqual(transferSubmit('EDITING', null, fields()), {
    do: 'send',
    request: { create: { fromAccountId: '1', toAccountId: '2', amount: '200.00', date: TODAY } },
  })
  assert.deepEqual(transferSubmit('EDITING', null, fields({ amount: '0,01', note: '  to savings\nfor the roof ' })), {
    do: 'send',
    request: { create: { fromAccountId: '1', toAccountId: '2', amount: '0.01', date: TODAY, note: 'to savings\nfor the roof' } },
  })
})

test('a transfer needs two different accounts and an amount above zero', () => {
  for (const sides of [{ from: '1', to: '1' }, { from: '', to: '2' }, { from: '1', to: '' }, { from: '', to: '' }]) {
    assert.deepEqual(transferSubmit('EDITING', null, fields(sides)), { do: 'refuse', error: 'transfers.errorAccounts' })
  }
  for (const amount of ['', '0', '0.00', '-1', '1.555', '10000000000', 'abc']) {
    assert.deepEqual(transferSubmit('EDITING', null, fields({ amount })), { do: 'refuse', error: 'tx.errorAmount' }, amount)
  }
  assert.deepEqual(transferSubmit('EDITING', null, fields({ from: '1', to: '1', amount: '' })), { do: 'refuse', error: 'transfers.errorAccounts' }, 'the accounts are said first')
  assert.deepEqual(transferSubmit('EDITING', saved, { ...asSaved, to: '1' }), { do: 'refuse', error: 'transfers.errorAccounts' }, 'an edit too')
})

test('an edited transfer sends only what changed, and nothing when nothing did', () => {
  assert.deepEqual(transferSubmit('EDITING', saved, asSaved), { do: 'close' })
  assert.deepEqual(transferSubmit('EDITING', saved, { ...asSaved, amount: '200,0', note: ' Rainy day ' }), { do: 'close' })
  assert.deepEqual(transferSubmit('EDITING', saved, { ...asSaved, amount: '250' }), { do: 'send', request: { patch: { amount: '250.00' } } })
  assert.deepEqual(transferSubmit('EDITING', saved, { ...asSaved, note: '' }), { do: 'send', request: { patch: { note: null } } }, 'an emptied note is cleared')
  assert.deepEqual(transferSubmit('EDITING', saved, { ...asSaved, to: '3', date: '2026-11-04' }), { do: 'send', request: { patch: { toAccountId: '3', date: '2026-11-04' } } })
  assert.deepEqual(transferSubmit('EDITING', saved, { ...asSaved, ...chooseSide(asSaved, 'from', '2') }), {
    do: 'send',
    request: { patch: { fromAccountId: '2', toAccountId: '1' } },
  })
  // a side that is archived by now and is left alone is not named, so the app does not check it
  const withOld: Transfer = { ...saved, toAccountId: '4', note: null }
  assert.deepEqual(transferSubmit('EDITING', withOld, fields({ from: '1', to: '4', amount: '10', date: '2026-11-03', note: '' })), { do: 'send', request: { patch: { amount: '10.00' } } })
})

// -- the currency of an account (docs/accounts-design.md, section 21.8) ----------------------------------------------------------------

test('a currency is three letters, trimmed and in capitals', () => {
  assert.equal(cleanCurrency('RON'), 'RON')
  assert.equal(cleanCurrency('ron'), 'RON', 'accepted in lower case, sent in capitals')
  assert.equal(cleanCurrency(' rOn '), 'RON')
  for (const bad of ['', '  ', 'RO', 'RONN', 'R0N', '123', 'R N', 'R-N', 'éur', 'ŞON']) assert.equal(cleanCurrency(bad), null, JSON.stringify(bad))
})

test('the app\'s own currency is told in any case, with spaces round it', () => {
  assert.equal(isMainCurrency('eur', 'EUR'), true)
  assert.equal(isMainCurrency(' EUR ', 'EUR'), true)
  assert.equal(isMainCurrency('EUR', 'eur'), true)
  assert.equal(isMainCurrency('RON', 'EUR'), false)
  assert.equal(isMainCurrency('', 'EUR'), false)
})

test('the currency of the main account and of an account with entries is shut; a new account\'s is not', () => {
  assert.equal(currencyLock(null), null, 'a new account')
  assert.equal(currencyLock(main), 'main')
  assert.equal(currencyLock(account('1', 'Main', { entries: 5 })), 'main', 'the main account says so before it says entries')
  assert.equal(currencyLock(cash), null, 'an empty account')
  assert.equal(currencyLock(account('3', 'Cash', { entries: 1 })), 'entries', 'one entry is enough')
  assert.equal(currencyLock(account('3', 'Cash', { entries: 0, archived: true })), null)
  assert.equal(currencyLock(account('10', 'Lei', { ...lei, entries: 2 })), 'entries')
})

test('a new account opens in the app\'s currency, an account in its own', () => {
  assert.equal(accountFields(null, '', TODAY).currency, 'EUR')
  assert.equal(fieldsOf(null, '', TODAY, 'ron').currency, 'RON', 'the app\'s currency as the app says it, in capitals')
  assert.equal(accountFields(cash, 'Cash', TODAY).currency, 'EUR')
  assert.deepEqual([accountFields(lei, 'Lei account', TODAY).currency, accountFields(lei, 'Lei account', TODAY).rate], ['RON', '5.00'])
  assert.equal(accountFields(lei, 'Lei account', TODAY).balanceToday, '', 'balance today starts empty')
  assert.equal(accountFields(card, 'Card', TODAY).rate, '', 'an account in the app\'s currency has no rate to show')
})

test('the code of the labels is there once the currency typed is three letters and not the app\'s', () => {
  const code = (currency: string) => foreignCode({ currency }, MAIN)
  assert.equal(code('RON'), 'RON')
  assert.equal(code('ron'), 'RON')
  assert.equal(code('EUR'), null)
  assert.equal(code('eur'), null)
  assert.equal(code('RO'), null, 'not yet')
  assert.equal(code(''), null)
  assert.equal(code('R0N'), null)
})

test('the rate row shows from the first letter of another currency, so it does not come and go with each letter', () => {
  const row = (a: Account | null, currency: string) => rateRow(a, { currency }, MAIN)
  assert.equal(row(null, 'EUR'), null, 'a new account in the app\'s currency')
  assert.equal(row(null, 'eur'), null)
  assert.equal(row(null, ''), null, 'emptied, to type another')
  assert.deepEqual(row(null, 'R'), { code: 'R' })
  assert.deepEqual(row(null, 'ro'), { code: 'RO' })
  assert.deepEqual(row(null, 'ron'), { code: 'RON' })
  assert.deepEqual(row(null, ' ron '), { code: 'RON' })
  assert.equal(row(null, 'EU')?.code, 'EU', 'the beginning of the app\'s own is another currency until it is complete')
  // a shut field: the rate of an account in another currency can still be changed, so its row shows; the main account's never does
  assert.equal(row(main, 'EUR'), null)
  assert.equal(row(main, 'RON'), null)
  assert.equal(row(account('3', 'Cash', { entries: 1 }), 'EUR'), null)
  assert.equal(row(account('3', 'Cash', { entries: 1 }), 'RON'), null, 'what is typed in a shut field means nothing')
  assert.deepEqual(row({ ...lei, entries: 4 }, 'RON'), { code: 'RON' })
  assert.deepEqual(row(lei, 'RON'), { code: 'RON' })
})

test('a new account in another currency is sent with its currency in capitals and its rate', () => {
  const create = (more: Partial<AccountFields>) => accountSubmit('EDITING', null, null, { ...accountFields(null, '', TODAY), name: 'Lei account', ...more })
  assert.deepEqual(create({ currency: 'RON', rate: '5.00' }), { do: 'send', request: { create: { name: 'Lei account', kind: 'CHECKING', currency: 'RON', rate: '5.00' } } })
  assert.deepEqual(create({ currency: 'ron', rate: ' 4.97 ' }), { do: 'send', request: { create: { name: 'Lei account', kind: 'CHECKING', currency: 'RON', rate: '4.97' } } })
  // the opening balance is in the account's currency and is not converted
  assert.deepEqual(create({ currency: 'RON', rate: '5', balance: '800', date: '2026-08-01' }), {
    do: 'send',
    request: { create: { name: 'Lei account', kind: 'CHECKING', openingBalance: '800.00', openingDate: '2026-08-01', currency: 'RON', rate: '5' } },
  })
})

test('a new account in the app\'s currency sends neither a currency nor a rate, whatever case and whatever rate was typed', () => {
  const create = (more: Partial<AccountFields>) => accountSubmit('EDITING', null, null, { ...accountFields(null, '', TODAY), name: 'Savings', ...more })
  const plain = { do: 'send', request: { create: { name: 'Savings', kind: 'CHECKING' } } }
  assert.deepEqual(create({}), plain)
  assert.deepEqual(create({ currency: 'EUR' }), plain)
  assert.deepEqual(create({ currency: 'eur' }), plain)
  assert.deepEqual(create({ currency: ' Eur ' }), plain)
  assert.deepEqual(create({ currency: 'EUR', rate: '5.00' }), plain, 'a rate left from before in a field that is hidden is not sent')
  assert.deepEqual(create({ currency: 'EUR', rate: 'abc' }), plain, 'and is not looked at')
})

test('a currency of two letters, four, digits or nothing is refused, before the rate and the amounts', () => {
  const create = (more: Partial<AccountFields>) => accountSubmit('EDITING', null, null, { ...accountFields(null, '', TODAY), name: 'Lei account', ...more })
  for (const currency of ['', '  ', 'RO', 'R', 'RONN', 'R0N', '123', 'R N']) {
    assert.deepEqual(create({ currency, rate: '5.00' }), { do: 'refuse', error: 'accounts.currencyError' }, JSON.stringify(currency))
  }
  assert.deepEqual(create({ currency: 'RO', rate: 'abc', balance: 'abc' }), { do: 'refuse', error: 'accounts.currencyError' }, 'the currency is said first')
  assert.deepEqual(create({ name: '', currency: 'RO' }), { do: 'refuse', error: 'accounts.nameError' }, 'the name before the currency')
})

test('a rate is needed exactly when the currency is another than the app\'s, and is above 0', () => {
  const create = (more: Partial<AccountFields>) => accountSubmit('EDITING', null, null, { ...accountFields(null, '', TODAY), name: 'Lei account', ...more })
  for (const rate of ['', ' ', '0', '0.0', '0.00', '-1', '-0.5', 'abc', '5,00', '5.', '.5', '1e3', '+5', '5 00', '1234567890', '1.123456789']) {
    assert.deepEqual(create({ currency: 'RON', rate }), { do: 'refuse', error: 'accounts.rateError' }, JSON.stringify(rate))
  }
  for (const rate of ['1', '5', '5.00', '0.0063', '0.00000001', '123456789.12345678']) assert.equal(create({ currency: 'RON', rate }).do, 'send', rate)
  assert.deepEqual(create({ currency: 'RON', rate: 'abc', balance: 'abc' }), { do: 'refuse', error: 'accounts.rateError' }, 'the rate before the amounts')
  assert.equal(create({ currency: 'EUR', rate: '' }).do, 'send', 'no rate for the app\'s own currency')
})

// A patch of the currency: the account, what was typed, and the answer.
const edit = (a: Account, more: Partial<AccountFields>) => {
  const start = accountFields(a, a.name, TODAY)
  return accountSubmit('EDITING', a, start, { ...start, ...more })
}
const patched = (a: Account, more: Partial<AccountFields>) => (edit(a, more) as { request?: { patch: unknown } }).request?.patch

test('an account in the app\'s currency made another one sends the currency and its rate', () => {
  assert.deepEqual(patched(cash, { currency: 'RON', rate: '5.00' }), { currency: 'RON', rate: '5.00' })
  assert.deepEqual(patched(cash, { currency: 'ron', rate: '5' }), { currency: 'RON', rate: '5' })
  assert.deepEqual(edit(cash, { currency: 'RON', rate: '' }), { do: 'refuse', error: 'accounts.rateError' }, 'the rate is needed')
  assert.deepEqual(edit(cash, { currency: 'RON', rate: '0' }), { do: 'refuse', error: 'accounts.rateError' })
  assert.deepEqual(edit(cash, { currency: 'RO' }), { do: 'refuse', error: 'accounts.currencyError' })
  assert.deepEqual(patched(cash, { currency: 'RON', rate: '5.00', name: 'Lei cash' }), { name: 'Lei cash', currency: 'RON', rate: '5.00' }, 'with other changes')
})

test('the rate alone of an account in another currency is sent when it changed, also when its currency is shut', () => {
  const used = { ...lei, entries: 3 }
  assert.deepEqual(patched(used, { rate: '4.95' }), { rate: '4.95' }, 'with entries: the rate may change at any time')
  assert.deepEqual(patched(lei, { rate: '4.95' }), { rate: '4.95' }, 'with none')
  assert.deepEqual(edit(used, { rate: '5.0' }), { do: 'close' }, 'the same number written another way')
  assert.deepEqual(edit(used, { rate: ' 5.00 ' }), { do: 'close' })
  assert.deepEqual(edit(used, { rate: '5.000' }), { do: 'close' })
  assert.deepEqual(edit(used, { rate: '' }), { do: 'refuse', error: 'accounts.rateError' }, 'a rate emptied is refused, not sent')
  assert.deepEqual(edit(used, { rate: '0' }), { do: 'refuse', error: 'accounts.rateError' })
  assert.deepEqual(edit(used, { rate: '-1' }), { do: 'refuse', error: 'accounts.rateError' })
  assert.deepEqual(edit(used, { rate: 'abc' }), { do: 'refuse', error: 'accounts.rateError' })
  assert.deepEqual(edit(used, { rate: '5,00' }), { do: 'refuse', error: 'accounts.rateError' }, 'a comma is not a rate: as in the change of the app\'s currency')
})

test('a currency changed to another foreign one is sent with its rate, even when the rate is the same number', () => {
  assert.deepEqual(patched(lei, { currency: 'USD', rate: '1.08' }), { currency: 'USD', rate: '1.08' })
  assert.deepEqual(patched(lei, { currency: 'usd' }), { currency: 'USD', rate: '5.00' }, 'the rate that was there is sent again, so the pair is explicit')
  assert.deepEqual(patched(lei, { currency: 'ron', rate: '4.9' }), { rate: '4.9' }, 'the same currency in lower case is not a change')
})

test('an account made back to the app\'s currency sends the app\'s own code and no rate', () => {
  assert.deepEqual(patched(lei, { currency: 'EUR' }), { currency: 'EUR' })
  assert.deepEqual(patched(lei, { currency: 'eur', rate: '5.00' }), { currency: 'EUR' }, 'sent as the app\'s code, whatever case was typed; the rate is not sent')
  assert.deepEqual(patched(lei, { currency: ' Eur ', rate: 'abc' }), { currency: 'EUR' }, 'a rate left in a field that is gone is not looked at')
  assert.deepEqual(patched(lei, { currency: 'EUR', name: 'Euro account' }), { name: 'Euro account', currency: 'EUR' })
  assert.deepEqual(edit(lei, { currency: 'EU' }), { do: 'refuse', error: 'accounts.currencyError' })
  const other = accountFields(lei, 'Lei account', TODAY)
  assert.deepEqual(accountSubmit('EDITING', lei, other, { ...other, currency: 'EUR' }), { do: 'send', request: { patch: { currency: 'EUR' } } })
  // an account that was made another currency on the other device while the app's currency was changed there: it is foreign with the app's own code;
  // the dialog sends nothing about its currency until the currency is changed (a save of a rename does not turn it into another account)
  const odd = account('13', 'Odd', { currency: 'EUR', foreign: true, rate: '1.00' })
  assert.deepEqual(edit(odd, {}), { do: 'close' }, 'nothing changed: nothing sent')
  assert.deepEqual(patched(odd, { currency: 'EUR', name: 'Fixed' }), { name: 'Fixed' }, 'the same code is not a change')
  assert.deepEqual(patched(odd, { currency: 'RON', rate: '5' }), { currency: 'RON', rate: '5' }, 'it can be made a real foreign account')
})

test('a save with nothing changed sends nothing, in any currency', () => {
  for (const a of [main, cash, card, lei, { ...lei, entries: 7 }, usd]) assert.deepEqual(edit(a, {}), { do: 'close' }, a.name)
})

test('the currency of the main account and of an account with entries is never sent, whatever is typed', () => {
  assert.deepEqual(edit(main, { currency: 'RON', rate: '5.00' }), { do: 'close' })
  assert.deepEqual(edit(main, { currency: 'RO', rate: 'abc' }), { do: 'close' }, 'a shut field is not looked at')
  assert.deepEqual(patched(main, { currency: 'RON', name: 'Home' }), { name: 'Home' })
  const used = account('3', 'Cash', { entries: 2 })
  assert.deepEqual(edit(used, { currency: 'RON', rate: '5.00' }), { do: 'close' })
  assert.deepEqual(patched(used, { currency: 'RON', rate: '5.00', name: 'Notes' }), { name: 'Notes' })
  // an account in another currency with entries: its currency stays, and typing another one changes nothing (the rate row shows the account's own code)
  const usedLei = { ...lei, entries: 2 }
  assert.deepEqual(edit(usedLei, { currency: 'USD' }), { do: 'close' })
  assert.deepEqual(edit(usedLei, { currency: 'EUR' }), { do: 'close' }, 'it does not become an account in the app\'s currency while entries point at it')
  assert.deepEqual(patched(usedLei, { currency: 'USD', rate: '4.5' }), { rate: '4.5' }, 'its rate still changes')
})

test('an archived account that is somehow changed keeps the rules of its currency', () => {
  const archived = { ...lei, archived: true }
  assert.deepEqual(patched(archived, { currency: 'USD', rate: '1.08' }), { currency: 'USD', rate: '1.08' })
})

// -- "balance today" ---------------------------------------------------------------------------------------------------------------

test('a balance today is not sent while it is empty, spaces alone being empty', () => {
  assert.deepEqual(edit(cash, { balanceToday: '' }), { do: 'close' })
  assert.deepEqual(edit(cash, { balanceToday: '   ' }), { do: 'close' })
  assert.deepEqual(patched(cash, { balanceToday: ' ', name: 'Wallet' }), { name: 'Wallet' })
  assert.equal(balanceTodayFilled({ balanceToday: '' }), false)
  assert.equal(balanceTodayFilled({ balanceToday: '  ' }), false)
  assert.equal(balanceTodayFilled({ balanceToday: '0' }), true, 'a balance of nothing is one')
})

test('a balance today is sent as `balance`, in the account\'s currency, signed', () => {
  const held = account('3', 'Cash', { balance: '100.00', openingBalance: '50.00', openingDate: '2026-01-01' })
  assert.deepEqual(patched(held, { balanceToday: '1662.90' }), { balance: '1662.90' })
  assert.deepEqual(patched(held, { balanceToday: '1662,9' }), { balance: '1662.90' }, 'a comma is a point')
  assert.deepEqual(patched(held, { balanceToday: '-12.5' }), { balance: '-12.50' }, 'a card below zero')
  assert.deepEqual(patched(held, { balanceToday: '0' }), { balance: '0.00' }, 'holding nothing today is a balance')
  assert.deepEqual(patched(held, { balanceToday: '9999999999.99' }), { balance: '9999999999.99' })
  assert.deepEqual(patched(lei, { balanceToday: '1500' }), { balance: '1500.00' }, 'an account in lei: lei')
  assert.deepEqual(patched(held, { balanceToday: '1.5' }), { balance: '1.50' }, 'one decimal is padded, as the opening balance pads it')
})

test('a balance today equal to the balance now writes nothing', () => {
  const held = account('3', 'Cash', { balance: '100.00', openingBalance: '50.00', openingDate: '2026-01-01' })
  assert.deepEqual(edit(held, { balanceToday: '100.00' }), { do: 'close' })
  assert.deepEqual(edit(held, { balanceToday: '100' }), { do: 'close' })
  assert.deepEqual(edit(held, { balanceToday: ' 100,0 ' }), { do: 'close' })
  assert.deepEqual(edit(lei, { balanceToday: '1662.90' }), { do: 'close' })
  assert.deepEqual(patched(held, { balanceToday: '100', name: 'Wallet' }), { name: 'Wallet' }, 'the other changes are still sent')
  assert.deepEqual(edit(account('3', 'Cash', { balance: '-0.00' }), { balanceToday: '0' }), { do: 'send', request: { patch: { balance: '0.00' } } }, 'the app never sends "-0.00"; if it did, text is not the number')
})

test('a balance today is refused as the opening balance is: "-0.00", "+1", "1,234.50", a third decimal and too much', () => {
  for (const bad of ['-', '-0', '-0.00', '-0,00', '+1', '+1.00', '1.555', '1,234.50', 'abc', '10000000000', '-10000000000.00', '- 5', '1e3', '--1', '5-']) {
    assert.deepEqual(edit(cash, { balanceToday: bad }), { do: 'refuse', error: 'accounts.balanceError' }, bad)
  }
  assert.deepEqual(edit(cash, { balanceToday: '-9999999999.99' }), { do: 'send', request: { patch: { balance: '-9999999999.99' } } }, 'the bound itself')
})

test('balance and opening balance are never sent together: when balance today is filled the opening balance is not used', () => {
  const held = account('3', 'Cash', { balance: '100.00', openingBalance: '50.00', openingDate: '2026-01-01' })
  assert.deepEqual(patched(held, { balanceToday: '200', balance: '70' }), { balance: '200.00' }, 'the opening balance typed is left out')
  assert.deepEqual(patched(held, { balanceToday: '200', balance: 'abc' }), { balance: '200.00' }, 'and is not looked at')
  assert.deepEqual(patched(held, { balanceToday: '100', balance: '70' }), undefined, 'a balance today that changes nothing does not let the opening balance through')
  assert.deepEqual(patched(held, { balanceToday: '', balance: '70' }), { openingBalance: '70.00' }, 'emptied, the opening balance works as before')
  assert.deepEqual(patched(held, { balanceToday: '200', date: '2026-02-01' }), { balance: '200.00', openingDate: '2026-02-01' }, 'the date of the opening balance may go with it')
  // an account with no opening balance has no date to move
  assert.deepEqual(patched(cash, { balanceToday: '200' }), { balance: '200.00' })
  const everyCase: Partial<AccountFields>[] = [{}, { balance: '1' }, { balance: '' }, { balance: 'x' }, { balanceToday: '5' }, { balanceToday: '5', balance: '1' }, { balanceToday: '0' }, { balanceToday: '5', date: '2020-01-01', balance: '' }]
  for (const a of [main, cash, card, lei, held]) {
    for (const more of everyCase) {
      const patch = patched(a, more) as Record<string, unknown> | undefined
      assert.ok(!(patch && 'balance' in patch && 'openingBalance' in patch), `${a.name} ${JSON.stringify(more)}`)
    }
  }
})

test('a balance today is for an account that exists: a new account does not send it', () => {
  const create = accountSubmit('EDITING', null, null, { ...accountFields(null, '', TODAY), name: 'Savings', balanceToday: '5', balance: '70', date: '2026-02-01' })
  assert.deepEqual(create, { do: 'send', request: { create: { name: 'Savings', kind: 'CHECKING', openingBalance: '70.00', openingDate: '2026-02-01' } } })
})

test('filling balance today puts the opening balance back to what it holds, and emptying it lets it be typed again', () => {
  const start = accountFields(account('3', 'Cash', { openingBalance: '50.00', openingDate: '2026-01-01' }), 'Cash', TODAY)
  const typedOpening = { ...start, balance: '99' }
  const filled = withBalanceToday(typedOpening, start, '200')
  assert.deepEqual([filled.balanceToday, filled.balance], ['200', '50.00'])
  assert.equal(filled.name, start.name, 'nothing else moves')
  const more = withBalanceToday(filled, start, '2000')
  assert.deepEqual([more.balanceToday, more.balance], ['2000', '50.00'])
  const emptied = withBalanceToday(more, start, '')
  assert.deepEqual([emptied.balanceToday, emptied.balance], ['', '50.00'], 'it can be typed in again')
  assert.equal(withBalanceToday({ ...start, balance: '77' }, start, '  ').balance, '77', 'spaces alone are empty: the opening balance is kept as typed')
})

// -- the transfer dialog and two currencies (section 21.8) ----------------------------------------------------------------------------

const sides = (from: string, to: string) => ({ from, to })

test('which transfers need a second amount: those between different currencies', () => {
  const need = (from: Account, to: Account) => transferCurrencies([from, to], sides(from.id, to.id))
  assert.deepEqual(need(main, cash), { amount: null, arrives: null }, 'two accounts in the app\'s currency: one field, as before')
  assert.deepEqual(need(lei, lei2), { amount: 'RON', arrives: null }, 'two in lei: one field, said to be in lei')
  assert.deepEqual(need(main, lei), { amount: 'EUR', arrives: 'RON' })
  assert.deepEqual(need(lei, main), { amount: 'RON', arrives: 'EUR' })
  assert.deepEqual(need(lei, usd), { amount: 'RON', arrives: 'USD' })
  assert.deepEqual(need(usd, lei), { amount: 'USD', arrives: 'RON' })
  assert.deepEqual(transferCurrencies(everyone, sides('99', '1')), { amount: null, arrives: null }, 'a side that is not there')
  assert.deepEqual(transferCurrencies(everyone, sides('1', '')), { amount: null, arrives: null })
  assert.deepEqual(transferCurrencies(everyone, sides('10', '99')), { amount: 'RON', arrives: null })
  // an account made in the app's own code on another device is a different currency from the main account (the app compares what is stored)
  const odd = account('13', 'Odd', { currency: 'EUR', foreign: true, rate: '1.00' })
  assert.deepEqual(need(main, odd), { amount: 'EUR', arrives: 'EUR' })
})

test('what arrives is worked out through the app\'s currency at the two rates, half up to the cent', () => {
  const arrival = (from: string, to: string, amount: string) => suggestedArrival(everyone, { from, to, amount })
  assert.equal(arrival('1', '10', '100'), '500.00', 'euros to lei: times the rate')
  assert.equal(arrival('10', '1', '500'), '100.00', 'lei to euros: divided by it')
  assert.equal(arrival('10', '1', '113.74'), '22.75', '113.74 / 5.00 = 22.748')
  assert.equal(arrival('10', '12', '100'), '21.60', 'lei to dollars: 100 / 5.00 x 1.08')
  assert.equal(arrival('12', '10', '21.60'), '100.00', 'and back')
  assert.equal(arrival('1', '10', '12,5'), '62.50', 'a comma is a point')
  assert.equal(arrival('1', '10', ' 7 '), '35.00')
  assert.equal(arrival('1', '3', '100'), null, 'the same currency: none')
  assert.equal(arrival('10', '11', '100'), null, 'lei to lei: none, whatever the two rates')
  assert.equal(arrival('1', '10', ''), null)
  assert.equal(arrival('1', '10', '0'), null)
  assert.equal(arrival('1', '10', 'abc'), null)
  assert.equal(arrival('1', '10', '-5'), null)
  assert.equal(arrival('1', '99', '100'), null, 'a side that is not there')
  // the chain euros to lei to euros gives the amount back, at a round rate and at a rate with decimals
  for (const amount of ['33.33', '0.01', '1234.56', '9999999.99']) {
    const lei5 = suggestedArrival(everyone, { from: '1', to: '10', amount })
    assert.equal(suggestedArrival(everyone, { from: '10', to: '1', amount: lei5 ?? '' }), amount, amount)
  }
  const odd = [main, account('14', 'Lei', { currency: 'RON', foreign: true, rate: '4.9712' })]
  for (const amount of ['33.33', '100.00', '250.10']) {
    const there = suggestedArrival(odd, { from: '1', to: '14', amount })
    assert.equal(suggestedArrival(odd, { from: '14', to: '1', amount: there ?? '' }), amount, `${amount} at 4.9712`)
  }
  // a rate above 1 turns a small amount into nothing: the second field is then empty, and has to be typed
  assert.equal(arrival('10', '1', '0.02'), '0.00')
})

test('the second amount follows the rates until it is typed in, and follows them again when it is emptied', () => {
  let typed = fields({ from: '1', to: '10', amount: '100' })
  assert.equal(arrivalText(everyone, typed), '500.00')
  typed = { ...typed, amount: '120' }
  assert.equal(arrivalText(everyone, typed), '600.00', 'it follows the amount')
  typed = withArrival(typed, '590')
  assert.deepEqual([typed.arrives, typed.arrivesTouched], ['590', true])
  assert.equal(arrivalText(everyone, typed), '590', 'theirs now')
  assert.equal(arrivalText(everyone, { ...typed, amount: '200' }), '590', 'a new amount does not change it')
  typed = withArrival(typed, '')
  assert.deepEqual([typed.arrives, typed.arrivesTouched], ['', false])
  assert.equal(arrivalText(everyone, typed), '600.00', 'emptied: the rates again')
  assert.equal(withArrival(typed, '   ').arrivesTouched, false, 'spaces alone are nothing')
  assert.equal(arrivalText(everyone, fields({ from: '1', to: '3', amount: '5', arrives: '9', arrivesTouched: true })), '9', 'the text is what it is; the owner shows it only between two currencies')
  assert.equal(arrivalText(everyone, fields({ from: '1', to: '10', amount: '', arrives: '', arrivesTouched: false })), '', 'no amount, nothing to work out')
})

test('swapping the sides or changing one shows or hides the second amount, and works out one that was not typed', () => {
  let typed = fields({ from: '1', to: '10', amount: '100' })
  const shown = () => transferCurrencies(everyone, typed).arrives
  assert.equal(shown(), 'RON')
  assert.equal(arrivalText(everyone, typed), '500.00')
  typed = { ...typed, ...chooseSide(typed, 'from', '10') }
  assert.deepEqual([typed.from, typed.to], ['10', '1'])
  assert.equal(shown(), 'EUR', 'swapped: lei to euros')
  assert.equal(arrivalText(everyone, typed), '20.00', 'the same 100, now lei')
  typed = { ...typed, ...chooseSide(typed, 'to', '11') }
  assert.equal(shown(), null, 'lei to lei: hidden')
  assert.equal(arrivalText(everyone, typed), '', 'and nothing to send')
  typed = { ...typed, ...chooseSide(typed, 'to', '12') }
  assert.equal(shown(), 'USD')
  assert.equal(arrivalText(everyone, typed), '21.60')
  typed = { ...typed, ...chooseSide(typed, 'to', '3') }
  assert.equal(arrivalText(everyone, typed), '20.00', 'lei to cash in euros')
  typed = { ...typed, ...chooseSide(typed, 'from', '1') }
  assert.equal(shown(), null, 'two accounts in the app\'s currency: hidden')
  // one that was typed stays as it was typed, whatever the sides (the labels change; the person fixes the numbers)
  const theirs = withArrival(fields({ from: '1', to: '10', amount: '100' }), '495')
  const swapped = { ...theirs, ...chooseSide(theirs, 'from', '10') }
  assert.deepEqual([swapped.amount, swapped.arrives, swapped.arrivesTouched], ['100', '495', true])
  assert.equal(arrivalText(everyone, swapped), '495')
})

test('a new transfer between two currencies is sent with the amount that arrives, typed or worked out', () => {
  const create = (more: Partial<TransferFields>) => transferSubmit('EDITING', null, fields({ from: '1', to: '10', amount: '100', ...more }))
  assert.deepEqual(create({}), { do: 'send', request: { create: { fromAccountId: '1', toAccountId: '10', amount: '100.00', toAmount: '500.00', date: TODAY } } })
  assert.deepEqual(create({ arrives: '495,5', arrivesTouched: true }), { do: 'send', request: { create: { fromAccountId: '1', toAccountId: '10', amount: '100.00', toAmount: '495.50', date: TODAY } } })
  assert.deepEqual(create({ from: '10', to: '1', amount: '250', note: 'back' }), { do: 'send', request: { create: { fromAccountId: '10', toAccountId: '1', amount: '250.00', toAmount: '50.00', date: TODAY, note: 'back' } } })
  assert.deepEqual(create({ from: '10', to: '12', amount: '100' }), { do: 'send', request: { create: { fromAccountId: '10', toAccountId: '12', amount: '100.00', toAmount: '21.60', date: TODAY } } })
  // emptied after typing: the rates again
  assert.deepEqual(create({ arrives: '', arrivesTouched: false }), { do: 'send', request: { create: { fromAccountId: '1', toAccountId: '10', amount: '100.00', toAmount: '500.00', date: TODAY } } })
})

test('a transfer between two accounts in the same currency sends no second amount, even with one left typed', () => {
  const create = (more: Partial<TransferFields>) => transferSubmit('EDITING', null, fields({ amount: '100', ...more }))
  const plain = (from: string, to: string) => ({ do: 'send', request: { create: { fromAccountId: from, toAccountId: to, amount: '100.00', date: TODAY } } })
  assert.deepEqual(create({ from: '1', to: '3', arrives: '500', arrivesTouched: true }), plain('1', '3'), 'euros to euros')
  assert.deepEqual(create({ from: '10', to: '11', arrives: '500', arrivesTouched: true }), plain('10', '11'), 'lei to lei')
  assert.deepEqual(create({ from: '10', to: '11' }), plain('10', '11'))
  assert.deepEqual(create({ from: '1', to: '3' }), plain('1', '3'))
})

test('a second amount that is missing or wrong is refused, once the first and the accounts are right', () => {
  const create = (more: Partial<TransferFields>) => transferSubmit('EDITING', null, fields({ from: '1', to: '10', amount: '100', ...more }))
  for (const arrives of ['0', '0.00', '-5', '5.555', 'abc', '10000000000', '1,234.50']) {
    assert.deepEqual(create({ arrives, arrivesTouched: true }), { do: 'refuse', error: 'transfers.errorArrives' }, arrives)
  }
  assert.deepEqual(transferSubmit('EDITING', null, fields({ from: '10', to: '1', amount: '0.02' })), { do: 'refuse', error: 'transfers.errorArrives' }, 'worked out as nothing: it has to be typed')
  assert.deepEqual(create({ amount: '' }), { do: 'refuse', error: 'tx.errorAmount' }, 'the amount before the arrival')
  assert.deepEqual(create({ to: '1' }), { do: 'refuse', error: 'transfers.errorAccounts' }, 'the accounts before the amounts')
})

const crossing: Transfer = { id: '8', fromAccountId: '1', toAccountId: '10', amount: '100.00', toAmount: '495.00', date: '2026-11-03', note: null }

test('a transfer with a second amount opens with it filled and typed, and one without opens following the rates', () => {
  assert.deepEqual(transferFieldsOfRow(crossing), { from: '1', to: '10', amount: '100.00', date: '2026-11-03', note: '', arrives: '495.00', arrivesTouched: true })
  assert.deepEqual(transferFieldsOfRow(saved), { from: '1', to: '2', amount: '200.00', date: '2026-11-03', note: 'Rainy day', arrives: '', arrivesTouched: false })
  assert.deepEqual(transferFieldsOf({ from: '1', to: '10' }, TODAY), { from: '1', to: '10', amount: '', date: TODAY, note: '', arrives: '', arrivesTouched: false })
})

test('an edited transfer sends the amount that arrives when it is new to the row or changed, and keeps one it has', () => {
  const asCrossing = transferFieldsOfRow(crossing)
  const patch = (more: Partial<TransferFields>) => transferSubmit('EDITING', crossing, { ...asCrossing, ...more })
  assert.deepEqual(patch({}), { do: 'close' }, 'nothing changed')
  assert.deepEqual(patch({ amount: '110' }), { do: 'send', request: { patch: { amount: '110.00' } } }, 'the one it has is kept: the person\'s own stays')
  assert.deepEqual(patch({ arrives: '490' }), { do: 'send', request: { patch: { toAmount: '490.00' } } })
  assert.deepEqual(patch({ arrives: '495' }), { do: 'close' }, 'the same amount written another way')
  assert.deepEqual(patch({ to: '12' }), { do: 'send', request: { patch: { toAccountId: '12' } } }, 'still two currencies: the second amount stays')
  assert.deepEqual(patch({ arrives: '' , arrivesTouched: false }), { do: 'send', request: { patch: { toAmount: '500.00' } } }, 'emptied: it follows the rates, which give another amount')
  assert.deepEqual(patch({ arrives: '0', arrivesTouched: true }), { do: 'refuse', error: 'transfers.errorArrives' })
})

test('a transfer edited so the two sides are in the same currency sends no second amount, and so again when they differ later', () => {
  const asCrossing = transferFieldsOfRow(crossing)
  // euros to lei made euros to euros: the amount that arrived is not sent (the app clears it)
  assert.deepEqual(transferSubmit('EDITING', crossing, { ...asCrossing, to: '3' }), { do: 'send', request: { patch: { toAccountId: '3' } } })
  assert.deepEqual(transferSubmit('EDITING', crossing, { ...asCrossing, to: '3', amount: '80' }), { do: 'send', request: { patch: { toAccountId: '3', amount: '80.00' } } })
  assert.deepEqual(transferSubmit('EDITING', crossing, { ...asCrossing, to: '3', arrives: '1' }), { do: 'send', request: { patch: { toAccountId: '3' } } }, 'whatever is left in the hidden field')
  // lei to lei
  const leiToEuro: Transfer = { ...crossing, fromAccountId: '10', toAccountId: '1', amount: '250.00', toAmount: '50.40' }
  assert.deepEqual(transferSubmit('EDITING', leiToEuro, { ...transferFieldsOfRow(leiToEuro), to: '11' }), { do: 'send', request: { patch: { toAccountId: '11' } } })
  // made the same and made different again in the one dialog: nothing changed in the end
  const there = { ...asCrossing, to: '3' }
  assert.deepEqual(transferSubmit('EDITING', crossing, { ...there, to: '10' }), { do: 'close' })
})

test('a transfer between the same currency edited so they differ is sent with the amount that arrives, which the row does not have', () => {
  const same: Transfer = { ...saved, toAmount: null }
  const asSame = transferFieldsOfRow(same)
  assert.deepEqual(transferSubmit('EDITING', same, { ...asSame, to: '10' }), { do: 'send', request: { patch: { toAccountId: '10', toAmount: '1000.00' } } }, 'worked out from the amount: 200 EUR at 5.00')
  assert.deepEqual(transferSubmit('EDITING', same, withArrival({ ...asSame, to: '10' }, '990')), { do: 'send', request: { patch: { toAccountId: '10', toAmount: '990.00' } } })
  assert.deepEqual(transferSubmit('EDITING', same, { ...asSame, from: '10', to: '12', amount: '100' }), { do: 'send', request: { patch: { fromAccountId: '10', toAccountId: '12', amount: '100.00', toAmount: '21.60' } } })
  assert.deepEqual(transferSubmit('EDITING', same, { ...asSame, to: '10', amount: '0.01' }), { do: 'send', request: { patch: { toAccountId: '10', amount: '0.01', toAmount: '0.05' } } })
  assert.deepEqual(transferSubmit('EDITING', { ...same, fromAccountId: '10', toAccountId: '1', amount: '0.02' }, { ...asSame, from: '10', to: '1', amount: '0.02', date: '2026-11-03' }), { do: 'refuse', error: 'transfers.errorArrives' }, 'a row of lei to euros that was never given its second amount: it has to be typed')
})
