import { useEffect, useId, useMemo, useRef, useState, type ReactNode } from 'react'
import { useLocation, useNavigate, useSearchParams } from 'react-router-dom'
import BillCodeModal from '../components/BillCodeModal'
import ClipPreview from '../components/ClipPreview'
import ConfirmModal from '../components/ConfirmModal'
import EntryDialog, { type EntryInitial } from '../components/EntryDialog'
import Icon from '../components/NavIcon'
import Pager from '../components/Pager'
import Pick, { PickOption } from '../components/Pick'
import { DayGroups, IconAction, Row, Table, TapRow } from '../components/RowParts'
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

  const atCurrentMonth = filters.mode === 'month' && filters.from === now.month && filters.to === now.month
  // the button for the current period is off when the year or the month shown is that one (and there is none for a range of dates)
  const currentPeriod = filters.mode === 'year' ? now.date.slice(0, 4) : now.month
  const atCurrentPeriod = filters.from === currentPeriod && filters.to === currentPeriod
  // how many filters are not as they start (this month, every category): the phone's filters button counts them
  const activeFilters = (atCurrentMonth ? 0 : 1) + (filters.categoryId && tab !== 'transfers' ? 1 : 0) + (filters.accountId ? 1 : 0)
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
   * What a transaction was in the account's own currency, when its account is in another one (the app's amount is the one in the row): small and quiet,
   * under the amount on the computer and in the small print of the row on the phone. Null for a transaction in an account in the app's currency; a native
   * amount that stayed on a row that moved there means nothing and is not shown.
   */
  function nativeText(transaction: Transaction): string | null {
    const account = transaction.nativeAmount === null ? null : accountById(accounts, transaction.accountId)
    return transaction.nativeAmount !== null && account?.foreign ? moneyIn(account.currency, settings.locale)(transaction.nativeAmount) : null
  }

  /** What the person's share was: income in green with a plus, spending in red with a minus, and nothing at all plain. `withNative` adds what it was in the account's currency. */
  function amount(transaction: Transaction, withNative = true) {
    const text = formatMoney(transaction.myShare, settings.currency, settings.locale)
    const native = withNative ? nativeText(transaction) : null
    const small = native ? <span className="cell-native">{native}</span> : null
    if (Number(transaction.myShare) === 0) {
      return (
        <span className="cell-amount">
          {text}
          {small}
        </span>
      )
    }
    const income = incomeCategories.has(transaction.categoryId)
    return (
      <span className={income ? 'cell-amount is-income' : 'cell-amount is-expense'}>
        {income ? `+${text}` : `-${text}`}
        {small}
      </span>
    )
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

  // A desktop row, a ledger: name, category, account (with more than one), date, then the amount at the right, an icon if it is shared, then edit,
  // delete and (when there is more to show) the chevron. Narrower than 40em the category and the date go under the name and the amount (primitives.css).
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
        <span className="cell-sub" title={categoryName(transaction)}>
          {categoryName(transaction)}
        </span>
        {multi ? (
          <span className="cell-sub cell-account" title={accountNames.get(transaction.accountId)}>
            {accountNames.get(transaction.accountId) ?? ''}
          </span>
        ) : null}
        <span className="cell-sub-end tx-date">{formatDate(transaction.paymentDate, settings.locale)}</span>
        {amount(transaction)}
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

  // A phone row: one button with the name and amount, the category (and "shared") under them. It opens to the details, the description
  // and the documents, and to labelled buttons for everything that can be done with the transaction.
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
        amount={amount(transaction, false)}
        sub={
          <>
            <span>{categoryName(transaction)}</span>
            {multi ? <span>{accountNames.get(transaction.accountId) ?? ''}</span> : null}
            {nativeText(transaction) ? <span>{nativeText(transaction)}</span> : null}
            {isShared(transaction) ? <span>{t('tx.shared')}</span> : null}
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

  const transferName = (transfer: Transfer) => `${accountNames.get(transfer.fromAccountId) ?? ''} → ${accountNames.get(transfer.toAccountId) ?? ''}`
  /**
   * What was moved: neutral, it is neither income nor spending. Each side in its own account's currency (the app's for an account in the app's
   * currency): one amount, or "what left → what arrived" when the two sides are in different currencies.
   */
  const transferAmount = (transfer: Transfer) => {
    const from = accountById(accounts, transfer.fromAccountId)
    const to = accountById(accounts, transfer.toAccountId)
    const side = (account: typeof from, value: string) => (account?.foreign ? moneyIn(account.currency, settings.locale)(value) : formatMoney(value, settings.currency, settings.locale))
    if (transfer.toAmount === null || from === null || to === null || currencyKey(from) === currencyKey(to)) {
      return <span className="cell-amount">{side(from, transfer.amount)}</span>
    }
    return (
      <span className="cell-amount is-pair">
        <span>{side(from, transfer.amount)}</span> <span className="pair-to">{`→ ${side(to, transfer.toAmount)}`}</span>
      </span>
    )
  }

  // A desktop row of the transfers: the day, from, to, the amount, the note, then edit and delete.
  function transferRow(transfer: Transfer) {
    const from = accountNames.get(transfer.fromAccountId) ?? ''
    const to = accountNames.get(transfer.toAccountId) ?? ''
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
        <div className="xfer-route">
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
        </div>
        {transferAmount(transfer)}
        <span className="cell-sub xfer-note" title={transfer.note ?? undefined}>
          {transfer.note ?? ''}
        </span>
      </Row>
    )
  }

  // A phone row of the transfers: "from → to" and the amount, the day and the note under them. It opens to edit and delete.
  function transferTapRow(transfer: Transfer) {
    const key = `transfer-${transfer.id}`
    return (
      <TapRow
        key={key}
        id={rows.panelId(key)}
        item={`transfer:${transfer.id}`}
        open={rows.isOpen(key)}
        onToggle={() => rows.toggle(key)}
        name={transferName(transfer)}
        amount={transferAmount(transfer)}
        sub={
          <>
            <span>{formatDate(transfer.date, settings.locale)}</span>
            {transfer.note ? <span>{transfer.note}</span> : null}
          </>
        }
      >
        <div className="row-panel-actions">
          <Button variant="outline" onClick={() => setEditingTransfer(transfer)}>
            <Icon name="pencil" className="btn-icon" />
            {t('common.edit')}
          </Button>
          <Button variant="outline" className="row-danger" onClick={() => setDeletingTransfer(transfer)}>
            <Icon name="trash" className="btn-icon" />
            {t('common.delete')}
          </Button>
        </div>
      </TapRow>
    )
  }

  const emptyText = tab === 'expenses' ? t('tx.emptyExpenses') : tab === 'income' ? t('tx.emptyIncome') : t('tx.empty')
  const showTransfers = tab === 'transfers'

  return (
    <section className="tx-page">
      <div className="tx-title one-line">
        <h1>{t('tx.title')}</h1>
        {/* The phone has the round add button for this. */}
        {phone ? null : (
          <Button onClick={() => openAdd()} {...tip('new', t('tx.add'))}>
            {t('tx.add')}
          </Button>
        )}
      </div>
      <SectionTabs label={t('tx.title')} value={tab} onChange={onTab} tabs={txTabs(multi).map((id) => ({ id, label: t(TX_TAB_LABEL[id]) }))} />
      <div className="toolbar wide tx-bar filter-bar">
        {showTransfers ? null : (
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
        )}
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
                {showTransfers ? null : categoryPick}
                {accountPick}
                {rangeControls}
              </div>
            ) : null}
          </>
        ) : (
          <>
            {showTransfers ? null : categoryPick}
            {accountPick}
            {rangeControls}
          </>
        )}
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
              <ul className="card-list">{transferPage.transfers.map(transferTapRow)}</ul>
            ) : (
              <Table
                className="tx-transfers"
                heads={[t('sync.field.date'), t('transfers.from'), t('transfers.to'), { label: t('tx.amount'), className: 'is-end' }, t('debts.note'), '']}
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
              <DayGroups rows={page.transactions} dateOf={(transaction) => transaction.paymentDate} locale={settings.locale} today={now.date} className="tx-list">
                {tapRow}
              </DayGroups>
            ) : (
              <Table
                className={multi ? 'tx-list has-account' : 'tx-list'}
                heads={[
                  t('common.name'),
                  t('common.category'),
                  ...(multi ? [{ label: t('accounts.account'), className: 'cell-account' }] : []),
                  t('sync.field.date'),
                  { label: t('tx.amount'), className: 'is-end' },
                  '',
                  '',
                ]}
                opens
              >
                {page.transactions.map(tableRow)}
                {totals ? (
                  <li className="card-item tx-total">
                    <span className="cell-name">{t('tx.totalRow', { count: totals.count })}</span>
                    <span className="tx-total-figures">
                      <span className="is-income">+{formatMoney(totals.income, settings.currency, settings.locale)}</span>
                      <span className="is-expense">-{formatMoney(totals.expense, settings.currency, settings.locale)}</span>
                      <b className={totals.net.startsWith('-') ? 'is-expense' : 'is-income'}>
                        {totals.net.startsWith('-') ? '-' : '+'}
                        {formatMoney(totals.net.replace('-', ''), settings.currency, settings.locale)}
                      </b>
                    </span>
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
