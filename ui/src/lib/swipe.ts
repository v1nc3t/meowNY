export type Direction = 'next' | 'prev'

/** How far a finger has to travel sideways, in pixels, for it to count as a swipe. */
export const SWIPE_MIN = 60
/** A swipe that starts this close to the edge of the screen is the system's "back" gesture, not ours. */
export const EDGE = 24

/**
 * Which way a finger went: swiping left goes to the next section, right to the previous one. A short move, a move that is more
 * up or down than sideways (scrolling), and one that starts at the edge of the screen are not swipes.
 */
export function swipeDirection(dx: number, dy: number, startX: number, screenWidth: number): Direction | null {
  if (Math.abs(dx) < SWIPE_MIN || Math.abs(dx) < Math.abs(dy) * 1.5) return null
  if (startX < EDGE || startX > screenWidth - EDGE) return null
  return dx < 0 ? 'next' : 'prev'
}

/** The page next to the current one in the list of tabs, or null at the ends and on a page that is not one of the tabs. */
export function neighbour(tabs: readonly string[], pathname: string, direction: Direction): string | null {
  const at = tabs.findIndex((path) => pathname === path || pathname.startsWith(`${path}/`))
  if (at === -1) return null
  return tabs[at + (direction === 'next' ? 1 : -1)] ?? null
}

/** The item next to the current one in a list, or null at the ends. */
export function step<T>(items: readonly T[], current: T, direction: Direction): T | null {
  const at = items.indexOf(current)
  if (at === -1) return null
  return items[at + (direction === 'next' ? 1 : -1)] ?? null
}
