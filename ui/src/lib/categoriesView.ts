export type { Category } from '../api/categories'
import type { Overview } from '../api/budgets'
import type { Category } from '../api/categories'
import { fromMinor, toMinor } from './money'

/** The categories as the options of a dropdown, in two sections: expense categories, then income ones, each in the order they came in. */
export function categoryOptions(categories: Category[], names: { expense: string; income: string }): { value: string; label: string; group: string }[] {
  const section = (type: Category['type'], group: string) =>
    categories.filter((category) => category.type === type).map((category) => ({ value: category.id, label: category.name, group }))
  return [...section('EXPENSE', names.expense), ...section('INCOME', names.income)]
}

export type CategoryTab = 'EXPENSE' | 'INCOME' | 'HISTORY'

/** The tabs of the categories page on a phone: the expenses, the income, and the history once there is some. (The computer shows them all at once.) */
export function categoryTabs(hasHistory: boolean): readonly CategoryTab[] {
  return hasHistory ? ['EXPENSE', 'INCOME', 'HISTORY'] : ['EXPENSE', 'INCOME']
}

/** The tab to show: the one chosen if it is among `tabs`, else the expenses (a tab that is gone, such as the history of no months, or a word nobody knows). */
export function shownTab(tabs: readonly CategoryTab[], chosen: string): CategoryTab {
  return tabs.find((tab) => tab === chosen) ?? 'EXPENSE'
}

/** The tab an address names: `?tab=income` and `?tab=history`, so other pages can link to the income (a phone shows that tab, the computer scrolls to that part). No tab and a word nobody knows are the expenses. */
export function tabOfSearch(search: string | URLSearchParams): CategoryTab {
  const wanted = new URLSearchParams(search).get('tab')
  return wanted === 'income' ? 'INCOME' : wanted === 'history' ? 'HISTORY' : 'EXPENSE'
}

/** The query of a tab: none for the expenses, the page's own address. */
export function tabQuery(tab: CategoryTab): Record<string, string> {
  return tab === 'INCOME' ? { tab: 'income' } : tab === 'HISTORY' ? { tab: 'history' } : {}
}

// -- the budgets of one side --------------------------------------------------------------------------------------------------

export type Side = 'EXPENSE' | 'INCOME'

/** Spending against its budget: green at or under it, red above it, plain when there is no budget. */
export function spentClass(spent: string, budget: string | null): string {
  if (budget === null) return 'cell-amount'
  return Number(spent) > Number(budget) ? 'cell-amount is-over' : 'cell-amount is-income'
}

/** Earning against its budget: green above it, plain otherwise. */
export function earnedClass(earned: string, budget: string | null): string {
  return budget !== null && Number(earned) > Number(budget) ? 'cell-amount is-income' : 'cell-amount'
}

/** What a budget row shows past its two amounts: what is left of the budget (or still to come, for income), and how much of it is used. */
export type BudgetGap = {
  /** Budget minus what was spent or earned, as an amount: below 0 when an expense went over; never below 0 for income (reached is its word). */
  left: string
  /** `floor(used x 100 / budget)`; it may be above 100. */
  percent: number
  /** From 0 to 1: how full the bar is. */
  fill: number
  /** over: an expense above its budget (red); good: income that reached its budget (green); plain otherwise. */
  tone: 'plain' | 'over' | 'good'
}

/** The left and used columns of a category (or of a side): none without a budget above 0. Whole cents, no floating point in the amounts. */
export function budgetGap(used: string, budget: string | null, type: Side): BudgetGap | null {
  const aim = budget === null ? null : minor(budget)
  if (aim === null || aim <= 0n) return null
  const done = minor(used)
  const rest = aim - done
  const percent = done > 0n ? Number((done * 100n) / aim) : 0
  const fill = Math.min(1, Math.max(0, percent / 100))
  if (type === 'EXPENSE') return { left: fromMinor(rest), percent, fill, tone: rest < 0n ? 'over' : 'plain' }
  return { left: fromMinor(rest > 0n ? rest : 0n), percent, fill, tone: rest <= 0n ? 'good' : 'plain' }
}

const minor = (amount: string): bigint => toMinor(amount) ?? 0n

export function amountsFor(overview: Overview[] | null, type: Side): Map<string, string> {
  const row = overview?.find((item) => item.type === type)
  return new Map((row?.categories ?? []).map((line) => [line.categoryId, line.amountLimit]))
}

export function spentFor(overview: Overview[] | null, type: Side): Map<string, string> {
  const row = overview?.find((item) => item.type === type)
  return new Map((row?.spent ?? []).map((line) => [line.categoryId, line.amount]))
}
