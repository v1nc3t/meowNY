import assert from 'node:assert/strict'
import { afterEach, beforeEach, test } from 'node:test'
import type { Account } from '../api/accounts.ts'
import type { Transfer } from '../api/transfers.ts'
import {
  accountFields,
  accountSubmit,
  chooseSide,
  dialogNext,
  transferStart,
  transferSubmit,
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
  ...more,
})
const main = account('1', 'Main account')
const card = account('2', 'Card', { kind: 'CREDIT_CARD', number: 'DE00 1234', openingBalance: '-120.00', openingDate: '2026-01-01' })
const cash = account('3', 'Cash', { kind: 'CASH' })
const old = account('4', 'Old', { archived: true })

// -- the states (section 5.4) -----------------------------------------------------------------------------------------------------

test('a dialog goes from editing to saving and back on an error, or closes on an answer', () => {
  assert.equal(dialogNext('EDITING', 'send'), 'SAVING')
  assert.equal(dialogNext('SAVING', 'ok'), 'CLOSED')
  assert.equal(dialogNext('SAVING', 'error'), 'EDITING', 'the fields are given back')
})

test('a second submit while saving changes nothing', () => {
  assert.equal(dialogNext('SAVING', 'send'), 'SAVING')
  assert.deepEqual(accountSubmit('SAVING', null, null, accountFields(null, '', TODAY)), { do: 'ignore' }, 'not even a wrong field is looked at')
  assert.deepEqual(transferSubmit('SAVING', null, { from: '1', to: '1', amount: '', date: TODAY, note: '' }), { do: 'ignore' })
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
  assert.deepEqual(accountFields(null, '', TODAY), { name: '', kind: 'CHECKING', number: '', balance: '', date: TODAY })
})

test('an account opens with its row; no opening balance is an empty field dated today', () => {
  assert.deepEqual(accountFields(card, 'Card', TODAY), { name: 'Card', kind: 'CREDIT_CARD', number: 'DE00 1234', balance: '-120.00', date: '2026-01-01' })
  assert.deepEqual(accountFields(main, 'hauptkonto', TODAY), { name: 'hauptkonto', kind: 'CHECKING', number: '', balance: '', date: TODAY })
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

const fields = (more: Partial<TransferFields> = {}): TransferFields => ({ from: '1', to: '2', amount: '200', date: TODAY, note: '', ...more })
const saved: Transfer = { id: '7', fromAccountId: '1', toAccountId: '2', amount: '200.00', date: '2026-11-03', note: 'Rainy day' }
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
  assert.deepEqual(transferSubmit('EDITING', withOld, { from: '1', to: '4', amount: '10', date: '2026-11-03', note: '' }), { do: 'send', request: { patch: { amount: '10.00' } } })
})
