import { useId, useState } from 'react'

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

export type OpenRows = ReturnType<typeof useOpenRow>
