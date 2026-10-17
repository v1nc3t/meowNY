import assert from 'node:assert/strict'
import { test } from 'node:test'
import { draftOfBill, draftOfTemplate, equalDraft, normalise, partsText, shareMark, toInput, withMode } from './split.ts'

const anna = { personId: '2', name: 'Anna' }
const bob = { guest: 'Bob', name: 'Bob' }

test('an equal split sends who, and no numbers', () => {
  assert.deepEqual(toInput(equalDraft([anna, bob])), { mode: 'EQUAL', parts: [{ personId: '2' }, { guest: 'Bob' }] })
})

test('a split with nobody on it is nothing yet', () => {
  assert.equal(toInput(equalDraft([])), null)
})

test('numbers go out the way Rust takes them: two decimals for percent and exact, whole numbers for shares', () => {
  const percent = withMode(equalDraft([anna, bob]), 'PERCENT')
  percent.rows[0]!.value = '30'
  percent.rows[1]!.value = '12.5'
  assert.deepEqual(toInput(percent), { mode: 'PERCENT', parts: [{ personId: '2', value: '30.00' }, { guest: 'Bob', value: '12.50' }] })
  const shares = withMode(equalDraft([anna, bob]), 'SHARES')
  shares.rows[0]!.value = '2'
  assert.deepEqual(toInput(shares), { mode: 'SHARES', mine: '1', parts: [{ personId: '2', value: '2' }, { guest: 'Bob', value: '1' }] })
  shares.rows[1]!.value = '1.5'
  assert.equal(toInput(shares), null, 'a share is a whole number')
})

test('the person has the rest in percent and exact unless they type their own', () => {
  const exact = withMode(equalDraft([anna]), 'EXACT')
  exact.rows[0]!.value = '40'
  assert.deepEqual(toInput(exact), { mode: 'EXACT', parts: [{ personId: '2', value: '40.00' }] })
  assert.deepEqual(toInput({ ...exact, mineIsRest: false, mine: '60' }), { mode: 'EXACT', mine: '60.00', parts: [{ personId: '2', value: '40.00' }] })
  assert.equal(toInput({ ...exact, mineIsRest: false, mine: 'x' }), null)
})

test('an empty field is nothing while typing, and a wrong one is not a number', () => {
  assert.equal(normalise('EXACT', ''), '0.00')
  assert.equal(normalise('SHARES', ''), null)
  assert.equal(normalise('EXACT', '-5'), null)
  assert.equal(normalise('PERCENT', '1,5'), null)
  assert.equal(normalise('SHARES', '007'), '007', 'Rust reads it as 7')
})

test('changing the mode starts the numbers again: shares at one, the others empty', () => {
  const draft = withMode(equalDraft([anna, bob]), 'SHARES')
  assert.deepEqual([draft.mine, draft.mineIsRest, ...draft.rows.map((row) => row.value)], ['1', false, '1', '1'])
  const back = withMode(draft, 'EXACT')
  assert.deepEqual([back.mine, back.mineIsRest, ...back.rows.map((row) => row.value)], ['', true, '', ''])
  assert.equal(withMode(back, 'EXACT'), back, 'the same mode changes nothing')
})

test('a saved bill opens as exact amounts, the person having the rest', () => {
  const draft = draftOfBill([
    { personId: '2', person: 'Anna', guest: null, amount: '25.00' },
    { personId: null, person: null, guest: 'Bob', amount: '25.00' },
  ])
  assert.deepEqual(toInput(draft), { mode: 'EXACT', parts: [{ personId: '2', value: '25.00' }, { guest: 'Bob', value: '25.00' }] })
})

test('a saved template opens as shares, the weights as they are kept', () => {
  const draft = draftOfTemplate({ mine: 2, parts: [{ personId: '2', person: 'Anna', guest: null, weight: 1 }] })
  assert.deepEqual(toInput(draft), { mode: 'SHARES', mine: '2', parts: [{ personId: '2', value: '1' }] })
})

test('the mark counts the person and the others', () => {
  assert.equal(shareMark([1, 2, 3]), '÷4')
  assert.equal(partsText([{ personId: '2', person: 'Anna', guest: null, amount: '25.00' }, { personId: null, person: null, guest: 'Bob', amount: '25.00' }]), 'Anna 25.00, Bob 25.00')
})
