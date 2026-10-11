import { useEffect, useRef, useState, type FormEvent, type ReactNode } from 'react'
import Modal, { modalFormId } from '../components/Modal'
import DateScroll from '../DateScroll'
import { api, errorMessage } from '../http'
import { amountString, formatMoney } from '../money'
import { useMoneySettings } from '../useMoneySettings'

type Group = { id: string; name: string }
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
  groupId: string
  q: string
}

export default function TransactionsPage() {
  const { settings, error } = useMoneySettings()
  const [groups, setGroups] = useState<Group[]>([])
  const [categories, setCategories] = useState<Category[]>([])
  const [filters, setFilters] = useState<Filters>(() => {
    const month = currentMonth()
    return { mode: 'month', from: month, to: month, categoryId: '', groupId: '', q: '' }
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
  const [editing, setEditing] = useState<string | null>(null)
  const [message, setMessage] = useState('')

  useEffect(() => {
    let cancelled = false
    void Promise.all([api('/api/v1/categories'), api('/api/v1/category-groups')]).then(async ([categoriesResponse, groupsResponse]) => {
      if (cancelled) return
      if (categoriesResponse.ok) setCategories((await categoriesResponse.json()) as Category[])
      if (groupsResponse.ok) setGroups((await groupsResponse.json()) as Group[])
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
    else if (filters.groupId) params.set('groupId', filters.groupId)
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
    setEditing(null)
  }

  function onMode(mode: RangeMode) {
    setFilters((current) => ({
      ...current,
      mode,
      from: shiftBound(current.from, current.mode, mode, 'from'),
      to: shiftBound(current.to, current.mode, mode, 'to'),
    }))
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

  function scopeLabel(): string {
    if (filters.groupId) return groups.find((group) => group.id === filters.groupId)?.name ?? 'group'
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
          <button
            type="button"
            onClick={() => {
              setMenu(null)
              setDraftMessage('')
              setAdding(true)
            }}
          >
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
                setEditing(null)
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
            selected={!filters.categoryId && !filters.groupId}
            onClick={() => {
              patchFilters({ categoryId: '', groupId: '' })
              setMenu(null)
            }}
          >
            all categories
          </PickOption>
          {groups.map((group) => (
            <PickOption
              key={`group-${group.id}`}
              selected={filters.groupId === group.id}
              onClick={() => {
                patchFilters({ categoryId: '', groupId: group.id })
                setMenu(null)
              }}
            >
              {group.name}
            </PickOption>
          ))}
          {categories.map((category) => (
            <PickOption
              key={`category-${category.id}`}
              selected={filters.categoryId === category.id}
              onClick={() => {
                patchFilters({ categoryId: category.id, groupId: '' })
                setMenu(null)
              }}
            >
              {category.name}
            </PickOption>
          ))}
        </Pick>
      </div>
      <p className="notice">{message}</p>
      {adding ? (
        <Modal title="add transaction" pending={pending} onClose={() => setAdding(false)}>
          <p className="notice">{draftMessage}</p>
          <form id={modalFormId} key={clientUuid} onSubmit={(event) => void onCreate(event)}>
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
            <PaymentDate defaultValue={today()} />
            <label>
              description
              <input name="description" maxLength={255} autoComplete="off" />
            </label>
          </form>
        </Modal>
      ) : null}
      {page ? (
        <>
          <div className="tx-pane wide">
            {page.transactions.length === 0 ? <p>no transactions.</p> : null}
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
                    <PaymentDate defaultValue={transaction.paymentDate} />
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
                  <div className="entry-row tx-row">
                    <div className="tx-main">
                      <p className="entry-main">{transaction.name}</p>
                      <p className="tx-sub">
                        {formatMoney(transaction.amount, settings.currency, settings.locale)} | {transaction.paymentDate}
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
                      <button type="button" className="tx-text" onClick={() => setEditing(transaction.id)}>
                        edit
                      </button>
                      <button type="button" className="tx-text" onClick={() => void onDelete(transaction.id)}>
                        delete
                      </button>
                    </div>
                  </div>
                )}
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
                  onClick={() => {
                    setOffset((number - 1) * (page?.limit ?? limit))
                    setEditing(null)
                  }}
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
