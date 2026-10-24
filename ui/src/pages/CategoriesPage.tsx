import { useEffect, useRef, useState, type FormEvent } from 'react'
import { budgets as budgetsApi, MONTH, type MonthHistory, type Overview, type Span } from '../api/budgets'
import { categories as categoriesApi, type Category } from '../api/categories'
import { errorText } from '../api/invoke'
import { monthLabel, shiftMonth } from '../api/months'
import { useLocation, useNavigate } from 'react-router-dom'
import AddCategoryModal from '../components/AddCategoryModal'
import BudgetSpanModal from '../components/BudgetSpanModal'
import ConfirmModal from '../components/ConfirmModal'
import EditCategoryModal from '../components/EditCategoryModal'
import MonthNav from '../components/MonthNav'
import SectionTabs from '../components/SectionTabs'
import Button from '../components/ui/Button'
import { CardTitle } from '../components/ui/Card'
import { useToday } from '../api/today'
import { amountString, formatMoney } from '../lib/money'
import { useMoneySettings, type MoneySettings } from '../hooks/useMoneySettings'
import { useProfile } from '../hooks/profile'
import { useSwipeHandler } from '../hooks/swipe'
import { step } from '../lib/swipe'
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
  const [month, setMonth] = useState(thisMonth)
  const [overview, setOverview] = useState<Overview[] | null>(null)
  const [history, setHistory] = useState<MonthHistory[] | null>(null)
  const pageRef = useRef<HTMLElement>(null)
  const [categories, setCategories] = useState<Category[] | null>(null)
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
    void categoriesApi.list().then(
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
  }, [reload, say])

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

  async function onDelete() {
    if (!deleting || pending) return
    setPending(true)
    try {
      await categoriesApi.remove(deleting.id)
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
    />,
  ]
  const historyTable = history ? (
    <table className="ui-table">
      <thead>
        <tr>
          <th scope="col">{t('budgets.colMonth')}</th>
          <th scope="col">{t('budgets.colExpenseBudget')}</th>
          <th scope="col">{t('budgets.colSpent')}</th>
          <th scope="col">{t('budgets.colIncomeBudget')}</th>
          <th scope="col">{t('budgets.colEarned')}</th>
        </tr>
      </thead>
      <tbody>
        {history.map((row) => {
          // red when more was spent than was budgeted, green when more was earned than was budgeted
          const overspent = row.expenseBudget !== null && Number(row.expenseActual) > Number(row.expenseBudget)
          const overearned = row.incomeBudget !== null && Number(row.incomeActual) > Number(row.incomeBudget)
          return (
            <tr key={row.month} className={row.month === month ? 'selected' : undefined}>
              <td>
                <Button
                  variant="link"
                  onClick={() => {
                    setMonth(row.month)
                    pageRef.current?.scrollTo({ top: 0 })
                  }}
                >
                  {monthLabel(row.month, language)}
                </Button>
              </td>
              <td>{row.expenseBudget === null ? '-' : money(row.expenseBudget)}</td>
              <td>
                <span className={overspent ? 'over' : undefined}>{money(row.expenseActual)}</span>
              </td>
              <td>{row.incomeBudget === null ? '-' : money(row.incomeBudget)}</td>
              <td>
                <span className={overearned ? 'earned-over' : undefined}>{money(row.incomeActual)}</span>
              </td>
            </tr>
          )
        })}
      </tbody>
    </table>
  ) : null

  return (
    <section className="tx-page budget-page" ref={pageRef}>
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
        <ConfirmModal
          title={t('rec.confirmTitle', { action: t('common.delete'), name: deleting.name })}
          pending={pending}
          onConfirm={() => void onDelete()}
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
          <div className="tx-pane wide budget-view" role="tabpanel">
            {tab === 'EXPENSE' ? halves[0] : null}
            {tab === 'INCOME' ? halves[1] : null}
            {tab === 'HISTORY' ? (
              <div className="ui-card budget-history">
                <div className="ui-table-wrap budget-history-scroll">{historyTable}</div>
              </div>
            ) : null}
          </div>
        </>
      ) : (
        <>
          <div className="budget-split wide">{halves}</div>
          {history && history.length > 0 ? (
            <section className="ui-card budget-history">
              <CardTitle>{t('budgets.history')}</CardTitle>
              <div className="ui-table-wrap budget-history-scroll">{historyTable}</div>
            </section>
          ) : null}
        </>
      )}
    </section>
  )
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
}: {
  title: string
  /** The title is already shown by a section around this one. */
  hideTitle?: boolean
  type: Side
  month: string
  overview: Overview | null
  amounts: Map<string, string>
  categories: Category[] | null
  money: (amount: string) => string
  onGlobal: (type: Side, raw: string) => Promise<string | null>
  onEdit: (category: Category) => void
  onDelete: (category: Category) => void
}) {
  const t = useT()
  const budgeted = overview?.budgeted ?? ''
  const over = overview !== null && Number(overview.notUsed) < 0
  return (
    <section className="ui-card budget-half">
      {hideTitle ? null : <CardTitle>{title}</CardTitle>}
      <div className="budget-table">
        <BudgetRow
          key={`${type}:${month}:${budgeted}`}
          label={t('budgets.budget')}
          value={budgeted}
          money={money}
          onSave={(raw) => onGlobal(type, raw)}
        />
        {overview ? (
          <>
            <div className="budget-row quiet">
              <span className="budget-label">{t('budgets.allocated')}</span>
              <span className="budget-amount">{money(overview.used)}</span>
              <span className="budget-actions" />
            </div>
            <div className="budget-row quiet">
              <span className="budget-label">{t('budgets.unallocated')}</span>
              <span className={over ? 'budget-amount over' : 'budget-amount'}>
                {money(overview.notUsed)}
              </span>
              <span className="budget-actions" />
            </div>
          </>
        ) : null}
      </div>
      {/* The category list scrolls on its own, so the summary above and the history below stay in view. */}
      <div className="budget-pane">
        {categories && categories.length > 0 ? (
          <div className="budget-table" key={month}>
            {categories.map((category) => (
              <CategoryRow
                key={category.id}
                category={category}
                value={amounts.get(category.id) ?? ''}
                money={money}
                onEdit={onEdit}
                onDelete={onDelete}
              />
            ))}
          </div>
        ) : null}
        {categories && categories.length === 0 ? <p>{t('common.noCategories')}</p> : null}
      </div>
    </section>
  )
}

/** One amount that is plain text until "edit" is clicked. Saving an empty field removes the budget. */
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
    if (!editing) setDraft(value)
  }, [value, editing])

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
    <form className={editing ? 'budget-row editing' : 'budget-row'} onSubmit={onSubmit}>
      <span className="budget-label">{label}</span>
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
        <span className={value === '' || Number(value) === 0 ? 'budget-amount unset' : 'budget-amount'}>
          {value === '' ? '-' : money(value)}
        </span>
      )}
      <span className="budget-actions">
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
          <Button
            key="edit"
            variant="ghost"
            size="sm"
            onClick={(event) => {
              event.preventDefault()
              setEditing(true)
            }}
          >
            {t('common.edit')}
          </Button>
        )}
      </span>
    </form>
  )
}

/** A category with its budget in the month, and what can be done with it: edit (name and budget, in a dialog) and delete. */
function CategoryRow({
  category,
  value,
  money,
  onEdit,
  onDelete,
}: {
  category: Category
  value: string
  money: (amount: string) => string
  onEdit: (category: Category) => void
  onDelete: (category: Category) => void
}) {
  const t = useT()
  return (
    <div className="budget-row">
      <span className="budget-label">{category.name}</span>
      <span className={value === '' || Number(value) === 0 ? 'budget-amount unset' : 'budget-amount'}>{value === '' ? '-' : money(value)}</span>
      <span className="budget-actions">
        <Button variant="ghost" size="sm" data-row-focus onClick={() => onEdit(category)}>
          {t('common.edit')}
        </Button>
        <Button variant="ghost" size="sm" className="row-danger" onClick={() => onDelete(category)}>
          {t('common.delete')}
        </Button>
      </span>
    </div>
  )
}
