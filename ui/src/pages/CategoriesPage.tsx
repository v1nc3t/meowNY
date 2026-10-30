import { useEffect, useRef, useState, type FormEvent } from 'react'
import { budgets as budgetsApi, MONTH, type MonthHistory, type Overview, type Span } from '../api/budgets'
import { categories as categoriesApi, type Category, type CategoryInMonth } from '../api/categories'
import { errorText } from '../api/invoke'
import { monthLabel, shiftMonth } from '../api/months'
import { useLocation, useNavigate } from 'react-router-dom'
import AddCategoryModal from '../components/AddCategoryModal'
import BudgetSpanModal from '../components/BudgetSpanModal'
import EditCategoryModal from '../components/EditCategoryModal'
import MonthNav from '../components/MonthNav'
import { IconAction } from '../components/RowParts'
import SectionTabs from '../components/SectionTabs'
import Button from '../components/ui/Button'
import { CardTitle } from '../components/ui/Card'
import { EmptyState } from '../components/ui/Pixel'
import { useToday } from '../api/today'
import { amountString, formatMoney } from '../lib/money'
import { useMoneySettings, type MoneySettings } from '../hooks/useMoneySettings'
import { useProfile } from '../hooks/profile'
import { useSwipeHandler } from '../hooks/swipe'
import { applyOrder } from '../lib/reorder'
import { step } from '../lib/swipe'
import { useSticky } from '../lib/sticky'
import { useReorder } from '../hooks/useReorder'
import { useToast } from '../hooks/toast'
import { useI18n, useT } from '../i18n/context'
import { useShortcut } from '../keys/registry'
import { tip } from '../keys/tip'
import { useRowNavigation } from '../keys/useRowNavigation'

type Side = 'EXPENSE' | 'INCOME'

const monthPattern = MONTH

export default function CategoriesPage() {
  const { settings, error } = useMoneySettings()
  const now = useToday()
  if (error) return <p className="notice">{error}</p>
  if (!settings || !now) return null
  return <CategoriesView settings={settings} thisMonth={now.month} />
}

function CategoriesView({ settings, thisMonth }: { settings: MoneySettings; thisMonth: string }) {
  const { t, language } = useI18n()
  const phone = useProfile() === 'phone'
  const [month, setMonth] = useSticky('categories.month', () => thisMonth)
  const [overview, setOverview] = useState<Overview[] | null>(null)
  const [history, setHistory] = useState<MonthHistory[] | null>(null)
  const paneRef = useRef<HTMLDivElement>(null)
  const [categories, setCategories] = useState<CategoryInMonth[] | null>(null)
  const [reload, setReload] = useState(0)
  const say = useToast()
  // the dialogs: a new category, the one being changed, the one about to be deleted
  const [adding, setAdding] = useState(false)
  const [editing, setEditing] = useState<Category | null>(null)
  const [deleting, setDeleting] = useState<Category | null>(null)
  const [pending, setPending] = useState(false)
  // The answer to "how far should the change reach", while that question is open (null: cancelled).
  const [spanAnswer, setSpanAnswer] = useState<((span: Span | null) => void) | null>(null)

  useEffect(() => {
    let cancelled = false
    const loading = monthPattern.test(month)
      ? categoriesApi.listInMonth(month)
      : categoriesApi.list().then((list) => list.map((category) => ({ ...category, deleted: false })))
    void loading.then(
      (list) => {
        if (!cancelled) setCategories(list)
      },
      (failure) => {
        if (cancelled) return
        say(errorText(failure), 'error')
        setCategories([])
      },
    )
    return () => {
      cancelled = true
    }
  }, [month, reload, say])

  useEffect(() => {
    if (!monthPattern.test(month)) {
      setOverview([])
      return
    }
    let cancelled = false
    void budgetsApi.overview(month).then(
      (result) => {
        if (!cancelled) setOverview(result)
      },
      (failure) => {
        if (cancelled) return
        say(errorText(failure), 'error')
        setOverview([])
      },
    )
    return () => {
      cancelled = true
    }
  }, [month, reload, say])

  useEffect(() => {
    let cancelled = false
    void budgetsApi.history().then(
      (rows) => {
        if (!cancelled) setHistory(rows)
      },
      () => undefined,
    )
    return () => {
      cancelled = true
    }
  }, [reload])

  // Keyboard: [ and ] step through months, t jumps to this month.
  useShortcut('prev', () => setMonth((current) => shiftMonth(current, -1)))
  useShortcut('next', () => setMonth((current) => shiftMonth(current, 1)))
  useShortcut('today', () => setMonth(thisMonth))
  // n adds a category, j and k move between rows.
  useShortcut('new', () => setAdding(true))

  // The phone's round button sends the person here to add a category: the form opens at once, once.
  const location = useLocation()
  const navigate = useNavigate()
  useEffect(() => {
    if ((location.state as { add?: boolean } | null)?.add !== true) return
    setAdding(true)
    navigate(location.pathname, { replace: true, state: null })
    // each time the page is asked, when it opens and when it is open already; the request is taken out of the location, so once
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [location.state])
  useRowNavigation()

  async function onDelete(span: Span) {
    if (!deleting || pending) return
    setPending(true)
    try {
      await categoriesApi.remove(deleting.id, month, span)
    } catch (failure) {
      say(errorText(failure), 'error')
    } finally {
      setPending(false)
    }
    setDeleting(null)
    setReload((value) => value + 1)
  }

  async function save(body: {
    type: string
    scope: 'GLOBAL' | 'CATEGORY'
    amountLimit: string | null
    categoryId?: string
  }, span?: Span): Promise<string | null> {
    if (!monthPattern.test(month)) return t('budgets.chooseMonth')
    try {
      await budgetsApi.set({ ...body, type: body.type as Side, month }, span)
    } catch (failure) {
      return errorText(failure)
    }
    setReload((value) => value + 1)
    return null
  }

  function amountFrom(raw: string): string | null | undefined {
    const trimmed = raw.trim()
    if (trimmed === '') return null
    const amount = amountString(trimmed, true)
    if (!amount) return undefined
    return amount
  }

  async function onGlobal(type: Side, raw: string): Promise<string | null> {
    const amountLimit = amountFrom(raw)
    if (amountLimit === undefined) {
      say(t('budgets.amountError'), 'error')
      return t('budgets.amountError')
    }
    // In a past month the person says how far the change reaches; cancelling leaves the budget as it was.
    let span: Span | undefined
    const current = overview?.find((row) => row.type === type)?.budgeted ?? null
    if (month < thisMonth && amountLimit !== current) {
      const answer = await new Promise<Span | null>((resolve) => setSpanAnswer(() => resolve))
      if (answer === null) return null
      span = answer
    }
    const errorText = await save({ type, scope: 'GLOBAL', amountLimit }, span)
    if (errorText) say(errorText, 'error')
    return errorText
  }

  // The new order shows at once; if it cannot be saved, the saved one comes back.
  async function onReorder(ids: string[]) {
    setCategories((current) => current && applyOrder(current, ids))
    try {
      await categoriesApi.reorder(ids)
    } catch (failure) {
      say(errorText(failure), 'error')
      setReload((value) => value + 1)
    }
  }

  // Zero and "not set" both read as a dash.
  const money = (amount: string) =>
    Number(amount) === 0 ? '-' : formatMoney(amount, settings.currency, settings.locale)
  const expenseCategories = categories?.filter((category) => category.type === 'EXPENSE') ?? null
  const incomeCategories = categories?.filter((category) => category.type === 'INCOME') ?? null

  // On a phone the three parts of the page are tabs, and a swipe moves between them before it moves to another page.
  const tabs = history && history.length > 0 ? (['EXPENSE', 'INCOME', 'HISTORY'] as const) : (['EXPENSE', 'INCOME'] as const)
  const [chosenTab, setTab] = useState<'EXPENSE' | 'INCOME' | 'HISTORY'>('EXPENSE')
  const tab = (tabs as readonly string[]).includes(chosenTab) ? chosenTab : 'EXPENSE'
  useSwipeHandler((direction) => {
    const next = step(tabs as readonly ('EXPENSE' | 'INCOME' | 'HISTORY')[], tab, direction)
    if (next === null) return false
    setTab(next)
    return true
  }, phone)

  const halves = [
    <BudgetHalf
      key="EXPENSE"
      title={t('budgets.expenses')}
      hideTitle={phone}
      type="EXPENSE"
      month={month}
      overview={overview?.find((row) => row.type === 'EXPENSE') ?? null}
      amounts={amountsFor(overview, 'EXPENSE')}
      categories={expenseCategories}
      money={money}
      onGlobal={onGlobal}
      onEdit={setEditing}
      onDelete={setDeleting}
      onReorder={onReorder}
      onAdd={() => setAdding(true)}
    />,
    <BudgetHalf
      key="INCOME"
      title={t('budgets.income')}
      hideTitle={phone}
      type="INCOME"
      month={month}
      overview={overview?.find((row) => row.type === 'INCOME') ?? null}
      amounts={amountsFor(overview, 'INCOME')}
      categories={incomeCategories}
      money={money}
      onGlobal={onGlobal}
      onEdit={setEditing}
      onDelete={setDeleting}
      onReorder={onReorder}
      onAdd={() => setAdding(true)}
    />,
  ]
  // The months, each a row you click to see that month above; spent is green within its budget and red above it, earned green above its budget.
  const historyList = history ? (
    <ul className="card-list hist-list">
      <li className="list-head" aria-hidden="true">
        <span>{t('budgets.colMonth')}</span>
        <span className="is-end">{t('budgets.colSpent')}</span>
        <span className="is-end">{t('budgets.colExpenseBudget')}</span>
        <span className="is-end">{t('budgets.colEarned')}</span>
        <span className="is-end">{t('budgets.colIncomeBudget')}</span>
      </li>
      {history.map((row) => {
        const overearned = row.incomeBudget !== null && Number(row.incomeActual) > Number(row.incomeBudget)
        return (
          <li key={row.month} className={row.month === month ? 'card-item is-selected' : 'card-item'}>
            <span className="cell-name">
              <Button
                variant="link"
                className="card-name"
                aria-current={row.month === month ? 'date' : undefined}
                onClick={() => {
                  setMonth(row.month)
                  paneRef.current?.scrollTo({ top: 0 })
                }}
              >
                {monthLabel(row.month, language)}
              </Button>
            </span>
            <span className="sr-only hist-label">{t('budgets.colSpent')}</span>
            <span className={spentClass(row.expenseActual, row.expenseBudget)}>{money(row.expenseActual)}</span>
            <span className="cell-sub-end hist-budget">
              <span className="sr-only">{t('budgets.colExpenseBudget')}</span>
              {row.expenseBudget === null ? '-' : money(row.expenseBudget)}
            </span>
            <span className="sr-only hist-label">{t('budgets.colEarned')}</span>
            <span className={overearned ? 'cell-amount is-income' : 'cell-amount'}>{money(row.incomeActual)}</span>
            <span className="cell-sub-end hist-budget">
              <span className="sr-only">{t('budgets.colIncomeBudget')}</span>
              {row.incomeBudget === null ? '-' : money(row.incomeBudget)}
            </span>
          </li>
        )
      })}
    </ul>
  ) : null

  return (
    <section className="tx-page budget-page">
      <div className="tx-title budget-title">
        <h1>{t('categories.title')}</h1>
        <div className="tx-actions">
          <Button onClick={() => setAdding(true)} {...tip('new', t('categories.add'))}>
            {t('categories.add')}
          </Button>
          <MonthNav month={month} thisMonth={thisMonth} onChange={setMonth} />
        </div>
      </div>
      {adding ? (
        <AddCategoryModal
          month={month}
          onClose={() => setAdding(false)}
          onCreated={() => {
            setAdding(false)
            setReload((value) => value + 1)
          }}
        />
      ) : null}
      {editing ? (
        <EditCategoryModal
          key={`${editing.id}:${month}`}
          category={editing}
          amount={amountsFor(overview, editing.type).get(editing.id) ?? ''}
          month={month}
          monthText={monthLabel(month, language)}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null)
            setReload((value) => value + 1)
          }}
        />
      ) : null}
      {spanAnswer ? (
        <BudgetSpanModal
          onChoose={(span) => {
            spanAnswer(span)
            setSpanAnswer(null)
          }}
          onClose={() => {
            spanAnswer(null)
            setSpanAnswer(null)
          }}
        />
      ) : null}
      {deleting ? (
        <BudgetSpanModal
          title={t('budgets.deleteTitle', { name: deleting.name })}
          labels={{ ONLY: t('budgets.deleteOnly'), FORWARD: t('budgets.deleteForward'), ALL: t('budgets.deleteAll') }}
          hints={{ ONLY: t('budgets.deleteOnlyHint'), FORWARD: t('budgets.deleteForwardHint'), ALL: t('budgets.deleteAllHint') }}
          danger
          pending={pending}
          onChoose={(span) => void onDelete(span)}
          onClose={() => setDeleting(null)}
        />
      ) : null}
      {phone ? (
        <>
          <SectionTabs
            label={t('categories.title')}
            value={tab}
            onChange={setTab}
            tabs={tabs.map((name) => ({ id: name, label: t(name === 'EXPENSE' ? 'budgets.expenses' : name === 'INCOME' ? 'budgets.income' : 'budgets.history') }))}
          />
          <div className="tx-pane wide budget-view" role="tabpanel" ref={paneRef}>
            {tab === 'EXPENSE' ? halves[0] : null}
            {tab === 'INCOME' ? halves[1] : null}
            {tab === 'HISTORY' ? historyList : null}
          </div>
        </>
      ) : (
        <div className="tx-pane wide budget-view" ref={paneRef}>
          <div className="budget-split">{halves}</div>
          {history && history.length > 0 ? (
            <section className="budget-history">
              <CardTitle>{t('budgets.history')}</CardTitle>
              {historyList}
            </section>
          ) : null}
        </div>
      )}
    </section>
  )
}

/** Spending against its budget: green at or under it, red above it, plain when there is no budget. */
function spentClass(spent: string, budget: string | null): string {
  if (budget === null) return 'cell-amount'
  return Number(spent) > Number(budget) ? 'cell-amount is-over' : 'cell-amount is-income'
}

function amountsFor(overview: Overview[] | null, type: Side): Map<string, string> {
  const row = overview?.find((item) => item.type === type)
  return new Map((row?.categories ?? []).map((line) => [line.categoryId, line.amountLimit]))
}

function BudgetHalf({
  title,
  hideTitle = false,
  type,
  month,
  overview,
  amounts,
  categories,
  money,
  onGlobal,
  onEdit,
  onDelete,
  onReorder,
  onAdd,
}: {
  title: string
  /** The title is already shown by a section around this one. */
  hideTitle?: boolean
  type: Side
  month: string
  overview: Overview | null
  amounts: Map<string, string>
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
  const budgeted = overview?.budgeted ?? ''
  return (
    <section className="budget-half">
      {hideTitle ? null : <CardTitle>{title}</CardTitle>}
      <ul className="card-list sum-list">
        <BudgetRow
          key={`${type}:${month}:${budgeted}`}
          label={t('budgets.budget')}
          value={budgeted}
          money={money}
          onSave={(raw) => onGlobal(type, raw)}
        />
        {overview ? (
          <li className="card-item">
            <span className="cell-name">
              <span className="cell-sub">{t(type === 'EXPENSE' ? 'budgets.colSpent' : 'budgets.colEarned')}</span>
            </span>
            <span className={type === 'EXPENSE' ? spentClass(overview.used, overview.budgeted) : 'cell-amount'}>{money(overview.used)}</span>
          </li>
        ) : null}
      </ul>
      {categories && categories.length > 0 ? (
        <ul className="card-list cat-list" key={month} ref={listRef}>
          <li className="list-head" aria-hidden="true">
            <span>{t('common.category')}</span>
            <span className="is-end">{t('budgets.budget')}</span>
            <span />
          </li>
          {categories.map((category) => (
            <CategoryRow
              key={category.id}
              category={category}
              value={amounts.get(category.id) ?? ''}
              money={money}
              onEdit={onEdit}
              onDelete={onDelete}
              hintId={hintId}
            />
          ))}
        </ul>
      ) : null}
      {/* the button is on the expenses side only: when both sides are empty, two of them would say the same */}
      {categories && categories.length === 0 ? (
        <EmptyState text={t('common.noCategories')} action={type === 'EXPENSE' ? { label: t('categories.add'), onClick: onAdd } : undefined} />
      ) : null}
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

/** A category with its budget in the month, and what can be done with it: edit (name and budget, in a dialog) and delete. */
function CategoryRow({
  category,
  value,
  money,
  onEdit,
  onDelete,
  hintId,
}: {
  category: CategoryInMonth
  value: string
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
  // Deleted from a later month on: it still counts here, and there is nothing to change about it.
  if (category.deleted) {
    return (
      <li className="card-item is-quiet" data-reorder-id={category.id}>
        <div className="cell-name">
          <span className="card-name" title={category.name}>
            {category.name}
          </span>
          <span className="row-mark">{t('budgets.deletedLater')}</span>
        </div>
        {amount}
        <div className="cell-actions" />
      </li>
    )
  }
  return (
    <li className="card-item" data-reorder-id={category.id}>
      <div className="cell-name">
        <span className="card-name" title={category.name}>
          {category.name}
        </span>
      </div>
      {amount}
      <div className="cell-actions">
        <IconAction icon="pencil" label={t('common.edit')} data-row-focus aria-describedby={hintId} onClick={() => onEdit(category)} />
        <IconAction icon="trash" label={t('common.delete')} danger onClick={() => onDelete(category)} />
      </div>
    </li>
  )
}
