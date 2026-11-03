import type { ComponentProps, ReactNode } from 'react'
import { useT } from '../i18n/context'
import Icon from './NavIcon'
import Button from './ui/Button'

/** An icon-only button at the end of a table row (edit, delete, preview...): grey until the pointer is on it. Its name is the label and the title. */
export function IconAction({
  icon,
  label,
  danger = false,
  ...rest
}: Omit<ComponentProps<'button'>, 'children' | 'title' | 'aria-label'> & { icon: string; label: string; danger?: boolean }) {
  return (
    <Button variant="ghost" size="icon" className={danger ? 'icon-btn is-danger' : 'icon-btn'} aria-label={label} title={label} {...rest}>
      <Icon name={icon} className="btn-icon" />
    </Button>
  )
}

/** The chevron that opens a row's panel; where a row has nothing to open it is an empty slot, so the buttons of all rows line up. */
export function Disclosure({ more = true, open, controls, onClick }: { more?: boolean; open: boolean; controls: string; onClick: () => void }) {
  const t = useT()
  if (!more) return <span className="icon-slot" />
  return (
    <Button
      variant="ghost"
      size="icon"
      className="icon-btn"
      aria-expanded={open}
      aria-controls={open ? controls : undefined}
      aria-label={t('common.details')}
      title={t('common.details')}
      onClick={onClick}
    >
      <Icon name="chevron-down" className="btn-icon chevron" />
    </Button>
  )
}

/** The note of a line of the debts, in the panel of its row: a "description", as in the transactions. */
export function NoteSection({ note }: { note: string }) {
  const t = useT()
  return (
    <section className="detail-section">
      <p className="detail-title">{t('common.description')}</p>
      <p className="detail-text">{note}</p>
    </section>
  )
}

/**
 * A row of a list on the phone: the name and the amount on the first line, what else under them, a chevron on the right. Tapping it
 * opens `children` under it (the details and a labelled button for everything that can be done). A row with no `children` has nothing
 * to open: it is plain, with no chevron.
 */
export function TapRow({
  id,
  item,
  open = false,
  onToggle,
  name,
  amount,
  sub,
  children,
  ...rest
}: {
  /** Names the panel, so the button can say what it opens. */
  id: string
  /** The `data-item` that jump-to finds the row by. */
  item?: string
  open?: boolean
  onToggle?: () => void
  name: ReactNode
  amount?: ReactNode
  sub?: ReactNode
  children?: ReactNode
} & Omit<ComponentProps<'li'>, 'children' | 'id' | 'onToggle'>) {
  const inside = (
    <>
      <span className="cell-name">
        <span className="card-name">{name}</span>
      </span>
      {amount}
      {sub ? <span className="cell-sub">{sub}</span> : null}
    </>
  )
  return (
    <li className={open ? 'card-item tap-row is-open' : 'card-item tap-row'} data-item={item} {...rest}>
      {children ? (
        <button type="button" className="row-toggle" aria-expanded={open} aria-controls={open ? id : undefined} onClick={onToggle}>
          {inside}
          <Icon name="chevron-down" className="chevron" />
        </button>
      ) : (
        <div className="row-toggle is-plain">{inside}</div>
      )}
      {open && children ? (
        <div className="row-panel tx-detail" id={id}>
          {children}
        </div>
      ) : null}
    </li>
  )
}
