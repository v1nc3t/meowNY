import { useEffect, useRef, type ReactNode } from 'react'

/**
 * The dropdown of the toolbars: a label, a button with the current choice, and a list that opens under it. It closes on a click
 * outside, on Escape and when a choice is made (the caller closes it).
 */
export default function Pick({
  label,
  inline = false,
  className,
  text,
  title,
  open,
  onToggle,
  onClose,
  children,
}: {
  label: string
  /** Beside other buttons: the label is for screen readers only. */
  inline?: boolean
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

  useEffect(() => {
    if (!open) return
    function onPointerDown(event: PointerEvent) {
      if (!ref.current?.contains(event.target as Node)) onClose()
    }
    function onKey(event: KeyboardEvent) {
      if (event.key === 'Escape') onClose()
    }
    document.addEventListener('pointerdown', onPointerDown)
    document.addEventListener('keydown', onKey)
    return () => {
      document.removeEventListener('pointerdown', onPointerDown)
      document.removeEventListener('keydown', onKey)
    }
  }, [open, onClose])

  return (
    <div className={className ? `pick ${className}` : 'pick'} ref={ref}>
      <span className={inline ? 'sr-only' : undefined}>{label}</span>
      <button type="button" className="pick-button" title={title} aria-expanded={open} aria-haspopup="listbox" onClick={onToggle}>
        <span className="pick-text">{text}</span>
      </button>
      {open ? (
        <ul className="pick-list" role="listbox">
          {children}
        </ul>
      ) : null}
    </div>
  )
}

export function PickOption({ selected, onClick, children }: { selected: boolean; onClick: () => void; children: string }) {
  return (
    <li>
      <button type="button" role="option" aria-selected={selected} onClick={onClick}>
        {children}
      </button>
    </li>
  )
}
