import { useEffect, useState, type FormEvent } from 'react'
import { api, errorMessage } from '../http'
import { amountString, formatMoney } from '../money'
import { useMoneySettings } from '../useMoneySettings'

type Category = { id: string; name: string; type: string }
type Overview = {
  type: string
  budgeted: string | null
  allocated: string | null
  unallocated: string | null
  overAllocated: boolean
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
  }) {
    if (!monthPattern.test(month)) {
      setMessage('Choose a month.')
      return
    }
    const response = await api('/api/v1/budgets', {
      method: 'PUT',
      body: JSON.stringify({ ...body, month }),
    })
    if (!response.ok) {
      setMessage(await errorMessage(response))
      return
    }
    setMessage('')
    setReload((value) => value + 1)
  }

  function limitFrom(event: FormEvent<HTMLFormElement>): string | null | undefined {
    const raw = String(new FormData(event.currentTarget).get('amount') ?? '').trim()
    if (raw === '') return null
    const amount = amountString(raw, true)
    if (!amount) {
      setMessage('Enter an amount with at most two decimals, or leave it blank to clear.')
      return undefined
    }
    return amount
  }

  async function onGlobal(event: FormEvent<HTMLFormElement>, type: 'EXPENSE' | 'INCOME') {
    event.preventDefault()
    const amountLimit = limitFrom(event)
    if (amountLimit === undefined) return
    await save({ type, scope: 'GLOBAL', amountLimit })
  }

  async function onCategory(event: FormEvent<HTMLFormElement>, category: Category) {
    event.preventDefault()
    const amountLimit = limitFrom(event)
    if (amountLimit === undefined) return
    await save({
      type: category.type,
      scope: 'CATEGORY',
      amountLimit,
      categoryId: category.id,
    })
  }

  if (error) return <p>{error}</p>
  if (!settings) return null

  const money = (amount: string | null) =>
    amount === null ? 'Not set' : formatMoney(amount, settings.currency, settings.locale)
  const rows = ['EXPENSE', 'INCOME'].flatMap((type) => (overview ?? []).filter((row) => row.type === type))

  return (
    <section>
      <h1>Budgets</h1>
      <p>{message}</p>
      <label>
        Month
        <input
          type="month"
          value={month}
          required
          onChange={(event) => setMonth(event.target.value)}
        />
      </label>
      {rows.map((row) => (
        <section key={row.type}>
          <h2>{row.type === 'INCOME' ? 'Income' : 'Expense'}</h2>
          <p>Budgeted {money(row.budgeted)}</p>
          <p>Allocated {money(row.allocated)}</p>
          <p>Unallocated {money(row.unallocated)}</p>
          {row.overAllocated && row.budgeted !== null ? (
            <p>Category budgets exceed the global budget.</p>
          ) : null}
        </section>
      ))}
      <form onSubmit={(event) => void onGlobal(event, 'EXPENSE')}>
        <h2>Global expense budget</h2>
        <label>
          Amount
          <input
            name="amount"
            inputMode="decimal"
            autoComplete="off"
            key={`EXPENSE:${month}:${overview?.find((row) => row.type === 'EXPENSE')?.budgeted ?? ''}`}
            defaultValue={overview?.find((row) => row.type === 'EXPENSE')?.budgeted ?? ''}
          />
        </label>
        <p>Blank clears the budget.</p>
        <button type="submit">Save expense budget</button>
      </form>
      <form onSubmit={(event) => void onGlobal(event, 'INCOME')}>
        <h2>Global income budget</h2>
        <label>
          Amount
          <input
            name="amount"
            inputMode="decimal"
            autoComplete="off"
            key={`INCOME:${month}:${overview?.find((row) => row.type === 'INCOME')?.budgeted ?? ''}`}
            defaultValue={overview?.find((row) => row.type === 'INCOME')?.budgeted ?? ''}
          />
        </label>
        <p>Blank clears the budget.</p>
        <button type="submit">Save income budget</button>
      </form>
      {/* shortcut: no budget list endpoint, category fields start empty after a reload or month change */}
      <div key={month}>
        {(['EXPENSE', 'INCOME'] as const).map((type) => (
          <section key={type}>
            <h2>{type === 'EXPENSE' ? 'Expense categories' : 'Income categories'}</h2>
            {(categories ?? [])
              .filter((category) => category.type === type)
              .map((category) => (
                <form key={category.id} onSubmit={(event) => void onCategory(event, category)}>
                  <label>
                    {category.name}
                    <input name="amount" inputMode="decimal" autoComplete="off" />
                  </label>
                  <button type="submit">Save {category.name} budget</button>
                </form>
              ))}
          </section>
        ))}
      </div>
    </section>
  )
}

function thisMonth(): string {
  const now = new Date()
  return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}`
}
