import { useEffect, useId, useMemo, useRef, useState, type ReactNode } from 'react'
import { useLocation, useNavigate, useSearchParams } from 'react-router-dom'
import PageHeader from '../components/PageHeader'
import BillCodeModal from '../components/BillCodeModal'
import ClipPreview from '../components/ClipPreview'
import ConfirmModal from '../components/ConfirmModal'
import EntryDialog, { type EntryInitial } from '../components/EntryDialog'
import Facts, { type FactItem } from '../components/Facts'
import Icon from '../components/NavIcon'
import Pager from '../components/Pager'
import PeriodNav from '../components/PeriodNav'
import Pick, { PickOption } from '../components/Pick'
import { DayGroups, IconAction, NameMark, PanelActions, Row, Table, TapRow } from '../components/RowParts'
import SectionTabs from '../components/SectionTabs'
import TransferModal from '../components/TransferModal'
import Button from '../components/ui/Button'
import { EmptyState } from '../components/ui/Pixel'
import DatePicker from '../components/DatePicker'
import { categories as categoriesApi, type Category } from '../api/categories'
import { errorText } from '../api/invoke'
import { shiftMonth } from '../api/months'
import { useToday, type Today } from '../api/today'
import { transactions as transactionsApi, type Transaction, type TransactionPage } from '../api/transactions'
import { transfers as transfersApi, type Transfer, type TransferPage } from '../api/transfers'
import { accountById, accountLabel, activeAccounts, currencyKey, manyAccounts } from '../lib/accountsView'
import { formatDate, formatMoney, moneyIn } from '../lib/money'
import { apiBound, presentRange, shiftBound, type RangeMode } from '../lib/range'
import { recall, saveChoice, storedChoice, storedId, useRemember } from '../lib/sticky'
import { isShared } from '../lib/split'
import { tabPanelProps } from '../lib/tabs'
import { entryTabOf, kindOf, offsetOf, TX_TAB_LABEL, txTabOf, txTabs, withTab, type TxTab } from '../lib/txTabs'
import { useAccounts } from '../hooks/useAccounts'
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
import { txTotals } from '../lib/txTotals'

type MenuName = 'range' | 'show' | 'category' | 'account'
const MODES = ['year', 'month', 'date'] as const
const LIMITS = [10, 50, 100] as const
type Filters = {
  mode: RangeMode
  from: string
  to: string
  categoryId: string
  /** Only with more than one account; '' is every account. */
  accountId: string
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
  const [reload, setReload] = useState(0)
  // The accounts are only spoken of with more than one that is not archived: then the list can be limited to one, each row names its own, and the
  // transfers have a tab.
  const accounts = useAccounts(reload)
  const multi = manyAccounts(accounts)
  // the tab is in the address (`?tab=income`), so back, forward and a reload keep it
  const [params, setParams] = useSearchParams()
  const tab = txTabOf(params, multi)
  // where the rows start belongs to the tab: another tab starts at the first page
  const [at, setAt] = useState<{ tab: TxTab; offset: number }>({ tab, offset: 0 })
  const offset = offsetOf(at, tab)
  const setOffset = (value: number) => setAt({ tab, offset: value })
  // The kind of period, the category and the rows per page are remembered after a restart; the dates only while the app stays open (after a
  // restart the page opens on the present, in the kind of period that was chosen).
  const [filters, setFilters] = useState<Filters>(() => ({
    ...presentRange(storedChoice('tx.period', MODES, 'month'), now),
    categoryId: storedId('tx.category'),
    accountId: storedId('tx.account'),
    q: '',
    ...recall<Partial<Filters>>('transactions.range', () => ({})),
  }))
  const range = useMemo(() => ({ mode: filters.mode, from: filters.from, to: filters.to }), [filters.mode, filters.from, filters.to])
  useRemember('transactions.range', range)
  const [limit, setLimit] = useState(() => storedChoice('tx.limit', LIMITS, 50))
  // the rows of the tab they were asked for: the list of another tab is not shown while its own is read
  const [loaded, setLoaded] = useState<{ tab: TxTab; list: TransactionPage } | null>(null)
  const page = loaded && loaded.tab === tab ? loaded.list : null
  const [transferPage, setTransferPage] = useState<TransferPage | null>(null)
  const [menu, setMenu] = useState<MenuName | null>(null)
  // the dialog that adds (what the opener asked for) or changes a transaction; the number makes each opening a new dialog
  const [adding, setAdding] = useState<EntryInitial | null>(null)
  const [dialogs, setDialogs] = useState(0)
  const [editing, setEditing] = useState<Transaction | null>(null)
  const [deleting, setDeleting] = useState<Transaction | null>(null)
  const [editingTransfer, setEditingTransfer] = useState<Transfer | null>(null)
  const [deletingTransfer, setDeletingTransfer] = useState<Transfer | null>(null)
  // the bill whose code for one friend is on the screen
  const [coding, setCoding] = useState<{ transaction: Transaction; personId: string; name: string; part: string } | null>(null)
  const location = useLocation()
  const navigate = useNavigate()
  const say = useToast()
  const clips = useClipCounts('transaction', page?.transactions.map((transaction) => transaction.id) ?? [], reload)
  // a document can send the person here for one of its transactions: its month is shown, and the row is scrolled to and outlined
  const { request: focus, done: focusDone } = useFocusRequest('transaction')
  useJumpTo(focus, focusDone, page)
  // The list waits for a short pause in typing instead of asking the database on every key.
  const searchText = useDebounced(filters.q, 200)
  const categoryNames = useMemo(() => new Map(categories.map((category) => [category.id, category.name])), [categories])
  const incomeCategories = useMemo(() => new Set(categories.filter((category) => category.type === 'INCOME').map((category) => category.id)), [categories])
  // the totals of the list, under it: only when the page holds the whole of it
  const totals = page ? txTotals(page.transactions, page.total, incomeCategories) : null
  // a remembered category that has been deleted since is no longer a filter, and the list is not asked for it before that is known
  const categoryKnown = filters.categoryId === '' || categoryNames.has(filters.categoryId)
  useEffect(() => {
    if (!categoriesReady || categoryKnown) return
    saveChoice('tx.category', '')
    setFilters((current) => ({ ...current, categoryId: '' }))
  }, [categoriesReady, categoryKnown])
  const accountNames = useMemo(() => new Map((accounts ?? []).map((account) => [account.id, accountLabel(account, t)])), [accounts, t])
  // a remembered account that is gone, archived or the only one left is no longer a filter, and the list is not asked for it before that is known
  // (the tab says "all" until they are read, so the list waits for them)
  const accountsReady = accounts !== null
  const accountKnown = filters.accountId === '' || (multi && activeAccounts(accounts ?? []).some((account) => account.id === filters.accountId))
  useEffect(() => {
    if (accounts === null || accountKnown) return
    saveChoice('tx.account', '')
    setFilters((current) => ({ ...current, accountId: '' }))
  }, [accounts, accountKnown])
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

  // The transactions of "all", "expenses" and "income": the tab says which kinds the app is asked for.
  useEffect(() => {
    if (tab === 'transfers' || !categoryKnown || !accountKnown || !accountsReady) return
    let cancelled = false
    const from = apiBound(filters.mode, filters.from, 'from')
    const to = apiBound(filters.mode, filters.to, 'to')
    const q = searchText.trim()
    const kind = kindOf(tab)
    void transactionsApi
      .list({
        ...(from ? { from } : {}),
        ...(to ? { to } : {}),
        ...(filters.categoryId ? { categoryId: filters.categoryId } : {}),
        ...(filters.accountId ? { accountId: filters.accountId } : {}),
        ...(kind ? { kind } : {}),
        ...(q ? { q } : {}),
        limit,
        offset,
      })
      .then(
        (result) => {
          if (!cancelled) setLoaded({ tab, list: result })
        },
        (failure) => {
          if (!cancelled) say(errorText(failure), 'error')
        },
      )
    return () => {
      cancelled = true
    }
  }, [tab, filters.mode, filters.from, filters.to, filters.categoryId, filters.accountId, categoryKnown, accountKnown, accountsReady, searchText, limit, offset, reload, say])

  // The transfers: the dates and the account (either side) limit them; the category and the search do not apply.
  useEffect(() => {
    if (tab !== 'transfers' || !accountKnown) return
    let cancelled = false
    const from = apiBound(filters.mode, filters.from, 'from')
    const to = apiBound(filters.mode, filters.to, 'to')
    void transfersApi
      .list({
        ...(from ? { from } : {}),
        ...(to ? { to } : {}),
        ...(filters.accountId ? { accountId: filters.accountId } : {}),
        limit,
        offset,
      })
      .then(
        (result) => {
          if (!cancelled) setTransferPage(result)
        },
        (failure) => {
          if (!cancelled) say(errorText(failure), 'error')
        },
      )
    return () => {
      cancelled = true
    }
  }, [tab, filters.mode, filters.from, filters.to, filters.accountId, accountKnown, limit, offset, reload, say])

  useEffect(() => {
    if (!focus) return
    const month = focus.date.slice(0, 7)
    setFilters({ mode: 'month', from: month, to: month, categoryId: '', accountId: '', q: '' })
    setLimit(100)
    setAt({ tab: 'all', offset: 0 })
    // the transaction may be of the other kind
    setParams((current) => withTab(current, 'all'), { replace: true })
    // once for each request: setting the address changes `setParams`, which would ask again
    // eslint-disable-next-line react-hooks/exhaustive-deps
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

  /** A new entry, on the tab that matches the list (an expense for "all"); `split` opens an expense with the bill dialog on, `friend` with that friend already on the bill. */
  function openAdd(options: EntryInitial = {}) {
    setMenu(null)
    setEditing(null)
    // a new dialog is a new transaction, whatever a cancelled one left
    setDialogs((value) => value + 1)
    setAdding({ tab: entryTabOf(tab), ...options })
  }

  // The debts pages send the person here to split a bill (with the friend they came from already on it), and the phone's round button to add one.
  useEffect(() => {
    const state = location.state as { add?: boolean; split?: boolean; friendId?: string; friendName?: string } | null
    if (!state?.split && !state?.add) return
    openAdd(state.split ? { tab: 'EXPENSE', split: true, ...(state.friendId && state.friendName ? { friend: { id: state.friendId, name: state.friendName } } : {}) } : {})
    // the request is taken out of the location, and the tab stays
    navigate({ pathname: location.pathname, search: location.search }, { replace: true, state: null })
    // each time the page is asked, when it opens and when it is open already; the request is taken out of the location, so once
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [location.state])

  function openEdit(transaction: Transaction) {
    setMenu(null)
    setAdding(null)
    setEditing(transaction)
  }

  function closeModal() {
    // documents may have been attached or removed in the dialog, so the marks on the list are asked for again
    if (adding || editing) setReload((value) => value + 1)
    setAdding(null)
    setEditing(null)
  }

  function onTab(next: TxTab) {
    setMenu(null)
    setParams(withTab(params, next))
  }

  // Keyboard: n adds, / searches, [ and ] step the month or the year, t jumps to this month or year, j and k move between rows.
  useShortcut('new', () => openAdd())
  useShortcut('search', () => searchRef.current?.focus())
  useShortcut('prev', () => stepPeriod(-1), filters.mode !== 'date')
  useShortcut('next', () => stepPeriod(1), filters.mode !== 'date')
  useShortcut('today', goCurrent)
  useRowNavigation()

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

  async function onDeleteTransfer(id: string) {
    try {
      await transfersApi.remove(id)
    } catch (failure) {
      say(errorText(failure), 'error')
      return
    } finally {
      setDeletingTransfer(null)
    }
    setReload((value) => value + 1)
  }

  function scopeLabel(): string {
    if (filters.categoryId) return categoryNames.get(filters.categoryId) ?? ''
    return t('tx.allCategories')
  }

  function pickAccount(accountId: string) {
    saveChoice('tx.account', accountId)
    patchFilters({ accountId })
    setMenu(null)
  }

  function pickCategory(categoryId: string) {
    saveChoice('tx.category', categoryId)
    patchFilters({ categoryId })
    setMenu(null)
  }

  const modeLabel = (mode: RangeMode) => (mode === 'date' ? t('tx.range') : mode === 'month' ? t('common.month') : t('common.year'))

  // the rows of the tab that is shown: transactions, or transfers on its own tab
  const listing = tab === 'transfers' ? transferPage : page
  const pageCount = listing && listing.limit > 0 ? Math.max(1, Math.ceil(listing.total / listing.limit)) : 0
  const currentPage = listing && listing.limit > 0 ? Math.floor(listing.offset / listing.limit) + 1 : 1

  // the period that "current" goes back to: this year when a year is shown, this month otherwise
  const currentPeriod = filters.mode === 'year' ? now.date.slice(0, 4) : now.month
  // how many filters are on: a kind of period other than the month, a category, an account (the month shown is in the header)
  const activeFilters = (filters.mode === 'month' ? 0 : 1) + (filters.categoryId && tab !== 'transfers' ? 1 : 0) + (filters.accountId ? 1 : 0)
  const categoryName = (transaction: Transaction) => categoryNames.get(transaction.categoryId) ?? (categoriesReady ? t('tx.deletedCategory') : '')

  // The period in the header: a month or a year to step through, or, for a range of dates, the dates (a button that opens the filters, where they are set).
  const periodNav =
    filters.mode === 'date' ? (
      <Button variant="outline" className="tx-range-button" aria-expanded={filtersOpen} aria-controls={filtersOpen ? filtersId : undefined} onClick={() => setFiltersOpen((open) => !open)}>
        {formatDate(filters.from, settings.locale)} – {formatDate(filters.to, settings.locale)}
      </Button>
    ) : (
      <PeriodNav
        keys
        kind={filters.mode}
        value={filters.from}
        current={currentPeriod}
        onChange={(value) => patchFilters({ from: value, to: value })}
        prevLabel={t(filters.mode === 'month' ? 'budgets.prevMonth' : 'dashboard.prevYear')}
        nextLabel={t(filters.mode === 'month' ? 'budgets.nextMonth' : 'dashboard.nextYear')}
      />
    )

  // The kind of period: a year, a month of a year, or exact dates (from one day to another, the end never before the start).
  const modePick = (
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
  )
  const rangeDates =
    filters.mode === 'date' ? (
      <>
        <div className="tx-date-group">
          <span>{t('tx.from')}</span>
          <DatePicker kind="date" value={filters.from} label={t('tx.from')} today={now.date} onChange={(from) => patchFilters(from > filters.to ? { from, to: from } : { from })} />
        </div>
        <div className="tx-date-group">
          <span>{t('tx.to')}</span>
          <DatePicker kind="date" value={filters.to} label={t('tx.to')} today={now.date} onChange={(to) => patchFilters(to < filters.from ? { from: to, to } : { to })} />
        </div>
      </>
    ) : null

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

  // With more than one account: all of them, or one. It sits beside the category.
  const accountText = filters.accountId ? (accountNames.get(filters.accountId) ?? '') : t('accounts.allAccounts')
  const accountPick = multi ? (
    <Pick
      label={t('accounts.account')}
      className="scope"
      text={accountText}
      title={accountText}
      open={menu === 'account'}
      onToggle={() => setMenu((current) => (current === 'account' ? null : 'account'))}
      onClose={() => setMenu(null)}
    >
      <PickOption selected={!filters.accountId} onClick={() => pickAccount('')}>
        {t('accounts.allAccounts')}
      </PickOption>
      {activeAccounts(accounts ?? []).map((account) => (
        <PickOption key={`account-${account.id}`} selected={filters.accountId === account.id} onClick={() => pickAccount(account.id)}>
          {accountNames.get(account.id) ?? ''}
        </PickOption>
      ))}
    </Pick>
  ) : null

  // Under the list: the pages, and how many rows a page has.
  const foot =
    listing && listing.total > 0 ? (
      <div className="list-foot wide">
        <Pager page={currentPage} count={pageCount} onPage={(number) => setOffset((number - 1) * listing.limit)} />
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

  /**
   * What a transaction was in the account's own currency, when its account is in another one (the app's amount is the one in the row): the code of that
   * currency and the amount, for the details. Null for a transaction in an account in the app's currency; a native amount that stayed on a row that moved
   * there means nothing and is not shown.
   */
  function nativeOf(transaction: Transaction): { code: string; text: string } | null {
    const account = transaction.nativeAmount === null ? null : accountById(accounts, transaction.accountId)
    return transaction.nativeAmount !== null && account?.foreign ? { code: account.currency.toUpperCase(), text: moneyIn(account.currency, settings.locale)(transaction.nativeAmount) } : null
  }

  /** What the person's share was: income in green with a plus, spending in red with a minus, and nothing at all plain. */
  function amount(transaction: Transaction) {
    const text = formatMoney(transaction.myShare, settings.currency, settings.locale)
    if (Number(transaction.myShare) === 0) return <span className="cell-amount">{text}</span>
    const income = incomeCategories.has(transaction.categoryId)
    return <span className={income ? 'cell-amount is-income' : 'cell-amount is-expense'}>{income ? `+${text}` : `-${text}`}</span>
  }

  /** The small icons after a name: a bill that is shared, documents attached. Marks are no buttons, so they may have a title. */
  function marks(transaction: Transaction, count: number): ReactNode {
    const shared = isShared(transaction)
    if (!shared && count === 0) return null
    return (
      <span className="row-mark-icon tx-marks">
        {shared ? (
          <span role="img" aria-label={t('tx.shared')} title={t('tx.shared')}>
            <Icon name="qr" className="mark-icon" />
          </span>
        ) : null}
        {count > 0 ? (
          <span role="img" aria-label={t('docs.clip', { count })} title={t('docs.clip', { count })}>
            <Icon name="clip" className="mark-icon" />
          </span>
        ) : null}
      </span>
    )
  }

  /**
   * The label and value pairs of a transaction's details. `all` (the phone) has the ones that a desktop row shows in its columns too: the account (with
   * more than one), the category and the date.
   */
  function factsOf(transaction: Transaction, all: boolean): FactItem[] {
    const native = nativeOf(transaction)
    const shared = isShared(transaction)
    const facts: (FactItem | false)[] = [
      all && multi && { label: t('accounts.account'), value: accountNames.get(transaction.accountId) ?? '' },
      all && { label: t('common.category'), value: categoryName(transaction) },
      all && { label: t('report.date'), value: formatDate(transaction.paymentDate, settings.locale) },
      native !== null && { label: t('entry.amountIn', { code: native.code }), value: native.text },
      shared && { label: t('split.paidBy'), value: transaction.paidBy?.name ?? t('split.you') },
      shared && { label: t('tx.billTotal'), value: formatMoney(transaction.amount, settings.currency, settings.locale) },
    ]
    return facts.filter((fact) => fact !== false)
  }

  /** What a row opens to besides its facts: the description, the split with a code for each friend on it (not for the one who paid), the documents (`count` of them). */
  function sections(transaction: Transaction, count: number): ReactNode {
    return (
      <>
        {transaction.description?.trim() ? (
          <section className="detail-section">
            <p className="detail-title">{t('common.description')}</p>
            <p className="detail-text">{transaction.description}</p>
          </section>
        ) : null}
        {isShared(transaction) ? (
          <section className="detail-section">
            <p className="detail-title">{t('split.section')}</p>
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
        {count > 0 ? (
          <section className="detail-section">
            <p className="detail-title">{t('docs.title')}</p>
            <ClipPreview target={{ kind: 'transaction', id: transaction.id }} name={transaction.name} count={count} />
          </section>
        ) : null}
      </>
    )
  }

  // A desktop row, a ledger: name (with its marks), category, account (with more than one), date, then the amount at the right, then edit, delete and
  // (when there is more to show) the chevron. Every cell is one line; with less room the account, then the date, then the category go (money.css).
  function tableRow(transaction: Transaction) {
    const count = clips[transaction.id] ?? 0
    const facts = factsOf(transaction, false)
    const more = facts.length > 0 || Boolean(transaction.description?.trim()) || count > 0
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
        panel={
          more ? (
            <>
              <Facts items={facts} className="tx-facts" />
              {sections(transaction, count)}
            </>
          ) : null
        }
      >
        <div className="cell-name">
          <span className="card-name" title={transaction.name}>
            {transaction.name}
          </span>
          {marks(transaction, count)}
        </div>
        <span className="cell-sub tx-cat" title={categoryName(transaction)}>
          {categoryName(transaction)}
        </span>
        {multi ? (
          <span className="cell-sub cell-account" title={accountNames.get(transaction.accountId)}>
            {accountNames.get(transaction.accountId) ?? ''}
          </span>
        ) : null}
        <span className="cell-sub-end tx-date">{formatDate(transaction.paymentDate, settings.locale)}</span>
        {amount(transaction)}
      </Row>
    )
  }

  // A phone row: the name (with its marks) and the amount, the category under the name. It opens to the facts, the description, the split and the
  // documents, and to buttons for everything that can be done with the transaction.
  function tapRow(transaction: Transaction) {
    const count = clips[transaction.id] ?? 0
    return (
      <TapRow
        key={transaction.id}
        id={rows.panelId(transaction.id)}
        item={`transaction:${transaction.id}`}
        open={rows.isOpen(transaction.id)}
        onToggle={() => rows.toggle(transaction.id)}
        name={<NameMark mark={marks(transaction, count)}>{transaction.name}</NameMark>}
        amount={amount(transaction)}
        meta={categoryName(transaction)}
        facts={factsOf(transaction, true)}
      >
        {sections(transaction, count)}
        <PanelActions
          actions={[
            { label: t('common.edit'), icon: 'pencil', onClick: () => openEdit(transaction) },
            { label: t('common.delete'), icon: 'trash', danger: true, onClick: () => setDeleting(transaction) },
          ]}
        />
      </TapRow>
    )
  }

  /** The name of a transfer, "from → to". */
  const transferName = (transfer: Transfer) => `${accountNames.get(transfer.fromAccountId) ?? ''} → ${accountNames.get(transfer.toAccountId) ?? ''}`
  /**
   * What was moved: neutral, it is neither income nor spending. Each side in its own account's currency (the app's for an account in the app's
   * currency): what left, and, when the two sides are in different currencies, what arrived (`arrived`, with the code of that currency).
   */
  function transferSides(transfer: Transfer): { sent: string; arrived: { code: string; text: string } | null } {
    const from = accountById(accounts, transfer.fromAccountId)
    const to = accountById(accounts, transfer.toAccountId)
    const side = (account: typeof from, value: string) => (account?.foreign ? moneyIn(account.currency, settings.locale)(value) : formatMoney(value, settings.currency, settings.locale))
    const sent = side(from, transfer.amount)
    if (transfer.toAmount === null || from === null || to === null || currencyKey(from) === currencyKey(to)) return { sent, arrived: null }
    return { sent, arrived: { code: to.currency.toUpperCase(), text: side(to, transfer.toAmount) } }
  }

  // A desktop row of the transfers: the day, from, to, the amount ("what left → what arrived" between two currencies), the note, then edit and delete.
  function transferRow(transfer: Transfer) {
    const from = accountNames.get(transfer.fromAccountId) ?? ''
    const to = accountNames.get(transfer.toAccountId) ?? ''
    const { sent, arrived } = transferSides(transfer)
    return (
      <Row
        key={transfer.id}
        item={`transfer:${transfer.id}`}
        actions={
          <>
            <IconAction icon="pencil" label={t('common.edit')} data-row-focus onClick={() => setEditingTransfer(transfer)} />
            <IconAction icon="trash" label={t('common.delete')} danger onClick={() => setDeletingTransfer(transfer)} />
          </>
        }
      >
        <span className="cell-sub xfer-date">{formatDate(transfer.date, settings.locale)}</span>
        <div className="cell-name xfer-from">
          <span className="card-name" title={from}>
            {from}
          </span>
        </div>
        <div className="cell-name xfer-to">
          <span className="card-name" title={to}>
            {to}
          </span>
        </div>
        <span className="cell-amount">{arrived ? `${sent} → ${arrived.text}` : sent}</span>
        <span className="cell-sub xfer-note" title={transfer.note ?? undefined}>
          {transfer.note ?? ''}
        </span>
      </Row>
    )
  }

  // A phone row of the transfers: "from → to" and the amount that left, the note under the name (the day is the heading). It opens to the two
  // accounts, what arrived, the day, and edit and delete.
  function transferTapRow(transfer: Transfer) {
    const key = `transfer-${transfer.id}`
    const { sent, arrived } = transferSides(transfer)
    const facts: (FactItem | false)[] = [
      { label: t('transfers.from'), value: accountNames.get(transfer.fromAccountId) ?? '' },
      { label: t('transfers.to'), value: accountNames.get(transfer.toAccountId) ?? '' },
      arrived !== null && { label: t('transfers.arrives', { code: arrived.code }), value: arrived.text },
      { label: t('report.date'), value: formatDate(transfer.date, settings.locale) },
    ]
    return (
      <TapRow
        key={key}
        id={rows.panelId(key)}
        item={`transfer:${transfer.id}`}
        open={rows.isOpen(key)}
        onToggle={() => rows.toggle(key)}
        name={transferName(transfer)}
        amount={<span className="cell-amount">{sent}</span>}
        meta={transfer.note || undefined}
        facts={facts.filter((fact) => fact !== false)}
      >
        <PanelActions
          actions={[
            { label: t('common.edit'), icon: 'pencil', onClick: () => setEditingTransfer(transfer) },
            { label: t('common.delete'), icon: 'trash', danger: true, onClick: () => setDeletingTransfer(transfer) },
          ]}
        />
      </TapRow>
    )
  }

  const emptyText = tab === 'expenses' ? t('tx.emptyExpenses') : tab === 'income' ? t('tx.emptyIncome') : t('tx.empty')
  const showTransfers = tab === 'transfers'

  /** What came in, what went out and what is left of the two, each with its word: a row of columns under the table, a strip over the list on a phone. */
  const sums = totals ? (
    <span className="tx-sums">
      <span className="tx-sum">
        <span className="tx-sum-label">{t('common.income')}</span>
        <span className="is-income">+{formatMoney(totals.income, settings.currency, settings.locale)}</span>
      </span>
      <span className="tx-sum">
        <span className="tx-sum-label">{t('common.expense')}</span>
        <span className="is-expense">-{formatMoney(totals.expense, settings.currency, settings.locale)}</span>
      </span>
      <span className="tx-sum">
        <span className="tx-sum-label">{t('tx.net')}</span>
        <b className={totals.net.startsWith('-') ? 'is-expense' : 'is-income'}>
          {totals.net.startsWith('-') ? '-' : '+'}
          {formatMoney(totals.net.replace('-', ''), settings.currency, settings.locale)}
        </b>
      </span>
    </span>
  ) : null

  return (
    <section className="tx-page">
      <PageHeader
        title={t('tx.title')}
        // The phone has the round add button for this.
        actions={[!phone && { label: t('tx.add'), icon: 'add', primary: true, shortcut: 'new', onClick: () => openAdd() }]}
      >
        {periodNav}
      </PageHeader>
      <SectionTabs label={t('tx.title')} value={tab} onChange={onTab} tabs={txTabs(multi).map((id) => ({ id, label: t(TX_TAB_LABEL[id]) }))} />
      <div className="tx-top wide">
        {showTransfers ? null : (
          <label className="tx-search" {...tip('search')}>
            <span className="sr-only">{t('common.search')}</span>
            <Icon name="search" className="btn-icon tx-search-icon" />
            <input
              ref={searchRef}
              placeholder={t('common.search')}
              onKeyDown={(event) => {
                if (event.key === 'Escape') event.currentTarget.blur()
              }}
              value={filters.q}
              maxLength={255}
              autoComplete="off"
              onChange={(event) => patchFilters({ q: event.target.value })}
            />
          </label>
        )}
        <Button
          variant="outline"
          size="icon"
          className="tx-filter-button"
          title={t('tx.filters')}
          aria-expanded={filtersOpen}
          aria-controls={filtersOpen ? filtersId : undefined}
          aria-label={activeFilters > 0 ? t('tx.filtersCount', { count: activeFilters }) : t('tx.filters')}
          onClick={() => setFiltersOpen((open) => !open)}
        >
          <Icon name="filter" className="btn-icon" />
          {activeFilters > 0 ? (
            <span className="count-badge" aria-hidden="true">
              {activeFilters}
            </span>
          ) : null}
        </Button>
        {filtersOpen ? (
          <div className="tx-filters" id={filtersId}>
            {showTransfers ? null : categoryPick}
            {accountPick}
            {modePick}
            {rangeDates}
          </div>
        ) : null}
      </div>
      {adding || editing ? (
        <EntryDialog
          key={editing ? editing.id : dialogs}
          transaction={editing}
          initial={adding ?? undefined}
          today={now.date}
          onClose={closeModal}
          onSaved={() => setReload((value) => value + 1)}
        />
      ) : null}
      {editingTransfer && accounts ? (
        <TransferModal
          key={`transfer:${editingTransfer.id}`}
          transfer={editingTransfer}
          accounts={accounts}
          today={now.date}
          onClose={() => setEditingTransfer(null)}
          onSaved={() => {
            // the dialog may have been closed while it saved, and another opened since: only its own is closed
            setEditingTransfer((current) => (current === editingTransfer ? null : current))
            setReload((value) => value + 1)
          }}
          onDelete={() => {
            setEditingTransfer(null)
            setDeletingTransfer(editingTransfer)
          }}
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
      {deletingTransfer ? (
        <ConfirmModal
          title={t('rec.confirmTitle', { action: t('common.delete'), name: transferName(deletingTransfer) })}
          onConfirm={() => void onDeleteTransfer(deletingTransfer.id)}
          onClose={() => setDeletingTransfer(null)}
        />
      ) : null}
      {showTransfers && transferPage ? (
        <>
          <div className="tx-pane wide" {...tabPanelProps(tab)}>
            {transferPage.transfers.length === 0 ? (
              <EmptyState text={t('tx.emptyTransfers')} />
            ) : phone ? (
              <DayGroups rows={transferPage.transfers} dateOf={(transfer) => transfer.date} locale={settings.locale} today={now.date}>
                {transferTapRow}
              </DayGroups>
            ) : (
              <Table
                tidy
                className="xfer-list"
                heads={[
                  { label: t('sync.field.date'), className: 'xfer-date' },
                  t('transfers.from'),
                  t('transfers.to'),
                  { label: t('tx.amount'), className: 'is-end' },
                  { label: t('debts.note'), className: 'xfer-note' },
                  '',
                ]}
              >
                {transferPage.transfers.map(transferRow)}
              </Table>
            )}
            {phone ? foot : null}
          </div>
          {phone ? null : foot}
        </>
      ) : null}
      {!showTransfers && page ? (
        <>
          <div className="tx-pane wide" {...tabPanelProps(tab)}>
            {page.transactions.length === 0 ? (
              <EmptyState text={emptyText} />
            ) : phone ? (
              <>
                {sums ? <div className="tx-total is-strip">{sums}</div> : null}
                <DayGroups rows={page.transactions} dateOf={(transaction) => transaction.paymentDate} locale={settings.locale} today={now.date}>
                  {tapRow}
                </DayGroups>
              </>
            ) : (
              <Table
                tidy
                className={multi ? 'tx-list has-account' : 'tx-list'}
                heads={[
                  t('common.name'),
                  { label: t('common.category'), className: 'tx-cat' },
                  ...(multi ? [{ label: t('accounts.account'), className: 'cell-account' }] : []),
                  { label: t('sync.field.date'), className: 'tx-date' },
                  { label: t('tx.amount'), className: 'is-end' },
                  '',
                ]}
                opens
              >
                {page.transactions.map(tableRow)}
                {totals ? (
                  <li className="card-item tx-total">
                    <span className="cell-name">{t('tx.totalRow', { count: totals.count })}</span>
                    {sums}
                  </li>
                ) : null}
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
