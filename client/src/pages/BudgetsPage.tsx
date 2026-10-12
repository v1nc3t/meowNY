import { useEffect, useState, type FormEvent } from 'react'
import DateScroll from '../DateScroll'
import { api, errorMessage } from '../http'
import { amountString, formatMoney } from '../money'
import { useMoneySettings } from '../useMoneySettings'

type Category = { id: string; name: string; type: string }
type Side = 'EXPENSE' | 'INCOME'
type Overview = {
  type: string
  budgeted: string | null
  allocated: string | null
  unallocated: string | null
  overAllocated: boolean
  categories?: { categoryId: string; amountLimit: string }[]
}

const monthPattern = /^\d{4}-(?:0[1-9]|1[0-2])$/

export default function BudgetsPage() {
  const { settings, error } = useMoneySettings()
  const [month, setMonth] = useState(() => thisMonth())
  const [overview, setOverview] = useState<Overview[] | null>(null)
  const [categories, setCategories] = useState<Category[] | null>(null)
  const [reload, setReload] = useState(0)
  const [message, setMessage] = useState('')

  useEffect(() => {
    let cancelled = false
    void api('/api/v1/categories').then(async (response) => {
      if (cancelled) return
      if (!response.ok) {
        setMessage(await errorMessage(response))
        setCategories([])
        return
      }
      setCategories((await response.json()) as Category[])
    })
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
    void api(`/api/v1/budgets/overview?month=${month}`).then(async (response) => {
      if (cancelled) return
      if (!response.ok) {
        setMessage(await errorMessage(response))
        setOverview([])
        return
      }
      setOverview((await response.json()) as Overview[])
    })
    return () => {
      cancelled = true
    }
  }, [month, reload])

  async function save(body: {
    type: string
    scope: 'GLOBAL' | 'CATEGORY'
    amountLimit: string | null
    categoryId?: string
  }): Promise<string | null> {
    if (!monthPattern.test(month)) return 'choose a month.'
    const response = await api('/api/v1/budgets', {
      method: 'PUT',
      body: JSON.stringify({ ...body, month }),
    })
    if (!response.ok) return errorMessage(response)
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

  if (error) return <p className="notice">{error}</p>
  if (!settings) return null

  const money = (amount: string | null) =>
    amount === null ? 'not set' : formatMoney(amount, settings.currency, settings.locale)
  const expenseCategories = categories?.filter((category) => category.type === 'EXPENSE') ?? null
  const incomeCategories = categories?.filter((category) => category.type === 'INCOME') ?? null

  return (
    <section className="tx-page budget-page">
      <div className="tx-title">
        <h1>budgets</h1>
        <label className="budget-date">
          date
          <DateScroll announce="date" kind="month" value={month} onChange={setMonth} />
        </label>
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
    </section>
  )
}

function thisMonth(): string {
  const now = new Date()
  return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}`
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
  money: (amount: string | null) => string
  onGlobal: (type: Side, raw: string) => Promise<string | null>
  onCategory: (category: Category, raw: string) => Promise<string | null>
}) {
  const budgeted = overview?.budgeted ?? ''
  const saveLabel = type === 'INCOME' ? 'income' : 'expense'
  return (
    <section className="budget-half">
      <h2>{title}</h2>
      {overview ? (
        <>
          <p>budgeted {money(overview.budgeted)}</p>
          <p>allocated {money(overview.allocated)}</p>
          <p>unallocated {money(overview.unallocated)}</p>
          {overview.overAllocated && overview.budgeted !== null ? (
            <p className="warn">category budgets exceed the global budget.</p>
          ) : null}
        </>
      ) : null}
      <h2>global {saveLabel} budget</h2>
      <AmountRow
        key={`${type}:${month}:${budgeted}`}
        label={`global ${saveLabel} amount`}
        value={budgeted}
        onSave={(raw) => onGlobal(type, raw)}
      />
      <p>blank clears the budget.</p>
      {categories ? (
        <ul className="rows" key={month}>
          {categories.map((category) => (
            <BudgetCategory
              key={category.id}
              category={category}
              amount={amounts.get(category.id) ?? ''}
              onSave={onCategory}
            />
          ))}
        </ul>
      ) : null}
      {categories && categories.length === 0 ? <p>no categories.</p> : null}
    </section>
  )
}

function BudgetCategory({
  category,
  amount,
  onSave,
}: {
  category: Category
  amount: string
  onSave: (category: Category, raw: string) => Promise<string | null>
}) {
  return (
    <li>
      <div className="entry-row budget-line">
        <p className="entry-main">{category.name}</p>
        <AmountRow label={`${category.name} amount`} value={amount} onSave={(raw) => onSave(category, raw)} />
      </div>
    </li>
  )
}

function AmountRow({
  value,
  label,
  onSave,
}: {
  value: string
  label: string
  onSave: (raw: string) => Promise<string | null>
}) {
  const [editing, setEditing] = useState(false)
  const [draft, setDraft] = useState(value)

  useEffect(() => {
    if (!editing) setDraft(value)
  }, [value, editing])

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const errorText = await onSave(draft)
    if (!errorText) setEditing(false)
  }

  return (
    <form className="amount-row" onSubmit={(event) => void onSubmit(event)}>
      <input
        aria-label={label}
        inputMode="decimal"
        autoComplete="off"
        readOnly={!editing}
        value={draft}
        onChange={(event) => setDraft(event.target.value)}
      />
      {editing ? (
        <button type="submit">save</button>
      ) : (
        <button type="button" onClick={() => setEditing(true)}>
          edit
        </button>
      )}
    </form>
  )
}
