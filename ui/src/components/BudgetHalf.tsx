import { useEffect, useRef, useState, type FormEvent } from 'react'
import type { Overview } from '../api/budgets'
import type { Category, CategoryInMonth } from '../api/categories'
import { amountsFor, budgetGap, earnedClass, spentClass, spentFor, type Side } from '../lib/categoriesView'
import { cn } from './ui/cn'
import Icon from './NavIcon'
import { MeterBar } from './StashParts'
import { IconAction, Row } from './RowParts'
import Button from './ui/Button'
import { CardTitle } from './ui/Card'
import { EmptyState } from './ui/Pixel'
import { useReorder } from '../hooks/useReorder'
import { useT } from '../i18n/context'

/**
 * One side of the budgets, expenses or income: its budget and what it adds up to this month, then its categories with what each adds
 * up to and its budget. The categories page loads both sides with `useBudgets`.
 */
export default function BudgetHalf({
  hideTitle = false,
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
  const { listRef, hintId, live } = useReorder(categories, onReorder)
  const side = overview?.find((row) => row.type === type) ?? null
  const amounts = amountsFor(overview, type)
  const spent = spentFor(overview, type)
  const budgeted = side?.budgeted ?? ''
  return (
    <section className="budget-half" data-side={type}>
      {hideTitle ? null : <CardTitle>{t(type === 'EXPENSE' ? 'budgets.expenses' : 'budgets.income')}</CardTitle>}
      <ul className="card-list sum-list">
        <BudgetRow
          key={`${type}:${month}:${budgeted}`}
          label={t('budgets.budget')}
          value={budgeted}
          money={money}
          onSave={(raw) => onGlobal(type, raw)}
        />
        {side ? (
          <li className="card-item">
            <span className="cell-name">
              <span className="cell-sub">{t(type === 'EXPENSE' ? 'budgets.colSpent' : 'budgets.colEarned')}</span>
            </span>
            <span className={type === 'EXPENSE' ? spentClass(side.used, side.budgeted) : earnedClass(side.used, side.budgeted)}>{money(side.used)}</span>
            {gapCells(side.used, side.budgeted || null, type, money)}
          </li>
        ) : null}
      </ul>
      {categories && categories.length > 0 ? (
        <ul className="card-list cat-list" key={month} ref={listRef}>
          <li className="list-head" aria-hidden="true">
            <span>{t('common.category')}</span>
            <span className="is-end">{t(type === 'EXPENSE' ? 'budgets.colSpent' : 'budgets.colEarned')}</span>
            <span className="is-end">{t('budgets.budget')}</span>
            <span className="is-end cat-gap">{t(type === 'EXPENSE' ? 'budgets.colLeft' : 'budgets.colToGo')}</span>
            <span className="cat-used">{t('budgets.colUsed')}</span>
            <span />
          </li>
          {categories.map((category) => (
            <CategoryRow
              key={category.id}
              category={category}
              type={type}
              value={amounts.get(category.id) ?? ''}
              spent={spent.get(category.id) ?? '0.00'}
              money={money}
              onEdit={onEdit}
              onDelete={onDelete}
              hintId={hintId}
            />
          ))}
        </ul>
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

/** One row of the budget of a side: its amount is plain text until "edit" is clicked. Saving an empty field removes the budget. */
function BudgetRow({
  label,
  value,
  money,
  onSave,
}: {
  label: string
  value: string
  money: (amount: string) => string
  onSave: (raw: string) => Promise<string | null>
}) {
  const t = useT()
  const [editing, setEditing] = useState(false)
  const [draft, setDraft] = useState(value)
  const inputRef = useRef<HTMLInputElement>(null)

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

  return (
    <li className={editing ? 'card-item is-editing' : 'card-item'}>
      <form className="budget-form" onSubmit={onSubmit}>
        <span className="cell-name">
          <span className="card-name">{label}</span>
        </span>
        {editing ? (
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
        ) : (
          <span className={value === '' || Number(value) === 0 ? 'cell-amount is-varies' : 'cell-amount'}>{value === '' ? '-' : money(value)}</span>
        )}
        <span className="cell-actions">
          {/* Distinct keys: reusing one button node and flipping its type to submit mid-click submits the form at once. */}
          {editing ? (
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
          ) : (
            <IconAction
              key="edit"
              icon="pencil"
              label={t('common.edit')}
              onClick={(event) => {
                event.preventDefault()
                setDraft(value)
                setEditing(true)
              }}
            />
          )}
        </span>
      </form>
    </li>
  )
}

/**
 * The two columns after the budget: what is left of it (red when an expense went over it), and a bar of how much is used with its percent; quiet
 * dashes without a budget.
 */
function gapCells(used: string, budget: string | null, type: Side, money: (amount: string) => string) {
  const gap = budgetGap(used, budget, type)
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

/**
 * A category with what it adds up to in the month and its budget, and what can be done with it: edit (name and budget, in a dialog) and
 * delete. The grip at its start tells that the row can be moved.
 */
function CategoryRow({
  category,
  type,
  value,
  spent,
  money,
  onEdit,
  onDelete,
  hintId,
}: {
  category: CategoryInMonth
  type: Side
  value: string
  spent: string
  money: (amount: string) => string
  onEdit: (category: Category) => void
  onDelete: (category: Category) => void
  /** The text that tells how to reorder, for the screen reader. */
  hintId: string
}) {
  const t = useT()
  const amount = (
    <span className={value === '' || Number(value) === 0 ? 'cell-amount is-varies' : 'cell-amount'}>{value === '' ? '-' : money(value)}</span>
  )
  // Nothing yet reads as a dash, like the budget; otherwise it is coloured against the budget as the total of the side is.
  const budget = value === '' ? null : value
  const tone = Number(spent) === 0 ? 'cell-amount is-varies' : type === 'EXPENSE' ? spentClass(spent, budget) : earnedClass(spent, budget)
  const spentCell = (
    <span className={`${tone} cell-spent`}>
      <span className="sr-only spent-label">{t(type === 'EXPENSE' ? 'budgets.colSpent' : 'budgets.colEarned')}</span> {money(spent)}
    </span>
  )
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
      {spentCell}
      {amount}
      {gapCells(spent, budget, type, money)}
    </Row>
  )
}
