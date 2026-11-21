/** How long a row has to be held, in milliseconds, before it can be dragged. */
export const HOLD_MS = 400
/** How far the pointer may wander during the hold, in pixels. More than that is a scroll or a swipe, not a hold. */
export const HOLD_SLOP = 8
/** Within this many pixels of the edge of the list, a drag scrolls the list. */
export const SCROLL_EDGE = 48
/** The most the list scrolls in one frame, in pixels. */
export const SCROLL_MAX = 14

/** A copy of `items` with the one at `from` taken out and put back at `to`. */
export function moveItem<T>(items: readonly T[], from: number, to: number): T[] {
  const copy = [...items]
  const [item] = copy.splice(from, 1)
  copy.splice(to, 0, item)
  return copy
}

/** The ids of the rows that can be moved (the others are read-only), in order. */
export function activeIds(rows: readonly { id: string; deleted: boolean }[]): string[] {
  return rows.filter((row) => !row.deleted).map((row) => row.id)
}

/**
 * Where a dragged row lands among the movable rows, from the middle of each of them as they were before the drag
 * (`mids`, top to bottom, the dragged one included at `from`) and the middle of the dragged row now (`y`).
 * It goes past a neighbour once its middle is past the neighbour's middle.
 */
export function targetIndex(mids: readonly number[], from: number, y: number): number {
  return mids.filter((mid, at) => at !== from && mid < y).length
}

/** Puts the rows named by `ids` into the places those rows took, in the new order. Rows that are not named stay where they are. */
export function applyOrder<T extends { id: string }>(rows: readonly T[], ids: readonly string[]): T[] {
  const named = new Set(ids)
  const byId = new Map(rows.map((row) => [row.id, row]))
  let next = 0
  return rows.map((row) => (named.has(row.id) ? (byId.get(ids[next++]) ?? row) : row))
}

/**
 * How far each row has to move, in pixels, to show a new order. `heights` are in the old order, `order` lists the old
 * positions in the new order (the identity is no move).
 */
export function shifts(heights: readonly number[], order: readonly number[]): number[] {
  const oldTop: number[] = []
  heights.reduce((top, height, at) => ((oldTop[at] = top), top + height), 0)
  const moves: number[] = heights.map(() => 0)
  order.reduce((top, was) => ((moves[was] = top - oldTop[was]), top + heights[was]), 0)
  return moves
}

/** How far to scroll a list in one frame while a row is dragged at `y`: up (negative) near the top, down near the bottom, else 0. */
export function scrollStep(y: number, top: number, bottom: number): number {
  if (y < top + SCROLL_EDGE) return -Math.round(SCROLL_MAX * Math.min(1, (top + SCROLL_EDGE - y) / SCROLL_EDGE))
  if (y > bottom - SCROLL_EDGE) return Math.round(SCROLL_MAX * Math.min(1, (y - (bottom - SCROLL_EDGE)) / SCROLL_EDGE))
  return 0
}
