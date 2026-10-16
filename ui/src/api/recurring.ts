import { z } from 'zod'
import { call } from './invoke'
import { Transaction } from './transactions'

const id = z.string().regex(/^[1-9]\d*$/)
const date = z.string().regex(/^\d{4}-\d{2}-\d{2}$/)

export const INTERVAL_UNITS = ['DAY', 'WEEK', 'MONTH', 'YEAR'] as const
export const IntervalUnit = z.enum(INTERVAL_UNITS)
export type IntervalUnit = z.infer<typeof IntervalUnit>

export const Recurring = z.object({
  id,
  categoryId: id,
  name: z.string(),
  description: z.string().nullable(),
  /** Decimal string such as "12.50", never converted to a number for storage or sums. Null for a bill with no fixed amount. */
  amount: z.string().regex(/^\d+\.\d{2}$/).nullable(),
  startDate: date,
  intervalUnit: IntervalUnit,
  intervalCount: z.number().int().positive(),
  nextDueDate: date,
  endDate: date.nullable(),
  isActive: z.boolean(),
})
export type Recurring = z.infer<typeof Recurring>

export const Dues = z.object({ overdue: z.array(Recurring), upcoming: z.array(Recurring) })
export type Dues = z.infer<typeof Dues>

export type NewRecurring = {
  name: string
  /** Left out or null for a bill whose amount changes every time. */
  amount?: string | null
  categoryId: string
  startDate: string
  intervalUnit: IntervalUnit
  intervalCount: number
  endDate?: string | null
  description?: string | null
}

/** Fields left out stay as they are; `endDate: null` and `description: null` clear them, and `amount: null` makes it a bill with no fixed amount. */
export type RecurringPatch = Partial<NewRecurring>

export const recurring = {
  list: () => call('list_recurring', z.array(Recurring)),
  create: (input: NewRecurring) => call('create_recurring', Recurring, { input }),
  update: (id: string, patch: RecurringPatch) => call('update_recurring', Recurring, { id, patch }),
  remove: (id: string) => call('delete_recurring', z.null(), { id }).then(() => undefined),
  /** Turns the current occurrence into a transaction. `amount` defaults to the planned one, `paidOn` to today. */
  pay: (id: string, options: { amount?: string; paidOn?: string } = {}) =>
    call('pay_recurring', Transaction, { id, amount: options.amount ?? null, paidOn: options.paidOn ?? null }),
  skip: (id: string) => call('skip_recurring', Recurring, { id }),
  setActive: (id: string, active: boolean) => call('set_recurring_active', Recurring, { id, active }),
  end: (id: string) => call('end_recurring', Recurring, { id }),
  dues: (withinDays?: number) => call('recurring_dues', Dues, { withinDays: withinDays ?? null }),
}
