import { z } from 'zod'
import { call } from './invoke'

export const Transaction = z.object({
  id: z.string().regex(/^[1-9]\d*$/),
  categoryId: z.string().regex(/^[1-9]\d*$/),
  clientUuid: z.string().nullable(),
  name: z.string(),
  /** Decimal string such as "12.50". Never converted to a number for storage or sums. */
  amount: z.string().regex(/^\d+\.\d{2}$/),
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
