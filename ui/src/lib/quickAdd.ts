import type { Category, CategoryType } from '../api/categories'

/** How many recent categories the quick add shows as buttons. */
export const RECENT_LIMIT = 6

/**
 * The categories to offer first when adding a transaction in a hurry: the ones used by the newest transactions, newest first,
 * then the rest in their own order, up to `limit`. Only categories of the chosen type that still exist.
 */
export function recentCategories(
  newestFirst: readonly { categoryId: string }[],
  categories: readonly Category[],
  kind: CategoryType,
  limit = RECENT_LIMIT,
): Category[] {
  const ofKind = new Map(categories.filter((category) => category.type === kind).map((category) => [category.id, category]))
  const picked: Category[] = []
  const seen = new Set<string>()
  for (const { categoryId } of newestFirst) {
    const category = ofKind.get(categoryId)
    if (category && !seen.has(categoryId)) {
      seen.add(categoryId)
      picked.push(category)
    }
    if (picked.length >= limit) return picked
  }
  for (const category of ofKind.values()) {
    if (picked.length >= limit) break
    if (!seen.has(category.id)) picked.push(category)
  }
  return picked
}

/** The longest a transaction name can be. */
export const NAME_MAX = 50

/** What to call a transaction typed in a hurry: the name if there is one, otherwise the category's name. */
export function quickName(typed: string, categoryName: string): string {
  const name = typed.trim() || categoryName.trim()
  return [...name].slice(0, NAME_MAX).join('')
}
