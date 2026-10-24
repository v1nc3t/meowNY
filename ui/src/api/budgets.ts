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
  /** Spent this month in the categories of this type. */
  used: Amount,
  /** The category budgets minus `used`; below zero when more was spent than the budgets hold. */
  notUsed: Amount,
  actual: Amount,
  remaining: Amount.nullable(),
  overAllocated: z.boolean(),
  categories: z.array(z.object({ categoryId: Id, amountLimit: Amount })),
})
export type Overview = z.infer<typeof Overview>

export const MonthHistory = z.object({
  month: z.string().regex(/^\d{4}-\d{2}$/),
  expenseBudget: Amount.nullable(),
  expenseActual: Amount,
  incomeBudget: Amount.nullable(),
  incomeActual: Amount,
})
export type MonthHistory = z.infer<typeof MonthHistory>

export type SetBudget = {
  type: CategoryType
  scope: 'GLOBAL' | 'CATEGORY'
  /** "YYYY-MM": the budget applies from this month on, until changed. */
  month: string
  /** Decimal string; null removes the budget from this month on. */
  amountLimit: string | null
  categoryId?: string
}

/** How far a budget change reaches: this month only, this month and every later one, or every month. */
export type Span = 'ONLY' | 'FORWARD' | 'ALL'

export const MONTH = /^\d{4}-(?:0[1-9]|1[0-2])$/

export const budgets = {
  set: (input: SetBudget, span?: Span) => call('set_budget', Budget, span ? { input, span } : { input }),
  /** One row per month since the first budget, newest first. */
  history: () => call('budget_history', z.array(MonthHistory)),
  overview: (month: string) => call('budget_overview', z.array(Overview), { month }),
}
