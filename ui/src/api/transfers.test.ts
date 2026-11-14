import assert from 'node:assert/strict'
import { afterEach, test } from 'node:test'
import { transactions, TransactionPage } from './transactions.ts'
import { Transfer, TransferPage, transfers } from './transfers.ts'

const transfer = { id: '3', fromAccountId: '1', toAccountId: '2', amount: '200.00', date: '2026-11-03', note: 'Monthly saving' }
const page = { transfers: [transfer, { ...transfer, id: '4', note: null }], total: 7, limit: 50, offset: 0 }
const ok = (schema: { safeParse: (value: unknown) => { success: boolean } }, value: unknown) => schema.safeParse(value).success

// The app's side of `invoke`: the commands that were sent, and what each answers.
function backend(answer: (command: string, args: unknown) => unknown) {
  const sent: { command: string; args: unknown }[] = []
  Object.defineProperty(globalThis, 'window', {
    value: {
      __TAURI_INTERNALS__: {
        invoke: async (command: string, args: unknown) => {
          sent.push({ command, args })
          return answer(command, args)
        },
      },
    },
    configurable: true,
  })
  return sent
}
afterEach(() => {
  Reflect.deleteProperty(globalThis, 'window')
})

test('a page of transfers as the app sends it (section 6.3) is accepted', () => {
  assert.deepEqual(TransferPage.parse(page), page)
  assert.ok(ok(TransferPage, { ...page, transfers: [], total: 0 }), 'no transfers at all')
  assert.ok(ok(TransferPage, { ...page, offset: 50 }))
})

test('a page of transfers with a wrong shape is refused', () => {
  const bad: [string, unknown][] = [
    ['money as a number', { ...page, transfers: [{ ...transfer, amount: 200 }] }],
    ['one decimal', { ...page, transfers: [{ ...transfer, amount: '200.0' }] }],
    ['a minus on a transfer', { ...page, transfers: [{ ...transfer, amount: '-200.00' }] }],
    ['an id as a number', { ...page, transfers: [{ ...transfer, id: 3 }] }],
    ['a date with a time', { ...page, transfers: [{ ...transfer, date: '2026-11-03T00:00:00Z' }] }],
    ['no note field', { ...page, transfers: [(({ note: _note, ...rest }) => rest)(transfer)] }],
    ['the total as text', { ...page, total: '7' }],
    ['a negative total', { ...page, total: -1 }],
    ['a limit of 0', { ...page, limit: 0 }],
    ['a fractional offset', { ...page, offset: 0.5 }],
    ['the list under another name', { total: 7, limit: 50, offset: 0, rows: [transfer] }],
    ['a list of nothing', { ...page, transfers: null }],
  ]
  for (const [name, value] of bad) assert.ok(!ok(TransferPage, value), name)
  assert.ok(ok(Transfer, transfer))
})

test('the list of transfers asks list_transfers with the filter, and checks the answer', async () => {
  const sent = backend(() => page)
  const filter = { accountId: '2', from: '2026-11-01', to: '2026-11-30', limit: 10, offset: 20 }
  assert.deepEqual(await transfers.list(filter), page)
  assert.deepEqual(sent, [{ command: 'list_transfers', args: { filter } }])
  await transfers.list()
  assert.deepEqual(sent[1].args, { filter: {} }, 'no filter is an empty one')
})

test('a wrong answer to list_transfers is an error, not a list', async () => {
  backend(() => ({ ...page, transfers: [{ ...transfer, amount: 200 }] }))
  await assert.rejects(transfers.list({}), { code: 'internal' })
  backend(() => {
    throw 'invalid_input'
  })
  await assert.rejects(transfers.list({ from: '2026-12-01', to: '2026-11-01' }), { code: 'invalid_input' })
})

test('the transactions filter carries kind, and leaves it out for both kinds', async () => {
  const answer = { transactions: [], total: 0, limit: 50, offset: 0 }
  assert.ok(ok(TransactionPage, answer))
  const sent = backend(() => answer)
  await transactions.list({ kind: 'INCOME', accountId: '2', limit: 50, offset: 0 })
  await transactions.list({ kind: 'EXPENSE' })
  await transactions.list({ from: '2026-11-01' })
  assert.deepEqual(
    sent.map((call) => JSON.stringify(call.args)),
    ['{"filter":{"kind":"INCOME","accountId":"2","limit":50,"offset":0}}', '{"filter":{"kind":"EXPENSE"}}', '{"filter":{"from":"2026-11-01"}}'],
  )
})
