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

/** The ring of the spending donut, in the 100 by 100 box it is drawn in. */
export const DONUT = { center: 50, outer: 47, inner: 33 }

/** A point on the ring: `fraction` of the way round, clockwise from the top. */
export function ringPoint(fraction: number, radius: number): [number, number] {
  const angle = fraction * 2 * Math.PI - Math.PI / 2
  return [DONUT.center + radius * Math.cos(angle), DONUT.center + radius * Math.sin(angle)]
}

/**
 * The outline of the slice of the ring from `from` to `to` (fractions of the way round, clockwise from the top): an arc along the outside,
 * a straight edge in towards the middle, an arc back along the inside and a straight edge out again. The two edges point at the centre
 * however thin the slice is. The whole ring is two circles.
 */
export function slicePath(from: number, to: number): string {
  const n = (value: number) => value.toFixed(3)
  const { center: c, outer, inner } = DONUT
  if (to - from >= 0.9999) {
    return `M${n(c)} ${n(c - outer)}A${outer} ${outer} 0 1 1 ${n(c)} ${n(c + outer)}A${outer} ${outer} 0 1 1 ${n(c)} ${n(c - outer)}ZM${n(c)} ${n(c - inner)}A${inner} ${inner} 0 1 0 ${n(c)} ${n(c + inner)}A${inner} ${inner} 0 1 0 ${n(c)} ${n(c - inner)}Z`
  }
  const [x1, y1] = ringPoint(from, outer)
  const [x2, y2] = ringPoint(to, outer)
  const [x3, y3] = ringPoint(to, inner)
  const [x4, y4] = ringPoint(from, inner)
  const large = to - from > 0.5 ? 1 : 0
  return `M${n(x1)} ${n(y1)}A${outer} ${outer} 0 ${large} 1 ${n(x2)} ${n(y2)}L${n(x3)} ${n(y3)}A${inner} ${inner} 0 ${large} 0 ${n(x4)} ${n(y4)}Z`
}
