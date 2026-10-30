import type { MouseEvent } from 'react'

/**
 * For the `<ul>` of a table whose rows open (transactions, recurring items): a click anywhere on a row that has a chevron presses the
 * chevron. It does not when the click lands on a button, link, field or label, inside the open panel, or ends a text selection. The
 * keyboard and screen readers use the chevron itself.
 */
export function toggleRow(event: MouseEvent<HTMLElement>) {
  const target = event.target as Element
  if (target.closest('button, a, input, select, textarea, label, .row-panel') || window.getSelection()?.isCollapsed === false) return
  target.closest('li')?.querySelector<HTMLElement>(':scope > .cell-actions [aria-expanded]')?.click()
}
