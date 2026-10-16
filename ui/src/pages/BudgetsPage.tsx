import { useEffect, useRef, useState, type FormEvent } from 'react'
import { budgets as budgetsApi, MONTH, type MonthHistory, type Overview } from '../api/budgets'
import { categories as categoriesApi, type Category } from '../api/categories'
import { errorText } from '../api/invoke'
import { monthLabel, shiftMonth } from '../api/months'
import MonthNav from '../components/MonthNav'
import SectionTabs from '../components/SectionTabs'
import { useToday } from '../api/today'
import { amountString, formatMoney } from '../lib/money'
import { useMoneySettings, type MoneySettings } from '../hooks/useMoneySettings'
import { useProfile } from '../hooks/profile'
import { useSwipeHandler } from '../hooks/swipe'
import { step } from '../lib/swipe'
import { useNotice } from '../hooks/useNotice'
import { useI18n, useT } from '../i18n/context'
import { useShortcut } from '../keys/registry'

type Side = 'EXPENSE' | 'INCOME'

const monthPattern = MONTH

export default function BudgetsPage() {
  const { settings, error } = useMoneySettings()
  const now = useToday()
  if (error) return <p className="notice">{error}</p>
  if (!settings || !now) return null
  return <BudgetsView settings={settings} thisMonth={now.month} />
}

function BudgetsView({ settings, thisMonth }: { settings: MoneySettings; thisMonth: string }) {
  const { t, language } = useI18n()
  const phone = useProfile() === 'phone'
  const [month, setMonth] = useState(thisMonth)
  const [overview, setOverview] = useState<Overview[] | null>(null)
  const [history, setHistory] = useState<MonthHistory[] | null>(null)
  const pageRef = useRef<HTMLElement>(null)
  const [categories, setCategories] = useState<Category[] | null>(null)
  const [reload, setReload] = useState(0)
  const [message, setMessage] = useNotice()

  useEffect(() => {
    let cancelled = false
    void categoriesApi.list().then(
      (list) => {
        if (!cancelled) setCategories(list)
      },
      (failure) => {
        if (cancelled) return
        setMessage(errorText(failure))
        setCategories([])
      },
    )
    return () => {
      cancelled = true
    }
  }, [setMessage])

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
        setMessage(errorText(failure))
        setOverview([])
      },
    )
    return () => {
      cancelled = true
    }
  }, [month, reload, setMessage])

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

  async function save(body: {
    type: string
    scope: 'GLOBAL' | 'CATEGORY'
    amountLimit: string | null
    categoryId?: string
  }): Promise<string | null> {
    if (!monthPattern.test(month)) return t('budgets.chooseMonth')
    try {
      await budgetsApi.set({ ...body, type: body.type as Side, month })
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
    if (amountLimit === undefined) return t('budgets.amountError')
    const errorText = await save({ type, scope: 'GLOBAL', amountLimit })
    setMessage(errorText ?? '')
    return errorText
  }

  async function onCategory(category: Category, raw: string): Promise<string | null> {
    const amountLimit = amountFrom(raw)
    if (amountLimit === undefined) {
      const errorText = t('budgets.amountError')
      setMessage(errorText)
      return errorText
    }
    const errorText = await save({
      type: category.type,
      scope: 'CATEGORY',
      amountLimit,
      categoryId: category.id,
    })
    setMessage(errorText ?? '')
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
      onCategory={onCategory}
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
      onCategory={onCategory}
    />,
  ]
  const historyTable = history ? (
    <table>
      <thead>
        <tr>
          <th>{t('budgets.colMonth')}</th>
          <th>{t('budgets.colExpenseBudget')}</th>
          <th>{t('budgets.colSpent')}</th>
          <th>{t('budgets.colIncomeBudget')}</th>
          <th>{t('budgets.colEarned')}</th>
        </tr>
      </thead>
      <tbody>
        {history.map((row) => {
          const overspent = row.expenseBudget !== null && Number(row.expenseActual) > Number(row.expenseBudget)
          return (
            <tr key={row.month} className={row.month === month ? 'selected' : undefined}>
              <td>
                <button
                  type="button"
                  onClick={() => {
                    setMonth(row.month)
                    pageRef.current?.scrollTo({ top: 0 })
                  }}
                >
                  {monthLabel(row.month, language)}
                </button>
              </td>
              <td>{row.expenseBudget === null ? '-' : money(row.expenseBudget)}</td>
              <td>
                <span className={overspent ? 'over' : undefined}>{money(row.expenseActual)}</span>
              </td>
              <td>{row.incomeBudget === null ? '-' : money(row.incomeBudget)}</td>
              <td>{money(row.incomeActual)}</td>
            </tr>
          )
        })}
      </tbody>
    </table>
  ) : null

  return (
    <section className="tx-page budget-page" ref={pageRef}>
      <div className="tx-title">
        <h1>{t('budgets.title')}</h1>
        <MonthNav month={month} thisMonth={thisMonth} onChange={setMonth} />
      </div>
      <p className="notice">{message}</p>
      {phone ? (
        <>
          <SectionTabs
            label={t('budgets.title')}
            value={tab}
            onChange={setTab}
            tabs={tabs.map((name) => ({ id: name, label: t(name === 'EXPENSE' ? 'budgets.expenses' : name === 'INCOME' ? 'budgets.income' : 'budgets.history') }))}
          />
          <div className="tx-pane wide budget-view" role="tabpanel">
            {tab === 'EXPENSE' ? halves[0] : null}
            {tab === 'INCOME' ? halves[1] : null}
            {tab === 'HISTORY' ? (
              <div className="budget-history">
                <div className="budget-history-scroll">{historyTable}</div>
              </div>
            ) : null}
          </div>
        </>
      ) : (
        <>
          <div className="budget-split wide">{halves}</div>
          {history && history.length > 0 ? (
            <section className="budget-history">
              <h2>{t('budgets.history')}</h2>
              <div className="budget-history-scroll">{historyTable}</div>
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
  onCategory,
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
  onCategory: (category: Category, raw: string) => Promise<string | null>
}) {
  const t = useT()
  const budgeted = overview?.budgeted ?? ''
  const over = overview !== null && overview.overAllocated && overview.budgeted !== null
  return (
    <section className="budget-half">
      {hideTitle ? null : <h2>{title}</h2>}
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
              <span className="budget-amount">{money(overview.allocated)}</span>
              <span className="budget-actions" />
            </div>
            <div className="budget-row quiet">
              <span className="budget-label">{t('budgets.unallocated')}</span>
              <span className={over ? 'budget-amount over' : 'budget-amount'}>
                {overview.unallocated === null ? '-' : money(overview.unallocated)}
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
              <BudgetRow
                key={category.id}
                label={category.name}
                value={amounts.get(category.id) ?? ''}
                money={money}
                onSave={(raw) => onCategory(category, raw)}
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
            <button key="save" type="submit">
              {t('common.save')}
            </button>
            {value !== '' ? (
              <button key="clear" type="button" onClick={() => void save('')}>
                {t('common.clear')}
              </button>
            ) : null}
            <button key="cancel" type="button" onClick={() => setEditing(false)}>
              {t('common.cancel')}
            </button>
          </>
        ) : (
          <button
            key="edit"
            type="button"
            onClick={(event) => {
              event.preventDefault()
              setEditing(true)
            }}
          >
            {t('common.edit')}
          </button>
        )}
      </span>
    </form>
  )
}
