import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import AddCategoryModal from '../components/AddCategoryModal'
import AttachArea from '../components/AttachArea'
import BillCodeModal from '../components/BillCodeModal'
import ClipPreview from '../components/ClipPreview'
import ConfirmModal from '../components/ConfirmModal'
import Dropdown, { type DropdownOption } from '../components/Dropdown'
import Modal, { modalFormId } from '../components/Modal'
import Pick, { PickOption } from '../components/Pick'
import DatePicker from '../components/DatePicker'
import SplitEditor from '../components/SplitEditor'
import { categories as categoriesApi, type Category } from '../api/categories'
import { debts } from '../api/debts'
import { errorText } from '../api/invoke'
import { useToday, type Today } from '../api/today'
import { transactions as transactionsApi, type Transaction, type TransactionPage } from '../api/transactions'
import { categoryOptions } from '../lib/categoriesView'
import { amountString, formatDate, formatMoney } from '../lib/money'
import { apiBound, shiftBound, type RangeMode } from '../lib/range'
import { draftOfBill, equalDraft, NOT_SPLIT, toInput, type SplitState } from '../lib/split'
import { useAttachments, type Attachments } from '../hooks/useAttachments'
import { useClipCounts } from '../hooks/useClipCounts'
import { useFocusRequest, useJumpTo } from '../hooks/useFocusRequest'
import { useMoneySettings, type MoneySettings } from '../hooks/useMoneySettings'
import { useProfile } from '../hooks/profile'
import { useNotice } from '../hooks/useNotice'
import { useT } from '../i18n/context'
import { useDebounced } from '../hooks/useDebounced'
import { useShortcut } from '../keys/registry'
import { tip } from '../keys/tip'
import { useRowNavigation } from '../keys/useRowNavigation'

type TransactionList = TransactionPage
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
  const now = useToday()
  if (error) return <p className="notice">{error}</p>
  if (!settings || !now) return null
  return <TransactionsView settings={settings} now={now} />
}

function TransactionsView({ settings, now }: { settings: MoneySettings; now: Today }) {
  const t = useT()
  const phone = useProfile() === 'phone'
  const [categories, setCategories] = useState<Category[]>([])
  const [categoriesReady, setCategoriesReady] = useState(false)
  const [filters, setFilters] = useState<Filters>(() => {
    const month = now.month
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
  const [draftMessage, setDraftMessage] = useNotice()
  const [editing, setEditing] = useState<Transaction | null>(null)
  const [split, setSplit] = useState<SplitState>(NOT_SPLIT)
  const [deleting, setDeleting] = useState<Transaction | null>(null)
  // the bill whose code for one friend is on the screen
  const [coding, setCoding] = useState<{ transaction: Transaction; personId: string; name: string; part: string } | null>(null)
  const location = useLocation()
  const navigate = useNavigate()
  const [message, setMessage] = useNotice()
  const onSplitReady = useCallback((ready: boolean) => setSplit((current) => (current.ready === ready ? current : { ...current, ready })), [])
  // The documents of the transaction being edited are stored and linked at once; those of one being made wait for it to be saved.
  const attachments = useAttachments(editing ? { kind: 'transaction', id: editing.id } : null)
  const clips = useClipCounts('transaction', page?.transactions.map((transaction) => transaction.id) ?? [], reload)
  // a document can send the person here for one of its transactions: its month is shown, and the row is scrolled to and outlined
  const { request: focus, done: focusDone } = useFocusRequest('transaction')
  useJumpTo(focus, focusDone, page)
  // The list waits for a short pause in typing instead of asking the database on every key.
  const searchText = useDebounced(filters.q, 200)
  const categoryNames = useMemo(() => new Map(categories.map((category) => [category.id, category.name])), [categories])

  useEffect(() => {
    let cancelled = false
    void categoriesApi.list().then(
      (list) => {
        if (cancelled) return
        setCategories(list)
        setCategoriesReady(true)
      },
      () => undefined,
    )
    return () => {
      cancelled = true
    }
  }, [reload])

  useEffect(() => {
    let cancelled = false
    const from = apiBound(filters.mode, filters.from, 'from')
    const to = apiBound(filters.mode, filters.to, 'to')
    const q = searchText.trim()
    void transactionsApi
      .list({
        ...(from ? { from } : {}),
        ...(to ? { to } : {}),
        ...(filters.categoryId ? { categoryId: filters.categoryId } : {}),
        ...(q ? { q } : {}),
        limit,
        offset,
      })
      .then(
        (result) => {
          if (!cancelled) setPage(result)
        },
        (failure) => {
          if (!cancelled) setMessage(errorText(failure))
        },
      )
    return () => {
      cancelled = true
    }
  }, [filters.mode, filters.from, filters.to, filters.categoryId, searchText, limit, offset, reload, setMessage])

  useEffect(() => {
    if (!focus) return
    const month = focus.date.slice(0, 7)
    setFilters({ mode: 'month', from: month, to: month, categoryId: '', q: '' })
    setLimit(100)
    setOffset(0)
  }, [focus])

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

  const searchRef = useRef<HTMLInputElement>(null)

  function goCurrentMonth() {
    setFilters((current) => ({ ...current, mode: 'month', from: now.month, to: now.month }))
    setOffset(0)
  }

  /** A new transaction; `split` opens it with the bill dialog on, `friend` with that friend already on the bill. */
  function openAdd(options: { split?: boolean; friend?: { id: string; name: string } } = {}) {
    setMenu(null)
    setDraftMessage('')
    attachments.reset()
    setEditing(null)
    setSplit(options.split ? { on: true, draft: equalDraft(options.friend ? [{ personId: options.friend.id, name: options.friend.name }] : []), ready: false, touched: false } : NOT_SPLIT)
    setAdding(true)
  }

  // The debts pages send the person here to split a bill (with the friend they came from already on it).
  useEffect(() => {
    const state = location.state as { split?: boolean; friendId?: string; friendName?: string } | null
    if (!state?.split) return
    openAdd({ split: true, ...(state.friendId && state.friendName ? { friend: { id: state.friendId, name: state.friendName } } : {}) })
    navigate(location.pathname, { replace: true, state: null })
    // once, when the page opens with that request
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  function openEdit(transaction: Transaction) {
    setMenu(null)
    setDraftMessage('')
    attachments.reset()
    setAdding(false)
    // a saved bill opens as exact amounts, as they are stored
    setSplit(transaction.split.length > 0 ? { on: true, draft: draftOfBill(transaction.split), ready: true, touched: false } : NOT_SPLIT)
    setEditing(transaction)
  }

  function closeModal() {
    // documents may have been attached or removed in the dialog, so the marks on the list are asked for again
    if (adding || editing) setReload((value) => value + 1)
    setAdding(false)
    setEditing(null)
  }

  // Keyboard: n adds, / searches, t jumps to this month, j and k move between rows.
  useShortcut('new', () => openAdd())
  useShortcut('search', () => searchRef.current?.focus())
  useShortcut('today', goCurrentMonth)
  useRowNavigation()

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
      setDraftMessage(t('tx.errorRequired'))
      return
    }
    if (!amount) {
      setDraftMessage(t('tx.errorAmount'))
      return
    }
    const body = { name, amount, paymentDate, categoryId, description: description.length > 0 ? description : null }
    // A shared bill is saved when its numbers add up; one that was shared and is left alone is scaled by the app to a new amount.
    const hadSplit = (editing?.split.length ?? 0) > 0
    const wantsSplit = split.on && categories.find((category) => category.id === categoryId)?.type !== 'INCOME'
    const sendSplit = wantsSplit && (split.touched || !hadSplit)
    const input = sendSplit ? toInput(split.draft) : null
    if (sendSplit && (!split.ready || !input)) {
      setDraftMessage(t('split.notComplete'))
      return
    }
    setPending(true)
    // what could not be attached to a new transaction: it is saved all the same, and the notice names the files
    let notAttached: string[] = []
    try {
      if (editing) {
        await transactionsApi.update(editing.id, body)
        if (input) await debts.split(editing.id, input)
        else if (hadSplit && !wantsSplit) await debts.unsplit(editing.id)
      } else {
        // creating again with the same client id gives back the same transaction, so a failed split can be tried again
        const created = await transactionsApi.create({ ...body, clientUuid })
        if (input) await debts.split(created.id, input)
        notAttached = await attachments.commit({ kind: 'transaction', id: created.id })
      }
    } catch (failure) {
      setDraftMessage(errorText(failure))
      return
    } finally {
      setPending(false)
    }
    setDraftMessage('')
    setMessage(notAttached.length > 0 ? t('docs.attachFailed', { names: notAttached.join(', ') }) : '')
    if (!editing) setClientUuid(crypto.randomUUID())
    closeModal()
    setReload((value) => value + 1)
  }

  async function onDelete(id: string) {
    try {
      await transactionsApi.remove(id)
    } catch (failure) {
      setMessage(errorText(failure))
      return
    } finally {
      setDeleting(null)
    }
    setMessage('')
    setReload((value) => value + 1)
  }

  function scopeLabel(): string {
    if (filters.categoryId) return categoryNames.get(filters.categoryId) ?? t('tx.deletedCategory')
    return t('tx.allCategories')
  }

  const modeLabel = (mode: RangeMode) => (mode === 'date' ? t('common.day') : mode === 'month' ? t('common.month') : t('common.year'))

  // shortcut: one button per page, upgrade if lists run past a few dozen pages
  const pageCount = page && page.limit > 0 ? Math.max(1, Math.ceil(page.total / page.limit)) : 0
  const currentPage = page && page.limit > 0 ? Math.floor(page.offset / page.limit) + 1 : 1

  // The current-month button and the range: beside "add" on a phone, where room is short, and first in the filters elsewhere.
  const rangeControls = (
    <>
      <button
        type="button"
        disabled={filters.mode === 'month' && filters.from === now.month && filters.to === now.month}
        onClick={goCurrentMonth}
        {...tip('today', t('tx.currentMonth'))}
      >
        {t('tx.currentMonth')}
      </button>
      <Pick
        label={t('tx.range')}
        className="mode"
        inline={phone}
        text={modeLabel(filters.mode)}
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
            {modeLabel(mode)}
          </PickOption>
        ))}
      </Pick>
    </>
  )

  return (
    <section className="tx-page">
      <div className="tx-title">
        <h1>{t('tx.title')}</h1>
        <div className="tx-actions">
          <button type="button" onClick={() => openAdd()} {...tip('new', t('tx.add'))}>
            {t('tx.add')}
          </button>
          {phone ? rangeControls : null}
        </div>
      </div>
      <div className="toolbar wide tx-bar">
        <label className="grow" {...tip('search')}>
          {t('common.search')}
          <input
            ref={searchRef}
            onKeyDown={(event) => {
              if (event.key === 'Escape') event.currentTarget.blur()
            }}
            value={filters.q}
            maxLength={255}
            autoComplete="off"
            onChange={(event) => patchFilters({ q: event.target.value })}
          />
        </label>
        <Pick
          label={t('tx.show')}
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
        <Pick
          label={t('common.category')}
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
            {t('tx.allCategories')}
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
        <div className="tx-range" data-mode={filters.mode}>
          {phone ? null : rangeControls}
          <div className="tx-date-group">
            <span>{t('tx.from')}</span>
            <DatePicker
              key={`from-${filters.mode}`}
              kind={filters.mode}
              value={filters.from}
              label={t('tx.from')}
              today={now.date}
              onChange={(from) => patchFilters({ from })}
            />
          </div>
          <div className="tx-date-group">
            <span>{t('tx.to')}</span>
            <DatePicker
              key={`to-${filters.mode}`}
              kind={filters.mode}
              value={filters.to}
              label={t('tx.to')}
              today={now.date}
              onChange={(to) => patchFilters({ to })}
            />
          </div>
        </div>
      </div>
      <p className="notice">{message}</p>
      {adding || editing ? (
        <TransactionModal
          key={editing ? editing.id : clientUuid}
          transaction={editing}
          todayDate={now.date}
          categories={categories}
          pending={pending}
          message={draftMessage}
          attachments={attachments}
          split={split}
          onSplit={setSplit}
          onSplitReady={onSplitReady}
          onSubmit={(event) => void onSave(event)}
          onClose={closeModal}
          onCategoryAdded={(category) => setCategories((current) => [...current, category])}
        />
      ) : null}
      {coding ? (
        <BillCodeModal
          transactionId={coding.transaction.id}
          person={{ id: coding.personId, name: coding.name }}
          part={coding.part}
          total={coding.transaction.amount}
          onClose={() => setCoding(null)}
        />
      ) : null}
      {deleting ? (
        <ConfirmModal title={t('rec.confirmTitle', { action: t('common.delete'), name: deleting.name })} onConfirm={() => void onDelete(deleting.id)} onClose={() => setDeleting(null)} />
      ) : null}
      {page ? (
        <>
          <div className="tx-pane wide">
            {page.transactions.length === 0 ? <p>{t('tx.empty')}</p> : null}
            <ul className="card-list">
              {page.transactions.map((transaction) => (
                <li key={transaction.id} className="card-item" data-item={`transaction:${transaction.id}`}>
                  <div className="card-body">
                    <p className="card-name">{transaction.name}</p>
                    <p className="card-meta">
                      <span className="card-amount">{formatMoney(transaction.myShare, settings.currency, settings.locale)}</span>
                      {[
                        categoryNames.get(transaction.categoryId) ?? (categoriesReady ? t('tx.deletedCategory') : ''),
                        formatDate(transaction.paymentDate, settings.locale),
                      ]
                        .filter(Boolean)
                        .map((part) => ` \u00b7 ${part}`)
                        .join('')}
                      {clips[transaction.id] ? (
                        <>
                          {' \u00b7 '}
                          <ClipPreview inline target={{ kind: 'transaction', id: transaction.id }} name={transaction.name} count={clips[transaction.id] ?? 0} />
                        </>
                      ) : null}
                    </p>
                    {transaction.split.length > 0 || transaction.description?.trim() ? (
                      <button
                        type="button"
                        className="tx-desc"
                        aria-expanded={openId === transaction.id}
                        onClick={() => setOpenId((current) => (current === transaction.id ? null : transaction.id))}
                      >
                        {t('common.details')}
                      </button>
                    ) : null}
                    {openId === transaction.id ? (
                      <div className="card-detail">
                        {transaction.split.length > 0 ? (
                          <section className="detail-section">
                            <p className="detail-title">{t('split.section')}</p>
                            <p>{t('split.paid', { amount: formatMoney(transaction.amount, settings.currency, settings.locale) })}</p>
                            <div className="split-part">
                              <span>{`${t('split.you')} ${formatMoney(transaction.myShare, settings.currency, settings.locale)}`}</span>
                            </div>
                            {transaction.split.map((part) => (
                              <div className="split-part" key={`${part.personId ?? 'guest'}-${part.person ?? part.guest}`}>
                                <span>{`${part.person ?? part.guest ?? ''} ${formatMoney(part.amount, settings.currency, settings.locale)}`}</span>
                                {part.personId && part.person ? (
                                  <button type="button" onClick={() => setCoding({ transaction, personId: part.personId ?? '', name: part.person ?? '', part: part.amount })}>
                                    {t('code.show')}
                                  </button>
                                ) : null}
                              </div>
                            ))}
                          </section>
                        ) : null}
                        {transaction.description?.trim() ? (
                          <section className="detail-section">
                            <p className="detail-title">{t('common.description')}</p>
                            <p>{transaction.description}</p>
                          </section>
                        ) : null}
                      </div>
                    ) : null}
                  </div>
                  <div className="card-actions">
                    <button type="button" data-row-focus onClick={() => openEdit(transaction)}>
                      {t('common.edit')}
                    </button>
                    <button type="button" onClick={() => setDeleting(transaction)}>
                      {t('common.delete')}
                    </button>
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
  todayDate,
  categories,
  pending,
  message,
  attachments,
  split,
  onSplit,
  onSplitReady,
  onSubmit,
  onClose,
  onCategoryAdded,
}: {
  transaction: Transaction | null
  todayDate: string
  categories: Category[]
  pending: boolean
  message: string
  attachments: Attachments
  split: SplitState
  onSplit: (split: SplitState) => void
  onSplitReady: (ready: boolean) => void
  onSubmit: (event: FormEvent<HTMLFormElement>) => void
  onClose: () => void
  onCategoryAdded: (category: Category) => void
}) {
  const t = useT()
  const [categoryId, setCategoryId] = useState(transaction?.categoryId ?? '')
  const [amount, setAmount] = useState(transaction?.amount ?? '')
  const [addingCategory, setAddingCategory] = useState(false)
  // The documents area is behind a switch, as the split is. It is open when the transaction already has documents (they are listed
  // as soon as they are loaded) or files were chosen for a new one, and then it cannot be closed on them.
  const [docsOn, setDocsOn] = useState(false)
  const hasDocuments = attachments.linked.length > 0 || attachments.pending.length > 0 || attachments.working !== ''
  const showDocuments = docsOn || hasDocuments
  // only an expense can be shared
  const canSplit = categories.find((category) => category.id === categoryId)?.type !== 'INCOME'
  const hadSplit = (transaction?.split.length ?? 0) > 0
  const rescaled = hadSplit && !split.touched && amountString(amount, false) !== transaction?.amount
  const options: DropdownOption[] = categoryOptions(categories, { expense: t('common.expense'), income: t('common.income') })
  if (transaction && categories.every((category) => category.id !== transaction.categoryId)) {
    options.unshift({ value: transaction.categoryId, label: t('tx.deletedCategory') })
  }
  return (
    <>
      <Modal title={transaction ? t('tx.editTitle') : t('tx.addTitle')} pending={pending} onClose={onClose}>
        <p className="notice">{message}</p>
        <form id={modalFormId} onSubmit={onSubmit}>
          <label>
            {t('common.name')}
            <input name="name" required maxLength={50} defaultValue={transaction?.name ?? ''} autoComplete="off" />
          </label>
          <label>
            {t('tx.amount')}
            <input
              name="amount"
              required
              inputMode="decimal"
              value={amount}
              onChange={(event) => setAmount(event.target.value)}
              autoComplete="off"
            />
          </label>
          <Dropdown
            label={t('common.category')}
            name="categoryId"
            value={categoryId}
            options={options}
            onChange={setCategoryId}
            placeholder={t('tx.chooseCategory')}
            action={{ label: t('categories.add'), onSelect: () => setAddingCategory(true) }}
          />
          <PaymentDate defaultValue={transaction?.paymentDate ?? todayDate} />
          {canSplit ? (
            <label>
              <input type="checkbox" checked={split.on} onChange={(event) => onSplit({ ...split, on: event.target.checked })} />
              {t('split.switch')}
            </label>
          ) : split.on ? (
            <p className="hint">{t('split.onlyExpenses')}</p>
          ) : null}
          {canSplit && split.on ? (
            rescaled ? (
              <p className="hint">{t('split.rescaled')}</p>
            ) : (
              <SplitEditor
                amount={amount}
                draft={split.draft}
                onChange={(draft) => onSplit({ ...split, draft, touched: true })}
                onReady={onSplitReady}
              />
            )
          ) : null}
          <label>
            {t('common.description')}
            <input name="description" maxLength={255} placeholder={t('common.optional')} defaultValue={transaction?.description ?? ''} autoComplete="off" />
          </label>
          <label>
            <input type="checkbox" checked={showDocuments} disabled={hasDocuments} onChange={(event) => setDocsOn(event.target.checked)} />
            {t('docs.attachSwitch')}
          </label>
        </form>
        {showDocuments ? <AttachArea attachments={attachments} /> : null}
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
  const t = useT()
  const [value, setValue] = useState(defaultValue)
  return (
    <div className="tx-date-group">
      <span>{t('tx.paymentDate')}</span>
      <DatePicker
        name="paymentDate"
        kind="date"
        value={value}
        label={t('tx.paymentDate')}
        today={defaultValue}
        onChange={setValue}
      />
    </div>
  )
}
