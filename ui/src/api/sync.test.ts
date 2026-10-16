import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { en } from '../i18n/en.ts'
import {
  allChosen,
  Attempts,
  checkCodeText,
  choicesInOrder,
  cleanDigits,
  cleanDeviceName,
  clockGapText,
  countdownText,
  filesLines,
  isBusy,
  isFinished,
  locksAfterBackground,
  nothingChanged,
  parseEvery,
  Peer,
  PHONE_PASSPHRASE_GOOD,
  reasonKey,
  REASON_KEYS,
  shouldWarn,
  Status,
  summaryLines,
  wantsLongerPassphrase,
} from './sync.ts'

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

test('every status the app sends is accepted', () => {
  for (const status of [
    { state: 'idle' },
    waiting,
    { state: 'connecting', purpose: 'pair' },
    showCode,
    enterCode,
    { state: 'running', purpose: 'sync' },
    reviewing,
    { state: 'waitingReview', purpose: 'sync' },
    done,
    { state: 'failed', purpose: 'sync', reason: 'expired' },
  ]) {
    assert.deepEqual(Status.parse(status), status)
  }
})

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

test('a paired device and the attempts are read as the app sends them', () => {
  const peer = { deviceId: 'ab'.repeat(16), name: 'Phone', pairedAt: '2026-10-15T10:20:30.123Z', lastSyncAt: null, currencyBehind: false, changesWaiting: null }
  assert.deepEqual(Peer.parse(peer), peer)
  assert.equal(Peer.parse({ ...peer, lastSyncAt: '2026-10-15T11:00:00.000Z', changesWaiting: 3 }).changesWaiting, 3)
  for (const bad of [{ ...peer, deviceId: 'AB'.repeat(16) }, { ...peer, deviceId: 'ab' }, { ...peer, pairedAt: 'now' }, { ...peer, name: undefined }, { ...peer, currencyBehind: 'yes' }, { ...peer, currencyBehind: undefined }, { ...peer, changesWaiting: -1 }, { ...peer, changesWaiting: 1.5 }, { ...peer, changesWaiting: undefined }]) {
    assert.equal(Peer.safeParse(bad).success, false, JSON.stringify(bad))
  }
  assert.deepEqual(Attempts.parse({ failed: 3, allowed: 5, left: 2 }), { failed: 3, allowed: 5, left: 2 })
  assert.equal(Attempts.nullable().parse(null), null)
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

test('every reason has a text, and one that is not known shows as an internal error', () => {
  for (const [reason, key] of Object.entries(REASON_KEYS)) {
    assert.equal(reasonKey(reason), key)
    assert.ok(key in en, key)
  }
  assert.equal(reasonKey('made_up'), 'sync.reason.internal')
  assert.equal(reasonKey(''), 'sync.reason.internal')
  assert.equal(reasonKey('toString'), 'sync.reason.internal')
})

test('busy and finished statuses are told apart', () => {
  assert.equal(isBusy(Status.parse({ state: 'idle' })), false)
  assert.equal(isBusy(Status.parse(waiting)), true)
  assert.equal(isBusy(Status.parse(showCode)), true)
  assert.equal(isBusy(Status.parse(enterCode)), true)
  assert.equal(isBusy(Status.parse(reviewing)), true)
  assert.equal(isBusy(Status.parse({ state: 'waitingReview', purpose: 'sync' })), true)
  assert.equal(isBusy(Status.parse(done)), false)
  assert.equal(isFinished(Status.parse(done)), true)
  assert.equal(isFinished(Status.parse({ state: 'failed', purpose: 'pair', reason: 'declined' })), true)
  assert.equal(isFinished(Status.parse(waiting)), false)
})

test('the numbers on screen are easy to read', () => {
  assert.equal(checkCodeText('123456'), '123 456')
  assert.equal(checkCodeText('000007'), '000 007')
  assert.equal(countdownText(120), '2:00')
  assert.equal(countdownText(299), '4:59')
  assert.equal(countdownText(9), '0:09')
  assert.equal(countdownText(-4), '0:00')
  assert.equal(clockGapText(75), '75 s')
  assert.equal(clockGapText(119), '119 s')
  assert.equal(clockGapText(180), '3 min')
})

test('a summary lists only what happened', () => {
  assert.deepEqual(summaryLines({ added: 5, updated: 2, replaced: 1, deleted: 0, merged: 0, dropped: 0 }), [
    { key: 'sync.sumAdded', count: 5 },
    { key: 'sync.sumUpdated', count: 2 },
    { key: 'sync.sumReplaced', count: 1 },
  ])
  const none = { added: 0, updated: 0, replaced: 0, deleted: 0, merged: 0, dropped: 0 }
  assert.equal(nothingChanged(none), true)
  assert.equal(nothingChanged({ ...none, merged: 1 }), false)
  for (const line of summaryLines({ added: 1, updated: 1, replaced: 1, deleted: 1, merged: 1, dropped: 1 })) assert.ok(line.key in en, line.key)
})

test('the lines about documents list only what happened, and every text exists', () => {
  assert.deepEqual(filesLines({ received: 2, sent: 0, failed: 1, waiting: 3, removed: 1, interrupted: false }), [
    { key: 'sync.filesReceived', count: 2 },
    { key: 'sync.filesRemoved', count: 1 },
    { key: 'sync.filesFailed', count: 1 },
    { key: 'sync.filesWaiting', count: 3 },
  ])
  assert.deepEqual(filesLines({ received: 0, sent: 0, failed: 0, waiting: 0, removed: 0, interrupted: true }), [])
  for (const line of filesLines({ received: 1, sent: 1, failed: 1, waiting: 1, removed: 1, interrupted: false })) assert.ok(line.key in en, line.key)
  assert.ok('sync.filesInterrupted' in en)
})

test('a device name follows the rule of the app', () => {
  assert.equal(cleanDeviceName('  my phone '), 'my phone')
  assert.equal(cleanDeviceName('x'.repeat(50)), 'x'.repeat(50))
  for (const bad of ['', '   ', 'x'.repeat(51), 'tab\there', 'new\nline', '\u0000']) assert.equal(cleanDeviceName(bad), null, JSON.stringify(bad))
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

test('"every" arrives as a number and a unit', () => {
  assert.deepEqual(parseEvery('1 MONTH'), { count: 1, unit: 'MONTH' })
  assert.deepEqual(parseEvery('12 WEEK'), { count: 12, unit: 'WEEK' })
  for (const bad of ['', 'MONTH', '0 MONTH', '1 FORTNIGHT', '1 month', '1  MONTH']) assert.equal(parseEvery(bad), null, bad)
})

