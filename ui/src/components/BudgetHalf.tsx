import { useEffect, useRef, useState, type FormEvent, type ReactNode } from 'react'
import type { Overview } from '../api/budgets'
import type { Category, CategoryInMonth } from '../api/categories'
import { amountsFor, budgetGap, earnedClass, spentClass, spentFor, type BudgetGap, type Side } from '../lib/categoriesView'
import { cn } from './ui/cn'
import Icon from './NavIcon'
import { MeterBar } from './StashParts'
import { IconAction, PanelActions, Row, TapRow } from './RowParts'
import Button from './ui/Button'
import { CardTitle } from './ui/Card'
import { EmptyState } from './ui/Pixel'
import { useOpenRow } from '../hooks/useOpenRow'
import { useProfile } from '../hooks/profile'
import { useReorder } from '../hooks/useReorder'
import { useT } from '../i18n/context'

/**
 * One side of the budgets, expenses or income: a list of its categories (budget, what was spent or earned, what is left, how much is used) under
 * a row for the side as a whole (the monthly budget, which can be changed there). On a phone the whole is a strip of three values over the list, and
 * a category is a row that opens to its numbers and its buttons; the grips to reorder are only there while `reordering` is on (the "more" menu).
 * The categories page loads both sides with `useBudgets`.
 */
export default function BudgetHalf({
  hideTitle = false,
  reordering = false,
  type,
  month,
  overview,
  categories,
  money,
  onGlobal,
  onEdit,
  onDelete,
  onReorder,
  onAdd,
}: {
  /** The title is already shown by a section around this one. */
  hideTitle?: boolean
  /** A phone shows the grips, and lets the rows be dragged, only while this is on. */
  reordering?: boolean
  type: Side
  month: string
  /** The overview of the month, both sides. */
  overview: Overview[] | null
  /** The categories of this side. */
  categories: CategoryInMonth[] | null
  money: (amount: string) => string
  onGlobal: (type: Side, raw: string) => Promise<string | null>
  onEdit: (category: Category) => void
  onDelete: (category: Category) => void
  onReorder: (ids: string[]) => void
  onAdd: () => void
}) {
  const t = useT()
  const phone = useProfile() === 'phone'
  const rows = useOpenRow()
  const { listRef, hintId, live } = useReorder(categories, onReorder)
  const side = overview?.find((row) => row.type === type) ?? null
  const amounts = amountsFor(overview, type)
  const spent = spentFor(overview, type)
  const budgeted = side?.budgeted ?? ''
  const spentWord = t(type === 'EXPENSE' ? 'budgets.colSpent' : 'budgets.colEarned')
  const leftWord = t(type === 'EXPENSE' ? 'budgets.colLeft' : 'budgets.colToGo')
  const ready = categories !== null && overview !== null
  const some = categories !== null && categories.length > 0
  const summary = (
    <BudgetSummary
      key={`${type}:${month}:${budgeted}`}
      phone={phone}
      type={type}
      value={budgeted}
      used={side?.used ?? null}
      spentWord={spentWord}
      leftWord={leftWord}
      money={money}
      onSave={(raw) => onGlobal(type, raw)}
    />
  )
  const rowProps = (category: CategoryInMonth) => ({
    category,
    type,
    value: amounts.get(category.id) ?? '',
    spent: spent.get(category.id) ?? '0.00',
    money,
    onEdit,
    onDelete,
  })
  return (
    <section className="budget-half" data-side={type}>
      {hideTitle ? null : <CardTitle>{t(type === 'EXPENSE' ? 'budgets.expenses' : 'budgets.income')}</CardTitle>}
      {phone && ready ? summary : null}
      {!phone && ready ? (
        // one list for the side's whole and its categories, so the amounts line up; the whole has no `data-reorder-id`, so it stays where it is
        <ul className="card-list is-tidy cat-list" key={month} ref={listRef}>
          {some ? (
            <li className="list-head" aria-hidden="true">
              <span>{t('common.category')}</span>
              <span className="is-end cat-budget">{t('budgets.colBudget')}</span>
              <span className="is-end cat-spent">{spentWord}</span>
              <span className="is-end cat-gap">{leftWord}</span>
              <span className="cat-used">{t('budgets.colUsed')}</span>
              <span />
            </li>
          ) : null}
          {summary}
          {categories?.map((category) => <CategoryRow key={category.id} {...rowProps(category)} hintId={hintId} />)}
        </ul>
      ) : null}
      {phone && some ? (
        <section className="list-group">
          <h2 className="list-group-head">
            <span className="list-group-name">{t('common.category')}</span>
            <span className="list-group-total">{leftWord}</span>
          </h2>
          <ul className="card-list is-flat cat-flat" key={month} ref={listRef}>
            {categories.map((category) => (
              <CategoryTapRow key={category.id} {...rowProps(category)} id={rows.panelId(category.id)} open={rows.isOpen(category.id)} onToggle={() => rows.toggle(category.id)} reordering={reordering} />
            ))}
          </ul>
        </section>
      ) : null}
      {categories && categories.length === 0 ? <EmptyState text={t('common.noCategories')} action={{ label: t('categories.add'), onClick: onAdd }} /> : null}
      <span id={hintId} className="sr-only">
        {t('categories.reorderHint')}
      </span>
      <div className="sr-only" aria-live="polite">
        {live}
      </div>
    </section>
  )
}

/**
 * The side as a whole: the monthly budget, what was spent (or earned) of it, what is left and how much is used. Its budget is plain text until
 * "edit" is pressed; saving an empty field removes the budget. On the desktop it is the first row of the list, in the columns of the categories
 * under it; on a phone it is a strip of three values with the bar of how much is used under it.
 */
function BudgetSummary({
  phone,
  type,
  value,
  used,
  spentWord,
  leftWord,
  money,
  onSave,
}: {
  phone: boolean
  type: Side
  value: string
  /** What was spent or earned so far; null while it is not known. */
  used: string | null
  spentWord: string
  leftWord: string
  money: (amount: string) => string
  onSave: (raw: string) => Promise<string | null>
}) {
  const t = useT()
  const [editing, setEditing] = useState(false)
  const [draft, setDraft] = useState(value)
  const inputRef = useRef<HTMLInputElement>(null)
  const label = t('budgets.budget')
  const gap = used === null ? null : budgetGap(used, value || null, type)

  useEffect(() => {
    if (editing) inputRef.current?.focus()
  }, [editing])

  async function save(raw: string) {
    const errorText = await onSave(raw)
    if (!errorText) setEditing(false)
  }

  function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    void save(draft)
  }

  const amount = <span className={value === '' || Number(value) === 0 ? 'cell-amount is-varies' : 'cell-amount'}>{value === '' ? '-' : money(value)}</span>
  const input = (
    <input
      ref={inputRef}
      aria-label={t('budgets.amountOf', { label })}
      inputMode="decimal"
      autoComplete="off"
      placeholder="0.00"
      value={draft}
      onChange={(event) => setDraft(event.target.value)}
      onKeyDown={(event) => {
        if (event.key === 'Escape') setEditing(false)
      }}
    />
  )
  // Distinct keys: reusing one button node and flipping its type to submit mid-click submits the form at once.
  const buttons = (
    <>
      <Button key="save" type="submit" size="sm">
        {t('common.save')}
      </Button>
      {value !== '' ? (
        <Button key="clear" variant="ghost" size="sm" onClick={() => void save('')}>
          {t('common.clear')}
        </Button>
      ) : null}
      <Button key="cancel" variant="ghost" size="sm" onClick={() => setEditing(false)}>
        {t('common.cancel')}
      </Button>
    </>
  )
  const edit = (
    <IconAction
      key="edit"
      icon="pencil"
      label={`${t('common.edit')}: ${label}`}
      onClick={(event) => {
        event.preventDefault()
        setDraft(value)
        setEditing(true)
      }}
    />
  )

  if (phone) {
    if (editing) {
      return (
        <form className="cat-strip is-editing" onSubmit={onSubmit}>
          <label>
            {label}
            {input}
          </label>
          <div className="cat-strip-buttons">{buttons}</div>
        </form>
      )
    }
    return (
      <div className="cat-strip">
        <dl>
          <div>
            <dt>{label}</dt>
            <dd>{amount}</dd>
          </div>
          <div>
            <dt>{spentWord}</dt>
            <dd>{used === null ? '-' : <span className={spentTone(used, value || null, type)}>{money(used)}</span>}</dd>
          </div>
          <div>
            <dt>{leftWord}</dt>
            <dd>{gap ? <span className={cn('cell-amount', gap.tone === 'over' && 'is-over', gap.tone === 'good' && 'is-good')}>{money(gap.left)}</span> : <span className="cell-none">—</span>}</dd>
          </div>
          {edit}
        </dl>
        {gap ? <MeterBar fill={gap.fill} reached={gap.tone === 'good'} over={gap.tone === 'over'} /> : null}
      </div>
    )
  }

  return (
    <li className={editing ? 'card-item cat-sum is-editing' : 'card-item cat-sum'}>
      <form className="budget-form" onSubmit={onSubmit}>
        <span className="cell-name">
          <span className="card-name">{t('budgets.total')}</span>
        </span>
        {editing ? input : amount}
        {editing ? null : (
          <>
            <span className={`${used === null ? 'cell-amount' : spentTone(used, value || null, type)} cat-spent`}>
              <span className="sr-only">{spentWord} </span>
              {used === null ? '-' : money(used)}
            </span>
            {gapCells(gap, money)}
          </>
        )}
        <span className="cell-actions">{editing ? buttons : edit}</span>
      </form>
    </li>
  )
}

/**
 * The two columns after the budget and what was spent: what is left of the budget (red when an expense went over it), and a bar of how much is
 * used with its percent; quiet dashes without a budget.
 */
function gapCells(gap: BudgetGap | null, money: (amount: string) => string) {
  if (!gap) {
    return (
      <>
        <span className="cell-sub-end cat-gap">
          <span className="cell-none">—</span>
        </span>
        <span className="cell-sub cat-used">
          <span className="cell-none">—</span>
        </span>
      </>
    )
  }
  return (
    <>
      <span className={cn('cell-sub-end cat-gap', gap.tone === 'over' && 'is-over', gap.tone === 'good' && 'is-good')}>{money(gap.left)}</span>
      <span className="cell-sub cat-used">
        <MeterBar fill={gap.fill} reached={gap.tone === 'good'} over={gap.tone === 'over'} />
        <span className="cat-used-figure">{gap.percent}%</span>
      </span>
    </>
  )
}

/** What a category adds up to, as a cell coloured against its budget: nothing yet reads as a dash; otherwise as the total of the side is. */
function spentTone(spent: string, budget: string | null, type: Side): string {
  return Number(spent) === 0 ? 'cell-amount is-varies' : type === 'EXPENSE' ? spentClass(spent, budget) : earnedClass(spent, budget)
}

type CategoryProps = {
  category: CategoryInMonth
  type: Side
  /** The budget of the category in this month, or an empty text. */
  value: string
  spent: string
  money: (amount: string) => string
  onEdit: (category: Category) => void
  onDelete: (category: Category) => void
}

/**
 * A category on the desktop: its budget, what it adds up to in the month, what is left of the budget and a bar of how much is used, and what can
 * be done with it: edit (name and budget, in a dialog) and delete. The grip at its start tells that the row can be moved.
 */
function CategoryRow({ category, type, value, spent, money, onEdit, onDelete, hintId }: CategoryProps & { /** The text that tells how to reorder, for the screen reader. */ hintId: string }) {
  const t = useT()
  const budget = value === '' ? null : value
  const gap = budgetGap(spent, budget, type)
  // Deleted from a later month on: it still counts here, and there is nothing to change about it.
  const gone = category.deleted
  return (
    <Row
      className={gone ? 'is-quiet' : undefined}
      data-reorder-id={category.id}
      actions={
        gone ? null : (
          <>
            <IconAction icon="pencil" label={t('common.edit')} data-row-focus aria-describedby={hintId} onClick={() => onEdit(category)} />
            <IconAction icon="trash" label={t('common.delete')} danger onClick={() => onDelete(category)} />
          </>
        )
      }
    >
      <div className="cell-name">
        {gone ? <span className="row-grip" /> : <Icon name="grip" className="row-grip" />}
        <span className="card-name" title={category.name}>
          {category.name}
        </span>
        {gone ? <span className="row-mark">{t('budgets.deletedLater')}</span> : null}
      </div>
      <span className={value === '' || Number(value) === 0 ? 'cell-amount is-varies cat-budget' : 'cell-amount cat-budget'}>{value === '' ? '-' : money(value)}</span>
      <span className={`${spentTone(spent, budget, type)} cat-spent`}>
        <span className="sr-only">{t(type === 'EXPENSE' ? 'budgets.colSpent' : 'budgets.colEarned')} </span>
        {money(spent)}
      </span>
      {gapCells(gap, money)}
    </Row>
  )
}

/**
 * A category on a phone: its name and what is left of its budget, a thin bar of how much is used under the name. It opens to its numbers and its
 * buttons. While `reordering` the rows are plain, with a grip, and can be held and dragged.
 */
function CategoryTapRow({
  category,
  type,
  value,
  spent,
  money,
  onEdit,
  onDelete,
  id,
  open,
  onToggle,
  reordering,
}: CategoryProps & { id: string; open: boolean; onToggle: () => void; reordering: boolean }) {
  const t = useT()
  const budget = value === '' ? null : value
  const gap = budgetGap(spent, budget, type)
  const gone = category.deleted
  const moving = reordering && !gone
  const facts = [
    { label: t('budgets.budget'), value: budget === null ? null : money(value) },
    { label: t(type === 'EXPENSE' ? 'budgets.colSpent' : 'budgets.colEarned'), value: money(spent) },
    gap ? { label: t(type === 'EXPENSE' ? 'budgets.colLeft' : 'budgets.colToGo'), value: money(gap.left), tone: gap.tone === 'over' ? ('negative' as const) : gap.tone === 'good' ? ('positive' as const) : undefined } : null,
    gap ? { label: t('budgets.colUsed'), value: `${gap.percent}%` } : null,
  ].filter((fact) => fact !== null)
  const name: ReactNode = (
    <span className="name-mark">
      {moving ? <Icon name="grip" className="row-grip" /> : null}
      <span className="name-text">{category.name}</span>
      {gone ? <span className="row-mark">{t('budgets.deletedLater')}</span> : null}
    </span>
  )
  return (
    <TapRow
      id={id}
      className={gone ? 'is-quiet' : undefined}
      data-reorder-id={moving ? category.id : undefined}
      open={!reordering && open}
      onToggle={onToggle}
      name={name}
      amount={gap ? <span className={cn('cell-amount', gap.tone === 'over' && 'is-over', gap.tone === 'good' && 'is-good')}>{money(gap.left)}</span> : <span className="cell-amount cell-none">—</span>}
      meta={gap ? <MeterBar fill={gap.fill} reached={gap.tone === 'good'} over={gap.tone === 'over'} /> : undefined}
      facts={reordering ? undefined : facts}
    >
      {reordering || gone ? null : (
        <PanelActions
          actions={[
            { label: t('common.edit'), icon: 'pencil', onClick: () => onEdit(category) },
            { label: t('common.delete'), icon: 'trash', danger: true, onClick: () => onDelete(category) },
          ]}
        />
      )}
    </TapRow>
  )
}
