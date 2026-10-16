import { formatPercent, toMinor } from '../lib/money.ts'

/** A round number at or above `value` (1, 2 or 5 times a power of ten), so axis lines land on even amounts. */
export function niceCeil(value: number): number {
  if (!(value > 0)) return 1
  const power = 10 ** Math.floor(Math.log10(value))
  const step = [1, 2, 5, 10].find((s) => s * power >= value) ?? 10
  return step * power
}

/** Percent of a budget used with one decimal, rounded half up like the Rust side. Null without a usable budget. */
export function usedPercent(actual: string, budget: string | null): number | null {
  const limit = toMinor(budget ?? '')
  const spent = toMinor(actual)
  if (limit === null || spent === null || limit <= 0n) return null
  return Number((spent * 2000n + limit) / (2n * limit)) / 10
}

/** How far `value` moved from `before`, in percent. Null when there is nothing to compare with. */
export function changePercent(value: string, before: string | undefined): number | null {
  const then = before === undefined ? NaN : Number(before)
  return then > 0 ? ((Number(value) - then) / then) * 100 : null
}

/** The savings rate as shown: "12.5%", "-8.3%" when more was spent than earned, and "0%" when there is no rate (no income). */
export function savingsRateText(rate: number | null, locale: string): string {
  // `|| 0` also turns -0 into 0, which would otherwise print as "-0%".
  return formatPercent(rate || 0, locale)
}
