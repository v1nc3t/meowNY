import { useEffect, useLayoutEffect, useRef, type ReactNode } from 'react'
import { focusChosen, useListKeys } from '../hooks/useListKeys'

/**
 * The dropdown of the toolbars: a label, a button with the current choice, and a list that opens under it. It closes on a click
 * outside, on Escape, on Tab and when a choice is made (the caller closes it). The button opens it with Enter, Space or an arrow, and
 * focus moves into the list; any way of closing from the keyboard or by a choice puts focus back on the button.
 */
export default function Pick({
  label,
  className,
  text,
  title,
  open,
  onToggle,
  onClose,
  children,
}: {
  label: string
  className?: string
  text: string
  /** For a choice that may be cut short in a narrow field: the whole of it, as a tooltip. */
  title?: string
  open: boolean
  onToggle: () => void
  onClose: () => void
  children: ReactNode
}) {
  const ref = useRef<HTMLDivElement>(null)
  const buttonRef = useRef<HTMLButtonElement>(null)
  const listRef = useRef<HTMLUListElement>(null)
  const onListKey = useListKeys(() => {
    onClose()
    buttonRef.current?.focus()
  })

  useEffect(() => {
    if (!open) return
    function onPointerDown(event: PointerEvent) {
      if (!ref.current?.contains(event.target as Node)) onClose()
    }
    // Before the page and a dialog around it see Escape, as in Dropdown: only the list closes.
    function onKey(event: KeyboardEvent) {
      if (event.key !== 'Escape') return
      event.preventDefault()
      event.stopPropagation()
      onClose()
      buttonRef.current?.focus()
    }
    document.addEventListener('pointerdown', onPointerDown)
    document.addEventListener('keydown', onKey, true)
    return () => {
      document.removeEventListener('pointerdown', onPointerDown)
      document.removeEventListener('keydown', onKey, true)
    }
  }, [open, onClose])

  useLayoutEffect(() => {
    if (open && listRef.current) focusChosen(listRef.current)
  }, [open])

  return (
    <div className={className ? `pick ${className}` : 'pick'} ref={ref}>
      <span>{label}</span>
      <button
        ref={buttonRef}
        type="button"
        className="pick-button"
        title={title}
        aria-expanded={open}
        aria-haspopup="listbox"
        onClick={onToggle}
        onKeyDown={(event) => {
          if (open || (event.key !== 'ArrowDown' && event.key !== 'ArrowUp')) return
          event.preventDefault()
          onToggle()
        }}
      >
        <span className="pick-text">{text}</span>
      </button>
      {open ? (
        // A choice closes the list (the caller does that), and the click that made it hands focus back to the button.
        <ul className="pick-list" role="listbox" aria-label={label} ref={listRef} onKeyDown={onListKey} onClick={() => buttonRef.current?.focus()}>
          {children}
        </ul>
      ) : null}
    </div>
  )
}

export function PickOption({ selected, onClick, children }: { selected: boolean; onClick: () => void; children: string }) {
  return (
    <li role="presentation">
      <button type="button" role="option" aria-selected={selected} onClick={onClick}>
        {children}
      </button>
    </li>
  )
}
