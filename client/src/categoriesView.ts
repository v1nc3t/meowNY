export type { Category } from './api/categories'
import type { Category } from './api/categories'
export type Kind = 'ALL' | 'INCOME' | 'EXPENSE'

export function visibleCategories(categories: Category[], kind: Kind, query: string): Category[] {
  const q = query.trim().toLowerCase()
  return categories.filter(
    (category) =>
      (kind === 'ALL' || category.type === kind) && (q.length === 0 || category.name.toLowerCase().includes(q)),
  )
}
