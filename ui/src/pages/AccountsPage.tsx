import { Fragment, useEffect, useMemo, useRef, useState, type MouseEvent, type ReactNode } from 'react'
import { Link, useNavigate, useParams, useSearchParams } from 'react-router-dom'
import { accounts as accountsApi, type Account, type AccountLine, type AccountStatement, type AccountsSummary, type Breakdown } from '../api/accounts'
import { debts } from '../api/debts'
import { ApiError, errorText } from '../api/invoke'
import { shiftMonth } from '../api/months'
import { useToday, type Today } from '../api/today'
import { transfers as transfersApi, type Transfer } from '../api/transfers'
import AccountModal from '../components/AccountModal'
import BalanceChart from '../components/BalanceChart'
import ConfirmModal from '../components/ConfirmModal'
import EntryDialog from '../components/EntryDialog'
import IncomeTab from '../components/IncomeTab'
import MonthNav from '../components/MonthNav'
import Icon from '../components/NavIcon'
import { DayGroups, IconAction, Row, Table, TapRow } from '../components/RowParts'
import SectionTabs from '../components/SectionTabs'
import ShareList from '../components/ShareList'
import TransferModal from '../components/TransferModal'
import YearNav from '../components/YearNav'
import Button from '../components/ui/Button'
import { Card, CardAction, CardHeader } from '../components/ui/Card'
import { buttonClass } from '../components/ui/cn'
import { useGoToTransaction } from '../hooks/useFocusRequest'
import { useMoneySettings, type MoneySettings } from '../hooks/useMoneySettings'
import { useOpenRow } from '../hooks/useOpenRow'
import { useProfile } from '../hooks/profile'
import { useSwipeHandler } from '../hooks/swipe'
import { useReorder } from '../hooks/useReorder'
import { useToast } from '../hooks/toast'
import { useT } from '../i18n/context'
import { useShortcut } from '../keys/registry'
import { tip } from '../keys/tip'
import { useRowNavigation } from '../keys/useRowNavigation'
import { ACCOUNT_TABS, accountLabel, accountName, accountsTab, activeAccounts, groupAccounts, lineWords, reorderWithinGroup, showAvailable, signedView, tabQuery, type AccountGroup } from '../lib/accountsView'
import type { EntryTab } from '../lib/entryDialog'
import { breakable, formatDate, formatMoney, moneyIn } from '../lib/money'
import { lastDay } from '../lib/range'
import { applyOrder } from '../lib/reorder'
import { saveChoice, storedChoice } from '../lib/sticky'
import { step } from '../lib/swipe'
import { tabPanelProps } from '../lib/tabs'

/** The dialog that is open. A transfer being changed is read first, so the dialog opens with its row. */
type Dialog =
  | { kind: 'add' }
  | { kind: 'edit'; account: Account }
  | { kind: 'transfer'; transfer: Transfer | null; fromId?: string }
  | { kind: 'entry'; tab: EntryTab; accountId: string }
  | { kind: 'archive'; account: Account }
  | { kind: 'delete'; account: Account }
  | { kind: 'deleteTransfer'; id: string; name: string }
  | null

export default function AccountsPage() {
  const { settings, error } = useMoneySettings()
  const now = useToday()
  if (error) return <p className="notice">{error}</p>
  if (!settings || !now) return null
  return <AccountsView settings={settings} now={now} />
}

/**
 * The accounts: where the money is. The list (in the person's order, with the balance of each), the archived ones apart, and, with an
 * account in the address, that account's page: its balance over the year and what moved it in a month. docs/accounts-design.md, section 13.
 */
function AccountsView({ settings, now }: { settings: MoneySettings; now: Today }) {
  const t = useT()
  const phone = useProfile() === 'phone'
  const navigate = useNavigate()
  // the account chosen in the address, whose page is shown instead of the list
  const { accountId } = useParams()
  const rows = useOpenRow()
  const say = useToast()
  const [accounts, setAccounts] = useState<Account[] | null>(null)
  const [summary, setSummary] = useState<AccountsSummary | null>(null)
  const [reload, setReload] = useState(0)
  const refresh = () => setReload((value) => value + 1)
  const [dialog, setDialog] = useState<Dialog>(null)
  const [pending, setPending] = useState(false)
  const [showArchived, setShowArchived] = useState(() => storedChoice('accounts.archived', [false, true], false))
  // the tab is in the address, so other pages can link to the income (/app/accounts?tab=income); an account's page has no tabs
  const [params, setParams] = useSearchParams()
  const tab = accountId ? 'accounts' : accountsTab(params)
  // a row was just dropped in a new place: the click that ends the drag must not open the account
  const dropped = useRef(false)
  const money = (amount: string) => formatMoney(amount, settings.currency, settings.locale)
  /** What an account holds, moved or has in the account's own currency (the app's for one in the app's currency, as always). */
  const moneyOf = (account: Pick<Account, 'foreign' | 'currency'>) => (account.foreign ? moneyIn(account.currency, settings.locale) : money)
  /** The same for a cell of a list, where an amount in another currency (longer by its code) may break after the code rather than reach the buttons. */
  const cellMoney = (account: Pick<Account, 'foreign' | 'currency'>) => (account.foreign ? (amount: string) => breakable(moneyOf(account)(amount)) : money)
  const wrap = (account: Pick<Account, 'foreign'>) => (account.foreign ? ' is-wrap' : '')

  useEffect(() => {
    let cancelled = false
    void Promise.all([accountsApi.list(), accountsApi.summary()]).then(
      ([list, sum]) => {
        if (cancelled) return
        setAccounts(list)
        setSummary(sum)
      },
      (failure) => {
        if (!cancelled) say(errorText(failure), 'error')
      },
    )
    return () => {
      cancelled = true
    }
  }, [reload, say])

  const active = useMemo(() => activeAccounts(accounts ?? []), [accounts])
  // the list as it is shown: one run of accounts, or one for each group (the groups in use are what the group field offers)
  const groups = useMemo(() => groupAccounts(accounts ?? []), [accounts])
  const grouped = groups.some((group) => group.name !== null)
  const archived = useMemo(() => (accounts ?? []).filter((account) => account.archived), [accounts])
  // a transfer needs two accounts to choose from
  const canTransfer = active.length >= 2
  const nameOf = (account: Account) => accountName(account, t)

  // The new order shows at once; if it cannot be saved, the saved one comes back.
  async function onReorder(ids: string[]) {
    dropped.current = true
    window.setTimeout(() => (dropped.current = false), 500)
    setAccounts((current) => current && applyOrder(current, ids))
    try {
      await accountsApi.reorder(ids)
    } catch (failure) {
      say(errorText(failure), 'error')
      refresh()
    }
  }
  /** An account moves among the accounts of its own group; what is sent is still every account that is not archived. */
  const onReorderGroup = (group: string | null, ids: string[]) => void onReorder(reorderWithinGroup(accounts ?? [], group, ids))

  /** Archive, restore, delete: the list is read again whatever the answer, and the dialog that asked closes. */
  async function run(action: () => Promise<unknown>, after?: () => void) {
    if (pending) return
    setPending(true)
    try {
      await action()
    } catch (failure) {
      say(errorText(failure), 'error')
      return
    } finally {
      setPending(false)
      setDialog(null)
      refresh()
    }
    after?.()
  }

  /** A transfer of a statement, read so that the dialog opens with it. */
  async function editTransfer(id: string) {
    try {
      const transfer = await transfersApi.get(id)
      setDialog({ kind: 'transfer', transfer })
    } catch (failure) {
      say(errorText(failure), 'error')
      refresh()
    }
  }

  const chosen = accountId ? (accounts?.find((account) => account.id === accountId) ?? null) : null
  /** The one place that adds an entry for an account: the add dialog of the transactions page, on the tab asked for and with this account chosen. */
  const addEntry = (tab: EntryTab, account: Account) => setDialog({ kind: 'entry', tab, accountId: account.id })

  // Keyboard: n adds an account (an expense, on an account's page; on the income tab the tab has its own), j and k move between rows.
  useShortcut('new', () => (chosen && !chosen.archived ? addEntry('EXPENSE', chosen) : setDialog({ kind: 'add' })), tab === 'accounts')
  useRowNavigation()
  // On a phone a swipe moves between the two tabs before it moves to another page.
  useSwipeHandler((direction) => {
    const next = step(ACCOUNT_TABS, tab, direction)
    if (next === null) return false
    setParams(tabQuery(next), { replace: true })
    return true
  }, phone && !accountId)

  const link = (account: Account) => `/app/accounts/${account.id}`

  /** A click anywhere on a desktop row opens the account, as its name does. Not one on a link or a button, one that ends a text selection, or the one that drops a dragged row. */
  function open(event: MouseEvent<HTMLElement>, account: Account) {
    if ((event.target as Element).closest('a, button, input, label') || window.getSelection()?.isCollapsed === false) return
    if (dropped.current) return
    navigate(link(account))
  }

  const kindText = (account: Account) => t(`accounts.kind.${account.kind}`)
  const lastText = (account: Account) => (account.lastActivity ? formatDate(account.lastActivity, settings.locale) : t('accounts.noActivity'))
  /** What the month did to the account so far: green with a plus, red with a minus, plain when nothing. */
  const monthCell = (account: Account, className: string) => {
    const view = signedView(account.changeThisMonth, cellMoney(account))
    return (
      <span className={`${className} ${view.tone}${wrap(account)}`}>
        <span className="sr-only">{t('accounts.thisMonth')} </span>
        {view.text}
      </span>
    )
  }
  /** The balance in the account's currency, and under it what is available when the account has a virtual balance, and what it is worth in the app's currency when it is in another. */
  const balanceCell = (account: Account, className: string) => (
    <span className={`${className}${wrap(account)}`}>
      {cellMoney(account)(account.balance)}
      {showAvailable(account) ? <span className="acct-available">{`${t('accounts.available')} ${moneyOf(account)(account.available)}`}</span> : null}
      {account.foreign ? <span className="acct-available acct-in-main">{t('accounts.inMain', { amount: money(account.balanceInMain) })}</span> : null}
    </span>
  )
  /** Small words under the name of an account: its currency when it is not the app's, and that the net worth leaves it out. */
  const marks = (account: Account) => {
    const words = [account.foreign ? account.currency.toUpperCase() : '', account.includeInNetWorth ? '' : t('accounts.notInNetWorth')].filter(Boolean).join(' · ')
    return words === '' ? null : (
      <span className="row-mark" title={words}>
        {words}
      </span>
    )
  }
  /** Delete for an account with nothing in it, archive for one with rows; the main account has neither. */
  const removal = (account: Account): 'delete' | 'archive' | null => (account.isMain ? null : account.entries === 0 ? 'delete' : 'archive')

  // A desktop row: name, type, number, last activity, this month, balance, then edit and archive (or delete, for an empty one). The grip
  // at its start says that the row can be held and dragged. The name is the link for the keyboard (j and k stop on it).
  function tableRow(account: Account, hintId: string) {
    const way = removal(account)
    return (
      <Row
        key={account.id}
        data-reorder-id={account.id}
        onClick={(event) => open(event, account)}
        actions={
          <>
            <IconAction icon="pencil" label={t('common.edit')} onClick={() => setDialog({ kind: 'edit', account })} />
            {way === 'delete' ? (
              <IconAction icon="trash" label={t('common.delete')} danger onClick={() => setDialog({ kind: 'delete', account })} />
            ) : way === 'archive' ? (
              <IconAction icon="archive" label={t('accounts.archive')} onClick={() => setDialog({ kind: 'archive', account })} />
            ) : (
              <span className="icon-slot" />
            )}
          </>
        }
      >
        <div className="cell-name">
          <Icon name="grip" className="row-grip" />
          <span className="acct-name">
            <Link className="card-name" to={link(account)} title={nameOf(account)} data-row-focus aria-describedby={hintId}>
              {nameOf(account)}
            </Link>
            {marks(account)}
          </span>
        </div>
        <span className="cell-sub">{kindText(account)}</span>
        <span className="cell-sub acct-number" title={account.number ?? undefined}>
          {account.number ?? ''}
        </span>
        <span className="cell-sub acct-last">{lastText(account)}</span>
        {monthCell(account, 'cell-amount acct-month')}
        {balanceCell(account, 'cell-amount acct-balance')}
      </Row>
    )
  }

  // A phone row: the name and the balance, the type, the last activity and this month under them. It opens to a labelled button for
  // everything that can be done.
  function tapRow(account: Account) {
    const way = removal(account)
    return (
      <TapRow
        key={account.id}
        id={rows.panelId(account.id)}
        data-archived={account.archived || undefined}
        open={rows.isOpen(account.id)}
        onToggle={() => rows.toggle(account.id)}
        name={
          <>
            {nameOf(account)}
            {marks(account)}
          </>
        }
        amount={balanceCell(account, 'cell-amount')}
        sub={
          <>
            <span>{kindText(account)}</span>
            {account.archived ? null : (
              <>
                <span>{lastText(account)}</span>
                {monthCell(account, 'acct-month-sub')}
              </>
            )}
          </>
        }
      >
        <div className="row-panel-actions">
          <Link to={link(account)} className={buttonClass('outline')}>
            {t('accounts.open')}
          </Link>
          {account.archived ? (
            <Button variant="outline" onClick={() => void run(() => accountsApi.unarchive(account.id))}>
              {t('accounts.restore')}
            </Button>
          ) : (
            <>
              <Button variant="outline" onClick={() => setDialog({ kind: 'edit', account })}>
                <Icon name="pencil" className="btn-icon" />
                {t('common.edit')}
              </Button>
              {canTransfer ? (
                <Button variant="outline" onClick={() => setDialog({ kind: 'transfer', transfer: null, fromId: account.id })}>
                  {t('transfers.add')}
                </Button>
              ) : null}
            </>
          )}
          {way === 'archive' && !account.archived ? (
            <Button variant="outline" onClick={() => setDialog({ kind: 'archive', account })}>
              <Icon name="archive" className="btn-icon" />
              {t('accounts.archive')}
            </Button>
          ) : null}
          {way === 'delete' ? (
            <Button variant="outline" className="row-danger" onClick={() => setDialog({ kind: 'delete', account })}>
              <Icon name="trash" className="btn-icon" />
              {t('common.delete')}
            </Button>
          ) : null}
        </div>
      </TapRow>
    )
  }

  // An archived account on the desktop: name, type, balance, restore, and delete when it is empty.
  function archivedRow(account: Account) {
    return (
      <Row
        key={account.id}
        data-archived
        onClick={(event) => open(event, account)}
        actions={
          <>
            <Button variant="outline" size="sm" className="row-primary" onClick={() => void run(() => accountsApi.unarchive(account.id))}>
              {t('accounts.restore')}
            </Button>
            {account.entries === 0 ? (
              <IconAction icon="trash" label={t('common.delete')} danger onClick={() => setDialog({ kind: 'delete', account })} />
            ) : (
              <span className="icon-slot" />
            )}
          </>
        }
      >
        <div className="cell-name">
          <span className="acct-name">
            <Link className="card-name" to={link(account)} title={nameOf(account)} data-row-focus>
              {nameOf(account)}
            </Link>
            {marks(account)}
          </span>
        </div>
        <span className="cell-sub">{kindText(account)}</span>
        {balanceCell(account, 'cell-amount')}
      </Row>
    )
  }

  /** The heading of a group, when there are groups: the accounts that have none come last, under "no group". */
  const groupTitle = (group: AccountGroup): ReactNode => (grouped ? <h2 className="acct-group" title={group.name ?? t('accounts.noGroup')}>{group.name ?? t('accounts.noGroup')}</h2> : null)
  const listHead = (
    <li className="list-head" aria-hidden="true">
      <span>{t('common.name')}</span>
      <span>{t('common.type')}</span>
      <span className="acct-number">{t('accounts.number')}</span>
      <span className="acct-last">{t('accounts.lastActivity')}</span>
      <span className="is-end">{t('accounts.thisMonth')}</span>
      <span className="is-end">{t('accounts.balance')}</span>
      <span />
    </li>
  )
  const transferDialog = dialog?.kind === 'transfer' ? dialog : null
  const editedTransfer = transferDialog?.transfer ?? null

  return (
    <section className="tx-page accounts-page">
      <div className="tx-title one-line">
        <h1>{t('accounts.title')}</h1>
        {accountId ? (
          // an account's page: what can be added to it (an archived account takes nothing); two by two on a phone, like the debts
          chosen && !chosen.archived ? (
            <div className="tx-actions debts-actions acct-actions">
              <Button onClick={() => addEntry('EXPENSE', chosen)} {...tip('new', t('accounts.addExpense'))}>
                {t('accounts.addExpense')}
              </Button>
              <Button variant="outline" onClick={() => addEntry('INCOME', chosen)}>
                {t('accounts.addIncome')}
              </Button>
              {canTransfer ? (
                <Button variant="outline" onClick={() => addEntry('TRANSFER', chosen)}>
                  {t('transfers.add')}
                </Button>
              ) : null}
            </div>
          ) : null
        ) : tab === 'accounts' ? (
          <div className="tx-actions">
            <Button onClick={() => setDialog({ kind: 'add' })} {...tip('new', t('accounts.add'))}>
              {t('accounts.add')}
            </Button>
            {canTransfer ? (
              <Button variant="outline" onClick={() => setDialog({ kind: 'transfer', transfer: null })}>
                {t('transfers.add')}
              </Button>
            ) : null}
          </div>
        ) : null}
      </div>
      {accountId ? null : (
        <SectionTabs
          label={t('accounts.title')}
          value={tab}
          onChange={(id) => setParams(tabQuery(id), { replace: true })}
          tabs={[
            { id: 'accounts', label: t('accounts.title') },
            { id: 'income', label: t('budgets.income') },
          ]}
        />
      )}
      {/* the dialogs and the pane are siblings: each key says what it is, or the pane of account 4 and the dialog of account 4 are one child */}
      {dialog?.kind === 'add' || dialog?.kind === 'edit' ? (
        <AccountModal
          key={dialog.kind === 'edit' ? `account:${dialog.account.id}` : 'account:new'}
          account={dialog.kind === 'edit' ? dialog.account : null}
          today={now.date}
          mainCurrency={settings.currency}
          groups={groups.flatMap((group) => group.name ?? [])}
          onClose={() => setDialog(null)}
          onSaved={() => {
            // the dialog may have been closed while it saved, and another opened since: only its own is closed
            setDialog((current) => (current === dialog ? null : current))
            refresh()
          }}
        />
      ) : null}
      {transferDialog && accounts ? (
        <TransferModal
          key={`transfer:${editedTransfer?.id ?? 'new'}`}
          transfer={editedTransfer}
          accounts={accounts}
          fromId={transferDialog.fromId}
          today={now.date}
          onClose={() => setDialog(null)}
          onSaved={() => {
            setDialog((current) => (current === dialog ? null : current))
            refresh()
          }}
          onDelete={editedTransfer ? () => setDialog({ kind: 'deleteTransfer', id: editedTransfer.id, name: t('transfers.add') }) : undefined}
        />
      ) : null}
      {dialog?.kind === 'entry' ? (
        <EntryDialog
          key={`entry:${dialog.tab}:${dialog.accountId}`}
          initial={{ tab: dialog.tab, accountId: dialog.accountId }}
          today={now.date}
          onClose={() => setDialog(null)}
          onSaved={refresh}
        />
      ) : null}
      {dialog?.kind === 'archive' ? (
        <ConfirmModal
          title={t('rec.confirmTitle', { action: t('accounts.archive'), name: nameOf(dialog.account) })}
          text={
            <>
              {t('accounts.archiveText')}
              {dialog.account.balance === '0.00' ? null : ` ${t('accounts.archiveHolds', { amount: moneyOf(dialog.account)(dialog.account.balance) })}`}
            </>
          }
          danger={false}
          pending={pending}
          onConfirm={() => void run(() => accountsApi.archive(dialog.account.id))}
          onClose={() => setDialog(null)}
        />
      ) : null}
      {dialog?.kind === 'delete' ? (
        <ConfirmModal
          title={t('rec.confirmTitle', { action: t('common.delete'), name: nameOf(dialog.account) })}
          text={t('accounts.deleteText')}
          pending={pending}
          // the page of an account that is gone has nothing to show
          onConfirm={() => void run(() => accountsApi.remove(dialog.account.id), () => (accountId === dialog.account.id ? navigate('/app/accounts') : undefined))}
          onClose={() => setDialog(null)}
        />
      ) : null}
      {dialog?.kind === 'deleteTransfer' ? (
        <ConfirmModal
          title={t('rec.confirmTitle', { action: t('common.delete'), name: dialog.name })}
          pending={pending}
          onConfirm={() => void run(() => transfersApi.remove(dialog.id))}
          onClose={() => setDialog(null)}
        />
      ) : null}
      {tab === 'income' ? <IncomeTab settings={settings} thisMonth={now.month} /> : null}
      {accounts && tab === 'accounts' ? (
        // its own pane for each page, so that an account opens at its top whatever the list was scrolled to
        <div key={`pane:${accountId ?? 'list'}`} className="tx-pane wide pane-stack" {...(accountId ? {} : tabPanelProps(tab))}>
          {accountId ? (
            chosen ? (
              <AccountPage
                key={chosen.id}
                account={chosen}
                settings={settings}
                now={now}
                reload={reload}
                onEdit={() => setDialog({ kind: 'edit', account: chosen })}
                onRestore={() => void run(() => accountsApi.unarchive(chosen.id))}
                onEditTransfer={(id) => void editTransfer(id)}
                onDeleteTransfer={(id, name) => setDialog({ kind: 'deleteTransfer', id, name })}
              />
            ) : (
              <>
                <div className="debts-name">
                  <Link to="/app/accounts" className={buttonClass('outline', 'sm')}>
                    {t('common.back')}
                  </Link>
                </div>
                <p className="notice">{errorText(new ApiError('not_found'))}</p>
              </>
            )
          ) : (
            <>
              {summary ? (
                <p className="acct-total">
                  <span className="dash-stat-label">{t('accounts.inAccounts')}</span> <b>{money(summary.inAccounts)}</b>
                </p>
              ) : null}
              {active.length > 0 && phone
                ? groups.map((group) => (
                    <Fragment key={group.name ?? ''}>
                      {groupTitle(group)}
                      <ul className="card-list">{group.accounts.map(tapRow)}</ul>
                    </Fragment>
                  ))
                : null}
              {active.length > 0 && !phone
                ? // Written out, not a `Table`: each list is what its reordering listens on, so each group has its own, and the head is over the first only.
                  groups.map((group, at) => (
                    <GroupList key={group.name ?? ''} group={group} title={groupTitle(group)} head={at === 0 ? listHead : null} onReorder={onReorderGroup}>
                      {tableRow}
                    </GroupList>
                  ))
                : null}
              {archived.length > 0 ? (
                <div className="debts-bar">
                  <label>
                    <input
                      type="checkbox"
                      checked={showArchived}
                      onChange={(event) => {
                        saveChoice('accounts.archived', event.target.checked)
                        setShowArchived(event.target.checked)
                      }}
                    />
                    {t('accounts.archivedCount', { count: archived.length })}
                  </label>
                </div>
              ) : null}
              {showArchived && archived.length > 0 && phone ? <ul className="card-list">{archived.map(tapRow)}</ul> : null}
              {showArchived && archived.length > 0 && !phone ? (
                <Table className="acct-archived" heads={[t('common.name'), t('common.type'), { label: t('accounts.balance'), className: 'is-end' }, '']}>
                  {archived.map(archivedRow)}
                </Table>
              ) : null}
            </>
          )}
        </div>
      ) : null}
    </section>
  )
}

/**
 * One run of accounts on the desktop, with the heading of its group when there are groups. Each has its own list, because the list is
 * what the reordering listens on: an account moves among the accounts of its own group, and the first and the last stop at its edges.
 */
function GroupList({
  group,
  title,
  head,
  onReorder,
  children,
}: {
  group: AccountGroup
  title: ReactNode
  /** The row of column names, over the first list only. */
  head: ReactNode
  onReorder: (group: string | null, ids: string[]) => void
  /** Draws a row; `hintId` names the text that tells how to reorder. */
  children: (account: Account, hintId: string) => ReactNode
}) {
  const t = useT()
  const items = useMemo(() => group.accounts.map((account) => ({ id: account.id, name: accountName(account, t), deleted: false })), [group.accounts, t])
  const { listRef, hintId, live } = useReorder(items, (ids) => onReorder(group.name, ids), 'accounts.reordered')
  return (
    <>
      {title}
      <ul className="card-list is-table acct-list" ref={listRef}>
        {head}
        {group.accounts.map((account) => children(account, hintId))}
      </ul>
      <span id={hintId} className="sr-only">
        {t('accounts.reorderHint')}
      </span>
      <div className="sr-only" aria-live="polite">
        {live}
      </div>
    </>
  )
}

/**
 * One account: back and edit on one line (transfer, over them, starts from this account); its name and balance; its type and number; the balance over a year; then a month to
 * step through, with what moved the account in it and the balance after each line. An archived account can only be looked at and restored.
 */
function AccountPage({
  account,
  settings,
  now,
  reload,
  onEdit,
  onRestore,
  onEditTransfer,
  onDeleteTransfer,
}: {
  account: Account
  settings: MoneySettings
  now: Today
  /** Read again when this changes. */
  reload: number
  onEdit: () => void
  onRestore: () => void
  onEditTransfer: (id: string) => void
  onDeleteTransfer: (id: string, name: string) => void
}) {
  const t = useT()
  const phone = useProfile() === 'phone'
  const navigate = useNavigate()
  const goToTransaction = useGoToTransaction()
  const say = useToast()
  const rows = useOpenRow()
  const [month, setMonth] = useState(now.month)
  const [statement, setStatement] = useState<AccountStatement | null>(null)
  const [breakdown, setBreakdown] = useState<Breakdown | null>(null)
  // The page of an account is in the account's currency: its title, balance and what is available, the statement (its lines and the balance after
  // each) and the two lists by category. The value in the app's currency is the one thing in the app's own.
  const appMoney = (amount: string) => formatMoney(amount, settings.currency, settings.locale)
  const money = account.foreign ? moneyIn(account.currency, settings.locale) : appMoney
  const statementMoney = (shown: { currency: string }) => (account.foreign ? moneyIn(shown.currency, settings.locale) : appMoney)
  // in the lines of the statement an amount may break after its code (see the lists of accounts)
  const lineWrap = account.foreign ? ' is-wrap' : ''

  useEffect(() => {
    let cancelled = false
    void accountsApi.statement(account.id, `${month}-01`, lastDay(month)).then(
      (loaded) => {
        if (!cancelled) setStatement(loaded)
      },
      (failure) => {
        if (!cancelled) say(errorText(failure), 'error')
      },
    )
    return () => {
      cancelled = true
    }
  }, [account.id, month, reload, say])

  useEffect(() => {
    let cancelled = false
    void accountsApi.breakdown(account.id, `${month}-01`, lastDay(month)).then(
      (loaded) => {
        if (!cancelled) setBreakdown(loaded)
      },
      (failure) => {
        if (!cancelled) say(errorText(failure), 'error')
      },
    )
    return () => {
      cancelled = true
    }
  }, [account.id, month, reload, say])

  // Keyboard: [ and ] step through months, t jumps to this month.
  useShortcut('prev', () => setMonth((current) => shiftMonth(current, -1)))
  useShortcut('next', () => setMonth((current) => shiftMonth(current, 1)))
  useShortcut('today', () => setMonth(now.month))

  // Newest first: the balance after each line still reads the same. The lines of another month are not shown under this one's name.
  // the same for the two lists by category: a month that was asked for earlier is not shown under this one's name
  const shown = breakdown && breakdown.from.slice(0, 7) === month ? breakdown : null
  // the lines and the two lists say which currency they are in (the account's)
  const lineMoney = (amount: string) => {
    const text = (statement ? statementMoney(statement) : money)(amount)
    return account.foreign ? breakable(text) : text
  }
  const lines = useMemo(() => (statement && statement.from.slice(0, 7) === month ? [...statement.lines].reverse() : null), [statement, month])
  const keyOf = (line: AccountLine) => `${line.source}-${line.id}`
  const isTransfer = (line: AccountLine) => line.source === 'TRANSFER_IN' || line.source === 'TRANSFER_OUT'
  const amountCell = (line: AccountLine) => {
    const view = signedView(line.amount, lineMoney)
    return <span className={`cell-amount ${view.tone}${lineWrap}`}>{view.text}</span>
  }

  /** The line of a friend's debts: the friend is found by name (a line names them, it has no id of theirs), and their history opens on it. */
  async function goToFriend(line: AccountLine) {
    try {
      const friend = (await debts.people(true)).find((person) => person.name === line.other)
      navigate(friend ? `/app/debts/${friend.id}` : '/app/debts', { state: friend ? { focus: { kind: 'ledger_entry', id: line.id, date: line.date } } : null })
    } catch (failure) {
      say(errorText(failure), 'error')
    }
  }

  // A desktop row: the day, what it was, the category (or the other account, or the friend), what it did to the account, the balance
  // after it, then where it leads: the transaction, the friend, or edit and delete for a transfer.
  function tableRow(line: AccountLine) {
    const words = lineWords(line, t)
    return (
      <Row
        key={keyOf(line)}
        actions={
          <>
            {line.source === 'TRANSACTION' ? (
              <IconAction
                icon="transactions"
                label={t('debts.goToTransaction')}
                data-row-focus
                onClick={() => goToTransaction({ transactionId: line.id, transactionDate: line.date, date: line.date })}
              />
            ) : null}
            {line.source === 'LEDGER' ? <IconAction icon="debts" label={`${t('nav.debts')}: ${line.other ?? ''}`} data-row-focus onClick={() => void goToFriend(line)} /> : null}
            {isTransfer(line) ? (
              <>
                <IconAction icon="pencil" label={t('common.edit')} data-row-focus onClick={() => onEditTransfer(line.id)} />
                <IconAction icon="trash" label={t('common.delete')} danger onClick={() => onDeleteTransfer(line.id, words.what)} />
              </>
            ) : (
              <span className="icon-slot" />
            )}
          </>
        }
      >
        <span className="cell-sub line-date">{formatDate(line.date, settings.locale)}</span>
        <div className="cell-name">
          <span className="card-name" title={words.what}>
            {words.what}
          </span>
        </div>
        <span className="cell-sub line-other" title={words.other}>
          {words.other}
        </span>
        {amountCell(line)}
        <span className={`cell-sub-end${lineWrap}`}>{lineMoney(line.balance)}</span>
      </Row>
    )
  }

  // A phone row: what it was and what it did to the account, the other side and the balance after it under them. It opens to a
  // labelled button for where it leads; the opening balance leads nowhere and is plain.
  function tapRow(line: AccountLine) {
    const words = lineWords(line, t)
    const key = keyOf(line)
    return (
      <TapRow
        key={key}
        id={rows.panelId(key)}
        open={rows.isOpen(key)}
        onToggle={() => rows.toggle(key)}
        name={words.what}
        amount={amountCell(line)}
        sub={
          <>
            {words.other ? <span>{words.other}</span> : null}
            <span>{`${t('accounts.balanceAfter')} ${lineMoney(line.balance)}`}</span>
          </>
        }
      >
        {line.source === 'OPENING' ? null : (
          <div className="row-panel-actions">
            {line.source === 'TRANSACTION' ? (
              <Button variant="outline" onClick={() => goToTransaction({ transactionId: line.id, transactionDate: line.date, date: line.date })}>
                <Icon name="transactions" className="btn-icon" />
                {t('debts.goToTransaction')}
              </Button>
            ) : null}
            {line.source === 'LEDGER' ? (
              <Button variant="outline" onClick={() => void goToFriend(line)}>
                <Icon name="debts" className="btn-icon" />
                {`${t('nav.debts')}: ${line.other ?? ''}`}
              </Button>
            ) : null}
            {isTransfer(line) ? (
              <>
                <Button variant="outline" onClick={() => onEditTransfer(line.id)}>
                  <Icon name="pencil" className="btn-icon" />
                  {t('common.edit')}
                </Button>
                <Button variant="outline" className="row-danger" onClick={() => onDeleteTransfer(line.id, words.what)}>
                  <Icon name="trash" className="btn-icon" />
                  {t('common.delete')}
                </Button>
              </>
            ) : null}
          </div>
        )}
      </TapRow>
    )
  }

  const details = [
    t(`accounts.kind.${account.kind}`),
    account.number,
    account.bic && `${t('accounts.bic')} ${account.bic}`,
    account.group,
    account.foreign && account.rate !== null && t('accounts.rateLine', { main: settings.currency.toUpperCase(), rate: account.rate, code: account.currency.toUpperCase() }),
    !account.includeInNetWorth && t('accounts.notInNetWorth'),
  ]
    .filter(Boolean)
    .join(' · ')
  return (
    <>
      <div className="debts-name">
        <Link to="/app/accounts" className={buttonClass('outline', 'sm')}>
          {t('common.back')}
        </Link>
        {account.archived ? (
          <Button variant="outline" size="sm" onClick={onRestore}>
            {t('accounts.restore')}
          </Button>
        ) : (
          <Button variant="outline" size="sm" onClick={onEdit}>
            {t('common.edit')}
          </Button>
        )}
      </div>
      {/* The title of the page is the account and what it holds. */}
      <h2 className="debts-heading debts-balance">
        {accountLabel(account, t)}: <span className={account.balance.startsWith('-') ? 'debt-owe' : undefined}>{money(account.balance)}</span>
      </h2>
      {account.foreign ? <p className="hint acct-available-line">{t('accounts.inMain', { amount: appMoney(account.balanceInMain) })}</p> : null}
      {showAvailable(account) ? (
        <p className="hint acct-available-line">
          {t('accounts.available')}: <b>{money(account.available)}</b>
        </p>
      ) : null}
      <p className="hint">{details}</p>
      {account.notes ? <p className="acct-notes">{account.notes}</p> : null}
      {account.isMain ? <p className="hint">{t('accounts.mainHint')}</p> : null}
      <Card className="acct-chart">
        <CardHeader>
          <CardAction>
            <YearNav month={month} thisMonth={now.month} onChange={setMonth} />
          </CardAction>
        </CardHeader>
        <BalanceChart accountId={account.id} currency={account.foreign ? account.currency : settings.currency} month={month} reload={reload} onPick={setMonth} settings={settings} />
      </Card>
      <MonthNav month={month} thisMonth={now.month} onChange={setMonth} />
      {shown ? (
        <div className="acct-breakdown">
          <ShareList title={t('accounts.spentByCategory')} shares={shown.expenses} total={shown.expenseTotal} tone="expense" money={statementMoney(shown)} />
          <ShareList title={t('accounts.cameInByCategory')} shares={shown.income} total={shown.incomeTotal} tone="income" money={statementMoney(shown)} />
        </div>
      ) : null}
      {lines && lines.length === 0 ? <p className="hint">{t('accounts.empty')}</p> : null}
      {lines && lines.length > 0 && phone ? (
        <DayGroups rows={lines} dateOf={(line) => line.date} locale={settings.locale} today={now.date}>
          {tapRow}
        </DayGroups>
      ) : null}
      {lines && lines.length > 0 && !phone ? (
        <Table
          className="acct-lines"
          heads={[t('report.date'), t('debts.what'), { label: '', className: 'line-other' }, { label: t('tx.amount'), className: 'is-end' }, { label: t('accounts.balanceAfter'), className: 'is-end' }, '']}
        >
          {lines.map(tableRow)}
        </Table>
      ) : null}
    </>
  )
}
