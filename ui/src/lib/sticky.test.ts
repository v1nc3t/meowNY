import assert from 'node:assert/strict'
import { afterEach, beforeEach, test } from 'node:test'
import { forgetChoices, saveChoice, storedChoice, storedId } from './sticky.ts'

// A stand-in for the browser's storage; `broken` makes every use throw, as storage does when it is blocked.
let data = new Map<string, string>()
let broken = false
const storage = {
  get length() {
    return data.size
  },
  key: (index: number) => [...data.keys()][index] ?? null,
  getItem(key: string) {
    if (broken) throw new Error('blocked')
    return data.get(key) ?? null
  },
  setItem(key: string, value: string) {
    if (broken) throw new Error('blocked')
    data.set(key, value)
  },
  removeItem(key: string) {
    if (broken) throw new Error('blocked')
    data.delete(key)
  },
}

beforeEach(() => {
  data = new Map()
  broken = false
  Object.defineProperty(globalThis, 'localStorage', { value: storage, configurable: true })
})

afterEach(() => {
  Reflect.deleteProperty(globalThis, 'localStorage')
})

const PERIODS = ['year', 'month', 'date'] as const

test('a saved choice comes back as the value it was', () => {
  saveChoice('tx.period', 'year')
  saveChoice('tx.limit', 100)
  saveChoice('debts.removed', true)
  assert.equal(storedChoice('tx.period', PERIODS, 'month'), 'year')
  assert.equal(storedChoice('tx.limit', [10, 50, 100], 50), 100)
  assert.equal(storedChoice('debts.removed', [false, true], false), true)
})

test('nothing saved, or something that is not one of the known choices, gives the fallback', () => {
  assert.equal(storedChoice('tx.period', PERIODS, 'month'), 'month')
  for (const text of ['', 'week', 'Year', '1', 'null', '{"mode":"year"}']) {
    data.set('choice.tx.period', text)
    assert.equal(storedChoice('tx.period', PERIODS, 'month'), 'month', text)
  }
  data.set('choice.tx.limit', '25')
  assert.equal(storedChoice('tx.limit', [10, 50, 100], 50), 50)
  data.set('choice.debts.removed', 'yes')
  assert.equal(storedChoice('debts.removed', [false, true], false), false)
})

test('an id is returned as stored, and empty when there is none', () => {
  assert.equal(storedId('tx.category'), '')
  saveChoice('tx.category', '42')
  assert.equal(storedId('tx.category'), '42')
})

test('storage that cannot be used gives the fallback and never throws', () => {
  broken = true
  assert.equal(storedChoice('tx.period', PERIODS, 'month'), 'month')
  assert.equal(storedId('tx.category'), '')
  assert.doesNotThrow(() => saveChoice('tx.period', 'year'))
  assert.doesNotThrow(() => forgetChoices())
  Reflect.deleteProperty(globalThis, 'localStorage')
  assert.equal(storedChoice('tx.limit', [10, 50, 100], 50), 50)
  assert.doesNotThrow(() => saveChoice('tx.limit', 10))
})

test('forgetting removes the choices and leaves everything else', () => {
  saveChoice('tx.category', '7')
  saveChoice('tx.period', 'date')
  data.set('ui-theme', 'dark')
  forgetChoices()
  assert.equal(storedId('tx.category'), '')
  assert.equal(storedChoice('tx.period', PERIODS, 'month'), 'month')
  assert.deepEqual([...data.keys()], ['ui-theme'])
})
