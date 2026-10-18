import assert from 'node:assert/strict'
import { test } from 'node:test'
import { Recurring } from '../api/recurring.ts'
import { sharedItems } from './recurringView.ts'

const item = (id: string, nextDueDate: string, o: Partial<Recurring> = {}): Recurring => ({
  id,
  categoryId: '1',
  name: `item ${id}`,
  description: null,
  amount: '10.00',
  startDate: '2026-01-01',
  intervalUnit: 'MONTH',
  intervalCount: 1,
  nextDueDate,
  endDate: null,
  isActive: true,
  split: null,
  ...o,
})
const split = (...names: [string | null, string | null][]) => ({
  mine: 1,
  parts: names.map(([person, guest], at) => ({ personId: person ? String(at + 1) : null, person, guest, weight: 1 })),
})

test('the shared items are the running ones with a split, the next to fall due first', () => {
  const list = [
    item('1', '2026-10-30', { split: split(['Anna', null]) }),
    item('2', '2026-10-20'),
    item('3', '2026-10-25', { split: split([null, 'Dave']), isActive: false }),
    item('4', '2026-10-20', { split: split(['Bob', null]) }),
    item('5', '2026-10-20', { split: split(['Cleo', null]) }),
  ]
  assert.deepEqual(sharedItems(list).map((i) => i.id), ['4', '5', '1'], 'no plain item, no paused one; by day, then by id')
  assert.deepEqual(sharedItems([]), [])
})
