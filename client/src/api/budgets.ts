import { z } from 'zod'
import { CategoryType } from './categories'
import { call } from './invoke'

const Amount = z.string().regex(/^-?\d+\.\d{2}$/)
const Id = z.string().regex(/^[1-9]\d*$/)

export const Budget = z.object({
  id: Id,
  type: CategoryType,
  scope: z.enum(['GLOBAL', 'CATEGORY']),
  categoryId: Id.nullable(),
  amountLimit: Amount.nullable(),
  effectiveFrom: z.string().regex(/^\d{4}-\d{2}-01$/),
})
export type Budget = z.infer<typeof Budget>

export const Overview = z.object({
  type: CategoryType,
  budgeted: Amount.nullable(),
  allocated: Amount,
  unallocated: Amount.nullable(),
  actual: Amount,
  remaining: Amount.nullable(),
  overAllocated: z.boolean(),
  categories: z.array(z.object({ categoryId: Id, amountLimit: Amount })),
})
export type Overview = z.infer<typeof Overview>

export type SetBudget = {
  type: CategoryType
  scope: 'GLOBAL' | 'CATEGORY'
  /** "YYYY-MM": the budget applies from this month on, until changed. */
  month: string
  /** Decimal string; null removes the budget from this month on. */
  amountLimit: string | null
  categoryId?: string
}

export const MONTH = /^\d{4}-(?:0[1-9]|1[0-2])$/

export const budgets = {
  set: (input: SetBudget) => call('set_budget', Budget, { input }),
  overview: (month: string) => call('budget_overview', z.array(Overview), { month }),
}
