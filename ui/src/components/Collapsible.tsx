import { useId, useState, type ReactNode } from 'react'

/**
 * A titled section that opens and closes, for a small screen where everything on one page is too much. The content stays
 * on the page while closed (only hidden), so a half-edited field is not lost by closing it.
 */
export default function Collapsible({ title, defaultOpen = false, children }: { title: string; defaultOpen?: boolean; children: ReactNode }) {
  const [open, setOpen] = useState(defaultOpen)
  const id = useId()
  return (
    <div className="collapse wide" data-open={open}>
      <h2>
        <button type="button" className="collapse-toggle" aria-expanded={open} aria-controls={id} onClick={() => setOpen((current) => !current)}>
          <span>{title}</span>
          <span className="collapse-mark" aria-hidden="true">
            {open ? '−' : '+'}
          </span>
        </button>
      </h2>
      <div id={id} className="collapse-body" hidden={!open}>
        {children}
      </div>
    </div>
  )
}
