import { useEffect, useRef, useState, type FormEvent, type ReactNode } from 'react'
import AddCategoryModal from '../components/AddCategoryModal'
import Dropdown from '../components/Dropdown'
import Modal, { modalFormId } from '../components/Modal'
import DateScroll from '../DateScroll'
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
type RangeMode = 'date' | 'month' | 'year'
type MenuName = 'range' | 'show' | 'category'
type Filters = {
  mode: RangeMode
  from: string
  to: string
  categoryId: string
  q: string
}

export default function TransactionsPage() {
  const { settings, error } = useMoneySettings()
  const [categories, setCategories] = useState<Category[]>([])
  const [categoriesReady, setCategoriesReady] = useState(false)
  const [filters, setFilters] = useState<Filters>(() => {
    const month = currentMonth()
    return { mode: 'month', from: month, to: month, categoryId: '', q: '' }
  })
  const [limit, setLimit] = useState(50)
  const [offset, setOffset] = useState(0)
  const [reload, setReload] = useState(0)
  const [page, setPage] = useState<TransactionList | null>(null)
  const [menu, setMenu] = useState<MenuName | null>(null)
  const [openId, setOpenId] = useState<string | null>(null)
  const [clientUuid, setClientUuid] = useState(() => crypto.randomUUID())
  const [pending, setPending] = useState(false)
  const [adding, setAdding] = useState(false)
  const [draftMessage, setDraftMessage] = useState('')
  const [editing, setEditing] = useState<Transaction | null>(null)
  const [message, setMessage] = useState('')

  useEffect(() => {
    let cancelled = false
    void api('/api/v1/categories').then(async (response) => {
      if (cancelled) return
      if (!response.ok) return
      setCategories((await response.json()) as Category[])
      setCategoriesReady(true)
    })
    return () => {
      cancelled = true
    }
  }, [reload])

  useEffect(() => {
    let cancelled = false
    const params = new URLSearchParams()
    const from = apiBound(filters.mode, filters.from, 'from')
    const to = apiBound(filters.mode, filters.to, 'to')
    if (from) params.set('from', from)
    if (to) params.set('to', to)
    if (filters.categoryId) params.set('categoryId', filters.categoryId)
    const q = filters.q.trim()
    if (q) params.set('q', q)
    params.set('limit', String(limit))
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
  }, [filters, limit, offset, reload])

  function patchFilters(patch: Partial<Filters>) {
    setFilters((current) => ({ ...current, ...patch }))
    setOffset(0)
  }

  function onMode(mode: RangeMode) {
    setFilters((current) => ({
      ...current,
      mode,
      from: shiftBound(current.from, current.mode, mode, 'from'),
      to: shiftBound(current.to, current.mode, mode, 'to'),
    }))
    setOffset(0)
  }

  function openAdd() {
    setMenu(null)
    setDraftMessage('')
    setEditing(null)
    setAdding(true)
  }

  function openEdit(transaction: Transaction) {
    setMenu(null)
    setDraftMessage('')
    setAdding(false)
    setEditing(transaction)
  }

  function closeModal() {
    setAdding(false)
    setEditing(null)
  }

  // One handler for both modes: the add modal creates, the same modal prefilled edits.
  async function onSave(event: FormEvent<HTMLFormElement>) {
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
    const body = { name, amount, paymentDate, categoryId, description: description.length > 0 ? description : null }
    setPending(true)
    const response = editing
      ? await api(`/api/v1/transactions/${encodeURIComponent(editing.id)}`, {
          method: 'PATCH',
          body: JSON.stringify(body),
        })
      : await api('/api/v1/transactions', {
          method: 'POST',
          body: JSON.stringify({ ...body, clientUuid }),
        })
    setPending(false)
    if (!response.ok) {
      setDraftMessage(await errorMessage(response))
      return
    }
    setDraftMessage('')
    setMessage('')
    if (!editing) setClientUuid(crypto.randomUUID())
    closeModal()
    setReload((value) => value + 1)
  }

  async function onDelete(id: string) {
    const response = await api(`/api/v1/transactions/${encodeURIComponent(id)}`, { method: 'DELETE' })
    if (!response.ok) {
      setMessage(await errorMessage(response))
      return
    }
    setMessage('')
    setReload((value) => value + 1)
  }

  function scopeLabel(): string {
    if (filters.categoryId) return categories.find((category) => category.id === filters.categoryId)?.name ?? 'deleted category'
    return 'all categories'
  }

  if (error) return <p className="notice">{error}</p>
  if (!settings) return null

  // shortcut: one button per page, upgrade if lists run past a few dozen pages
  const pageCount = page && page.limit > 0 ? Math.max(1, Math.ceil(page.total / page.limit)) : 0
  const currentPage = page && page.limit > 0 ? Math.floor(page.offset / page.limit) + 1 : 1

  return (
    <section className="tx-page">
      <div className="tx-title">
        <h1>transactions</h1>
        <div className="tx-actions">
          <button type="button" onClick={openAdd}>
            add
          </button>
        </div>
      </div>
      <div className="toolbar wide tx-bar">
        <label className="grow">
          search
          <input
            value={filters.q}
            maxLength={255}
            autoComplete="off"
            onChange={(event) => patchFilters({ q: event.target.value })}
          />
        </label>
        <Pick
          label="show"
          className="limit"
          text={String(limit)}
          open={menu === 'show'}
          onToggle={() => setMenu((current) => (current === 'show' ? null : 'show'))}
          onClose={() => setMenu(null)}
        >
          {['10', '50', '100'].map((value) => (
            <PickOption
              key={value}
              selected={String(limit) === value}
              onClick={() => {
                setLimit(Number(value))
                setOffset(0)
                setMenu(null)
              }}
            >
              {value}
            </PickOption>
          ))}
        </Pick>
        <div className="tx-range">
          <Pick
            label="range"
            className="mode"
            text={filters.mode === 'date' ? 'day' : filters.mode}
            open={menu === 'range'}
            onToggle={() => setMenu((current) => (current === 'range' ? null : 'range'))}
            onClose={() => setMenu(null)}
          >
            {(['date', 'month', 'year'] as const).map((mode) => (
              <PickOption
                key={mode}
                selected={filters.mode === mode}
                onClick={() => {
                  onMode(mode)
                  setMenu(null)
                }}
              >
                {mode === 'date' ? 'day' : mode}
              </PickOption>
            ))}
          </Pick>
          <label>
            from
            <DateScroll
              key={`from-${filters.mode}`}
              kind={filters.mode}
              value={filters.from}
              onChange={(from) => patchFilters({ from })}
            />
          </label>
          <label>
            to
            <DateScroll
              key={`to-${filters.mode}`}
              kind={filters.mode}
              value={filters.to}
              onChange={(to) => patchFilters({ to })}
            />
          </label>
        </div>
        <Pick
          label="category"
          className="scope"
          text={scopeLabel()}
          open={menu === 'category'}
          onToggle={() => setMenu((current) => (current === 'category' ? null : 'category'))}
          onClose={() => setMenu(null)}
        >
          <PickOption
            selected={!filters.categoryId}
            onClick={() => {
              patchFilters({ categoryId: '' })
              setMenu(null)
            }}
          >
            all categories
          </PickOption>
          {categories.map((category) => (
            <PickOption
              key={`category-${category.id}`}
              selected={filters.categoryId === category.id}
              onClick={() => {
                patchFilters({ categoryId: category.id })
                setMenu(null)
              }}
            >
              {category.name}
            </PickOption>
          ))}
        </Pick>
      </div>
      <p className="notice">{message}</p>
      {adding || editing ? (
        <TransactionModal
          key={editing ? editing.id : clientUuid}
          transaction={editing}
          categories={categories}
          pending={pending}
          message={draftMessage}
          onSubmit={(event) => void onSave(event)}
          onClose={closeModal}
          onCategoryAdded={(category) => setCategories((current) => [...current, category])}
        />
      ) : null}
      {page ? (
        <>
          <div className="tx-pane wide">
            {page.transactions.length === 0 ? <p>no transactions.</p> : null}
            <ul className="rows">
            {page.transactions.map((transaction) => (
              <li key={transaction.id}>
                <div className="entry-row tx-row">
                  <div className="tx-main">
                    <p className="entry-main">{transaction.name}</p>
                    <p className="tx-sub">
                      {[
                        formatMoney(transaction.amount, settings.currency, settings.locale),
                        categories.find((category) => category.id === transaction.categoryId)?.name ??
                          (categoriesReady ? 'deleted category' : ''),
                        transaction.paymentDate,
                      ]
                        .filter(Boolean)
                        .join(' | ')}
                    </p>
                    {transaction.description?.trim() ? (
                      <button
                        type="button"
                        className="tx-desc"
                        aria-expanded={openId === transaction.id}
                        onClick={() => setOpenId((current) => (current === transaction.id ? null : transaction.id))}
                      >
                        {openId === transaction.id ? transaction.description : 'description'}
                      </button>
                    ) : null}
                  </div>
                  <div className="entry-actions">
                    <button type="button" className="tx-text" onClick={() => openEdit(transaction)}>
                      edit
                    </button>
                    <button type="button" className="tx-text" onClick={() => void onDelete(transaction.id)}>
                      delete
                    </button>
                  </div>
                </div>
              </li>
            ))}
            </ul>
          </div>
          <div className="page-numbers">
            {Array.from({ length: pageCount }, (_, index) => {
              const number = index + 1
              return (
                <button
                  key={number}
                  type="button"
                  aria-current={number === currentPage ? 'page' : undefined}
                  onClick={() => setOffset((number - 1) * (page?.limit ?? limit))}
                >
                  {number}
                </button>
              )
            })}
          </div>
        </>
      ) : null}
    </section>
  )
}

// Add and edit share this modal. Field order: name, amount, category, date, description.
function TransactionModal({
  transaction,
  categories,
  pending,
  message,
  onSubmit,
  onClose,
  onCategoryAdded,
}: {
  transaction: Transaction | null
  categories: Category[]
  pending: boolean
  message: string
  onSubmit: (event: FormEvent<HTMLFormElement>) => void
  onClose: () => void
  onCategoryAdded: (category: Category) => void
}) {
  const [categoryId, setCategoryId] = useState(transaction?.categoryId ?? '')
  const [addingCategory, setAddingCategory] = useState(false)
  const options = categories.map((category) => ({ value: category.id, label: category.name }))
  if (transaction && categories.every((category) => category.id !== transaction.categoryId)) {
    options.unshift({ value: transaction.categoryId, label: 'deleted category' })
  }
  return (
    <>
      <Modal title={transaction ? 'edit transaction' : 'add transaction'} pending={pending} onClose={onClose}>
        <p className="notice">{message}</p>
        <form id={modalFormId} onSubmit={onSubmit}>
          <label>
            name
            <input name="name" required maxLength={50} defaultValue={transaction?.name ?? ''} autoComplete="off" />
          </label>
          <label>
            amount
            <input
              name="amount"
              required
              inputMode="decimal"
              defaultValue={transaction?.amount ?? ''}
              autoComplete="off"
            />
          </label>
          <Dropdown
            label="category"
            name="categoryId"
            value={categoryId}
            options={options}
            onChange={setCategoryId}
            placeholder="choose a category"
            action={{ label: 'add category', onSelect: () => setAddingCategory(true) }}
          />
          <PaymentDate defaultValue={transaction?.paymentDate ?? today()} />
          <label>
            description
            <input name="description" maxLength={255} defaultValue={transaction?.description ?? ''} autoComplete="off" />
          </label>
        </form>
      </Modal>
      {addingCategory ? (
        <AddCategoryModal
          onClose={() => setAddingCategory(false)}
          onCreated={(category) => {
            onCategoryAdded(category)
            setCategoryId(category.id)
            setAddingCategory(false)
          }}
        />
      ) : null}
    </>
  )
}

function PaymentDate({ defaultValue }: { defaultValue: string }) {
  const [value, setValue] = useState(defaultValue)
  return (
    <label>
      payment date
      <DateScroll name="paymentDate" kind="date" value={value} onChange={setValue} />
    </label>
  )
}

function Pick({
  label,
  className,
  text,
  open,
  onToggle,
  onClose,
  children,
}: {
  label: string
  className?: string
  text: string
  open: boolean
  onToggle: () => void
  onClose: () => void
  children: ReactNode
}) {
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    function onPointerDown(event: PointerEvent) {
      if (!ref.current?.contains(event.target as Node)) onClose()
    }
    function onKey(event: KeyboardEvent) {
      if (event.key === 'Escape') onClose()
    }
    document.addEventListener('pointerdown', onPointerDown)
    document.addEventListener('keydown', onKey)
    return () => {
      document.removeEventListener('pointerdown', onPointerDown)
      document.removeEventListener('keydown', onKey)
    }
  }, [open, onClose])

  return (
    <div className={className ? `pick ${className}` : 'pick'} ref={ref}>
      <span>{label}</span>
      <button type="button" className="pick-button" aria-expanded={open} aria-haspopup="listbox" onClick={onToggle}>
        {text}
      </button>
      {open ? (
        <ul className="pick-list" role="listbox">
          {children}
        </ul>
      ) : null}
    </div>
  )
}

function PickOption({ selected, onClick, children }: { selected: boolean; onClick: () => void; children: string }) {
  return (
    <li>
      <button type="button" role="option" aria-selected={selected} onClick={onClick}>
        {children}
      </button>
    </li>
  )
}

function apiBound(mode: RangeMode, value: string, edge: 'from' | 'to'): string {
  if (mode === 'date') return /^\d{4}-\d{2}-\d{2}$/.test(value) ? value : ''
  if (mode === 'month') {
    if (!/^\d{4}-(0[1-9]|1[0-2])$/.test(value)) return ''
    return edge === 'from' ? `${value}-01` : lastDay(value)
  }
  if (!/^\d{4}$/.test(value)) return ''
  return edge === 'from' ? `${value}-01-01` : `${value}-12-31`
}

function shiftBound(value: string, fromMode: RangeMode, toMode: RangeMode, edge: 'from' | 'to'): string {
  if (!value || fromMode === toMode) return fromMode === toMode ? value : ''
  if (fromMode === 'date' && /^\d{4}-\d{2}-\d{2}$/.test(value)) {
    if (toMode === 'month') return value.slice(0, 7)
    return value.slice(0, 4)
  }
  if (fromMode === 'month' && /^\d{4}-(0[1-9]|1[0-2])$/.test(value)) {
    if (toMode === 'year') return value.slice(0, 4)
    return edge === 'from' ? `${value}-01` : lastDay(value)
  }
  if (fromMode === 'year' && /^\d{4}$/.test(value)) {
    if (toMode === 'month') return edge === 'from' ? `${value}-01` : `${value}-12`
    return edge === 'from' ? `${value}-01-01` : `${value}-12-31`
  }
  return ''
}

function lastDay(yearMonth: string): string {
  const year = Number(yearMonth.slice(0, 4))
  const month = Number(yearMonth.slice(5, 7))
  const day = new Date(Date.UTC(year, month, 0)).getUTCDate()
  return `${yearMonth}-${String(day).padStart(2, '0')}`
}

function currentMonth(): string {
  const now = new Date()
  return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}`
}

function today(): string {
  const now = new Date()
  const month = String(now.getMonth() + 1).padStart(2, '0')
  const day = String(now.getDate()).padStart(2, '0')
  return `${now.getFullYear()}-${month}-${day}`
}
