import assert from 'node:assert/strict'
import { afterEach, beforeEach, test } from 'node:test'
import type { Account } from '../api/accounts.ts'
import '../i18n/languages.ts'
import { setActiveLanguage, t } from '../i18n/translate.ts'
import {
  accountChoice,
  accountLabel,
  accountName,
  accountOptions,
  accountsTab,
  activeAccounts,
  barWidth,
  defaultAccountId,
  groupAccounts,
  hasGroups,
  lineWords,
  MAIN_NAME,
  rememberAccount,
  reorderWithinGroup,
  several,
  showAvailable,
  signedView,
  tabQuery,
} from './accountsView.ts'

// A stand-in for the browser's storage; `broken` makes every use throw, as storage does when it is blocked.
let data = new Map<string, string>()
let broken = false
const storage = {
  getItem(key: string) {
    if (broken) throw new Error('blocked')
    return data.get(key) ?? null
  },
  setItem(key: string, value: string) {
    if (broken) throw new Error('blocked')
    data.set(key, value)
  },
}

beforeEach(() => {
  data = new Map()
  broken = false
  Object.defineProperty(globalThis, 'localStorage', { value: storage, configurable: true })
  setActiveLanguage('en')
})

afterEach(() => {
  Reflect.deleteProperty(globalThis, 'localStorage')
  setActiveLanguage('en')
})

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
  ...more,
})
const main = account('1', MAIN_NAME)
const savings = account('2', 'Savings', { kind: 'SAVINGS' })
const cash = account('3', 'Cash', { kind: 'CASH' })
const old = account('4', 'Old card', { kind: 'CREDIT_CARD', archived: true })

test('several is more than one account that is not archived', () => {
  assert.equal(several(null), false, 'not read yet')
  assert.equal(several([]), false)
  assert.equal(several([main]), false)
  assert.equal(several([main, old]), false, 'an archived extra does not count')
  assert.equal(several([main, old, account('5', 'Older', { archived: true })]), false, 'nor do many of them')
  assert.equal(several([main, savings]), true)
  assert.equal(several([main, savings, old]), true)
  assert.deepEqual(activeAccounts([main, old, savings]).map((each) => each.id), ['1', '2'], 'in the order they came in')
})

test('a new row starts in the last account used, when it is still there and not archived', () => {
  const list = [main, savings, cash, old]
  assert.equal(defaultAccountId(list), '1', 'nothing remembered: the first in the list')
  rememberAccount('3')
  assert.equal(data.get('choice.account.last'), '3')
  assert.equal(defaultAccountId(list), '3')
  assert.equal(defaultAccountId([savings, main, cash]), '3', 'whatever the order')
})

test('a remembered account that is gone or archived gives the first active account', () => {
  rememberAccount('3')
  assert.equal(defaultAccountId([main, savings]), '1', 'deleted since')
  assert.equal(defaultAccountId([savings, main, { ...cash, archived: true }]), '2', 'archived since: the first of the list as it is ordered')
  rememberAccount('4')
  assert.equal(defaultAccountId([main, savings, old]), '1', 'an archived one was never a choice')
  for (const junk of ['', '0', 'abc', '1 ', 'null']) {
    data.set('choice.account.last', junk)
    assert.equal(defaultAccountId([savings, main]), '2', junk)
  }
  assert.equal(defaultAccountId([]), '', 'no account at all')
  assert.equal(defaultAccountId([old]), '', 'only archived ones')
})

test('storage that cannot be used never throws: the first account, and nothing is remembered', () => {
  broken = true
  assert.equal(defaultAccountId([main, savings]), '1')
  assert.doesNotThrow(() => rememberAccount('2'))
  assert.equal(defaultAccountId([main, savings]), '1')
  Reflect.deleteProperty(globalThis, 'localStorage')
  assert.equal(defaultAccountId([savings, main]), '2')
  assert.doesNotThrow(() => rememberAccount('2'))
})

test('remembering no account leaves the last one', () => {
  rememberAccount('2')
  rememberAccount('')
  assert.equal(data.get('choice.account.last'), '2')
})

test('the main account reads in the language of the person until it is renamed', () => {
  assert.equal(accountName(main, t), 'main account')
  setActiveLanguage('de')
  assert.equal(accountName(main, t), 'hauptkonto')
  assert.equal(accountName({ ...main, name: 'Girokonto' }, t), 'Girokonto', 'a rename shows as typed')
  assert.equal(accountName({ ...main, name: 'main account' }, t), 'main account', 'only the exact stored name is the untouched one')
  assert.equal(accountName({ ...main, name: 'Main account ' }, t), 'Main account ')
  assert.equal(accountName({ ...savings, name: MAIN_NAME }, t), MAIN_NAME, 'another account with that name is not the main one')
  assert.equal(accountName(savings, t), 'Savings')
})

test('an archived account says so in a form, and only the one a row is in is offered', () => {
  assert.equal(accountLabel(old, t), 'Old card (archived)')
  assert.equal(accountLabel(savings, t), 'Savings')
  const list = [main, savings, old]
  assert.deepEqual(accountOptions(list, t), [{ value: '1', label: 'main account' }, { value: '2', label: 'Savings' }])
  assert.deepEqual(accountOptions(list, t, ['4', undefined]).map((option) => option.label), ['Old card (archived)', 'main account', 'Savings'])
  assert.deepEqual(accountOptions(list, t, ['2']).map((option) => option.value), ['1', '2'], 'an active one is not listed twice')
  assert.deepEqual(accountOptions(list, t, ['99']).map((option) => option.value), ['1', '2'], 'an unknown one is not made up')
})

test('with one account a form has no account field and says nothing about accounts', () => {
  for (const list of [null, [], [main], [main, old]]) {
    const choice = accountChoice(list, null)
    assert.equal(choice.shown, false)
    assert.equal(choice.send, undefined)
  }
  assert.equal(accountChoice([main, old], null, '4').send, undefined, 'a row in an archived account stays there')
  assert.equal(accountChoice([main], '1', undefined).send, undefined, 'even with something picked')
})

test('with several accounts a new row is sent with its account, and a row that exists only when it moves', () => {
  const list = [main, savings, cash, old]
  assert.deepEqual(accountChoice(list, null), { shown: true, value: '1', send: '1' })
  rememberAccount('3')
  assert.deepEqual(accountChoice(list, null), { shown: true, value: '3', send: '3' }, 'a new row starts in the last one used')
  assert.deepEqual(accountChoice(list, '2'), { shown: true, value: '2', send: '2' }, 'what was picked wins')
  assert.deepEqual(accountChoice(list, null, '2'), { shown: true, value: '2', send: undefined }, 'a row keeps its own, not the last used')
  assert.deepEqual(accountChoice(list, null, '4'), { shown: true, value: '4', send: undefined }, 'an archived one too')
  assert.deepEqual(accountChoice(list, '3', '2'), { shown: true, value: '3', send: '3' }, 'moved')
  assert.deepEqual(accountChoice(list, '2', '2'), { shown: true, value: '2', send: undefined }, 'picked the one it had')
})

test('a line of a statement is named by what it was, or by its other side when it has no words', () => {
  assert.deepEqual(lineWords({ source: 'TRANSACTION', what: 'Lidl', other: 'Groceries' }, t), { what: 'Lidl', other: 'Groceries' })
  assert.deepEqual(lineWords({ source: 'TRANSACTION', what: 'Lidl', other: null }, t), { what: 'Lidl', other: '' })
  assert.deepEqual(lineWords({ source: 'OPENING', what: '', other: null }, t), { what: 'opening balance', other: '' })
  assert.deepEqual(lineWords({ source: 'TRANSFER_IN', what: 'Rainy day', other: 'Savings' }, t), { what: 'Rainy day', other: 'transfer from Savings' })
  assert.deepEqual(lineWords({ source: 'TRANSFER_OUT', what: '', other: 'Savings' }, t), { what: 'transfer to Savings', other: '' })
  assert.deepEqual(lineWords({ source: 'LEDGER', what: '', other: 'Anna' }, t), { what: 'Anna', other: '' })
  assert.deepEqual(lineWords({ source: 'LEDGER', what: 'Cash', other: 'Anna' }, t), { what: 'Cash', other: 'Anna' })
})

test('a signed amount shows its sign and its colour, and nothing is plain', () => {
  const format = (amount: string) => `€${amount}`
  assert.deepEqual(signedView('12.50', format), { text: '+€12.50', tone: 'is-income' })
  assert.deepEqual(signedView('-12.50', format), { text: '-€12.50', tone: 'is-expense' })
  assert.deepEqual(signedView('0.00', format), { text: '€0.00', tone: '' })
  assert.deepEqual(signedView('-0.00', format), { text: '€0.00', tone: '' }, 'never a minus on nothing')
})

// -- groups -----------------------------------------------------------------------------------------------------------------------

const names = (groups: ReturnType<typeof groupAccounts>) => groups.map((group) => [group.name, group.accounts.map((a) => a.id)])
const a1 = account('1', 'One', { group: 'Bank' })
const a2 = account('2', 'Two')
const a3 = account('3', 'Three', { group: 'Cards' })
const a4 = account('4', 'Four', { group: 'Bank' })
const a5 = account('5', 'Five', { group: 'Cards', archived: true })

test('with no group in use the list is one run with no name', () => {
  assert.equal(hasGroups([main, savings]), false)
  assert.deepEqual(names(groupAccounts([main, savings, cash])), [[null, ['1', '2', '3']]])
  assert.deepEqual(names(groupAccounts([])), [[null, []]])
  assert.deepEqual(names(groupAccounts([main, account('9', 'Old', { archived: true, group: 'Bank' })])), [[null, ['1']]], 'the group of an archived account does not count')
  assert.equal(hasGroups([account('2', 'X', { group: '' })]), false, 'an empty group is none')
})

test('with some groups the groups come in the order of their first account and the accounts with none come last', () => {
  assert.equal(hasGroups([a1, a2]), true)
  assert.deepEqual(names(groupAccounts([a2, a3, a1, a4])), [['Cards', ['3']], ['Bank', ['1', '4']], [null, ['2']]])
  assert.deepEqual(names(groupAccounts([a1, a2, a3, a4])), [['Bank', ['1', '4']], ['Cards', ['3']], [null, ['2']]], 'a group is one run even when its accounts are apart')
})

test('with every account in a group there is no "no group" run', () => {
  assert.deepEqual(names(groupAccounts([a3, a1, a4])), [['Cards', ['3']], ['Bank', ['1', '4']]])
  assert.deepEqual(names(groupAccounts([a1])), [['Bank', ['1']]])
})

test('an archived account is never in a group, and groups are told apart by their exact name', () => {
  assert.deepEqual(names(groupAccounts([a1, a5, a3, old])), [['Bank', ['1']], ['Cards', ['3']]])
  assert.deepEqual(names(groupAccounts([account('1', 'A', { group: 'bank' }), account('2', 'B', { group: 'Bank' })])), [['bank', ['1']], ['Bank', ['2']]])
})

test('an account moves only among the accounts of its own group, and every account is sent once', () => {
  const all = [a1, a2, a3, a4, a5]
  // the group's two accounts swap; the places of the others stay
  assert.deepEqual(reorderWithinGroup(all, 'Bank', ['4', '1']), ['4', '2', '3', '1'])
  assert.deepEqual(reorderWithinGroup(all, 'Bank', ['1', '4']), ['1', '2', '3', '4'], 'no move, no change')
  assert.deepEqual(reorderWithinGroup(all, null, ['2']), ['1', '2', '3', '4'], 'a run of one')
  assert.deepEqual(reorderWithinGroup([a1, a2, a3, a4], 'Cards', ['3']), ['1', '2', '3', '4'])
  // three in a group, the last goes first
  const three = [account('1', 'A', { group: 'G' }), account('2', 'B'), account('3', 'C', { group: 'G' }), account('4', 'D', { group: 'G' })]
  assert.deepEqual(reorderWithinGroup(three, 'G', ['4', '1', '3']), ['4', '2', '1', '3'])
  assert.deepEqual(reorderWithinGroup(three, null, ['2']), ['1', '2', '3', '4'])
})

test('the first and the last of a group cannot leave it, whatever the ids say', () => {
  const list = [account('1', 'A', { group: 'G' }), account('2', 'B', { group: 'H' }), account('3', 'C', { group: 'G' })]
  // an id of the other group is not the group's to place: it is ignored
  assert.deepEqual(reorderWithinGroup(list, 'G', ['2', '3', '1']), ['3', '2', '1'])
  assert.deepEqual(reorderWithinGroup(list, 'G', ['3', '2', '1']), ['3', '2', '1'])
  assert.deepEqual(reorderWithinGroup(list, 'H', ['3', '1', '2']), ['1', '2', '3'], 'ids of another group only: nothing moves')
})

test('an unknown, archived or repeated id is ignored, and the result is still every active account once', () => {
  const all = [a1, a2, a3, a4, a5]
  assert.deepEqual(reorderWithinGroup(all, 'Bank', ['99', '4', '1']), ['4', '2', '3', '1'], 'unknown')
  assert.deepEqual(reorderWithinGroup(all, 'Cards', ['5', '3']), ['1', '2', '3', '4'], 'archived: not an active account, not sent')
  assert.deepEqual(reorderWithinGroup(all, 'Bank', ['4', '4', '1', '1']), ['4', '2', '3', '1'], 'twice')
  assert.deepEqual(reorderWithinGroup(all, 'Bank', []), ['1', '2', '3', '4'], 'none')
  assert.deepEqual(reorderWithinGroup(all, 'Bank', ['4']), ['1', '2', '3', '4'], 'a part of the group: the one named takes its own place')
  assert.deepEqual(reorderWithinGroup(all, 'Nobody', ['1', '4']), ['1', '2', '3', '4'], 'a group that is not there')
  assert.deepEqual(reorderWithinGroup([], null, ['1']), [])
  for (const ids of [['4', '1'], ['1', '1', '4'], ['x', '1', '4', '3']]) {
    const sent = reorderWithinGroup(all, 'Bank', ids)
    assert.deepEqual([...sent].sort(), ['1', '2', '3', '4'], ids.join())
  }
})

test('the available line shows only when there is a virtual balance', () => {
  assert.equal(showAvailable({ virtualBalance: '0.00' }), false)
  assert.equal(showAvailable({ virtualBalance: '-0.00' }), false, 'minus nothing is nothing')
  assert.equal(showAvailable({ virtualBalance: '1000.00' }), true)
  assert.equal(showAvailable({ virtualBalance: '-250.00' }), true)
  assert.equal(showAvailable({ virtualBalance: '0.01' }), true)
  assert.equal(showAvailable({ virtualBalance: '-0.01' }), true)
})

// -- the tab in the address -------------------------------------------------------------------------------------------------------

test('the income tab is ?tab=income, and every other address is the accounts', () => {
  assert.equal(accountsTab('?tab=income'), 'income')
  assert.equal(accountsTab('tab=income'), 'income')
  assert.equal(accountsTab(new URLSearchParams({ tab: 'income' })), 'income')
  assert.equal(accountsTab(''), 'accounts')
  assert.equal(accountsTab('?tab=accounts'), 'accounts')
  for (const bad of ['?tab=', '?tab=INCOME', '?tab=expenses', '?tab=Income ', '?tab=income2', '?other=income', '?tab=nope&tab=income']) {
    // the word is exact, and the first value is the one that counts
    assert.equal(accountsTab(bad), 'accounts', bad)
  }
  assert.equal(accountsTab('?x=1&tab=income'), 'income')
})

test('the query of a tab is the one that reads back as it, and the accounts have none', () => {
  assert.deepEqual(tabQuery('accounts'), {})
  assert.deepEqual(tabQuery('income'), { tab: 'income' })
  for (const tab of ['accounts', 'income'] as const) assert.equal(accountsTab(new URLSearchParams(tabQuery(tab))), tab)
})

// -- the bars of the breakdown ----------------------------------------------------------------------------------------------------

test('the largest amount fills the bar and the others are a share of it', () => {
  assert.equal(barWidth('200.00', '200.00'), 100)
  assert.equal(barWidth('100.00', '200.00'), 50)
  assert.equal(barWidth('0.01', '200.00'), 0.005)
  assert.equal(barWidth('0.00', '200.00'), 0)
})

test('a bar needs no division by zero, and never leaves its track', () => {
  assert.equal(barWidth('0.00', '0.00'), 0, 'a total of zero: no bar')
  assert.equal(barWidth('5.00', '0.00'), 0, 'no largest amount: no bar')
  assert.equal(barWidth('50.00', '50.00'), 100, 'one share')
  assert.equal(barWidth('300.00', '200.00'), 100, 'never past the end')
  assert.equal(barWidth('-5.00', '200.00'), 0)
  assert.equal(barWidth('abc', '200.00'), 0)
  assert.equal(barWidth('5.00', ''), 0)
})
