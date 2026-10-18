export type { Category } from '../api/categories'
import type { Category } from '../api/categories'
export type Kind = 'ALL' | 'INCOME' | 'EXPENSE'

export function visibleCategories(categories: Category[], kind: Kind, query: string): Category[] {
  const q = query.trim().toLowerCase()
  return categories.filter(
    (category) =>
      (kind === 'ALL' || category.type === kind) && (q.length === 0 || category.name.toLowerCase().includes(q)),
  )
}

/** The categories as the options of a dropdown, in two sections: expense categories, then income ones, each in the order they came in. */
export function categoryOptions(categories: Category[], names: { expense: string; income: string }): { value: string; label: string; group: string }[] {
  const section = (type: Category['type'], group: string) =>
    categories.filter((category) => category.type === type).map((category) => ({ value: category.id, label: category.name, group }))
  return [...section('EXPENSE', names.expense), ...section('INCOME', names.income)]
}
