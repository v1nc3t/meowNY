/**
 * The item that a key moves to in a menu of `count` items, as an index, from item `at` (-1 when none has the focus). The arrows go round:
 * down from the last item is the first, up from the first is the last (up with none focused is the last). Home and End go to the ends.
 * Null for a key that does not move, and for a menu with no items.
 */
export function menuTarget(key: string, at: number, count: number): number | null {
  if (count <= 0) return null
  switch (key) {
    case 'ArrowDown':
      return at + 1 >= count ? 0 : at + 1
    case 'ArrowUp':
      return at <= 0 ? count - 1 : at - 1
    case 'Home':
      return 0
    case 'End':
      return count - 1
    default:
      return null
  }
}
