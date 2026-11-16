export type { Category } from '../api/categories'
import type { Overview } from '../api/budgets'
import type { Category } from '../api/categories'

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

export function amountsFor(overview: Overview[] | null, type: Side): Map<string, string> {
  const row = overview?.find((item) => item.type === type)
  return new Map((row?.categories ?? []).map((line) => [line.categoryId, line.amountLimit]))
}

export function spentFor(overview: Overview[] | null, type: Side): Map<string, string> {
  const row = overview?.find((item) => item.type === type)
  return new Map((row?.spent ?? []).map((line) => [line.categoryId, line.amount]))
}
