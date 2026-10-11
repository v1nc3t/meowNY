import { useEffect, useState, type FormEvent } from 'react'
import Modal, { modalFormId } from '../components/Modal'
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
  const [draft, setDraft] = useState<Category | null>(null)
  const [draftMessage, setDraftMessage] = useState('')

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

  function limitFrom(event: FormEvent<HTMLFormElement>, report: (text: string) => void): string | null | undefined {
    const raw = String(new FormData(event.currentTarget).get('amount') ?? '').trim()
    if (raw === '') return null
    const amount = amountString(raw, true)
    if (!amount) {
      report('enter an amount with at most two decimals, or leave it blank to clear.')
      return undefined
    }
    return amount
  }

  async function onGlobal(event: FormEvent<HTMLFormElement>, type: 'EXPENSE' | 'INCOME') {
    event.preventDefault()
    const amountLimit = limitFrom(event, setMessage)
    if (amountLimit === undefined) return
    setMessage((await save({ type, scope: 'GLOBAL', amountLimit })) ?? '')
  }

  async function onCategory(event: FormEvent<HTMLFormElement>, category: Category) {
    event.preventDefault()
    const amountLimit = limitFrom(event, setDraftMessage)
    if (amountLimit === undefined) return
    const errorText = await save({
      type: category.type,
      scope: 'CATEGORY',
      amountLimit,
      categoryId: category.id,
    })
    if (errorText) {
      setDraftMessage(errorText)
      return
    }
    setDraft(null)
    setDraftMessage('')
  }

  if (error) return <p className="notice">{error}</p>
  if (!settings) return null

  const money = (amount: string | null) =>
    amount === null ? 'not set' : formatMoney(amount, settings.currency, settings.locale)
  const rows = ['EXPENSE', 'INCOME'].flatMap((type) => (overview ?? []).filter((row) => row.type === type))

  return (
    <section>
      <h1>budgets</h1>
      <p className="notice">{message}</p>
      <label>
        month
        <input type="month" value={month} required onChange={(event) => setMonth(event.target.value)} />
      </label>
      {rows.map((row) => (
        <section key={row.type}>
          <h2>{row.type === 'INCOME' ? 'income' : 'expense'}</h2>
          <p>budgeted {money(row.budgeted)}</p>
          <p>allocated {money(row.allocated)}</p>
          <p>unallocated {money(row.unallocated)}</p>
          {row.overAllocated && row.budgeted !== null ? (
            <p className="warn">category budgets exceed the global budget.</p>
          ) : null}
        </section>
      ))}
      <form onSubmit={(event) => void onGlobal(event, 'EXPENSE')}>
        <h2>global expense budget</h2>
        <label>
          amount
          <input
            name="amount"
            inputMode="decimal"
            autoComplete="off"
            key={`EXPENSE:${month}:${overview?.find((row) => row.type === 'EXPENSE')?.budgeted ?? ''}`}
            defaultValue={overview?.find((row) => row.type === 'EXPENSE')?.budgeted ?? ''}
          />
        </label>
        <p>blank clears the budget.</p>
        <button type="submit">save expense budget</button>
      </form>
      <form onSubmit={(event) => void onGlobal(event, 'INCOME')}>
        <h2>global income budget</h2>
        <label>
          amount
          <input
            name="amount"
            inputMode="decimal"
            autoComplete="off"
            key={`INCOME:${month}:${overview?.find((row) => row.type === 'INCOME')?.budgeted ?? ''}`}
            defaultValue={overview?.find((row) => row.type === 'INCOME')?.budgeted ?? ''}
          />
        </label>
        <p>blank clears the budget.</p>
        <button type="submit">save income budget</button>
      </form>
      {/* shortcut: no budget list endpoint, category fields start empty after a reload or month change */}
      <div className="wide" key={month}>
        {(['EXPENSE', 'INCOME'] as const).map((type) => (
          <section className="wide" key={type}>
            <h2>{type === 'EXPENSE' ? 'expense categories' : 'income categories'}</h2>
            <ul className="rows">
              {(categories ?? [])
                .filter((category) => category.type === type)
                .map((category) => (
                  <li key={category.id}>
                    <div className="entry-row">
                      <p className="entry-main">{category.name}</p>
                      <div className="entry-actions">
                        <button
                          type="button"
                          onClick={() => {
                            setDraftMessage('')
                            setDraft(category)
                          }}
                        >
                          set budget
                        </button>
                      </div>
                    </div>
                  </li>
                ))}
            </ul>
          </section>
        ))}
      </div>
      {draft ? (
        <Modal
          title="set budget"
          onClose={() => {
            setDraft(null)
            setDraftMessage('')
          }}
        >
          <p className="notice">{draftMessage}</p>
          <form id={modalFormId} onSubmit={(event) => void onCategory(event, draft)}>
            <p>{draft.name}</p>
            <label>
              amount
              <input name="amount" inputMode="decimal" autoComplete="off" />
            </label>
            <p>blank clears the budget.</p>
          </form>
        </Modal>
      ) : null}
    </section>
  )
}

function thisMonth(): string {
  const now = new Date()
  return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}`
}
