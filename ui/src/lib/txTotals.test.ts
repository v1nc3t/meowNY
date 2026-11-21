import assert from 'node:assert/strict'
import { test } from 'node:test'
import { txTotals } from './txTotals.ts'

const income = new Set(['9'])

test('the totals add up money in and money out by what each costs the person, and the net is what is left', () => {
  const rows = [
    { categoryId: '9', myShare: '3200.00' },
    { categoryId: '4', myShare: '47.35' },
    { categoryId: '4', myShare: '0.05' },
    { categoryId: '5', myShare: '12.60' },
  ]
  assert.deepEqual(txTotals(rows, 4, income), { count: 4, income: '3200.00', expense: '60.00', net: '3140.00' })
})

test('more going out than coming in is a net below 0', () => {
  assert.deepEqual(txTotals([{ categoryId: '4', myShare: '10.00' }, { categoryId: '9', myShare: '2.50' }], 2, income), { count: 2, income: '2.50', expense: '10.00', net: '-7.50' })
})

test('no totals for one page of many, or for fewer than two rows', () => {
  const two = [{ categoryId: '4', myShare: '1.00' }, { categoryId: '4', myShare: '2.00' }]
  assert.equal(txTotals(two, 3, income), null, 'the rest are on other pages')
  assert.equal(txTotals(two.slice(0, 1), 1, income), null)
  assert.equal(txTotals([], 0, income), null)
})
