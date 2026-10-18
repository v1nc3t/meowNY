import type { Recurring } from '../api/recurring'

/** The recurring items that are split with friends or guests and still running, the next to fall due first. */
export function sharedItems(items: readonly Recurring[]): Recurring[] {
  return items.filter((item) => item.isActive && item.split !== null).sort((a, b) => a.nextDueDate.localeCompare(b.nextDueDate) || Number(a.id) - Number(b.id))
}

/** The names the payments of an item are shared with: the friends and the guests, not the person. */
export function sharedWith(item: Recurring): string[] {
  return (item.split?.parts ?? []).map((part) => part.person ?? part.guest ?? '').filter((name) => name !== '')
}
