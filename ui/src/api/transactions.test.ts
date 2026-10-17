import assert from 'node:assert/strict'
import { test } from 'node:test'
import { Transaction, TransactionPage } from './transactions.ts'

const row = {
  id: '4',
  categoryId: '2',
  clientUuid: null,
  name: 'Coffee',
  amount: '3.50',
  myShare: '3.50',
  split: [],
  paymentDate: '2026-06-15',
  description: null,
}

test('a transaction answer keeps money as a two-decimal string', () => {
  assert.ok(Transaction.safeParse(row).success)
  assert.ok(!Transaction.safeParse({ ...row, amount: 3.5 }).success)
  assert.ok(!Transaction.safeParse({ ...row, amount: '3.5' }).success)
  assert.ok(!Transaction.safeParse({ ...row, paymentDate: '15/06/2026' }).success)
})

test('a shared bill carries the parts of the others, and what it costs the person', () => {
  const shared = { ...row, amount: '100.00', myShare: '25.00', split: [{ personId: '2', person: 'Anna', guest: null, amount: '75.00' }] }
  assert.ok(Transaction.safeParse(shared).success)
  assert.ok(Transaction.safeParse({ ...shared, split: [{ personId: null, person: null, guest: 'Bob', amount: '75.00' }] }).success)
  assert.ok(!Transaction.safeParse({ ...shared, myShare: 25 }).success)
  assert.ok(!Transaction.safeParse({ ...row, myShare: undefined }).success, 'the share is always there')
})

test('a page needs its paging numbers', () => {
  assert.ok(TransactionPage.safeParse({ transactions: [row], total: 1, limit: 50, offset: 0 }).success)
  assert.ok(!TransactionPage.safeParse({ transactions: [row], total: 1 }).success)
})
