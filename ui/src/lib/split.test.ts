import assert from 'node:assert/strict'
import { test } from 'node:test'
import { draftOfTemplate, equalDraft, normalise, reducedWeights, toInput, withMode } from './split.ts'

const anna = { personId: '2', name: 'Anna' }
const bob = { guest: 'Bob', name: 'Bob' }

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

test('an empty field is nothing while typing, and a wrong one is not a number', () => {
  assert.equal(normalise('EXACT', ''), '0.00')
  assert.equal(normalise('SHARES', ''), null)
  assert.equal(normalise('EXACT', '-5'), null)
  assert.equal(normalise('PERCENT', '1,5'), '1.50', 'a comma is a decimal mark, as a phone keypad types it')
  assert.equal(normalise('PERCENT', '1,5,5'), null)
  assert.equal(normalise('SHARES', '007'), '007', 'Rust reads it as 7')
})

test('changing the mode starts the numbers again: shares at one, the others empty', () => {
  const draft = withMode(equalDraft([anna, bob]), 'SHARES')
  assert.deepEqual([draft.mine, draft.mineIsRest, ...draft.rows.map((row) => row.value)], ['1', false, '1', '1'])
  const back = withMode(draft, 'EXACT')
  assert.deepEqual([back.mine, back.mineIsRest, ...back.rows.map((row) => row.value)], ['', true, '', ''])
  assert.equal(withMode(back, 'EXACT'), back, 'the same mode changes nothing')
})

test('a saved template opens as shares, in lowest terms, so what was typed as 19 and 1 does not come back as 1900 and 100', () => {
  const template = (mine: number, ...weights: number[]) => ({ mine, parts: weights.map((weight, at) => ({ personId: String(at + 2), person: `P${at}`, guest: null, weight })) })
  assert.deepEqual(toInput(draftOfTemplate(template(2, 1))), { mode: 'SHARES', mine: '2', parts: [{ personId: '2', value: '1' }] })
  // amounts of 19.00 and 1.00 are kept as 1900 and 100
  assert.deepEqual(toInput(draftOfTemplate(template(100, 1900))), { mode: 'SHARES', mine: '1', parts: [{ personId: '2', value: '19' }] })
  assert.deepEqual(toInput(draftOfTemplate(template(1900, 100))), { mode: 'SHARES', mine: '19', parts: [{ personId: '2', value: '1' }] })
  assert.deepEqual(toInput(draftOfTemplate(template(3, 3, 6))), { mode: 'SHARES', mine: '1', parts: [{ personId: '2', value: '1' }, { personId: '3', value: '2' }] })
})

test('the weights of a whole percent split open as percent, with two decimals', () => {
  const draft = draftOfTemplate({ mine: 8100, parts: [{ personId: '2', person: 'Anna', guest: null, weight: 1900 }] })
  assert.deepEqual(toInput(draft), { mode: 'PERCENT', mine: '81.00', parts: [{ personId: '2', value: '19.00' }] })
  const odd = draftOfTemplate({ mine: 3334, parts: [{ personId: '2', person: 'A', guest: null, weight: 3333 }, { personId: null, person: null, guest: 'B', weight: 3333 }] })
  assert.deepEqual(toInput(odd), { mode: 'PERCENT', mine: '33.34', parts: [{ personId: '2', value: '33.33' }, { guest: 'B', value: '33.33' }] })
  const small = draftOfTemplate({ mine: 9905, parts: [{ personId: '2', person: 'A', guest: null, weight: 95 }] })
  assert.equal(small.rows[0]?.value, '0.95')
})

test('the weights of a template are shown in lowest terms and never changed otherwise', () => {
  assert.deepEqual(reducedWeights([5000, 5000]), [1, 1])
  assert.deepEqual(reducedWeights([2500, 7500]), [1, 3])
  assert.deepEqual(reducedWeights([2, 1, 1]), [2, 1, 1])
  assert.deepEqual(reducedWeights([4, 2, 0]), [2, 1, 0], 'a part of nothing stays nothing')
  assert.deepEqual(reducedWeights([0, 3]), [0, 1])
  assert.deepEqual(reducedWeights([]), [])
  assert.deepEqual(reducedWeights([0, 0]), [0, 0])
})
