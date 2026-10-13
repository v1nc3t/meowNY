import { useEffect, useRef, useState, type FormEvent } from 'react'
import { budgets as budgetsApi, MONTH, type MonthHistory, type Overview } from '../api/budgets'
import { categories as categoriesApi, type Category } from '../api/categories'
import { errorText } from '../api/invoke'
import { monthLabel, monthNames, shiftMonth, withMonth, withYear, yearChoices } from '../api/months'
import Dropdown from '../components/Dropdown'
import { useToday } from '../api/today'
import { amountString, formatMoney } from '../money'
import { useMoneySettings, type MoneySettings } from '../useMoneySettings'
import { useNotice } from '../useNotice'

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
  }, [])

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
  }, [month, reload])

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

  async function save(body: {
    type: string
    scope: 'GLOBAL' | 'CATEGORY'
    amountLimit: string | null
    categoryId?: string
  }): Promise<string | null> {
    if (!monthPattern.test(month)) return 'choose a month.'
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
    if (amountLimit === undefined) return 'enter an amount with at most two decimals, or leave it blank to clear.'
    const errorText = await save({ type, scope: 'GLOBAL', amountLimit })
    setMessage(errorText ?? '')
    return errorText
  }

  async function onCategory(category: Category, raw: string): Promise<string | null> {
    const amountLimit = amountFrom(raw)
    if (amountLimit === undefined) {
      const errorText = 'enter an amount with at most two decimals, or leave it blank to clear.'
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

  return (
    <section className="tx-page budget-page" ref={pageRef}>
      <div className="tx-title">
        <h1>budgets</h1>
        <div className="month-nav">
          <button type="button" aria-label="previous month" onClick={() => setMonth(shiftMonth(month, -1))}>
            &lsaquo;
          </button>
          <Dropdown
            compact
            label="month"
            value={String(Number(month.slice(5, 7)))}
            options={monthNames(settings.locale).map((name, index) => ({ value: String(index + 1), label: name }))}
            onChange={(value) => setMonth(withMonth(month, Number(value)))}
          />
          <Dropdown
            compact
            label="year"
            value={month.slice(0, 4)}
            options={yearChoices(month, Number(thisMonth.slice(0, 4))).map((year) => ({
              value: String(year),
              label: String(year),
            }))}
            onChange={(value) => setMonth(withYear(month, Number(value)))}
          />
          <button type="button" aria-label="next month" onClick={() => setMonth(shiftMonth(month, 1))}>
            &rsaquo;
          </button>
          <button type="button" disabled={month === thisMonth} onClick={() => setMonth(thisMonth)}>
            current
          </button>
        </div>
      </div>
      <p className="notice">{message}</p>
      <div className="budget-split wide">
        <BudgetHalf
          title="expenses"
          type="EXPENSE"
          month={month}
          overview={overview?.find((row) => row.type === 'EXPENSE') ?? null}
          amounts={amountsFor(overview, 'EXPENSE')}
          categories={expenseCategories}
          money={money}
          onGlobal={onGlobal}
          onCategory={onCategory}
        />
        <BudgetHalf
          title="income"
          type="INCOME"
          month={month}
          overview={overview?.find((row) => row.type === 'INCOME') ?? null}
          amounts={amountsFor(overview, 'INCOME')}
          categories={incomeCategories}
          money={money}
          onGlobal={onGlobal}
          onCategory={onCategory}
        />
      </div>
      {history && history.length > 0 ? (
        <section className="budget-history">
          <h2>history</h2>
          <div className="budget-history-scroll">
            <table>
              <thead>
                <tr>
                  <th>month</th>
                  <th>expense budget</th>
                  <th>spent</th>
                  <th>income budget</th>
                  <th>earned</th>
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
                          {monthLabel(row.month, settings.locale)}
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
          </div>
        </section>
      ) : null}
    </section>
  )
}

function amountsFor(overview: Overview[] | null, type: Side): Map<string, string> {
  const row = overview?.find((item) => item.type === type)
  return new Map((row?.categories ?? []).map((line) => [line.categoryId, line.amountLimit]))
}

function BudgetHalf({
  title,
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
  type: Side
  month: string
  overview: Overview | null
  amounts: Map<string, string>
  categories: Category[] | null
  money: (amount: string) => string
  onGlobal: (type: Side, raw: string) => Promise<string | null>
  onCategory: (category: Category, raw: string) => Promise<string | null>
}) {
  const budgeted = overview?.budgeted ?? ''
  const over = overview !== null && overview.overAllocated && overview.budgeted !== null
  return (
    <section className="budget-half">
      <h2>{title}</h2>
      <div className="budget-table">
        <BudgetRow
          key={`${type}:${month}:${budgeted}`}
          label="budget"
          value={budgeted}
          money={money}
          onSave={(raw) => onGlobal(type, raw)}
        />
        {overview ? (
          <>
            <div className="budget-row quiet">
              <span className="budget-label">allocated</span>
              <span className="budget-amount">{money(overview.allocated)}</span>
              <span className="budget-actions" />
            </div>
            <div className="budget-row quiet">
              <span className="budget-label">unallocated</span>
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
        {categories && categories.length === 0 ? <p>no categories.</p> : null}
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
          aria-label={`${label} amount`}
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
              save
            </button>
            {value !== '' ? (
              <button key="clear" type="button" onClick={() => void save('')}>
                clear
              </button>
            ) : null}
            <button key="cancel" type="button" onClick={() => setEditing(false)}>
              cancel
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
            edit
          </button>
        )}
      </span>
    </form>
  )
}
