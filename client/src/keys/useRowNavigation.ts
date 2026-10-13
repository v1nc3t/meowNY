import { useEffect } from 'react'
import { registerShortcut } from './registry'

/**
 * j and k move between the rows of a list. A row's first action (edit, rename) carries
 * `data-row-focus`; moving focuses it, and Enter then opens it like a click.
 */
export function useRowNavigation(): void {
  useEffect(() => {
    function move(step: 1 | -1) {
      const rows = Array.from(document.querySelectorAll<HTMLElement>('[data-row-focus]'))
      if (rows.length === 0) return
      const at = rows.indexOf(document.activeElement as HTMLElement)
      const next = at === -1 ? (step === 1 ? 0 : rows.length - 1) : Math.min(rows.length - 1, Math.max(0, at + step))
      rows[next].focus()
      rows[next].scrollIntoView({ block: 'nearest' })
    }
    const offs = [registerShortcut('rowNext', () => move(1)), registerShortcut('rowPrev', () => move(-1))]
    return () => offs.forEach((off) => off())
  }, [])
}
