import { z } from 'zod'
import { call } from './invoke'

// Money moved between two of the person's own accounts (docs/accounts-design.md, section 6.3). Never income, never an expense.
const id = z.string().regex(/^[1-9]\d*$/)

export const Transfer = z.object({
  id,
  fromAccountId: id,
  toAccountId: id,
  /** Always above 0. */
  amount: z.string().regex(/^\d+\.\d{2}$/),
  date: z.string().regex(/^\d{4}-\d{2}-\d{2}$/),
  note: z.string().nullable(),
})
export type Transfer = z.infer<typeof Transfer>

export type NewTransfer = { fromAccountId: string; toAccountId: string; amount: string; date: string; note?: string }

/** Fields left out stay as they are; `note: null` clears it. */
export type TransferPatch = { fromAccountId?: string; toAccountId?: string; amount?: string; date?: string; note?: string | null }

export const TransferPage = z.object({
  transfers: z.array(Transfer),
  total: z.number().int().nonnegative(),
  limit: z.number().int().positive(),
  offset: z.number().int().nonnegative(),
})
export type TransferPage = z.infer<typeof TransferPage>

/** `accountId` matches either side of a transfer; `from` and `to` are days ("YYYY-MM-DD"), both included. Newest first. */
export type TransferFilter = { accountId?: string; from?: string; to?: string; limit?: number; offset?: number }

export const transfers = {
  list: (filter: TransferFilter = {}) => call('list_transfers', TransferPage, { filter }),
  create: (input: NewTransfer) => call('create_transfer', Transfer, { input }),
  get: (id: string) => call('get_transfer', Transfer, { id }),
  update: (id: string, patch: TransferPatch) => call('update_transfer', Transfer, { id, patch }),
  remove: (id: string) => call('delete_transfer', z.null(), { id }).then(() => undefined),
}
