import assert from 'node:assert/strict'
import { test } from 'node:test'
import { BillPreview, Entry, Person, Reminders, Settled, SplitPreview, Statement, Summary } from './debts.ts'

// What the Rust side sends, as its own tests show it (debts/people.rs, ledger.rs, splits.rs, codes.rs, reminders.rs).
const anna = { id: '1', name: 'Anna', archived: false, remindOn: '2026-10-20', remindEveryDays: 7, balance: '-12.50' }
const entry = { id: '4', personId: '1', kind: 'REPAID_TO_ME', amount: '10.00', date: '2026-10-01', note: null, transactionId: null }

test('a friend carries a balance that may be below zero and a reminder that may be missing', () => {
  assert.ok(Person.safeParse(anna).success)
  assert.ok(Person.safeParse({ ...anna, remindOn: null, remindEveryDays: null }).success)
  assert.ok(!Person.safeParse({ ...anna, balance: -12.5 }).success)
  assert.ok(!Person.safeParse({ ...anna, balance: '12.5' }).success)
  assert.ok(!Person.safeParse({ ...anna, remindEveryDays: 0 }).success)
  assert.ok(!Person.safeParse({ ...anna, id: '0' }).success)
})

test('entries, what marking as paid answers and a friend\'s statement are checked', () => {
  assert.ok(Entry.safeParse(entry).success)
  assert.ok(!Entry.safeParse({ ...entry, kind: 'GIFT' }).success)
  assert.ok(!Entry.safeParse({ ...entry, amount: '-1.00' }).success, 'the amount of an entry is above zero: the kind says which way')
  assert.ok(Settled.safeParse({ entry, before: '42.00', after: '-8.00', over: '8.00' }).success)
  const line = { date: '2026-10-01', kind: 'BILL', what: 'Dinner', amount: '25.00', effect: '25.00', balance: '25.00', entryId: null, transactionId: '9' }
  assert.ok(Statement.safeParse({ person: anna, lines: [line, { ...line, kind: 'LENT', effect: '-5.00', entryId: '4', transactionId: null }] }).success)
  assert.ok(!Statement.safeParse({ person: anna, lines: [{ ...line, kind: 'SOMETHING' }] }).success)
  assert.ok(Summary.safeParse({ owedToYou: '25.00', youOwe: '10.50', net: '14.50', people: [anna] }).success)
  assert.ok(!Summary.safeParse({ owedToYou: '-1.00', youOwe: '0.00', net: '0.00', people: [] }).success)
})

test('a preview of a split says whether it is complete and what is left', () => {
  const preview = { complete: false, left: '-5.00', mine: '50.00', parts: [{ personId: '1', person: 'Anna', guest: null, amount: '30.00' }, { personId: null, person: null, guest: 'Carl', amount: '25.00' }] }
  assert.ok(SplitPreview.safeParse(preview).success)
  assert.ok(!SplitPreview.safeParse({ ...preview, complete: 'yes' }).success)
  assert.ok(!SplitPreview.safeParse({ ...preview, parts: [{ ...preview.parts[0], amount: 30 }] }).success)
})

test('a bill code that was read, and the reminders that are due', () => {
  const read = { id: '0123456789abcdef0123456789abcdef', from: 'Vincent', what: 'Dinner', date: '2026-10-10', amount: '25.00', total: '100.00', currency: 'USD', ownCurrency: 'EUR', sameCurrency: false, alreadyAdded: false, friendId: null }
  assert.ok(BillPreview.safeParse(read).success)
  assert.ok(BillPreview.safeParse({ ...read, friendId: '3' }).success)
  assert.ok(!BillPreview.safeParse({ ...read, id: 'ABC' }).success)
  assert.ok(!BillPreview.safeParse({ ...read, currency: 'usd' }).success)
  assert.ok(!BillPreview.safeParse({ ...read, alreadyAdded: undefined }).success)
  const due = { today: '2026-10-17', due: [{ personId: '1', name: 'Anna', dueOn: '2026-10-10' }], upcoming: [{ date: '2026-10-17', count: 2 }] }
  assert.ok(Reminders.safeParse(due).success)
  assert.ok(Reminders.safeParse({ ...due, due: [], upcoming: [] }).success)
  assert.ok(!Reminders.safeParse({ ...due, upcoming: [{ date: '2026-10-17', count: 0 }] }).success)
  assert.ok(!Reminders.safeParse({ ...due, today: '17/10/2026' }).success)
})
