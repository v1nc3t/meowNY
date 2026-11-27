import { fromMinor, toMinor } from './money'

// The total row under the transactions: what came in, what went out and what is left of the two, in the app's currency.
// Money is whole minor units (bigint): no floating point touches it.

export type TxTotals = { count: number; income: string; expense: string; net: string }

/**
 * The totals of the transactions shown, by what each costs the person (`myShare`); a transaction in an income category counts as money in, every
 * other one as money out. Only when the page holds the whole list (`total` is no more than what is shown): the sum of one page of many would read as
 * the total of the period. `income` and `expense` are sizes, `net` is signed.
 */
export function txTotals(rows: readonly { categoryId: string; myShare: string }[], total: number, incomeCategories: ReadonlySet<string>): TxTotals | null {
  if (rows.length < 2 || total > rows.length) return null
  let income = 0n
  let expense = 0n
  for (const row of rows) {
    const cents = toMinor(row.myShare) ?? 0n
    if (incomeCategories.has(row.categoryId)) income += cents
    else expense += cents
  }
  return { count: rows.length, income: fromMinor(income), expense: fromMinor(expense), net: fromMinor(income - expense) }
}
