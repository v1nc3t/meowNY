import { isLiability, type Account } from '../api/accounts'
import { convertBetween, fromMinor, toMinor } from './money.ts'

// The maths of the dashboard's widgets that is more than adding two amounts (docs/dashboard-design.md, section 4): net worth by month, the biggest
// categories and transactions, the newest transactions, what the loans come to, and the points of a sparkline. Plain functions with no React.
// Money is whole minor units (bigint) while it is added; amounts go in and out as the two-decimal text the app sends.

/** An amount in an account's currency turned into the app's, at the account's rate (null: it is in the app's currency already). Signed; 0 for a text that is not an amount. */
function inMain(amount: string, rate: string | null): bigint {
  const minor = toMinor(amount) ?? 0n
  if (rate === null || minor === 0n) return minor
  const converted = toMinor(convertBetween(fromMinor(minor < 0n ? -minor : minor), rate, null) ?? '0.00') ?? 0n
  return minor < 0n ? -converted : converted
}

/** The balances of one account at the end of months ("YYYY-MM": "-12.50"), and its rate (null for an account in the app's currency). */
export type BalanceSeries = { rate: string | null; balances: ReadonlyMap<string, string> }

/** What the accounts hold together at the end of each month, in the app's currency: each account at its own rate. A month an account has no balance for counts 0 for it. */
export function netWorthByMonth(series: readonly BalanceSeries[], months: readonly string[]): { month: string; value: string }[] {
  return months.map((month) => ({ month, value: fromMinor(series.reduce((sum, one) => sum + inMain(one.balances.get(month) ?? '0.00', one.rate), 0n)) }))
}

export type Share = { id: string; name: string; amount: string }

/** The `count` biggest shares: those with the same id (a category in several accounts) are added together, nothing at 0 or below is kept, the largest comes first (the name decides a tie). */
export function topShares(shares: readonly Share[], count: number): Share[] {
  const total = new Map<string, { name: string; sum: bigint }>()
  for (const share of shares) {
    const was = total.get(share.id)
    total.set(share.id, { name: was?.name ?? share.name, sum: (was?.sum ?? 0n) + (toMinor(share.amount) ?? 0n) })
  }
  return [...total]
    .filter(([, one]) => one.sum > 0n)
    .sort(([, a], [, b]) => (a.sum === b.sum ? a.name.localeCompare(b.name) : a.sum > b.sum ? -1 : 1))
    .slice(0, Math.max(0, count))
    .map(([id, one]) => ({ id, name: one.name, amount: fromMinor(one.sum) }))
}

type Row = { id: string; paymentDate: string; myShare: string }

/** One transaction at most once: the same id in two lists (a filter that overlaps another) is one row. */
function distinct<T extends Pick<Row, 'id'>>(rows: readonly T[]): T[] {
  const seen = new Set<string>()
  return rows.filter((row) => !seen.has(row.id) && !!seen.add(row.id))
}

const newestFirst = (a: Row, b: Row) => (a.paymentDate === b.paymentDate ? Number(b.id) - Number(a.id) : a.paymentDate < b.paymentDate ? 1 : -1)

/** The `count` transactions that cost the most (what they cost the person, `myShare`); the newest first among equals. */
export function biggest<T extends Row>(rows: readonly T[], count: number): T[] {
  return distinct(rows)
    .sort((a, b) => {
      const [x, y] = [toMinor(a.myShare) ?? 0n, toMinor(b.myShare) ?? 0n]
      return x === y ? newestFirst(a, b) : x > y ? -1 : 1
    })
    .slice(0, Math.max(0, count))
}

/** The `count` newest transactions: by day, and for the same day the one made last. */
export function newest<T extends Row>(rows: readonly T[], count: number): T[] {
  return distinct(rows).sort(newestFirst).slice(0, Math.max(0, count))
}

/** The months of a sparkline: the `count` months that end with `last`, the first first, none after `today`. */
export function sparkMonths(last: string, today: string, count: number, shift: (month: string, delta: number) => string): string[] {
  const end = last < today ? last : today
  return Array.from({ length: count }, (_, i) => shift(end, i - count + 1))
}

/** The points of a sparkline in a box of `width` by `height` (the first at the left, the last at the right, the highest at the top), `pad` from the edge. A flat run is a line across the middle; one value is a point in the middle. */
export function sparkPoints(values: readonly number[], width: number, height: number, pad = 2): [number, number][] {
  if (values.length === 0) return []
  const low = Math.min(...values)
  const high = Math.max(...values)
  const room = { x: Math.max(0, width - 2 * pad), y: Math.max(0, height - 2 * pad) }
  return values.map((value, i) => [
    values.length === 1 ? width / 2 : pad + (i / (values.length - 1)) * room.x,
    high === low ? height / 2 : pad + (1 - (value - low) / (high - low)) * room.y,
  ])
}

/** When the loans are all paid at their installments: the latest month, "never" when one of them never is, null when no loan has a payoff to say. */
export type LatestPayoff = { month: string } | { never: true } | null

/** The loans still owing (not archived) with what they cost each month, in the app's currency, and when the last of them is paid off. */
export function loanFigures(accounts: readonly Account[]): { installment: string; payoff: LatestPayoff } {
  const owing = accounts.filter((account) => isLiability(account.kind) && !account.archived && account.liability?.state === 'OWING')
  let installment = 0n
  let payoff: LatestPayoff = null
  for (const account of owing) {
    installment += account.installment === null ? 0n : inMain(account.installment, account.rate)
    const one = account.liability?.payoff
    if (!one) continue
    if ('never' in one) payoff = { never: true }
    else if (!payoff || ('month' in payoff && one.month > payoff.month)) payoff = { month: one.month }
  }
  return { installment: fromMinor(installment), payoff }
}
