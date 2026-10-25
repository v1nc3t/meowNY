import { z } from 'zod'
import { call } from './invoke'
import type { Span } from './budgets'

export const CategoryType = z.enum(['INCOME', 'EXPENSE'])
export type CategoryType = z.infer<typeof CategoryType>

export const Category = z.object({
  id: z.string().regex(/^[1-9]\d*$/),
  name: z.string(),
  type: CategoryType,
})
export type Category = z.infer<typeof Category>

/** A category as the budget page of one month shows it: `deleted` means it is gone from a later month on, and can only be looked at. */
export const CategoryInMonth = Category.extend({ deleted: z.boolean() })
export type CategoryInMonth = z.infer<typeof CategoryInMonth>

export const NAME_MAX = 50

/** Trimmed name, or null when it is empty, too long or has control characters. Rust checks the same. */
export function cleanCategoryName(input: string): string | null {
  const name = input.trim()
  if (name === '' || [...name].length > NAME_MAX || /\p{Cc}/u.test(name)) return null
  return name
}

export const categories = {
  list: (type?: CategoryType) => call('list_categories', z.array(Category), { kind: type ?? null }),
  /** The categories that count in a month ("YYYY-MM"), including ones deleted from a later month on. */
  listInMonth: (month: string, type?: CategoryType) =>
    call('list_categories_in_month', z.array(CategoryInMonth), { kind: type ?? null, month }),
  create: (name: string, type: CategoryType) => call('create_category', Category, { name, kind: type }),
  rename: (id: string, name: string) => call('rename_category', Category, { id, name }),
  /** Without a month and a span the category is deleted from now on. With them it is deleted for that period, seen from that month. */
  remove: (id: string, month?: string, span?: Span) =>
    call('delete_category', z.null(), month && span ? { id, month, span } : { id }).then(() => undefined),
  /** The ids of all categories of one type that are not deleted, in the order to keep. */
  reorder: (ids: string[]) => call('reorder_categories', z.null(), { ids }).then(() => undefined),
}
