import { useCallback, useEffect, useId, useMemo, useRef, useState, type FormEvent, type ReactNode } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import AddCategoryModal from '../components/AddCategoryModal'
import AttachArea from '../components/AttachArea'
import BillCodeModal from '../components/BillCodeModal'
import ClipPreview from '../components/ClipPreview'
import ConfirmModal from '../components/ConfirmModal'
import Dropdown, { type DropdownOption } from '../components/Dropdown'
import Modal, { modalFormId } from '../components/Modal'
import Icon from '../components/NavIcon'
import Pager from '../components/Pager'
import Pick, { PickOption } from '../components/Pick'
import { DayGroups, IconAction, Row, Table, TapRow } from '../components/RowParts'
import Button from '../components/ui/Button'
import { EmptyState } from '../components/ui/Pixel'
import DatePicker from '../components/DatePicker'
import SplitEditor from '../components/SplitEditor'
import { categories as categoriesApi, type Category } from '../api/categories'
import { debts } from '../api/debts'
import { errorText } from '../api/invoke'
import { shiftMonth } from '../api/months'
import { useToday, type Today } from '../api/today'
import { transactions as transactionsApi, type Transaction, type TransactionPage } from '../api/transactions'
import { categoryOptions } from '../lib/categoriesView'
import { amountString, formatDate, formatMoney } from '../lib/money'
import { apiBound, presentRange, shiftBound, type RangeMode } from '../lib/range'
import { recall, saveChoice, storedChoice, storedId, useRemember } from '../lib/sticky'
import { draftOfBill, equalDraft, NOT_SPLIT, toInput, type SplitState } from '../lib/split'
import { useAttachments, type Attachments } from '../hooks/useAttachments'
import { useClipCounts } from '../hooks/useClipCounts'
import { useFocusRequest, useJumpTo } from '../hooks/useFocusRequest'
import { useMoneySettings, type MoneySettings } from '../hooks/useMoneySettings'
import { useOpenRow } from '../hooks/useOpenRow'
import { useProfile } from '../hooks/profile'
import { useToast } from '../hooks/toast'
import { useT } from '../i18n/context'
import { useDebounced } from '../hooks/useDebounced'
import { useShortcut } from '../keys/registry'
import { tip } from '../keys/tip'
import { useRowNavigation } from '../keys/useRowNavigation'
import { remindersChanged } from '../lib/reminders'

type TransactionList = TransactionPage
/** A bill with parts of others, or a friend who paid it. */
const isShared = (transaction: Transaction) => transaction.split.length > 0 || transaction.paidBy !== null
type MenuName = 'range' | 'show' | 'category'
const MODES = ['year', 'month', 'date'] as const
const LIMITS = [10, 50, 100] as const
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
  // The kind of period, the category and the rows per page are remembered after a restart; the dates only while the app stays open (after a
  // restart the page opens on the present, in the kind of period that was chosen).
  const [filters, setFilters] = useState<Filters>(() => ({
    ...presentRange(storedChoice('tx.period', MODES, 'month'), now),
    categoryId: storedId('tx.category'),
    q: '',
    ...recall<Partial<Filters>>('transactions.range', () => ({})),
  }))
  const range = useMemo(() => ({ mode: filters.mode, from: filters.from, to: filters.to }), [filters.mode, filters.from, filters.to])
  useRemember('transactions.range', range)
  const [limit, setLimit] = useState(() => storedChoice('tx.limit', LIMITS, 50))
  const [offset, setOffset] = useState(0)
  const [reload, setReload] = useState(0)
  const [page, setPage] = useState<TransactionList | null>(null)
  const [menu, setMenu] = useState<MenuName | null>(null)
  const [clientUuid, setClientUuid] = useState(() => crypto.randomUUID())
  // the transaction a first try made when its bill then failed: the next try changes that one (the same client id would give back the old
  // row, with the old amount, and the bill would be shared on that)
  const [savedId, setSavedId] = useState<string | null>(null)
  const [pending, setPending] = useState(false)
  const [adding, setAdding] = useState(false)
    const [editing, setEditing] = useState<Transaction | null>(null)
  const [split, setSplit] = useState<SplitState>(NOT_SPLIT)
  const [deleting, setDeleting] = useState<Transaction | null>(null)
  // the bill whose code for one friend is on the screen
  const [coding, setCoding] = useState<{ transaction: Transaction; personId: string; name: string; part: string } | null>(null)
  const location = useLocation()
  const navigate = useNavigate()
  const say = useToast()
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
  const incomeCategories = useMemo(() => new Set(categories.filter((category) => category.type === 'INCOME').map((category) => category.id)), [categories])
  // a remembered category that has been deleted since is no longer a filter, and the list is not asked for it before that is known
  const categoryKnown = filters.categoryId === '' || categoryNames.has(filters.categoryId)
  useEffect(() => {
    if (!categoriesReady || categoryKnown) return
    saveChoice('tx.category', '')
    setFilters((current) => ({ ...current, categoryId: '' }))
  }, [categoriesReady, categoryKnown])
  // the phone keeps the period and the category behind a "filters" button
  const [filtersOpen, setFiltersOpen] = useState(false)
  const filtersId = useId()
  const rows = useOpenRow()

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
    if (!categoryKnown) return
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
          if (!cancelled) say(errorText(failure), 'error')
        },
      )
    return () => {
      cancelled = true
    }
  }, [filters.mode, filters.from, filters.to, filters.categoryId, categoryKnown, searchText, limit, offset, reload, say])

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

  // A year or a month is one value for both ends; a range keeps the first and the last day of what was shown. The current year, shown
  // by month, is this month.
  function onMode(mode: RangeMode) {
    saveChoice('tx.period', mode)
    setFilters((current) => {
      const from =
        mode === 'month' && current.mode === 'year' && current.from === now.date.slice(0, 4) ? now.month : shiftBound(current.from, current.mode, mode, 'from')
      return { ...current, mode, from, to: mode === 'date' ? shiftBound(current.to, current.mode, mode, 'to') : from }
    })
    setOffset(0)
  }

  const searchRef = useRef<HTMLInputElement>(null)

  // The current year when a year is shown, the current month otherwise; a range of dates is taken to the current month.
  function goCurrent() {
    const mode = filters.mode === 'year' ? 'year' : 'month'
    const from = mode === 'year' ? now.date.slice(0, 4) : now.month
    saveChoice('tx.period', mode)
    setFilters((current) => ({ ...current, mode, from, to: from }))
    setOffset(0)
  }

  // One step back or forward in the month or the year shown; the arrows and the [ and ] keys use it, never in a range of dates.
  function stepPeriod(delta: number) {
    setFilters((current) => {
      if (current.mode === 'date') return current
      const from = current.mode === 'month' ? shiftMonth(current.from, delta) : String(Number(current.from) + delta)
      return { ...current, from, to: from }
    })
    setOffset(0)
  }

  /** A new transaction; `split` opens it with the bill dialog on, `friend` with that friend already on the bill. */
  function openAdd(options: { split?: boolean; friend?: { id: string; name: string } } = {}) {
    setMenu(null)
    attachments.reset()
    setEditing(null)
    // a new dialog is a new transaction, whatever a cancelled one left
    setClientUuid(crypto.randomUUID())
    setSavedId(null)
    setSplit(options.split ? { on: true, draft: equalDraft(options.friend ? [{ personId: options.friend.id, name: options.friend.name }] : []), ready: false, touched: false } : NOT_SPLIT)
    setAdding(true)
  }

  // The debts pages send the person here to split a bill (with the friend they came from already on it), and the phone's round button to add one.
  useEffect(() => {
    const state = location.state as { add?: boolean; split?: boolean; friendId?: string; friendName?: string } | null
    if (!state?.split && !state?.add) return
    openAdd(state.split ? { split: true, ...(state.friendId && state.friendName ? { friend: { id: state.friendId, name: state.friendName } } : {}) } : {})
    navigate(location.pathname, { replace: true, state: null })
    // each time the page is asked, when it opens and when it is open already; the request is taken out of the location, so once
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [location.state])

  function openEdit(transaction: Transaction) {
    setMenu(null)
    attachments.reset()
    setAdding(false)
    // a saved bill opens as exact amounts, as they are stored
    setSplit(isShared(transaction) ? { on: true, draft: draftOfBill(transaction.split, transaction.paidBy), ready: true, touched: false } : NOT_SPLIT)
    setEditing(transaction)
  }

  function closeModal() {
    // documents may have been attached or removed in the dialog, so the marks on the list are asked for again
    if (adding || editing) setReload((value) => value + 1)
    setAdding(false)
    setEditing(null)
  }

  // Keyboard: n adds, / searches, [ and ] step the month or the year, t jumps to this month or year, j and k move between rows.
  useShortcut('new', () => openAdd())
  useShortcut('search', () => searchRef.current?.focus())
  useShortcut('prev', () => stepPeriod(-1), filters.mode !== 'date')
  useShortcut('next', () => stepPeriod(1), filters.mode !== 'date')
  useShortcut('today', goCurrent)
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
      say(t('tx.errorRequired'), 'error')
      return
    }
    if (!amount) {
      say(t('tx.errorAmount'), 'error')
      return
    }
    const body = { name, amount, paymentDate, categoryId, description: description.length > 0 ? description : null }
    // A shared bill is saved when its numbers add up; one that was shared and is left alone is scaled by the app to a new amount.
    const hadSplit = editing ? isShared(editing) : false
    const wantsSplit = split.on && categories.find((category) => category.id === categoryId)?.type !== 'INCOME'
    const sendSplit = wantsSplit && (split.touched || !hadSplit)
    const input = sendSplit ? toInput(split.draft) : null
    if (sendSplit && (!split.ready || !input)) {
      say(t('split.notComplete'), 'error')
      return
    }
    setPending(true)
    // what could not be attached to a new transaction: it is saved all the same, and the toast names the files
    let notAttached: string[] = []
    try {
      if (editing) {
        await transactionsApi.update(editing.id, body)
        if (input) await debts.split(editing.id, input)
        else if (hadSplit && !wantsSplit) await debts.unsplit(editing.id)
      } else {
        // creating again with the same client id gives back the same transaction, so a failed split can be tried again
        const created = savedId ? await transactionsApi.update(savedId, body) : await transactionsApi.create({ ...body, clientUuid })
        setSavedId(created.id)
        if (input) await debts.split(created.id, input)
        notAttached = await attachments.commit({ kind: 'transaction', id: created.id })
      }
    } catch (failure) {
      say(errorText(failure), 'error')
      return
    } finally {
      setPending(false)
    }
    if (notAttached.length > 0) say(t('docs.attachFailed', { names: notAttached.join(', ') }), 'warning')
    if (!editing) {
      setClientUuid(crypto.randomUUID())
      setSavedId(null)
    }
    // a shared bill changes what friends owe, and so the reminders
    if (input) remindersChanged()
    closeModal()
    setReload((value) => value + 1)
  }

  async function onDelete(id: string) {
    try {
      await transactionsApi.remove(id)
    } catch (failure) {
      say(errorText(failure), 'error')
      return
    } finally {
      setDeleting(null)
    }
    setReload((value) => value + 1)
  }

  function scopeLabel(): string {
    if (filters.categoryId) return categoryNames.get(filters.categoryId) ?? ''
    return t('tx.allCategories')
  }

  function pickCategory(categoryId: string) {
    saveChoice('tx.category', categoryId)
    patchFilters({ categoryId })
    setMenu(null)
  }

  const modeLabel = (mode: RangeMode) => (mode === 'date' ? t('tx.range') : mode === 'month' ? t('common.month') : t('common.year'))

  const pageCount = page && page.limit > 0 ? Math.max(1, Math.ceil(page.total / page.limit)) : 0
  const currentPage = page && page.limit > 0 ? Math.floor(page.offset / page.limit) + 1 : 1

  const atCurrentMonth = filters.mode === 'month' && filters.from === now.month && filters.to === now.month
  // the button for the current period is off when the year or the month shown is that one (and there is none for a range of dates)
  const currentPeriod = filters.mode === 'year' ? now.date.slice(0, 4) : now.month
  const atCurrentPeriod = filters.from === currentPeriod && filters.to === currentPeriod
  // how many filters are not as they start (this month, every category): the phone's filters button counts them
  const activeFilters = (atCurrentMonth ? 0 : 1) + (filters.categoryId ? 1 : 0)
  const categoryName = (transaction: Transaction) => categoryNames.get(transaction.categoryId) ?? (categoriesReady ? t('tx.deletedCategory') : '')

  // The period: a year, a month of a year, or exact dates (from one day to another, the end never before the start).
  const rangeControls = (
    <div className="tx-range">
      <Pick
        label={t('tx.period')}
        className="mode"
        text={modeLabel(filters.mode)}
        open={menu === 'range'}
        onToggle={() => setMenu((current) => (current === 'range' ? null : 'range'))}
        onClose={() => setMenu(null)}
      >
        {MODES.map((mode) => (
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
      {filters.mode === 'date' ? (
        <>
          <div className="tx-date-group">
            <span>{t('tx.from')}</span>
            <DatePicker
              kind="date"
              value={filters.from}
              label={t('tx.from')}
              today={now.date}
              onChange={(from) => patchFilters(from > filters.to ? { from, to: from } : { from })}
            />
          </div>
          <div className="tx-date-group">
            <span>{t('tx.to')}</span>
            <DatePicker
              kind="date"
              value={filters.to}
              label={t('tx.to')}
              today={now.date}
              onChange={(to) => patchFilters(to < filters.from ? { from: to, to } : { to })}
            />
          </div>
        </>
      ) : (
        <div className="month-nav">
          <Button
            variant="ghost"
            size="icon"
            aria-label={t(filters.mode === 'month' ? 'budgets.prevMonth' : 'dashboard.prevYear')}
            {...tip('prev')}
            onClick={() => stepPeriod(-1)}
          >
            &lsaquo;
          </Button>
          <DatePicker kind={filters.mode} value={filters.from} label={t('tx.period')} today={now.date} onChange={(value) => patchFilters({ from: value, to: value })} />
          <Button
            variant="ghost"
            size="icon"
            aria-label={t(filters.mode === 'month' ? 'budgets.nextMonth' : 'dashboard.nextYear')}
            {...tip('next')}
            onClick={() => stepPeriod(1)}
          >
            &rsaquo;
          </Button>
        </div>
      )}
      {filters.mode === 'date' ? null : (
        <Button variant="outline" disabled={atCurrentPeriod} onClick={goCurrent} {...tip('today')}>
          {t('common.current')}
        </Button>
      )}
    </div>
  )

  const categoryPick = (
    <Pick
      label={t('common.category')}
      className="scope"
      text={scopeLabel()}
      title={scopeLabel()}
      open={menu === 'category'}
      onToggle={() => setMenu((current) => (current === 'category' ? null : 'category'))}
      onClose={() => setMenu(null)}
    >
      <PickOption selected={!filters.categoryId} onClick={() => pickCategory('')}>
        {t('tx.allCategories')}
      </PickOption>
      {categories.map((category) => (
        <PickOption
          key={`category-${category.id}`}
          selected={filters.categoryId === category.id}
          onClick={() => pickCategory(category.id)}
        >
          {category.name}
        </PickOption>
      ))}
    </Pick>
  )

  // Under the list: the pages, and how many rows a page has.
  const foot =
    page && page.total > 0 ? (
      <div className="list-foot wide">
        <Pager page={currentPage} count={pageCount} onPage={(number) => setOffset((number - 1) * page.limit)} />
        <Pick
          label={t('tx.perPage')}
          className="limit per-page"
          text={String(limit)}
          open={menu === 'show'}
          onToggle={() => setMenu((current) => (current === 'show' ? null : 'show'))}
          onClose={() => setMenu(null)}
        >
          {LIMITS.map((value) => (
            <PickOption
              key={value}
              selected={limit === value}
              onClick={() => {
                saveChoice('tx.limit', value)
                setLimit(value)
                setOffset(0)
                setMenu(null)
              }}
            >
              {String(value)}
            </PickOption>
          ))}
        </Pick>
      </div>
    ) : null

  /** What the person's share was: income in green with a plus, spending in red with a minus, and nothing at all plain. */
  function amount(transaction: Transaction) {
    const text = formatMoney(transaction.myShare, settings.currency, settings.locale)
    if (Number(transaction.myShare) === 0) return <span className="cell-amount">{text}</span>
    const income = incomeCategories.has(transaction.categoryId)
    return <span className={income ? 'cell-amount is-income' : 'cell-amount is-expense'}>{income ? `+${text}` : `-${text}`}</span>
  }

  /** What a row opens to: the split, with a code for each friend on it (not for the one who paid), the description and the documents (`count` of them). */
  function details(transaction: Transaction, count: number): ReactNode {
    return (
      <>
        {isShared(transaction) ? (
          <section className="detail-section">
            <div className="split-head">
              <p className="detail-title">{t('split.section')}</p>
              <p className="split-paid">
                {transaction.paidBy
                  ? t('split.paidByFriend', { name: transaction.paidBy.name, amount: formatMoney(transaction.amount, settings.currency, settings.locale) })
                  : t('split.paid', { amount: formatMoney(transaction.amount, settings.currency, settings.locale) })}
              </p>
            </div>
            <table className="split-table">
              <tbody>
                <tr className="is-you">
                  <th scope="row">{t('split.you')}</th>
                  <td>{formatMoney(transaction.myShare, settings.currency, settings.locale)}</td>
                  <td />
                </tr>
                {transaction.split.map((part) => (
                  <tr key={`${part.personId ?? 'guest'}-${part.person ?? part.guest}`}>
                    <th scope="row">{part.person ?? part.guest ?? ''}</th>
                    <td>{formatMoney(part.amount, settings.currency, settings.locale)}</td>
                    <td>
                      {part.personId && part.person && part.personId !== transaction.paidBy?.personId ? (
                        <Button
                          variant="outline"
                          size="sm"
                          onClick={() => setCoding({ transaction, personId: part.personId ?? '', name: part.person ?? '', part: part.amount })}
                        >
                          <Icon name="qr" className="btn-icon" />
                          {t('code.show')}
                        </Button>
                      ) : null}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </section>
        ) : null}
        {transaction.description?.trim() ? (
          <section className="detail-section">
            <p className="detail-title">{t('common.description')}</p>
            <p className="detail-text">{transaction.description}</p>
          </section>
        ) : null}
        {count > 0 ? (
          <section className="detail-section">
            <p className="detail-title">{t('docs.title')}</p>
            <ClipPreview target={{ kind: 'transaction', id: transaction.id }} name={transaction.name} count={count} />
          </section>
        ) : null}
      </>
    )
  }

  /** The small marks of a phone row, after its category: shared with friends and a note. (The desktop's rows have a column for shared.) */
  function marks(transaction: Transaction) {
    return (
      <>
        {isShared(transaction) ? <span className="row-mark">{t('tx.shared')}</span> : null}
        {transaction.description?.trim() ? (
          <span className="row-mark-icon" title={t('common.description')}>
            <Icon name="note" className="mark-icon" />
            <span className="sr-only">{t('common.description')}</span>
          </span>
        ) : null}
      </>
    )
  }

  // A desktop row: name, amount, date, category, an icon if it is shared, then edit, delete and (when there is more to show) the chevron.
  function tableRow(transaction: Transaction) {
    const count = clips[transaction.id] ?? 0
    const more = isShared(transaction) || Boolean(transaction.description?.trim()) || count > 0
    return (
      <Row
        key={transaction.id}
        rows={rows}
        id={transaction.id}
        item={`transaction:${transaction.id}`}
        actions={
          <>
            <IconAction icon="pencil" label={t('common.edit')} data-row-focus onClick={() => openEdit(transaction)} />
            <IconAction icon="trash" label={t('common.delete')} danger onClick={() => setDeleting(transaction)} />
          </>
        }
        panel={more ? details(transaction, count) : null}
      >
        <div className="cell-name">
          <span className="card-name" title={transaction.name}>
            {transaction.name}
          </span>
        </div>
        {amount(transaction)}
        <span className="cell-sub-end">{formatDate(transaction.paymentDate, settings.locale)}</span>
        <span className="cell-sub">{categoryName(transaction)}</span>
        <span className="cell-mark">
          {isShared(transaction) ? (
            <span className="row-mark-icon" title={t('tx.shared')}>
              <Icon name="qr" className="mark-icon" />
              <span className="sr-only">{t('tx.shared')}</span>
            </span>
          ) : null}
        </span>
      </Row>
    )
  }

  // A phone row: one button with the name and amount, the category and marks under them. It opens to the details and to labelled
  // buttons for everything that can be done with the transaction.
  function tapRow(transaction: Transaction) {
    const count = clips[transaction.id] ?? 0
    return (
      <TapRow
        key={transaction.id}
        id={rows.panelId(transaction.id)}
        item={`transaction:${transaction.id}`}
        open={rows.isOpen(transaction.id)}
        onToggle={() => rows.toggle(transaction.id)}
        name={transaction.name}
        amount={amount(transaction)}
        sub={
          <>
            <span>{categoryName(transaction)}</span>
            {marks(transaction)}
            {count ? (
              <span className="row-mark-icon">
                <Icon name="clip" className="mark-icon" />
                <span aria-hidden="true">{count}</span>
                <span className="sr-only">{t('docs.clip', { count })}</span>
              </span>
            ) : null}
          </>
        }
      >
        {details(transaction, count)}
        <div className="row-panel-actions">
          <Button variant="outline" onClick={() => openEdit(transaction)}>
            <Icon name="pencil" className="btn-icon" />
            {t('common.edit')}
          </Button>
          <Button variant="outline" className="row-danger" onClick={() => setDeleting(transaction)}>
            <Icon name="trash" className="btn-icon" />
            {t('common.delete')}
          </Button>
        </div>
      </TapRow>
    )
  }

  return (
    <section className="tx-page">
      <div className="tx-title one-line">
        <h1>{t('tx.title')}</h1>
        <Button onClick={() => openAdd()} {...tip('new', t('tx.add'))}>
          {t('tx.add')}
        </Button>
      </div>
      <div className="toolbar wide tx-bar filter-bar">
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
        {phone ? (
          <>
            <Button
              variant="outline"
              className="filter-toggle"
              aria-expanded={filtersOpen}
              aria-controls={filtersOpen ? filtersId : undefined}
              aria-label={activeFilters > 0 ? t('tx.filtersCount', { count: activeFilters }) : undefined}
              onClick={() => setFiltersOpen((open) => !open)}
            >
              <Icon name="filter" className="btn-icon" />
              {t('tx.filters')}
              {activeFilters > 0 ? (
                <span className="count-badge" aria-hidden="true">
                  {activeFilters}
                </span>
              ) : null}
            </Button>
            {filtersOpen ? (
              <div className="filter-panel" id={filtersId}>
                {categoryPick}
                {rangeControls}
              </div>
            ) : null}
          </>
        ) : (
          <>
            {categoryPick}
            {rangeControls}
          </>
        )}
      </div>
      {adding || editing ? (
        <TransactionModal
          key={editing ? editing.id : clientUuid}
          transaction={editing}
          todayDate={now.date}
          categories={categories}
          pending={pending}
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
          target={{ kind: 'transaction', id: coding.transaction.id }}
          person={{ id: coding.personId, name: coding.name }}
          part={coding.part}
          total={coding.transaction.amount}
          paidBy={coding.transaction.paidBy?.name}
          onClose={() => setCoding(null)}
        />
      ) : null}
      {deleting ? (
        <ConfirmModal title={t('rec.confirmTitle', { action: t('common.delete'), name: deleting.name })} onConfirm={() => void onDelete(deleting.id)} onClose={() => setDeleting(null)} />
      ) : null}
      {page ? (
        <>
          <div className="tx-pane wide">
            {page.transactions.length === 0 ? (
              <EmptyState text={t('tx.empty')} />
            ) : phone ? (
              <DayGroups rows={page.transactions} dateOf={(transaction) => transaction.paymentDate} locale={settings.locale} today={now.date} className="tx-list">
                {tapRow}
              </DayGroups>
            ) : (
              <Table className="tx-list" heads={[t('common.name'), { label: t('tx.amount'), className: 'is-end' }, t('sync.field.date'), t('common.category'), '', '']} opens>
                {page.transactions.map(tableRow)}
              </Table>
            )}
            {phone ? foot : null}
          </div>
          {phone ? null : foot}
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
  const hadSplit = transaction ? isShared(transaction) : false
  const rescaled = hadSplit && !split.touched && amountString(amount, false) !== transaction?.amount
  const options: DropdownOption[] = categoryOptions(categories, { expense: t('common.expense'), income: t('common.income') })
  if (transaction && categories.every((category) => category.id !== transaction.categoryId)) {
    options.unshift({ value: transaction.categoryId, label: t('tx.deletedCategory') })
  }
  return (
    <>
      <Modal title={transaction ? t('tx.editTitle') : t('tx.addTitle')} pending={pending} onClose={onClose}>
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
            <textarea name="description" rows={3} maxLength={255} placeholder={t('common.optional')} defaultValue={transaction?.description ?? ''} autoComplete="off" />
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
