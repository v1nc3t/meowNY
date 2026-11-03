import { useId, useState, type KeyboardEvent } from 'react'

/** Which row of a list is open (one at a time), and the ids that name the panels. Hand the result to `Row` of the same list. */
export function useOpenRow() {
  const [openId, setOpenId] = useState<string | null>(null)
  const prefix = useId()
  return {
    isOpen: (id: string) => openId === id,
    toggle: (id: string) => setOpenId((current) => (current === id ? null : id)),
    /** The `id` of the row's panel, which the chevron names in `aria-controls`. */
    panelId: (id: string) => `${prefix}-${id}`,
  }
}

/**
 * Escape in an open row, on its chevron or in its panel, closes the panel and leaves focus on the chevron. A key that something in the
 * panel already used (a field, a list, a dialog) is left alone.
 */
export function closeOnEscape(event: KeyboardEvent<HTMLElement>, open: boolean, close: () => void): void {
  if (event.key !== 'Escape' || !open || event.defaultPrevented || (event.target as Element).closest('dialog')) return
  event.preventDefault()
  close()
  event.currentTarget.querySelector<HTMLElement>(':scope > .cell-actions [aria-expanded], :scope > [aria-expanded]')?.focus()
}

export type OpenRows = ReturnType<typeof useOpenRow>
