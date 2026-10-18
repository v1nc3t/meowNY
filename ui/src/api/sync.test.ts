import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { allChosen, choicesInOrder, cleanDigits, FIELDS, locksAfterBackground, PHONE_PASSPHRASE_GOOD, REASON_KEYS, shouldWarn, Status, TABLES, wantsLongerPassphrase } from './sync.ts'

// What the Rust side sends, as its own tests show it (sync/service.rs, sync/peers.rs, vault.rs).
const waiting = { state: 'waiting', purpose: 'sync', qr: 'MNY1:x', address: '192.168.1.2:5000', expiresInSecs: 120 }
const showCode = { state: 'showCode', purpose: 'pair', code: '123456', peer: 'Phone' }
const enterCode = { state: 'enterCode', purpose: 'pair', triesLeft: 2, wrong: true }
const reviewing = {
  state: 'reviewing',
  purpose: 'sync',
  conflicts: [
    { table: 'transactions', id: 'ab'.repeat(16), title: 'Coffee', mineDeleted: false, theirsDeleted: false, fields: [{ field: 'amount', mine: '3.50', theirs: '4.00' }] },
    { table: 'categories', id: 'cd'.repeat(16), title: 'Food', mineDeleted: true, theirsDeleted: false, fields: [{ field: 'name', mine: '', theirs: 'Food' }] },
  ],
  more: 3,
}
const done = {
  state: 'done',
  purpose: 'sync',
  summary: { added: 5, updated: 2, replaced: 1, deleted: 0, merged: 0, dropped: 0 },
  files: { received: 2, sent: 0, failed: 1, waiting: 3, removed: 1, interrupted: false },
  warnings: [{ kind: 'clock_off', seconds: 75 }],
  differences: false,
  oneWay: true,
  peer: 'Phone',
}

test('a status the app would never send is refused', () => {
  for (const bad of [
    {},
    { state: 'sleeping' },
    { ...waiting, purpose: 'both' },
    { ...waiting, qr: '' },
    { ...waiting, expiresInSecs: -1 },
    { ...showCode, code: '12345' },
    { ...showCode, code: '12345a' },
    { ...enterCode, triesLeft: -1 },
    { state: 'checkCode', purpose: 'pair', code: '123456', peer: 'Phone', needsAnswer: true },
    { ...reviewing, conflicts: [{ ...reviewing.conflicts[0], table: 'sync_peers' }] },
    { ...reviewing, conflicts: [{ ...reviewing.conflicts[0], fields: [{ field: 'secret', mine: 'a', theirs: 'b' }] }] },
    { ...reviewing, more: -1 },
    { ...done, oneWay: undefined },
    { ...done, summary: { ...done.summary, added: -1 } },
    { ...done, files: undefined },
    { ...done, files: { ...done.files, waiting: -1 } },
    { ...done, files: { ...done.files, interrupted: undefined } },
    { ...done, warnings: [{ kind: 'other', seconds: 1 }] },
    { state: 'failed', purpose: 'sync', reason: 'Not A Word' },
    { state: 'failed', purpose: 'sync' },
  ]) {
    assert.equal(Status.safeParse(bad).success, false, JSON.stringify(bad))
  }
})

test('the reasons the screens translate are exactly the ones the app can give', () => {
  const source = readFileSync(new URL('../../../src-tauri/src/sync/service.rs', import.meta.url), 'utf8')
  const reasonFn = source.slice(source.indexOf('pub fn reason('), source.indexOf('struct Shared'))
  assert.ok(reasonFn.length > 200, 'the reason function was not found')
  const given = new Set<string>()
  for (const match of reasonFn.matchAll(/=> "([a-z_]+)"/g)) given.add(match[1])
  for (const match of source.matchAll(/\.fail\(run, purpose, "([a-z_]+)"\)/g)) given.add(match[1])
  assert.ok(given.size >= 20, `only ${given.size} reasons were found`)
  assert.deepEqual([...given].sort(), Object.keys(REASON_KEYS).sort())
})

test('the lock screen warns from the third wrong passphrase on, while a try is left', () => {
  const at = (failed: number) => ({ failed, allowed: 5, left: 5 - failed })
  assert.equal(shouldWarn(null), false)
  assert.deepEqual([0, 1, 2, 3, 4, 5].map((failed) => shouldWarn(at(failed))), [false, false, false, true, true, false])
})

test('the phone asks for a longer passphrase until it is long', () => {
  assert.equal(wantsLongerPassphrase(''), false)
  assert.equal(wantsLongerPassphrase('short one'), true)
  assert.equal(wantsLongerPassphrase('x'.repeat(PHONE_PASSPHRASE_GOOD - 1)), true)
  assert.equal(wantsLongerPassphrase('x'.repeat(PHONE_PASSPHRASE_GOOD)), false)
  assert.equal(wantsLongerPassphrase('🐈'.repeat(PHONE_PASSPHRASE_GOOD)), false, 'letters are counted, not bytes')
})

test('the phone locks when it was away for the time chosen, and never when the choice is never', () => {
  assert.equal(locksAfterBackground(59_000, 60), false)
  assert.equal(locksAfterBackground(60_000, 60), true)
  assert.equal(locksAfterBackground(10 * 60_000, 300), true)
  assert.equal(locksAfterBackground(10 * 3_600_000, 0), false)
})

test('the digits are kept to six numbers, whatever was typed or pasted', () => {
  assert.equal(cleanDigits('048 213'), '048213')
  assert.equal(cleanDigits('04a8-2b13'), '048213')
  assert.equal(cleanDigits('1234567890'), '123456')
  assert.equal(cleanDigits(''), '')
})

test('every conflict needs an answer, and the answers go out in the order of the conflicts', () => {
  const conflicts = Status.parse(reviewing).state === 'reviewing' ? (Status.parse(reviewing) as { conflicts: Parameters<typeof allChosen>[0] }).conflicts : []
  const [a, b] = [conflicts[0].id, conflicts[1].id]
  assert.equal(allChosen(conflicts, {}), false)
  assert.equal(allChosen(conflicts, { [a]: 'mine' }), false)
  assert.equal(allChosen(conflicts, { [a]: 'mine', [b]: 'theirs' }), true)
  assert.equal(allChosen([], {}), false, 'nothing to choose is not an answer')
  assert.deepEqual(choicesInOrder(conflicts, { [b]: 'theirs', [a]: 'mine' }), ['mine', 'theirs'])
  assert.equal(choicesInOrder(conflicts, { [a]: 'mine' }), null)
})


test('the tables and fields are the ones the Rust side sends', () => {
  const wire = readFileSync(new URL('../../../src-tauri/src/sync/wire.rs', import.meta.url), 'utf8')
  const names = [...(/pub fn name\(self\)[\s\S]*?\n    \}/.exec(wire)?.[0] ?? '').matchAll(/=> "(\w+)"/g)].map((m) => m[1])
  assert.deepEqual([...TABLES].sort(), names.sort())
  const engine = readFileSync(new URL('../../../src-tauri/src/sync/engine.rs', import.meta.url), 'utf8')
  const facts = /fn facts\(conn[\s\S]*?\n\}\n/.exec(engine)?.[0] ?? ''
  const sent = new Set([...facts.matchAll(/\("(\w+)", /g)].map((m) => m[1]))
  for (const field of sent) assert.ok((FIELDS as readonly string[]).includes(field ?? ''), `${field} is not a known field`)
})
