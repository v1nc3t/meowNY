import { z } from 'zod'
import { call } from './invoke'

export const CategoryType = z.enum(['INCOME', 'EXPENSE'])
export type CategoryType = z.infer<typeof CategoryType>

export const Category = z.object({
  id: z.string().regex(/^[1-9]\d*$/),
  name: z.string(),
  type: CategoryType,
})
export type Category = z.infer<typeof Category>

export const NAME_MAX = 50

/** Trimmed name, or null when it is empty, too long or has control characters. Rust checks the same. */
export function cleanCategoryName(input: string): string | null {
  const name = input.trim()
  if (name === '' || [...name].length > NAME_MAX || /\p{Cc}/u.test(name)) return null
  return name
}

export const categories = {
  list: (type?: CategoryType) => call('list_categories', z.array(Category), { kind: type ?? null }),
  create: (name: string, type: CategoryType) => call('create_category', Category, { name, kind: type }),
  rename: (id: string, name: string) => call('rename_category', Category, { id, name }),
  remove: (id: string) => call('delete_category', z.null(), { id }).then(() => undefined),
}
