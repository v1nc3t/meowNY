import assert from 'node:assert/strict'
import { afterEach, beforeEach, test } from 'node:test'
import { accountsSetUp, forgetGuide, loadGuide, nextStep, saveGuide, startGuide, stepsFor, type StepId } from './gettingStarted.ts'

let data = new Map<string, string>()
let broken = false
const storage = {
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
afterEach(() => Reflect.deleteProperty(globalThis, 'localStorage'))

const ids = (where: { phone: boolean; served: boolean }) => stepsFor(where).map((step) => step.id)

test('the desktop app has every step; a served page cannot pair a phone; a phone has neither backups nor pairing', () => {
  assert.equal(ids({ phone: false, served: false }).length, 11)
  assert.ok(ids({ phone: false, served: false }).includes('phone'))
  assert.ok(!ids({ phone: false, served: true }).includes('phone'))
  assert.ok(ids({ phone: false, served: true }).includes('backup'))
  const phone = ids({ phone: true, served: false })
  assert.ok(!phone.includes('phone') && !phone.includes('backup'))
  assert.equal(phone[0], 'vault')
})

test('the main account the app starts with is not set up until it is renamed, given a balance, or joined by another', () => {
  assert.equal(accountsSetUp([{ name: 'Main account', openingBalance: '0.00' }]), false)
  assert.equal(accountsSetUp([{ name: 'Main account', openingBalance: '-0.00' }]), false)
  assert.equal(accountsSetUp([{ name: 'Bank', openingBalance: '0.00' }]), true)
  assert.equal(accountsSetUp([{ name: 'Main account', openingBalance: '120.50' }]), true)
  assert.equal(accountsSetUp([{ name: 'Main account', openingBalance: '0.00' }, { name: 'Cash', openingBalance: '0.00' }]), true)
  assert.equal(accountsSetUp([]), false)
})

test('the next step is the first one not done', () => {
  const steps = stepsFor({ phone: false, served: false })
  assert.equal(nextStep(steps, new Set<StepId>(['vault'])), 'accounts')
  assert.equal(nextStep(steps, new Set<StepId>(['vault', 'accounts', 'transaction'])), 'categories')
  assert.equal(nextStep(steps, new Set(steps.map((step) => step.id))), null)
})

test('the steps open for new data, stay hidden once hidden, and go with the data', () => {
  assert.equal(loadGuide(), null)
  assert.equal(startGuide('key'), 'key')
  assert.equal(loadGuide(), 'open')
  saveGuide('hidden')
  assert.equal(loadGuide(), 'hidden')
  forgetGuide()
  assert.equal(loadGuide(), null)
})

test('blocked storage shows nothing and throws nothing', () => {
  broken = true
  assert.equal(loadGuide(), null)
  saveGuide('open')
  forgetGuide()
  assert.equal(startGuide(1), 1)
})
