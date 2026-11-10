import { useRef, type KeyboardEvent } from 'react'

/** Where focus lands when a list opens: the chosen option, or the first. */
export function focusChosen(list: HTMLElement) {
  const first = list.querySelector<HTMLElement>('[aria-selected="true"]') ?? list.querySelector<HTMLElement>('button')
  first?.focus()
}

/**
 * The keys of an open option list, for its onKeyDown. The arrows, Page Up and Down, Home and End move between the options, and typing
 * the start of a label jumps to it. `leave` closes the list and puts focus back on the control that opened it: Tab calls it, so the
 * next Tab goes on from the control and not through the list, which can sit at the far end of the page.
 */
export function useListKeys(leave: () => void) {
  const typed = useRef({ text: '', timer: 0 })
  return (event: KeyboardEvent<HTMLElement>) => {
    if (event.ctrlKey || event.metaKey || event.altKey) return
    const items = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>('button'))
    const at = items.indexOf(document.activeElement as HTMLButtonElement)
    const moves: Record<string, number> = {
      ArrowDown: at + 1,
      ArrowUp: at - 1,
      PageDown: at + 10,
      PageUp: at - 10,
      Home: 0,
      End: items.length - 1,
    }
    const to = moves[event.key]
    if (to !== undefined) {
      event.preventDefault()
      items[Math.min(Math.max(to, 0), items.length - 1)]?.focus()
      return
    }
    if (event.key === 'Tab') {
      // Shift+Tab stops on the control; Tab carries on from it (the list is gone by the time the browser moves).
      if (event.shiftKey) event.preventDefault()
      leave()
      return
    }
    // Type to jump: letters typed quickly build a prefix, and the first matching option gets focus. A space is the key that chooses,
    // until it follows letters.
    if (event.key.length !== 1 || (event.key === ' ' && !typed.current.text)) return
    event.preventDefault()
    window.clearTimeout(typed.current.timer)
    typed.current.text += event.key.toLowerCase()
    typed.current.timer = window.setTimeout(() => (typed.current.text = ''), 700)
    const prefix = typed.current.text
    const from = prefix.length > 1 ? at : at + 1
    const ordered = [...items.slice(Math.max(from, 0)), ...items.slice(0, Math.max(from, 0))]
    ordered.find((item) => item.textContent?.trim().toLowerCase().startsWith(prefix))?.focus()
  }
}
