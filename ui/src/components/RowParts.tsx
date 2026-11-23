import type { ComponentProps, ReactNode, Ref } from 'react'
import { Link } from 'react-router-dom'
import { closeOnEscape, type OpenRows } from '../hooks/useOpenRow'
import { useT } from '../i18n/context'
import { byDay, dayHeading } from '../lib/dayHeading'
import { toggleRow } from '../lib/toggleRow'
import Facts, { type FactItem } from './Facts'
import Icon from './NavIcon'
import Button from './ui/Button'
import { buttonClass, cn } from './ui/cn'

/**
 * The box of a table on the desktop and its head: one small grey label for each column, `{ label, className }` for one with a class of its
 * own (`is-end` for the amounts at the right), an empty string for the columns of buttons. The rows are the children. With `opens` a click anywhere on a row presses its
 * chevron (see `toggleRow`).
 */
export function Table({
  className,
  heads,
  opens = false,
  tidy = false,
  listRef,
  children,
}: {
  /** What names this list in the styles (`tx-list`): the columns are its `--cols`. */
  className: string
  heads: (string | { label: string; className: string })[]
  opens?: boolean
  /**
   * A list that follows docs/ui-design.md: every cell is one line (a name is cut with an ellipsis, an amount never breaks), and a narrow
   * list has fewer columns instead of rows of two lines. The list's own styles say which columns go, and when.
   */
  tidy?: boolean
  /** The list's own element, for what listens on it (the `listRef` of `useReorder`). */
  listRef?: Ref<HTMLUListElement>
  children: ReactNode
}) {
  return (
    <ul ref={listRef} className={cn('card-list is-table', tidy && 'is-tidy', className)} onClick={opens ? toggleRow : undefined}>
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
 * The phone's list by day: a flat list (`ListGroup`) with the day as its heading, for each day. `children` draws one row (a `TapRow`);
 * `today` is a date, for the heading to leave out the year of this one.
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
  /** Added to the list of each day. */
  className?: string
  children: (row: T) => ReactNode
}) {
  return byDay(rows, dateOf).map((day) => (
    <ListGroup key={day.date} heading={dayHeading(day.date, locale, today.slice(0, 4))} className={className}>
      {day.rows.map((row) => children(row))}
    </ListGroup>
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
 * A flat list on the phone (docs/ui-design.md, section 5): no box round it, a hairline between the rows, and over them a small heading line:
 * the group's name at the left and what its rows add up to (`total`) at the right. `heading` and `total` may each be left out; with neither
 * there is no heading line. The rows (`TapRow`) are the children.
 */
export function ListGroup({
  heading,
  total,
  className,
  children,
}: {
  heading?: string
  /** What the group's rows hold together, at the right of the heading line. */
  total?: ReactNode
  /** Added to the list. */
  className?: string
  children: ReactNode
}) {
  return (
    <section className="list-group">
      {heading || total ? (
        <h2 className="list-group-head">
          <span className="list-group-name">{heading}</span>
          {total ? <span className="list-group-total">{total}</span> : null}
        </h2>
      ) : null}
      <ul className={cn('card-list is-flat', className)}>{children}</ul>
    </section>
  )
}

/**
 * A name with one mark after it (an icon, a short badge): the name is cut with an ellipsis when it is too long, the mark always stays whole.
 * With no mark it is the name alone.
 */
export function NameMark({ children, mark }: { children: ReactNode; mark?: ReactNode }) {
  return mark ? (
    <span className="name-mark">
      <span className="name-text">{children}</span>
      {mark}
    </span>
  ) : (
    children
  )
}

/** One button of the open panel of a phone row (see `PanelActions`): a button that runs `onClick`, or a link to `to`. */
export type PanelAction = { label: string; icon: string; onClick?: () => void; to?: string; danger?: boolean }

/**
 * The buttons of an open phone row, in one row, under its facts: an icon with a short word each; with more than three, only the icons (each
 * keeps its label for screen readers). Every action the row has is here, so nothing is out of reach on a phone. A `false` or `null` in the
 * list is left out (`canEdit && { … }`).
 */
export function PanelActions({ actions: given }: { actions: (PanelAction | false | null | undefined)[] }) {
  const actions = given.filter((action) => !!action)
  const words = actions.length <= 3
  return (
    <div className={cn('row-panel-actions', !words && 'is-icons')}>
      {actions.map((action) => {
        const content = (
          <>
            <Icon name={action.icon} className="btn-icon" />
            {words ? action.label : null}
          </>
        )
        const className = cn(action.to ? buttonClass('outline', words ? 'default' : 'icon') : undefined, action.danger && 'row-danger')
        return action.to ? (
          <Link key={action.label} to={action.to} className={className} aria-label={words ? undefined : action.label}>
            {content}
          </Link>
        ) : (
          <Button key={action.label} variant="outline" size={words ? 'default' : 'icon'} className={cn(action.danger && 'row-danger')} aria-label={words ? undefined : action.label} onClick={action.onClick}>
            {content}
          </Button>
        )
      })}
    </div>
  )
}

/**
 * A row of a list on the phone: the name and the amount on the first line and a chevron on the right, and under the name, at most, one
 * short grey value (`meta`). Tapping it opens its panel: `facts` (label and value pairs), then `children` (the buttons: see
 * `PanelActions`). A row with no `facts` and no `children` has nothing to open: it is plain, and the room of its chevron is left empty,
 * so its amount ends where the others do.
 */
export function TapRow({
  id,
  item,
  open = false,
  onToggle,
  name,
  amount,
  action,
  meta,
  facts,
  sub,
  className,
  children,
  ...rest
}: {
  /** Names the panel, so the button can say what it opens. */
  id: string
  /** The `data-item` that jump-to finds the row by. */
  item?: string
  open?: boolean
  onToggle?: () => void
  /** One line, cut with an ellipsis; at most one mark after it (an icon, a short badge). */
  name: ReactNode
  /** Right aligned, in tabular figures, as wide as the amounts of the other rows: they end in a line. */
  amount?: ReactNode
  /** A button at the end of the first line, where the amount would be: only for a row with nothing to open (no `facts`, no `children`), as a button cannot sit in the one that opens. */
  action?: ReactNode
  /** The one fact you scan for in this list, in small grey under the name (a category, a next due date, an account's type). One short value, never several joined by dots. */
  meta?: ReactNode
  /** What the open row shows first: label and value pairs, then `children`. */
  facts?: FactItem[]
  /** @deprecated use meta + facts (docs/ui-design.md §5) */
  sub?: ReactNode
  /** The buttons of the open row, after its facts. */
  children?: ReactNode
} & Omit<ComponentProps<'li'>, 'children' | 'id' | 'onToggle'>) {
  const details = Boolean(children) || (facts !== undefined && facts.length > 0)
  const inside = (
    <>
      <span className="cell-name">
        <span className="card-name">{name}</span>
      </span>
      {amount}
      {meta ? <span className="cell-meta">{meta}</span> : null}
      {sub ? <span className="cell-sub">{sub}</span> : null}
    </>
  )
  return (
    <li
      className={cn('card-item tap-row', open && 'is-open', className)}
      data-item={item}
      onKeyDown={onToggle ? (event) => closeOnEscape(event, open, onToggle) : undefined}
      {...rest}
    >
      {details ? (
        <button type="button" className="row-toggle" aria-expanded={open} aria-controls={open ? id : undefined} onClick={onToggle}>
          {inside}
          <Icon name="chevron-down" className="chevron" />
        </button>
      ) : (
        <div className={action ? 'row-toggle is-plain has-action' : 'row-toggle is-plain'}>
          {inside}
          {action ?? <span className="chevron" />}
        </div>
      )}
      {open && details ? (
        <div className="row-panel tx-detail" id={id}>
          {facts ? <Facts items={facts} /> : null}
          {children}
        </div>
      ) : null}
    </li>
  )
}
