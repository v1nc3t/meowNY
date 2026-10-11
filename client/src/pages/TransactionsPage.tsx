import { useEffect, useState, type FormEvent } from 'react'
import Modal from '../components/Modal'
import { api, errorMessage } from '../http'
import { amountString, formatMoney } from '../money'
import { useMoneySettings } from '../useMoneySettings'

type Category = { id: string; name: string; type: string }
type Transaction = {
  id: string
  categoryId: string
  name: string
  amount: string
  paymentDate: string
  description: string | null
}
type TransactionList = { transactions: Transaction[]; total: number; limit: number; offset: number }
type Filters = { from: string; to: string; categoryId: string; q: string }

const pageLimit = 20

export default function TransactionsPage() {
  const { settings, error } = useMoneySettings()
  const [categories, setCategories] = useState<Category[]>([])
  const [filters, setFilters] = useState<Filters>({ from: '', to: '', categoryId: '', q: '' })
  const [offset, setOffset] = useState(0)
  const [reload, setReload] = useState(0)
  const [page, setPage] = useState<TransactionList | null>(null)
  const [clientUuid, setClientUuid] = useState(() => crypto.randomUUID())
  const [pending, setPending] = useState(false)
  const [adding, setAdding] = useState(false)
  const [draftMessage, setDraftMessage] = useState('')
  const [editing, setEditing] = useState<string | null>(null)
  const [message, setMessage] = useState('')

  useEffect(() => {
    let cancelled = false
    void api('/api/v1/categories').then(async (response) => {
      if (cancelled || !response.ok) return
      setCategories((await response.json()) as Category[])
    })
    return () => {
      cancelled = true
    }
  }, [reload])

  useEffect(() => {
    let cancelled = false
    const params = new URLSearchParams()
    if (filters.from) params.set('from', filters.from)
    if (filters.to) params.set('to', filters.to)
    if (filters.categoryId) params.set('categoryId', filters.categoryId)
    if (filters.q) params.set('q', filters.q)
    params.set('limit', String(pageLimit))
    params.set('offset', String(offset))
    void api(`/api/v1/transactions?${params}`).then(async (response) => {
      if (cancelled) return
      if (!response.ok) {
        setMessage(await errorMessage(response))
        return
      }
      setPage((await response.json()) as TransactionList)
    })
    return () => {
      cancelled = true
    }
  }, [filters, offset, reload])

  function onFilter(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = new FormData(event.currentTarget)
    setFilters({
      from: String(form.get('from') ?? ''),
      to: String(form.get('to') ?? ''),
      categoryId: String(form.get('categoryId') ?? ''),
      q: String(form.get('q') ?? '').trim(),
    })
    setOffset(0)
    setEditing(null)
  }

  async function onCreate(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (pending) return
    const form = new FormData(event.currentTarget)
    const name = String(form.get('name') ?? '').trim()
    const amount = amountString(String(form.get('amount') ?? ''), false)
    const paymentDate = String(form.get('paymentDate') ?? '')
    const categoryId = String(form.get('categoryId') ?? '')
    const description = String(form.get('description') ?? '').trim()
    if (!name || !paymentDate || !categoryId) {
      setDraftMessage('name, category, and payment date are required.')
      return
    }
    if (!amount) {
      setDraftMessage('enter an amount greater than zero, with at most two decimals.')
      return
    }
    setPending(true)
    const response = await api('/api/v1/transactions', {
      method: 'POST',
      body: JSON.stringify({
        name,
        amount,
        paymentDate,
        categoryId,
        description: description.length > 0 ? description : null,
        clientUuid,
      }),
    })
    setPending(false)
    if (!response.ok) {
      setDraftMessage(await errorMessage(response))
      return
    }
    setDraftMessage('')
    setMessage('')
    setAdding(false)
    setClientUuid(crypto.randomUUID())
    setReload((value) => value + 1)
  }

  async function onEdit(event: FormEvent<HTMLFormElement>, id: string) {
    event.preventDefault()
    const form = new FormData(event.currentTarget)
    const name = String(form.get('name') ?? '').trim()
    const amount = amountString(String(form.get('amount') ?? ''), false)
    const paymentDate = String(form.get('paymentDate') ?? '')
    const categoryId = String(form.get('categoryId') ?? '')
    const description = String(form.get('description') ?? '').trim()
    if (!name || !paymentDate || !categoryId) {
      setMessage('name, category, and payment date are required.')
      return
    }
    if (!amount) {
      setMessage('enter an amount greater than zero, with at most two decimals.')
      return
    }
    const response = await api(`/api/v1/transactions/${encodeURIComponent(id)}`, {
      method: 'PATCH',
      body: JSON.stringify({
        name,
        amount,
        paymentDate,
        categoryId,
        description: description.length > 0 ? description : null,
      }),
    })
    if (!response.ok) {
      setMessage(await errorMessage(response))
      return
    }
    setEditing(null)
    setMessage('')
    setReload((value) => value + 1)
  }

  async function onDelete(id: string) {
    const response = await api(`/api/v1/transactions/${encodeURIComponent(id)}`, { method: 'DELETE' })
    if (!response.ok) {
      setMessage(await errorMessage(response))
      return
    }
    if (editing === id) setEditing(null)
    setMessage('')
    setReload((value) => value + 1)
  }

  function categoryName(id: string): string {
    return categories.find((category) => category.id === id)?.name ?? 'deleted category'
  }

  if (error) return <p className="notice">{error}</p>
  if (!settings) return null

  return (
    <section>
      <h1>transactions</h1>
      <div className="page-actions">
        <button
          type="button"
          onClick={() => {
            setDraftMessage('')
            setAdding(true)
          }}
        >
          add transaction
        </button>
      </div>
      <p className="notice">{message}</p>
      {adding ? (
        <Modal title="add transaction" onClose={() => setAdding(false)}>
          <p className="notice">{draftMessage}</p>
          <form key={clientUuid} onSubmit={(event) => void onCreate(event)}>
            <label>
              category
              <select name="categoryId" required defaultValue="">
                <option value="" disabled>
                  choose a category
                </option>
                {categories.map((category) => (
                  <option key={category.id} value={category.id}>
                    {category.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              name
              <input name="name" required maxLength={50} autoComplete="off" />
            </label>
            <label>
              amount
              <input name="amount" required inputMode="decimal" autoComplete="off" />
            </label>
            <label>
              payment date
              <input name="paymentDate" type="date" required defaultValue={today()} />
            </label>
            <label>
              description
              <input name="description" maxLength={255} autoComplete="off" />
            </label>
            <button type="submit" disabled={pending}>
              add transaction
            </button>
          </form>
        </Modal>
      ) : null}
      <form onSubmit={onFilter}>
        <h2>filter</h2>
        <label>
          from
          <input name="from" type="date" />
        </label>
        <label>
          to
          <input name="to" type="date" />
        </label>
        <label>
          category
          <select name="categoryId" defaultValue="">
            <option value="">all categories</option>
            {categories.map((category) => (
              <option key={category.id} value={category.id}>
                {category.name}
              </option>
            ))}
          </select>
        </label>
        <label>
          text
          <input name="q" maxLength={255} autoComplete="off" />
        </label>
        <button type="submit">apply</button>
      </form>
      <h2>list</h2>
      {page ? (
        <>
          <p>
            {page.total === 0
              ? 'no transactions.'
              : `${page.offset + 1}–${page.offset + page.transactions.length} of ${page.total}`}
          </p>
          <ul className="rows">
            {page.transactions.map((transaction) => (
              <li key={transaction.id}>
                {editing === transaction.id ? (
                  <form onSubmit={(event) => void onEdit(event, transaction.id)}>
                    <label>
                      category
                      <select name="categoryId" required defaultValue={transaction.categoryId}>
                        {categories.every((category) => category.id !== transaction.categoryId) ? (
                          <option value={transaction.categoryId}>deleted category</option>
                        ) : null}
                        {categories.map((category) => (
                          <option key={category.id} value={category.id}>
                            {category.name}
                          </option>
                        ))}
                      </select>
                    </label>
                    <label>
                      name
                      <input name="name" required maxLength={50} defaultValue={transaction.name} autoComplete="off" />
                    </label>
                    <label>
                      amount
                      <input
                        name="amount"
                        required
                        inputMode="decimal"
                        defaultValue={transaction.amount}
                        autoComplete="off"
                      />
                    </label>
                    <label>
                      payment date
                      <input name="paymentDate" type="date" required defaultValue={transaction.paymentDate} />
                    </label>
                    <label>
                      description
                      <input
                        name="description"
                        maxLength={255}
                        defaultValue={transaction.description ?? ''}
                        autoComplete="off"
                      />
                    </label>
                    <div className="entry-actions">
                      <button type="submit">save</button>
                      <button type="button" onClick={() => setEditing(null)}>
                        cancel
                      </button>
                    </div>
                  </form>
                ) : (
                  <div className="entry-row">
                    <p className="entry-main">
                      {transaction.name} {formatMoney(transaction.amount, settings.currency, settings.locale)}
                      <span className="entry-meta">
                        {transaction.paymentDate} {categoryName(transaction.categoryId)}
                        {transaction.description ? ` ${transaction.description}` : ''}
                      </span>
                    </p>
                    <div className="entry-actions">
                      <button type="button" onClick={() => setEditing(transaction.id)}>
                        edit
                      </button>
                      <button type="button" onClick={() => void onDelete(transaction.id)}>
                        delete
                      </button>
                    </div>
                  </div>
                )}
              </li>
            ))}
          </ul>
          <div className="page-actions">
            <button type="button" disabled={page.offset === 0} onClick={() => setOffset(Math.max(0, page.offset - page.limit))}>
              previous
            </button>
            <button
              type="button"
              disabled={page.offset + page.limit >= page.total}
              onClick={() => setOffset(page.offset + page.limit)}
            >
              next
            </button>
          </div>
        </>
      ) : null}
    </section>
  )
}

function today(): string {
  const now = new Date()
  const month = String(now.getMonth() + 1).padStart(2, '0')
  const day = String(now.getDate()).padStart(2, '0')
  return `${now.getFullYear()}-${month}-${day}`
}
