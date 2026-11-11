export type { Category } from '../api/categories'
import type { Category } from '../api/categories'

/** The categories as the options of a dropdown, in two sections: expense categories, then income ones, each in the order they came in. */
export function categoryOptions(categories: Category[], names: { expense: string; income: string }): { value: string; label: string; group: string }[] {
  const section = (type: Category['type'], group: string) =>
    categories.filter((category) => category.type === type).map((category) => ({ value: category.id, label: category.name, group }))
  return [...section('EXPENSE', names.expense), ...section('INCOME', names.income)]
}
