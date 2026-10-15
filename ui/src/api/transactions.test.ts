import assert from 'node:assert/strict'
import { test } from 'node:test'
import { Transaction, TransactionPage } from './transactions.ts'

const row = {
  id: '4',
  categoryId: '2',
  clientUuid: null,
  name: 'Coffee',
  amount: '3.50',
  paymentDate: '2026-06-15',
  description: null,
}

test('a transaction answer keeps money as a two-decimal string', () => {
  assert.ok(Transaction.safeParse(row).success)
  assert.ok(!Transaction.safeParse({ ...row, amount: 3.5 }).success)
  assert.ok(!Transaction.safeParse({ ...row, amount: '3.5' }).success)
  assert.ok(!Transaction.safeParse({ ...row, paymentDate: '15/06/2026' }).success)
})

test('a page needs its paging numbers', () => {
  assert.ok(TransactionPage.safeParse({ transactions: [row], total: 1, limit: 50, offset: 0 }).success)
  assert.ok(!TransactionPage.safeParse({ transactions: [row], total: 1 }).success)
})
