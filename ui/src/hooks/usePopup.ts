import { useCallback, useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent } from 'react'
import { placePopup } from '../lib/placeList'

/**
 * What a button that opens a small popup under it needs (the menu of `MoreMenu`, the popover of `HelpButton`): whether it is open, the
 * refs of the button and the popup, the popup's place, and the keys of the popup. Escape closes it and gives the focus back to the button;
 * Tab closes it too, and goes on from the button. A press outside closes it. `host` is where to draw the popup: the dialog the button is
 * in, or the body (a modal dialog leaves everything outside it inert).
 */
export function usePopup(align: 'start' | 'end') {
  const [host, setHost] = useState<HTMLElement | null>(null)
  const buttonRef = useRef<HTMLButtonElement>(null)
  const popupRef = useRef<HTMLDivElement>(null)
  const open = host !== null

  const openPopup = useCallback(() => setHost(buttonRef.current?.closest('dialog') ?? document.body), [])
  const close = useCallback(() => {
    setHost(null)
    buttonRef.current?.focus()
  }, [])

  useEffect(() => {
    if (!open) return
    function onPointerDown(event: PointerEvent) {
      const target = event.target as Node
      if (buttonRef.current?.contains(target) || popupRef.current?.contains(target)) return
      setHost(null)
    }
    const away = () => setHost(null)
    document.addEventListener('pointerdown', onPointerDown)
    window.addEventListener('resize', away)
    return () => {
      document.removeEventListener('pointerdown', onPointerDown)
      window.removeEventListener('resize', away)
    }
  }, [open])

  useLayoutEffect(() => {
    if (host && popupRef.current && buttonRef.current) placePopup(popupRef.current, buttonRef.current, align)
  }, [host, align])

  /** For the popup's onKeyDown: Escape and Tab close it. */
  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === 'Escape') {
      event.preventDefault()
      // not the dialog's own Escape: it would close the dialog the popup is in
      event.stopPropagation()
      close()
    } else if (event.key === 'Tab') {
      // Shift+Tab stops on the button; Tab carries on from it (the popup is gone by the time the browser moves).
      if (event.shiftKey) event.preventDefault()
      close()
    }
  }

  return { open, host, openPopup, close, buttonRef, popupRef, onKeyDown }
}
