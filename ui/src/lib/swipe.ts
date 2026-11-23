export type Direction = 'next' | 'prev'

/** How far a finger has to travel sideways, in pixels, for it to count as a swipe. */
export const SWIPE_MIN = 60
/** A swipe that starts this close to the edge of the screen is not one of the pages' own swipes: at the left edge it opens the menu. */
export const EDGE = 24

/** A move far enough sideways, and more sideways than up or down (not a scroll). */
function sideways(dx: number, dy: number): boolean {
  return Math.abs(dx) >= SWIPE_MIN && Math.abs(dx) >= Math.abs(dy) * 1.5
}

/**
 * Which way a finger went: swiping left goes to the next section, right to the previous one. A short move, a move that is more
 * up or down than sideways (scrolling), and one that starts at the edge of the screen are not swipes.
 */
export function swipeDirection(dx: number, dy: number, startX: number, screenWidth: number): Direction | null {
  if (!sideways(dx, dy)) return null
  if (startX < EDGE || startX > screenWidth - EDGE) return null
  return dx < 0 ? 'next' : 'prev'
}

/** Whether a move opens the menu: a swipe to the right that starts in the first `EDGE` pixels at the left of the screen. */
export function opensMenu(dx: number, dy: number, startX: number): boolean {
  return dx > 0 && sideways(dx, dy) && startX < EDGE
}

/** Whether a move closes the menu, which is open: a swipe to the left, from anywhere in it. */
export function closesMenu(dx: number, dy: number): boolean {
  return dx < 0 && sideways(dx, dy)
}

/** The item next to the current one in a list, or null at the ends. */
export function step<T>(items: readonly T[], current: T, direction: Direction): T | null {
  const at = items.indexOf(current)
  if (at === -1) return null
  return items[at + (direction === 'next' ? 1 : -1)] ?? null
}
