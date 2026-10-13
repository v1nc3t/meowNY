import { useCallback, useEffect, useState } from 'react'

/** How long a notice stays on screen before it hides by itself. */
export const NOTICE_SECONDS = 6

/**
 * Message state that clears itself after a few seconds. It works like `useState('')`:
 * set a message to show it, set '' to hide it now. Setting the same text again shows it again
 * and restarts the timer.
 */
export function useNotice(seconds = NOTICE_SECONDS): [string, (text: string) => void] {
  const [notice, setNotice] = useState({ text: '', serial: 0 })

  useEffect(() => {
    if (!notice.text) return
    const timer = setTimeout(() => {
      // Only clear the message this timer was started for, never a newer one.
      setNotice((current) => (current.serial === notice.serial ? { text: '', serial: current.serial } : current))
    }, seconds * 1000)
    return () => clearTimeout(timer)
  }, [notice, seconds])

  const show = useCallback((text: string) => setNotice((current) => ({ text, serial: current.serial + 1 })), [])
  return [notice.text, show]
}
