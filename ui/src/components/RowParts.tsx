import type { ComponentProps, ReactNode } from 'react'
import { closeOnEscape, type OpenRows } from '../hooks/useOpenRow'
import { useT } from '../i18n/context'
import { byDay, dayHeading } from '../lib/dayHeading'
import { toggleRow } from '../lib/toggleRow'
import Icon from './NavIcon'
import Button from './ui/Button'
import { cn } from './ui/cn'

/**
 * The box of a table on the desktop and its head: one small grey label for each column, `{ label, className }` for one with a class of its
 * own (`is-end` for the amounts at the right), an empty string for the columns of buttons. The rows are the children. With `opens` a click anywhere on a row presses its
 * chevron (see `toggleRow`).
 */
export function Table({
  className,
  heads,
  opens = false,
  children,
}: {
  /** What names this list in the styles (`tx-list`): the columns are its `--cols`. */
  className: string
  heads: (string | { label: string; className: string })[]
  opens?: boolean
  children: ReactNode
}) {
  return (
    <ul className={`card-list is-table ${className}`} onClick={opens ? toggleRow : undefined}>
      <li className="list-head" aria-hidden="true">
        {heads.map((head, column) =>
          typeof head === 'string' ? (
            <span key={column}>{head}</span>
          ) : (
            <span key={column} className={head.className}>
              {head.label}
            </span>
          ),
        )}
      </li>
      {children}
    </ul>
  )
}

/**
 * A row of a `Table`: the cells (`children`), the icon buttons (`actions`), then the chevron that opens `panel` under the row. A row with
 * `panel={null}` has nothing to open: an empty slot where the chevron would be keeps its buttons in line with the others. A list whose rows
 * never open leaves out `rows` and `id`, and has no chevron or slot at all.
 */
export function Row({
  rows,
  id,
  item,
  actions,
  panel = null,
  note,
  className,
  children,
  ...rest
}: {
  /** The list's `useOpenRow()`, and this row's name in it. */
  rows?: OpenRows
  id?: string
  /** The `data-item` that jump-to finds the row by. */
  item?: string
  actions: ReactNode
  panel?: ReactNode
  /** A line across the row, under its cells and buttons. */
  note?: ReactNode
} & Omit<ComponentProps<'li'>, 'id'>) {
  const more = Boolean(panel)
  const opens = rows && id !== undefined ? { rows, id, panelId: rows.panelId(id) } : null
  const open = more && opens !== null && opens.rows.isOpen(opens.id)
  return (
    <li
      className={cn('card-item', className, open && 'is-open')}
      data-item={item}
      onKeyDown={opens ? (event) => closeOnEscape(event, open, () => opens.rows.toggle(opens.id)) : undefined}
      {...rest}
    >
      {children}
      <div className="cell-actions">
        {actions}
        {opens ? <Disclosure more={more} open={open} controls={opens.panelId} onClick={() => opens.rows.toggle(opens.id)} /> : null}
      </div>
      {note ? <p className="cell-note">{note}</p> : null}
      {open ? (
        <div className="row-panel tx-detail" id={opens?.panelId}>
          {panel}
        </div>
      ) : null}
    </li>
  )
}

/**
 * The phone's list by day: a small heading over the rows of each day. `children` draws one row (a `TapRow`); `today` is a date, for the
 * heading to leave out the year of this one.
 */
export function DayGroups<T>({
  rows,
  dateOf,
  locale,
  today,
  className,
  children,
}: {
  rows: readonly T[]
  dateOf: (row: T) => string
  locale: string
  today: string
  /** Added to the `card-list` of each day. */
  className?: string
  children: (row: T) => ReactNode
}) {
  return byDay(rows, dateOf).map((day) => (
    <div key={day.date} className="list-day">
      <h2>{dayHeading(day.date, locale, today.slice(0, 4))}</h2>
      <ul className={cn('card-list', className)}>{day.rows.map((row) => children(row))}</ul>
    </div>
  ))
}

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
  action,
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
  /** A button at the end of the first line, where the amount would be: only for a row with nothing to open (no `children`), as a button cannot sit in the one that opens. */
  action?: ReactNode
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
    <li
      className={open ? 'card-item tap-row is-open' : 'card-item tap-row'}
      data-item={item}
      onKeyDown={onToggle ? (event) => closeOnEscape(event, open, onToggle) : undefined}
      {...rest}
    >
      {children ? (
        <button type="button" className="row-toggle" aria-expanded={open} aria-controls={open ? id : undefined} onClick={onToggle}>
          {inside}
          <Icon name="chevron-down" className="chevron" />
        </button>
      ) : (
        <div className="row-toggle is-plain">
          {inside}
          {action}
        </div>
      )}
      {open && children ? (
        <div className="row-panel tx-detail" id={id}>
          {children}
        </div>
      ) : null}
    </li>
  )
}
