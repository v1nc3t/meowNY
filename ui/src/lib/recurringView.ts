import type { Category } from '../api/categories'
import type { Recurring } from '../api/recurring'

/** The recurring items that are split with friends or guests and still running, the next to fall due first. */
export function sharedItems(items: readonly Recurring[]): Recurring[] {
  return items.filter((item) => item.isActive && item.split !== null).sort((a, b) => a.nextDueDate.localeCompare(b.nextDueDate) || Number(a.id) - Number(b.id))
}

/** The names the payments of an item are shared with: the friends and the guests, not the person. */
export function sharedWith(item: Recurring): string[] {
  return (item.split?.parts ?? []).map((part) => part.person ?? part.guest ?? '').filter((name) => name !== '')
}

/**
 * The recurring items in their two lists: income, for an item whose category is an income, and expenses for every other (an item whose category
 * is gone is an expense, as the forms always took it). The order of `items` stays; an item that is paused or ended stays in its list.
 */
export function groupByType(items: readonly Recurring[], categories: readonly Category[]): { expenses: Recurring[]; income: Recurring[] } {
  const incomeIds = new Set(categories.filter((category) => category.type === 'INCOME').map((category) => category.id))
  return { expenses: items.filter((item) => !incomeIds.has(item.categoryId)), income: items.filter((item) => incomeIds.has(item.categoryId)) }
}
