import { createContext, useContext, useEffect, useRef } from 'react'
import type { Direction } from '../lib/swipe'

/** A page's answer to a swipe: true when it used the swipe itself (it moved to another tab of its own), false to let the app go on. */
export type SwipeHandler = (direction: Direction) => boolean

/** Lets a page take a swipe before the app uses it to go to the next section. */
export const SwipeContext = createContext<{ register: (handler: SwipeHandler) => () => void }>({ register: () => () => undefined })

/** The page answers swipes for as long as it is on screen and `enabled`. */
export function useSwipeHandler(handler: SwipeHandler, enabled = true): void {
  const { register } = useContext(SwipeContext)
  const latest = useRef(handler)
  useEffect(() => {
    latest.current = handler
  })
  useEffect(() => {
    if (!enabled) return
    return register((direction) => latest.current(direction))
  }, [register, enabled])
}

/**
 * True when a touch that starts on `target` is not meant as a swipe between sections: it is on a field, in a dialog or a list that
 * is open, or in something that scrolls sideways (a wide table), where the finger is moving the content.
 */
export function blocksSwipe(target: Element | null): boolean {
  for (let el = target; el; el = el.parentElement) {
    const tag = el.tagName
    if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || tag === 'DIALOG') return true
    if (el instanceof HTMLElement && el.isContentEditable) return true
    if (el.classList.contains('pick-list')) return true
    if (el.scrollWidth > el.clientWidth + 1) {
      const overflow = getComputedStyle(el).overflowX
      if (overflow === 'auto' || overflow === 'scroll') return true
    }
  }
  return false
}
