import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { Account } from '../api/accounts.ts'
import type { Entry, Stash } from '../api/stashes.ts'
import {
  activeStashes,
  archivedStashes,
  canAdd,
  canTake,
  checkEntry,
  chips,
  dayWords,
  deltaCheck,
  figures,
  fitsIn,
  homeAccounts,
  kindForKey,
  monthWords,
  moneyOfEntry,
  moneyStart,
  moneySubmit,
  moneyTab,
  nextLine,
  nextText,
  notAfter,
  otherAccounts,
  overBooked,
  percentOf,
  previewBar,
  progress,
  reachedNow,
  savedAfter,
  signedAmount,
  targetLine,
} from './stashView.ts'

// docs/stash-design.md, section 3.1: the delta rule, and what the money dialog shows of it. Money is whole cents; nothing here uses floating point.
const OK = { ok: true }

test('money in must fit in what is free: exactly free is allowed, one cent more is not', () => {
  assert.deepEqual(deltaCheck('300.00', '500.00', '500.00', null), OK)
  assert.deepEqual(deltaCheck('300.00', '500.01', '500.00', null), { ok: false, reason: 'overFree', free: '500.00' })
  assert.deepEqual(deltaCheck('0.00', '0.01', '0.01', null), OK)
  assert.deepEqual(deltaCheck('0.00', '0.01', '0.00', null), { ok: false, reason: 'overFree', free: '0.00' }, 'nothing is free')
})

test('money in is refused whatever it is when the account has less than nothing free', () => {
  // the stashes hold more than the account has (spending after money was set aside, or a sync): free is below 0, and the sentence says 0.00
  assert.deepEqual(deltaCheck('300.00', '0.01', '-20.50', null), { ok: false, reason: 'overFree', free: '0.00' })
  assert.deepEqual(deltaCheck('300.00', '100.00', '-0.01', '1000.00'), { ok: false, reason: 'overFree', free: '0.00' })
})

test('money in must keep the stash at or under its target: exactly the target is allowed, one cent more is not', () => {
  assert.deepEqual(deltaCheck('450.00', '750.00', '5000.00', '1200.00'), OK, 'exactly what is left')
  assert.deepEqual(deltaCheck('450.00', '750.01', '5000.00', '1200.00'), { ok: false, reason: 'overTarget', over: '0.01' })
  assert.deepEqual(deltaCheck('450.00', '800.00', '5000.00', '1200.00'), { ok: false, reason: 'overTarget', over: '50.00' })
  assert.deepEqual(deltaCheck('1200.00', '0.01', '5000.00', '1200.00'), { ok: false, reason: 'overTarget', over: '0.01' }, 'a stash that has reached it takes nothing')
})

test('a stash with no target is limited by what is free only', () => {
  assert.deepEqual(deltaCheck('450.00', '4000.00', '4000.00', null), OK)
  assert.deepEqual(deltaCheck('450.00', '4000.01', '4000.00', null), { ok: false, reason: 'overFree', free: '4000.00' })
})

test('when money in breaks both limits the sentence is about the one that allows less', () => {
  // free 100, left 50, 200 asked: the target allows less
  assert.deepEqual(deltaCheck('450.00', '200.00', '100.00', '500.00'), { ok: false, reason: 'overTarget', over: '150.00' })
  // free 30, left 50, 200 asked: free allows less
  assert.deepEqual(deltaCheck('450.00', '200.00', '30.00', '500.00'), { ok: false, reason: 'overFree', free: '30.00' })
  // the two allow the same: the target
  assert.deepEqual(deltaCheck('450.00', '200.00', '50.00', '500.00'), { ok: false, reason: 'overTarget', over: '150.00' })
  // a stash over its target after a sync allows nothing more: the target, even with free money
  assert.deepEqual(deltaCheck('1440.00', '10.00', '5.00', '1200.00'), { ok: false, reason: 'overTarget', over: '250.00' })
})

test('money out must leave 0 or more: all that is saved is allowed, one cent more is not', () => {
  assert.deepEqual(deltaCheck('450.00', '-450.00', '0.00', '1200.00'), OK, 'to 0')
  assert.deepEqual(deltaCheck('450.00', '-450.01', '0.00', '1200.00'), { ok: false, reason: 'overSaved', saved: '450.00' })
  assert.deepEqual(deltaCheck('450.00', '-0.01', '-90.00', null), OK, 'free does not matter for money out')
  assert.deepEqual(deltaCheck('0.00', '-0.01', '100.00', null), { ok: false, reason: 'overSaved', saved: '0.00' })
})

test('a stash that went below 0 after a sync takes nothing out', () => {
  assert.deepEqual(deltaCheck('-100.00', '-0.01', '100.00', null), { ok: false, reason: 'overSaved', saved: '0.00' })
  // but money can go in: the rule is on what the write does, and 0 or more only for money out
  assert.deepEqual(deltaCheck('-100.00', '40.00', '100.00', '1200.00'), OK)
  assert.deepEqual(deltaCheck('-100.00', '100.01', '100.00', null), { ok: false, reason: 'overFree', free: '100.00' })
})

test('no change is always allowed, whatever the numbers', () => {
  assert.deepEqual(deltaCheck('1440.00', '0.00', '-50.00', '1200.00'), OK, 'above the target and over-booked')
  assert.deepEqual(deltaCheck('-100.00', '0.00', '0.00', null), OK)
  assert.deepEqual(deltaCheck('450.00', '-0.00', '0.00', null), OK)
})

test('large amounts keep their cents: no floating point', () => {
  assert.deepEqual(deltaCheck('0.00', '9999999999.99', '9999999999.99', '9999999999.99'), OK)
  assert.deepEqual(deltaCheck('0.00', '9999999999.99', '9999999999.98', null), { ok: false, reason: 'overFree', free: '9999999999.98' })
  assert.deepEqual(deltaCheck('9007199254740.99', '0.01', '9999999999.99', '9007199254741.00'), OK, 'beyond what a float keeps')
  assert.deepEqual(deltaCheck('9007199254740.99', '0.02', '9999999999.99', '9007199254741.00'), { ok: false, reason: 'overTarget', over: '0.01' })
})

test('an add that brings its money from another account counts the transfer in what is free', () => {
  const base = { kind: 'ADD', saved: '300.00', free: '0.00', target: '1200.00', amount: '200.00' } as const
  assert.deepEqual(checkEntry(base), { ok: false, reason: 'overFree', free: '0.00' }, 'nothing is free, no transfer')
  assert.deepEqual(checkEntry({ ...base, bring: true }), OK, 'the transfer makes the room')
  assert.deepEqual(checkEntry({ ...base, bring: true, amount: '900.01' }), { ok: false, reason: 'overTarget', over: '0.01' }, 'the target still counts')
  // with free below 0 the transfer only covers the deficit: the core refuses the add whatever comes in
  assert.deepEqual(checkEntry({ ...base, free: '-50.00', bring: true }), { ok: false, reason: 'overFree', free: '0.00' })
  assert.deepEqual(checkEntry({ ...base, free: '-50.00', bring: true, amount: '0.01' }), { ok: false, reason: 'overFree', free: '0.00' })
})

test('a take is checked against what is saved, a transfer out does not change that', () => {
  assert.deepEqual(checkEntry({ kind: 'TAKE', saved: '450.00', free: '0.00', target: '1200.00', amount: '450.00' }), OK)
  assert.deepEqual(checkEntry({ kind: 'TAKE', saved: '450.00', free: '0.00', target: '1200.00', amount: '450.01' }), { ok: false, reason: 'overSaved', saved: '450.00' })
  // "bring" is for an add: it changes nothing for a take
  assert.deepEqual(checkEntry({ kind: 'TAKE', saved: '450.00', free: '0.00', target: null, amount: '450.01', bring: true }), { ok: false, reason: 'overSaved', saved: '450.00' })
})

test('an entry being changed is checked by the difference it makes, as the core does', () => {
  // an add of 100 made the stash 450; it is changed to 150: 50 more has to fit in free
  const add = { kind: 'ADD', saved: '450.00', free: '50.00', target: '500.00', old: '100.00' } as const
  assert.deepEqual(checkEntry({ ...add, amount: '150.00' }), OK, 'exactly free and exactly the target')
  assert.deepEqual(checkEntry({ ...add, amount: '150.01' }), { ok: false, reason: 'overTarget', over: '0.01' })
  assert.deepEqual(checkEntry({ ...add, target: null, amount: '150.01' }), { ok: false, reason: 'overFree', free: '50.00' })
  // down to 0.01: 99.99 comes out, so the stash must hold that much: it holds 450
  assert.deepEqual(checkEntry({ ...add, amount: '0.01' }), OK)
  // an add edited down below what was taken since: the stash holds 40 of which this add was 100
  assert.deepEqual(checkEntry({ kind: 'ADD', saved: '40.00', free: '0.00', target: null, old: '100.00', amount: '30.00' }), { ok: false, reason: 'overSaved', saved: '40.00' })
  assert.deepEqual(checkEntry({ kind: 'ADD', saved: '40.00', free: '0.00', target: null, old: '100.00', amount: '60.00' }), OK, 'exactly what is held')
  // an add edited down is allowed even when nothing is free (the stash holds more than the account has)
  assert.deepEqual(checkEntry({ kind: 'ADD', saved: '450.00', free: '-300.00', target: null, old: '100.00', amount: '50.00' }), OK)
  // the same entry, unchanged: no change
  assert.deepEqual(checkEntry({ kind: 'ADD', saved: '1440.00', free: '-300.00', target: '1200.00', old: '100.00', amount: '100.00' }), OK, 'only the note changed, above the target')
})

test('a take being changed: more out needs saved, less out puts money back and needs free and the target', () => {
  const take = { kind: 'TAKE', saved: '410.00', free: '10.00', target: '450.00', old: '40.00' } as const
  assert.deepEqual(checkEntry({ ...take, amount: '450.00' }), OK, 'more out: 410 comes out of 410')
  assert.deepEqual(checkEntry({ ...take, amount: '450.01' }), { ok: false, reason: 'overSaved', saved: '410.00' })
  assert.deepEqual(checkEntry({ ...take, amount: '30.00' }), OK, 'less out: 10 goes back, 10 is free')
  assert.deepEqual(checkEntry({ ...take, amount: '29.99' }), { ok: false, reason: 'overFree', free: '10.00' })
  assert.deepEqual(checkEntry({ ...take, free: '100.00', old: '90.00', amount: '40.00' }), { ok: false, reason: 'overTarget', over: '10.00' }, '50 goes back: 460, 10 over 450')
})

test('a stash that takes no money in refuses an entry that would put more in, and allows the rest', () => {
  const closed = { saved: '450.00', free: '1000.00', target: '1200.00', closed: true } as const
  assert.deepEqual(checkEntry({ ...closed, kind: 'ADD', amount: '1.00' }), { ok: false, reason: 'closed' })
  assert.deepEqual(checkEntry({ ...closed, kind: 'TAKE', amount: '450.00' }), OK, 'it can be emptied')
  assert.deepEqual(checkEntry({ ...closed, kind: 'ADD', old: '100.00', amount: '50.00' }), OK, 'an add edited down takes money out')
  assert.deepEqual(checkEntry({ ...closed, kind: 'ADD', old: '100.00', amount: '100.00' }), OK, 'only the note changed')
  assert.deepEqual(checkEntry({ ...closed, kind: 'ADD', old: '100.00', amount: '100.01' }), { ok: false, reason: 'closed' })
})

test('what the stash holds after, in cents', () => {
  assert.equal(savedAfter('450.00', 10000n), '550.00')
  assert.equal(savedAfter('450.00', -45001n), '-0.01')
  assert.equal(signedAmount('ADD', '12.50'), 1250n)
  assert.equal(signedAmount('TAKE', '12.50'), -1250n)
})

// -- progress and the bars -------------------------------------------------------------------------------------------------------------

test('the percent is floored, 0 for a stash with nothing or less, none without a target, and may be above 100', () => {
  assert.equal(percentOf('450.00', '1200.00'), 37)
  assert.equal(percentOf('0.01', '1200.00'), 0)
  assert.equal(percentOf('0.00', '1200.00'), 0)
  assert.equal(percentOf('-100.00', '1200.00'), 0)
  assert.equal(percentOf('450.00', null), null)
  assert.equal(percentOf('1440.00', '1200.00'), 120)
  assert.equal(percentOf('1200.00', '1200.00'), 100)
  assert.equal(percentOf('1199.99', '1200.00'), 99)
})

test('progress fills the bar from 0 to 1, and says reached, over and below', () => {
  assert.deepEqual(progress('300.00', '1200.00'), { fill: 0.25, percent: 25, reached: false, over: false, below: false })
  assert.deepEqual(progress('1200.00', '1200.00'), { fill: 1, percent: 100, reached: true, over: false, below: false }, 'exactly the target')
  assert.deepEqual(progress('1199.99', '1200.00'), { fill: 0.9999, percent: 99, reached: false, over: false, below: false })
  // after a sync: above the target the bar stays full and says so, below 0 it is empty
  assert.deepEqual(progress('1440.00', '1200.00'), { fill: 1, percent: 120, reached: true, over: true, below: false })
  assert.deepEqual(progress('-100.00', '1200.00'), { fill: 0, percent: 0, reached: false, over: false, below: true })
  assert.deepEqual(progress('0.00', '1200.00'), { fill: 0, percent: 0, reached: false, over: false, below: false })
  // no target: no bar, and the amount says it all
  assert.deepEqual(progress('450.00', null), { fill: 0, percent: null, reached: false, over: false, below: false })
  assert.deepEqual(progress('-5.00', null), { fill: 0, percent: null, reached: false, over: false, below: true })
})

test('the preview bar: an add shows what is there solid and what goes in striped', () => {
  assert.deepEqual(previewBar('300.00', '600.00', '1200.00'), { solid: 0.25, stripe: 0.25, tone: 'add' })
  assert.deepEqual(previewBar('300.00', '1200.00', '1200.00'), { solid: 0.25, stripe: 0.75, tone: 'add' }, 'to the target')
  assert.deepEqual(previewBar('0.00', '300.00', '1200.00'), { solid: 0, stripe: 0.25, tone: 'add' })
})

test('the preview bar: a take shows what stays solid and what goes out striped', () => {
  assert.deepEqual(previewBar('600.00', '300.00', '1200.00'), { solid: 0.25, stripe: 0.25, tone: 'take' })
  assert.deepEqual(previewBar('600.00', '0.00', '1200.00'), { solid: 0, stripe: 0.5, tone: 'take' }, 'all of it')
})

test('the preview bar of a stash with no target is as wide as the larger of the two amounts', () => {
  assert.deepEqual(previewBar('100.00', '400.00', null), { solid: 0.25, stripe: 0.75, tone: 'add' })
  assert.deepEqual(previewBar('400.00', '100.00', null), { solid: 0.25, stripe: 0.75, tone: 'take' })
  assert.deepEqual(previewBar('0.00', '0.00', null), { solid: 0, stripe: 0, tone: 'same' })
})

test('the preview bar never leaves the track: a stash below 0 has nothing solid, one over its target is full', () => {
  assert.deepEqual(previewBar('-100.00', '200.00', '1200.00'), { solid: 0, stripe: 0.1666, tone: 'add' })
  const over = previewBar('1440.00', '1440.00', '1200.00')
  assert.deepEqual(over, { solid: 1, stripe: 0, tone: 'same' })
  const stretched = previewBar('1000.00', '1440.00', '1200.00')
  assert.equal(stretched.solid + stretched.stripe, 1, 'clamped at the end of the track')
  assert.deepEqual(previewBar('450.00', '450.00', '1200.00'), { solid: 0.375, stripe: 0, tone: 'same' })
})

// -- chips -----------------------------------------------------------------------------------------------------------------------------

const stash = (more: Partial<Stash> = {}): Stash => ({
  id: '2',
  name: 'Holiday',
  accountId: '3',
  account: 'Savings',
  currency: 'EUR',
  archived: false,
  accountArchived: false,
  target: '1200.00',
  targetDate: '2027-06-30',
  notes: null,
  saved: '450.00',
  left: '750.00',
  percent: 37,
  reached: false,
  perMonth: '107.15',
  overdue: false,
  free: '1220.35',
  entries: 3,
  lastEntry: '2026-11-02',
  ...more,
})

test('the chips of an add: the rest, a month, and all that is free only when it is less than the rest', () => {
  assert.deepEqual(chips(stash(), 'ADD'), [
    { id: 'rest', amount: '750.00' },
    { id: 'month', amount: '107.15' },
  ])
  assert.deepEqual(chips(stash({ free: '300.00' }), 'ADD'), [
    { id: 'rest', amount: '750.00' },
    { id: 'month', amount: '107.15' },
    { id: 'free', amount: '300.00' },
  ])
  assert.equal(chips(stash({ free: '750.00' }), 'ADD').some((chip) => chip.id === 'free'), false, 'free exactly the rest: the rest says it')
  assert.equal(chips(stash({ free: '750.01' }), 'ADD').some((chip) => chip.id === 'free'), false)
  assert.equal(chips(stash({ free: '749.99' }), 'ADD').some((chip) => chip.id === 'free'), true)
})

test('the chips of an add for a stash with no target: all that is free, and nothing else', () => {
  const open = stash({ target: null, targetDate: null, left: null, percent: null, perMonth: null })
  assert.deepEqual(chips(open, 'ADD'), [{ id: 'free', amount: '1220.35' }])
  assert.deepEqual(chips({ ...open, free: '0.00' }, 'ADD'), [], 'nothing free, nothing to offer')
  assert.deepEqual(chips({ ...open, free: '-5.00' }, 'ADD'), [], 'free below 0')
})

test('a chip above 0 only: nothing left, no month, nothing free', () => {
  assert.deepEqual(chips(stash({ left: '0.00', perMonth: null, reached: true }), 'ADD'), [], 'a stash that reached its target')
  assert.deepEqual(chips(stash({ perMonth: null, targetDate: null }), 'ADD'), [{ id: 'rest', amount: '750.00' }], 'no date: no month')
  assert.deepEqual(chips(stash({ free: '0.00' }), 'ADD'), [{ id: 'rest', amount: '750.00' }, { id: 'month', amount: '107.15' }], 'nothing free is not a chip')
  assert.deepEqual(chips(stash({ free: '-1.00' }), 'ADD').some((chip) => chip.id === 'free'), false)
})

test('two chips of one amount are one chip, the first stays', () => {
  // a target this month: a month is the whole rest
  assert.deepEqual(chips(stash({ perMonth: '750.00' }), 'ADD'), [{ id: 'rest', amount: '750.00' }])
  // the free money equals a month's share
  assert.deepEqual(chips(stash({ free: '107.15' }), 'ADD'), [{ id: 'rest', amount: '750.00' }, { id: 'month', amount: '107.15' }])
  // half of 0.02 is 0.01, which is not all of it
  assert.deepEqual(chips(stash({ saved: '0.02' }), 'TAKE'), [{ id: 'all', amount: '0.02' }, { id: 'half', amount: '0.01' }])
})

test('the chips of a take: all that is saved, and half rounded down to the cent', () => {
  assert.deepEqual(chips(stash(), 'TAKE'), [{ id: 'all', amount: '450.00' }, { id: 'half', amount: '225.00' }])
  assert.deepEqual(chips(stash({ saved: '450.01' }), 'TAKE'), [{ id: 'all', amount: '450.01' }, { id: 'half', amount: '225.00' }], 'rounded down')
  assert.deepEqual(chips(stash({ saved: '0.01' }), 'TAKE'), [{ id: 'all', amount: '0.01' }], 'half of a cent is nothing')
  assert.deepEqual(chips(stash({ saved: '0.00' }), 'TAKE'), [])
  assert.deepEqual(chips(stash({ saved: '-100.00' }), 'TAKE'), [], 'a stash below 0 offers no take')
  assert.deepEqual(chips(stash({ saved: '9999999999.99' }), 'TAKE'), [{ id: 'all', amount: '9999999999.99' }, { id: 'half', amount: '4999999999.99' }])
})

// -- what a stash can do and say -------------------------------------------------------------------------------------------------------

test('add is for a stash and an account that are not archived, take out for a stash that holds something', () => {
  assert.equal(canAdd(stash()), true)
  assert.equal(canAdd(stash({ accountArchived: true })), false)
  assert.equal(canAdd(stash({ archived: true })), false)
  assert.equal(canTake(stash()), true)
  assert.equal(canTake(stash({ saved: '0.00' })), false)
  assert.equal(canTake(stash({ saved: '-0.01' })), false)
  assert.equal(canTake(stash({ saved: '0.01' })), true)
  assert.equal(overBooked('-0.01'), true)
  assert.equal(overBooked('0.00'), false)
  assert.equal(overBooked('12.00'), false)
})

test('the active stashes and the archived ones keep their order', () => {
  const list = [stash({ id: '1' }), stash({ id: '2', archived: true }), stash({ id: '3' }), stash({ id: '4', archived: true })]
  assert.deepEqual(activeStashes(list).map((each) => each.id), ['1', '3'])
  assert.deepEqual(archivedStashes(list).map((each) => each.id), ['2', '4'])
})

test('the line under a card: reached, overdue, what is left with a month, what is left, or nothing', () => {
  assert.deepEqual(nextLine(stash()), { kind: 'left', left: '750.00', perMonth: '107.15', until: '2027-06-30' })
  assert.deepEqual(nextLine(stash({ targetDate: null, perMonth: null })), { kind: 'left', left: '750.00', perMonth: null, until: null })
  assert.deepEqual(nextLine(stash({ reached: true, left: '0.00', perMonth: null })), { kind: 'reached' })
  assert.deepEqual(nextLine(stash({ overdue: true, perMonth: null, targetDate: '2026-06-30' })), { kind: 'overdue', since: '2026-06-30' })
  assert.equal(nextLine(stash({ target: null, left: null, perMonth: null, targetDate: null })), null)
  assert.deepEqual(nextLine(stash({ saved: '1440.00', reached: true, left: '0.00', perMonth: null })), { kind: 'reached' }, 'above the target is reached too')
})

test('a date in words is lower case in the person\'s language and never shifts with the time zone', () => {
  assert.equal(monthWords('2027-06-30', 'en-GB'), 'june 2027')
  assert.equal(dayWords('2026-06-30', 'en-GB'), '30 june 2026')
  assert.equal(dayWords('2026-01-01', 'en-GB'), '1 january 2026')
  assert.equal(monthWords('not a date', 'en-GB'), 'not a date')
  assert.equal(dayWords('2026-06-30', 'xx-invalid-locale-!'), '2026-06-30', 'a locale that Intl cannot use')
})

// -- the accounts a stash may use ------------------------------------------------------------------------------------------------------

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
const accounts = [
  account('1'),
  account('3', { kind: 'SAVINGS' }),
  account('4', { archived: true }),
  account('5', { currency: 'RON', foreign: true, rate: '5.00' }),
  account('6'),
  account('7', { currency: 'RON', foreign: true, rate: '5.00' }),
]

test('money comes from or goes to another account that is active and in the stash\'s currency', () => {
  assert.deepEqual(otherAccounts(accounts, { accountId: '3' }).map((each) => each.id), ['1', '6'], 'not the account itself, not an archived one, not lei')
  assert.deepEqual(otherAccounts(accounts, { accountId: '5' }).map((each) => each.id), ['7'], 'lei with lei')
  assert.deepEqual(otherAccounts(accounts, { accountId: '99' }), [])
  assert.deepEqual(otherAccounts([account('1')], { accountId: '1' }), [], 'one account: nothing to choose, the switch is not shown')
})

test('a stash that is made lives in any active account; one that exists only moves within its currency', () => {
  assert.deepEqual(homeAccounts(accounts, null), { choices: accounts.filter((each) => !each.archived), hidden: false })
  const moved = homeAccounts(accounts, { accountId: '3' })
  assert.deepEqual(moved.choices.map((each) => each.id), ['1', '3', '6'])
  assert.equal(moved.hidden, true, 'the lei accounts are left out: the line "only accounts in EUR"')
  assert.equal(homeAccounts(accounts, { accountId: '5' }).hidden, true)
  assert.equal(homeAccounts([account('1'), account('3')], { accountId: '3' }).hidden, false)
  // a stash in an archived account keeps it in the list, so that it shows and stays unless it is changed
  assert.deepEqual(homeAccounts(accounts, { accountId: '4' }).choices.map((each) => each.id), ['1', '3', '4', '6'])
})

test('a stash moves to an account when what it holds fits in what is free there', () => {
  const holds = { accountId: '3', saved: '300.00' }
  assert.equal(fitsIn(holds, account('1', { free: '300.00' })), true, 'exactly')
  assert.equal(fitsIn(holds, account('1', { free: '299.99' })), false)
  assert.equal(fitsIn(holds, account('1', { free: '-5.00' })), false)
  assert.equal(fitsIn({ accountId: '3', saved: '0.00' }, account('1', { free: '0.00' })), true, 'a stash that holds 0 always moves')
  assert.equal(fitsIn({ accountId: '3', saved: '0.00' }, account('1', { free: '-5.00' })), true, 'even into an account with free below 0')
  assert.equal(fitsIn({ accountId: '3', saved: '-40.00' }, account('1', { free: '0.00' })), true, 'below 0 it holds nothing')
  assert.equal(fitsIn(holds, account('3', { free: '0.00' })), true, 'its own account is not a move')
})

// -- the dialog ----------------------------------------------------------------------------------------------------------------------

const TODAY = '2026-11-18'
const entry = (more: Partial<Entry> = {}): Entry => ({
  id: '9',
  stashId: '2',
  kind: 'ADD',
  amount: '100.00',
  date: '2026-11-02',
  note: 'november',
  transferId: null,
  otherAccountId: null,
  otherAccount: null,
  savedAfter: '450.00',
  ...more,
})

test('a new dialog is empty, dated today, with no transfer, on the tab asked for', () => {
  assert.deepEqual(moneyStart('ADD', TODAY), { kind: 'ADD', amount: '', date: TODAY, note: '', transfer: false, account: '' })
  assert.equal(moneyStart('TAKE', TODAY).kind, 'TAKE')
  assert.deepEqual(moneyOfEntry(entry({ note: null })), { kind: 'ADD', amount: '100.00', date: '2026-11-02', note: '', transfer: false, account: '' })
})

test('changing the tab keeps the amount, the date and the note and drops the transfer', () => {
  const typed = { kind: 'ADD', amount: '12,5', date: '2026-11-10', note: 'for the flights', transfer: true, account: '1' } as const
  assert.deepEqual(moneyTab(typed, 'TAKE'), { kind: 'TAKE', amount: '12,5', date: '2026-11-10', note: 'for the flights', transfer: false, account: '' })
})

test('a date after today is held at today', () => {
  assert.equal(notAfter('2026-11-19', TODAY), TODAY)
  assert.equal(notAfter(TODAY, TODAY), TODAY)
  assert.equal(notAfter('2026-01-02', TODAY), '2026-01-02')
})

test('Alt+1 and Alt+2 choose add and take out, by the key\'s code', () => {
  const key = (code: string, more: Partial<{ altKey: boolean; ctrlKey: boolean; metaKey: boolean; shiftKey: boolean }> = {}) => ({ code, altKey: true, ctrlKey: false, metaKey: false, shiftKey: false, ...more })
  assert.equal(kindForKey(key('Digit1')), 'ADD')
  assert.equal(kindForKey(key('Digit2')), 'TAKE')
  assert.equal(kindForKey(key('Digit3')), null)
  assert.equal(kindForKey(key('Digit1', { altKey: false })), null, 'a digit alone is typed into the amount')
  assert.equal(kindForKey(key('Digit1', { ctrlKey: true })), null)
  assert.equal(kindForKey(key('Digit2', { shiftKey: true })), null)
})

test('a submit sends a new add with only what was filled in', () => {
  const fields = { ...moneyStart('ADD', TODAY), amount: '12,5', note: '  november  ' }
  assert.deepEqual(moneySubmit('EDITING', null, fields, TODAY, { ok: true }), { do: 'send', request: { add: { amount: '12.50', date: TODAY, note: 'november' } } })
  assert.deepEqual(moneySubmit('EDITING', null, { ...fields, note: '   ' }, TODAY, { ok: true }), { do: 'send', request: { add: { amount: '12.50', date: TODAY } } }, 'an empty note is none')
  // with the transfer on: the account the money comes from
  assert.deepEqual(moneySubmit('EDITING', null, { ...fields, transfer: true, account: '1' }, TODAY, { ok: true }), {
    do: 'send',
    request: { add: { amount: '12.50', date: TODAY, note: 'november', fromAccountId: '1' } },
  })
  // a transfer switched on with no account chosen sends none
  assert.deepEqual(moneySubmit('EDITING', null, { ...fields, transfer: true, account: '' }, TODAY, { ok: true }), { do: 'send', request: { add: { amount: '12.50', date: TODAY, note: 'november' } } })
})

test('a submit sends a take with the account the money goes to', () => {
  const fields = { ...moneyStart('TAKE', TODAY), amount: '40', transfer: true, account: '1', note: 'deposit for the flat' }
  assert.deepEqual(moneySubmit('EDITING', null, fields, TODAY, { ok: true }), {
    do: 'send',
    request: { take: { amount: '40.00', date: TODAY, note: 'deposit for the flat', toAccountId: '1' } },
  })
  assert.deepEqual(moneySubmit('EDITING', null, { ...fields, transfer: false }, TODAY, { ok: true }), { do: 'send', request: { take: { amount: '40.00', date: TODAY, note: 'deposit for the flat' } } })
})

test('a submit refuses an amount that is no amount, a date after today and a note that is too long, and sends nothing', () => {
  const fields = { ...moneyStart('ADD', TODAY), amount: '5' }
  for (const amount of ['', '0', '0.00', '-5', '1.234', 'abc', '10000000000']) {
    assert.deepEqual(moneySubmit('EDITING', null, { ...fields, amount }, TODAY, { ok: true }), { do: 'refuse', error: 'tx.errorAmount' }, amount)
  }
  assert.deepEqual(moneySubmit('EDITING', null, { ...fields, date: '2026-11-19' }, TODAY, { ok: true }), { do: 'refuse', error: 'stashes.dateError' })
  assert.deepEqual(moneySubmit('EDITING', null, { ...fields, date: '' }, TODAY, { ok: true }), { do: 'refuse', error: 'stashes.dateError' })
  assert.deepEqual(moneySubmit('EDITING', null, { ...fields, note: 'x'.repeat(201) }, TODAY, { ok: true }), { do: 'refuse', error: 'stashes.noteError' })
  assert.equal(moneySubmit('EDITING', null, { ...fields, note: 'x'.repeat(200) }, TODAY, { ok: true }).do, 'send')
  assert.equal(moneySubmit('EDITING', null, { ...fields, note: '\u{1F408}'.repeat(200) }, TODAY, { ok: true }).do, 'send', 'characters, not code units')
})

test('a submit does nothing while a request is out, while the dialog is gone, or while the amount breaks the rule', () => {
  const fields = { ...moneyStart('ADD', TODAY), amount: '5' }
  assert.deepEqual(moneySubmit('SAVING', null, fields, TODAY, { ok: true }), { do: 'ignore' })
  assert.deepEqual(moneySubmit('CLOSED', null, fields, TODAY, { ok: true }), { do: 'ignore' })
  assert.deepEqual(moneySubmit('EDITING', null, fields, TODAY, { ok: false, reason: 'overFree', free: '1.00' }), { do: 'ignore' })
})

test('an entry being changed sends only what is different, and nothing different just closes', () => {
  const row = entry()
  const same = moneyOfEntry(row)
  assert.deepEqual(moneySubmit('EDITING', row, same, TODAY, { ok: true }), { do: 'close' })
  assert.deepEqual(moneySubmit('EDITING', row, { ...same, amount: '100' }, TODAY, { ok: true }), { do: 'close' }, '"100" is "100.00"')
  assert.deepEqual(moneySubmit('EDITING', row, { ...same, amount: '120' }, TODAY, { ok: true }), { do: 'send', request: { patch: { amount: '120.00' } } })
  assert.deepEqual(moneySubmit('EDITING', row, { ...same, date: '2026-11-03' }, TODAY, { ok: true }), { do: 'send', request: { patch: { date: '2026-11-03' } } })
  assert.deepEqual(moneySubmit('EDITING', row, { ...same, note: 'december' }, TODAY, { ok: true }), { do: 'send', request: { patch: { note: 'december' } } })
  assert.deepEqual(moneySubmit('EDITING', row, { ...same, note: '' }, TODAY, { ok: true }), { do: 'send', request: { patch: { note: null } } }, 'an emptied note clears it')
  assert.deepEqual(moneySubmit('EDITING', entry({ note: null }), { ...moneyOfEntry(entry({ note: null })), note: ' ' }, TODAY, { ok: true }), { do: 'close' }, 'no note and still none')
  assert.deepEqual(moneySubmit('EDITING', row, { ...same, amount: '1', note: 'x' }, TODAY, { ok: true }), { do: 'send', request: { patch: { amount: '1.00', note: 'x' } } })
  // a linked entry: the transfer is not changed, and nothing about it is sent
  const linked = entry({ transferId: '7', otherAccountId: '1', otherAccount: 'Current account' })
  assert.deepEqual(moneySubmit('EDITING', linked, { ...moneyOfEntry(linked), amount: '90' }, TODAY, { ok: true }), { do: 'send', request: { patch: { amount: '90.00' } } })
})

test('an add that brings the stash to its target is noticed once', () => {
  assert.equal(reachedNow({ reached: false }, { reached: true }), true)
  assert.equal(reachedNow({ reached: true }, { reached: true }), false, 'it had reached it already')
  assert.equal(reachedNow({ reached: false }, { reached: false }), false)
  assert.equal(reachedNow({ reached: true }, { reached: false }), false, 'a take that brings it below')
})

test('the words of a card: saved of target with the percent, or saved alone; what is next in a line', () => {
  const say = (key: string, vars: Record<string, string | number> = {}) => `${key}${Object.entries(vars).map(([name, value]) => ` ${name}=${value}`).join('')}`
  const plain = (amount: string) => amount
  assert.equal(figures(stash(), plain, say as never), 'stashes.of saved=450.00 target=1200.00 · stashes.percent percent=37')
  assert.equal(figures(stash({ target: null, percent: null }), plain, say as never), 'stashes.saved amount=450.00')
  assert.equal(figures(stash({ saved: '-5.00', percent: 0 }), plain, say as never), 'stashes.of saved=-5.00 target=1200.00 · stashes.percent percent=0', 'below 0, as it is')
  assert.deepEqual(nextText({ kind: 'left', left: '750.00', perMonth: '107.15', until: '2027-06-30' }, plain, say as never, 'en-GB'), {
    text: 'stashes.left amount=750.00 · stashes.perMonth amount=107.15 date=june 2027',
    warning: false,
  })
  assert.deepEqual(nextText({ kind: 'left', left: '750.00', perMonth: null, until: null }, plain, say as never, 'en-GB'), { text: 'stashes.left amount=750.00', warning: false })
  assert.deepEqual(nextText({ kind: 'reached' }, plain, say as never, 'en-GB'), { text: 'stashes.reached', warning: false })
  assert.deepEqual(nextText({ kind: 'overdue', since: '2026-06-30' }, plain, say as never, 'en-GB'), { text: 'stashes.overdue date=30 june 2026', warning: true })
})

test('the target of a row in one line: the amount and the date, each alone when the other is missing, a dash for none', () => {
  const plain = (amount: string) => amount
  assert.equal(targetLine({ target: '1200.00', targetDate: '2027-06-30' }, plain, 'en-GB'), '1200.00 · 30/06/2027')
  assert.equal(targetLine({ target: '1200.00', targetDate: null }, plain, 'en-GB'), '1200.00')
  assert.equal(targetLine({ target: null, targetDate: '2027-06-30' }, plain, 'en-GB'), '30/06/2027')
  assert.equal(targetLine({ target: null, targetDate: null }, plain, 'en-GB'), '—')
})
