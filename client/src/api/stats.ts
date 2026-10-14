import { z } from 'zod'
import { Overview } from './budgets'
import { CategoryType } from './categories'
import { call } from './invoke'

// Numbers for the charts. The Rust side aggregates; the page only draws. Money stays a
// two-decimal string and is never summed or compared as a JavaScript number.
const Amount = z.string().regex(/^-?\d+\.\d{2}$/)
const Id = z.string().regex(/^[1-9]\d*$/)
const Month = z.string().regex(/^\d{4}-(?:0[1-9]|1[0-2])$/)
/** A percentage with one decimal. Only used for bar lengths and labels. */
const Percent = z.number().finite()

export const CategoryLine = z.object({
  categoryId: Id,
  name: z.string(),
  type: CategoryType,
  budgeted: Amount.nullable(),
  actual: Amount,
  remaining: Amount.nullable(),
  pctUsed: Percent.nullable(),
})
export type CategoryLine = z.infer<typeof CategoryLine>

export const BudgetVsActual = z.object({
  month: Month,
  categories: z.array(CategoryLine),
  totals: z.array(Overview),
})
export type BudgetVsActual = z.infer<typeof BudgetVsActual>

export const CategoryTrend = z.object({
  categoryId: Id,
  points: z.array(z.object({ month: Month, budgeted: Amount.nullable(), actual: Amount })),
})
export type CategoryTrend = z.infer<typeof CategoryTrend>

export const YearPoint = z.object({
  month: Month,
  income: Amount,
  expense: Amount,
  savings: Amount,
  savingsRate: Percent.nullable(),
})
export type YearPoint = z.infer<typeof YearPoint>

export const TopCategory = z.object({
  categoryId: Id,
  name: z.string(),
  actual: Amount,
  share: Percent.nullable(),
})
export type TopCategory = z.infer<typeof TopCategory>

export const Commitments = z.object({
  expense: Amount,
  income: Amount,
  items: z.array(z.object({ id: Id, name: z.string(), type: CategoryType, perMonth: Amount })),
})
export type Commitments = z.infer<typeof Commitments>

export const PlannedVsPaid = z.object({
  recurringId: Id,
  name: z.string(),
  paidCount: z.number().int().positive(),
  planned: Amount,
  paid: Amount,
  difference: Amount,
})
export type PlannedVsPaid = z.infer<typeof PlannedVsPaid>

export const stats = {
  /** Every category for one month ("YYYY-MM"), with the income and expense totals. */
  budgetVsActual: (month: string) => call('budget_vs_actual', BudgetVsActual, { month }),
  /** One category month by month, `from` to `to` inclusive. */
  categoryTrend: (categoryId: string, from: string, to: string) =>
    call('category_trend', CategoryTrend, { categoryId, from, to }),
  /** The 12 months of a year ("YYYY"). */
  yearTrend: (year: string) => call('year_trend', z.array(YearPoint), { year }),
  topCategories: (month: string, kind: CategoryType, limit = 5) =>
    call('top_categories', z.array(TopCategory), { month, kind, limit }),
  recurringCommitments: () => call('recurring_commitments', Commitments),
  recurringPlannedVsPaid: (month: string) => call('recurring_planned_vs_paid', z.array(PlannedVsPaid), { month }),
}
