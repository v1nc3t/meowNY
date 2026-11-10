import { useEffect, useRef } from 'react'
import { rememberFocus } from '../lib/focusKeeper'

/**
 * For a dialog: puts the focus back on the control that opened it, when the dialog closes. Call it above the effect that shows the dialog,
 * so it sees the focus before the dialog takes it.
 */
export function useReturnFocus(): void {
  const back = useRef<() => void>(undefined)
  useEffect(() => {
    // Once: the second run of an effect in development would find the focus in the dialog.
    back.current ??= rememberFocus()
    return back.current
  }, [])
}
