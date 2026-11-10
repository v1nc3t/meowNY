/** One slot of a pager: a page number, or 'gap' where pages are left out. */
export type PageItem = number | 'gap'

const run = (from: number, to: number) => Array.from({ length: to - from + 1 }, (_, index) => from + index)

/**
 * The page buttons to show for page `current` of `count`: the first and the last page, `around` pages on each side of the current
 * one, and a gap where pages are left out. A gap always stands for two pages or more, and near either end the run grows to fill the
 * slots, so the pager keeps the same width while you page through. All pages are shown when they fit.
 */
export function pageItems(current: number, count: number, around: number): PageItem[] {
  const slots = 2 * around + 5
  if (count <= slots) return run(1, count)
  if (current <= around + 3) return [...run(1, slots - 2), 'gap', count]
  if (current >= count - around - 2) return [1, 'gap', ...run(count - slots + 3, count)]
  return [1, 'gap', ...run(current - around, current + around), 'gap', count]
}
