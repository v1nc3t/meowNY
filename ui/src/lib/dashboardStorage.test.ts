import assert from 'node:assert/strict'
import { afterEach, beforeEach, test } from 'node:test'
import { defaultLayout, moveWidget } from './dashboardLayout.ts'
import { forgetLayouts, layoutKey, loadLayout, saveLayout } from './dashboardStorage.ts'

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

test('each profile has its own key, and a layout comes back as it went', () => {
  assert.equal(layoutKey('desktop'), 'dashboard.layout.v1.desktop')
  const moved = moveWidget(defaultLayout('desktop'), 'debts', 0)
  saveLayout('desktop', moved)
  assert.deepEqual(loadLayout('desktop'), moved)
  assert.deepEqual(loadLayout('phone'), defaultLayout('phone'))
})

test('forgetting removes both layouts', () => {
  saveLayout('desktop', moveWidget(defaultLayout('desktop'), 'debts', 0))
  saveLayout('phone', moveWidget(defaultLayout('phone'), 'stashes', 0))
  forgetLayouts()
  assert.equal(data.size, 0)
})

test('storage that throws is the default dashboard, and nothing throws', () => {
  broken = true
  assert.deepEqual(loadLayout('phone'), defaultLayout('phone'))
  assert.doesNotThrow(() => saveLayout('phone', defaultLayout('phone')))
  assert.doesNotThrow(() => forgetLayouts())
})
