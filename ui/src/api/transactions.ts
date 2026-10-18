import { z } from 'zod'
import { call } from './invoke'

/** One friend or guest's part of a shared bill. A friend has `personId` and `person`; a guest only `guest`. */
export const PartOut = z.object({
  personId: z.string().regex(/^[1-9]\d*$/).nullable(),
  person: z.string().nullable(),
  guest: z.string().nullable(),
  amount: z.string().regex(/^\d+\.\d{2}$/),
})
export type PartOut = z.infer<typeof PartOut>

export const Transaction = z.object({
  id: z.string().regex(/^[1-9]\d*$/),
  categoryId: z.string().regex(/^[1-9]\d*$/),
  clientUuid: z.string().nullable(),
  name: z.string(),
  /** What was paid. Decimal string such as "12.50". Never converted to a number for storage or sums. */
  amount: z.string().regex(/^\d+\.\d{2}$/),
  /** What it costs the person: the amount minus the parts of the others. The same as `amount` for a bill that is not shared. */
  myShare: z.string().regex(/^\d+\.\d{2}$/),
  /** The parts of the others, when the bill is shared; empty otherwise. */
  split: z.array(PartOut),
  paymentDate: z.string().regex(/^\d{4}-\d{2}-\d{2}$/),
  description: z.string().nullable(),
})
export type Transaction = z.infer<typeof Transaction>

export const TransactionPage = z.object({
  transactions: z.array(Transaction),
  total: z.number().int().nonnegative(),
  limit: z.number().int().positive(),
  offset: z.number().int().nonnegative(),
})
export type TransactionPage = z.infer<typeof TransactionPage>

export type NewTransaction = {
  name: string
  amount: string
  paymentDate: string
  categoryId: string
  description?: string | null
  /** Makes create safe to retry: the same value never creates a second transaction. */
  clientUuid?: string
}

/** Fields left out stay as they are; `description: null` clears it. */
export type TransactionPatch = Partial<Omit<NewTransaction, 'clientUuid'>>

export type TransactionFilter = {
  from?: string
  to?: string
  categoryId?: string
  q?: string
  limit?: number
  offset?: number
}

export const transactions = {
  list: (filter: TransactionFilter = {}) => call('list_transactions', TransactionPage, { filter }),
  create: (input: NewTransaction) => call('create_transaction', Transaction, { input }),
  update: (id: string, patch: TransactionPatch) => call('update_transaction', Transaction, { id, patch }),
  remove: (id: string) => call('delete_transaction', z.null(), { id }).then(() => undefined),
}
