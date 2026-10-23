import { useEffect, useRef, useState } from 'react'

/** A start that is quicker than this shows no loading screen at all, so a fast start never flashes. */
export const SHOW_AFTER_MS = 150
/** Once the loading screen is up it stays at least this long, so it is never just a flicker. */
export const MIN_SHOW_MS = 600

/** Waits for the work and for the minimum time, whichever is later. A failure is passed on after the same wait. */
export async function atLeast<T>(work: Promise<T>, ms = MIN_SHOW_MS): Promise<T> {
  const timer = new Promise<void>((resolve) => setTimeout(resolve, ms))
  try {
    return await work
  } finally {
    await timer
  }
}

/**
 * For a check that may be quick or slow: `show` is true only when `active` has lasted longer than SHOW_AFTER_MS, and then stays
 * true for at least MIN_SHOW_MS. `blank` is true while the check runs but the loading screen is not up yet.
 */
export function useDecryptingGate(active: boolean): { show: boolean; blank: boolean } {
  const [show, setShow] = useState(false)
  const shownAt = useRef(0)

  useEffect(() => {
    if (active && !show) {
      const timer = setTimeout(() => {
        shownAt.current = Date.now()
        setShow(true)
      }, SHOW_AFTER_MS)
      return () => clearTimeout(timer)
    }
    if (!active && show) {
      const timer = setTimeout(() => setShow(false), Math.max(0, MIN_SHOW_MS - (Date.now() - shownAt.current)))
      return () => clearTimeout(timer)
    }
  }, [active, show])

  return { show, blank: active && !show }
}

/**
 * A form that is hidden (not removed) while the loading screen shows keeps what was typed. When it comes back after a failure,
 * the cursor goes back into its first field, where it was before the submit.
 */
export function useRefocus(busy: boolean) {
  const form = useRef<HTMLFormElement>(null)
  const wasBusy = useRef(false)
  useEffect(() => {
    if (wasBusy.current && !busy) form.current?.querySelector('input')?.focus()
    wasBusy.current = busy
  }, [busy])
  return form
}
