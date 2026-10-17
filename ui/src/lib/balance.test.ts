import assert from 'node:assert/strict'
import { test } from 'node:test'
import { balanceView, entryOfLine } from './balance.ts'

test('a balance says who owes whom', () => {
  assert.deepEqual(balanceView('25.00'), { tone: 'owed', abs: '25.00' })
  assert.deepEqual(balanceView('-12.50'), { tone: 'owe', abs: '12.50' })
  assert.deepEqual(balanceView('0.00'), { tone: 'even', abs: '0.00' })
})

const line = { date: '2026-10-01', kind: 'LENT', what: 'train tickets', amount: '40.00', effect: '40.00', balance: '40.00', entryId: '7', transactionId: null } as const

test('a line of an entry can be edited, a part of a bill cannot', () => {
  assert.deepEqual(entryOfLine(line, '3'), { id: '7', personId: '3', kind: 'LENT', amount: '40.00', date: '2026-10-01', note: 'train tickets', transactionId: null })
  assert.equal(entryOfLine({ ...line, what: '' }, '3')?.note, null)
  assert.equal(entryOfLine({ ...line, kind: 'BILL', entryId: null, transactionId: '9' }, '3'), null)
})
