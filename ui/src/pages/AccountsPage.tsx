import { useEffect, useMemo, useRef, useState, type MouseEvent, type ReactNode } from 'react'
import { Link, Navigate, useLocation, useNavigate, useParams } from 'react-router-dom'
import { accounts as accountsApi, isLiability, type Account, type AccountLine, type AccountStatement, type AccountsSummary, type Breakdown } from '../api/accounts'
import { debts } from '../api/debts'
import { ApiError, errorText } from '../api/invoke'
import { shiftMonth } from '../api/months'
import { stashes as stashesApi, type Stash } from '../api/stashes'
import { useToday, type Today } from '../api/today'
import { transfers as transfersApi, type Transfer } from '../api/transfers'
import AccountModal from '../components/AccountModal'
import BalanceChart from '../components/BalanceChart'
import ConfirmModal from '../components/ConfirmModal'
import EntryDialog from '../components/EntryDialog'
import Facts, { type FactItem } from '../components/Facts'
import LoanPayModal from '../components/LoanPayModal'
import MonthNav from '../components/MonthNav'
import Icon from '../components/NavIcon'
import PageHeader from '../components/PageHeader'
import { DayGroups, IconAction, ListGroup, NameMark, PanelActions, Row, Table, TapRow, type PanelAction } from '../components/RowParts'
import ShareList from '../components/ShareList'
import StashModal from '../components/StashModal'
import { StashMini } from '../components/StashParts'
import TransferModal from '../components/TransferModal'
import YearNav from '../components/YearNav'
import Button from '../components/ui/Button'
import { Card, CardAction, CardHeader } from '../components/ui/Card'
import { cn } from '../components/ui/cn'
import { useGoToTransaction } from '../hooks/useFocusRequest'
import { useMoneySettings, type MoneySettings } from '../hooks/useMoneySettings'
import { useOpenRow } from '../hooks/useOpenRow'
import { useProfile } from '../hooks/profile'
import { useReorder } from '../hooks/useReorder'
import { useToast } from '../hooks/toast'
import { useT } from '../i18n/context'
import { useShortcut } from '../keys/registry'
import { useRowNavigation } from '../keys/useRowNavigation'
import { accountLabel, accountName, activeAccounts, freeOf, groupAccounts, lineWords, nameMark, oldIncomeAddress, reorderWithinGroup, showAvailable, signedView, type AccountGroup } from '../lib/accountsView'
import type { EntryTab } from '../lib/entryDialog'
import { formatDate, formatMoney, moneyIn, sumAmounts } from '../lib/money'
import { lastDay } from '../lib/range'
import { applyOrder } from '../lib/reorder'
import { saveChoice, storedChoice } from '../lib/sticky'
import { loanFacts, rateWords } from '../lib/loansView'
import { activeStashes, isSome, overBooked } from '../lib/stashView'

/** The dialog that is open. A transfer being changed is read first, so the dialog opens with its row. */
type Dialog =
  | { kind: 'add' }
  | { kind: 'edit'; account: Account }
  | { kind: 'transfer'; transfer: Transfer | null; fromId?: string }
  | { kind: 'entry'; tab: EntryTab; accountId: string; amount?: string; name?: string }
  | { kind: 'pay'; liability: Account }
  | { kind: 'stash'; accountId: string }
  | { kind: 'archive'; account: Account }
  | { kind: 'delete'; account: Account }
  | { kind: 'deleteTransfer'; id: string; name: string }
  | null

export default function AccountsPage() {
  const { settings, error } = useMoneySettings()
  const now = useToday()
  const { accountId } = useParams()
  const { search } = useLocation()
  // the income tab this page had for a while is on the categories page again: an old link goes there
  if (!accountId && oldIncomeAddress(search)) return <Navigate to="/app/categories?tab=income" replace />
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
  // a row was just dropped in a new place: the click that ends the drag must not open the account
  const dropped = useRef(false)
  const money = (amount: string) => formatMoney(amount, settings.currency, settings.locale)
  /** What an account holds, moved or has in the account's own currency (the app's for one in the app's currency, as always). */
  const moneyOf = (account: Pick<Account, 'foreign' | 'currency'>) => (account.foreign ? moneyIn(account.currency, settings.locale) : money)

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

  // the list holds the assets: the credits, loans and mortgages are on the debts page
  const assets = useMemo(() => (accounts ?? []).filter((account) => !isLiability(account.kind)), [accounts])
  const active = useMemo(() => activeAccounts(assets), [assets])
  // the list as it is shown: one run of accounts, or one for each group (the groups in use are what the group field offers)
  const groups = useMemo(() => groupAccounts(assets), [assets])
  const grouped = groups.some((group) => group.name !== null)
  const archived = useMemo(() => assets.filter((account) => account.archived), [assets])
  // a transfer needs two accounts to choose from, a loan being one of them
  const canTransfer = activeAccounts(accounts ?? []).length >= 2
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
  /** Book interest: an income in the account, filled in with what it returns in a month. */
  const bookInterest = (account: Account) => setDialog({ kind: 'entry', tab: 'INCOME', accountId: account.id, amount: account.returnPerMonth ?? undefined, name: t('accounts.interestNote') })

  // Keyboard: n adds an account (an expense, on an account's page; a payment on the page of a debt), j and k move between rows.
  useShortcut('new', () => (chosen && !chosen.archived ? (isLiability(chosen.kind) ? setDialog({ kind: 'pay', liability: chosen }) : addEntry('EXPENSE', chosen)) : setDialog({ kind: 'add' })))
  useRowNavigation()

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
    const view = signedView(account.changeThisMonth, moneyOf(account))
    return (
      <span className={`${className} ${view.tone}`}>
        <span className="sr-only">{t('accounts.thisMonth')} </span>
        {view.text}
      </span>
    )
  }
  /** The mark after the name of an account that the net worth leaves out: an eye with a slash. A mark is no button, so it may have a title. */
  const mark = (account: Account) => {
    const words = nameMark(account, t)
    return words === '' ? null : (
      <span className="row-mark-icon" role="img" aria-label={words} title={words}>
        <Icon name="eye-off" className="mark-icon" />
      </span>
    )
  }
  /** Delete for an account with nothing in it, archive for one with rows; the main account has neither. */
  const removal = (account: Account): 'delete' | 'archive' | null => (account.isMain ? null : account.entries === 0 ? 'delete' : 'archive')
  /** What the account can spend; over-booked (the stashes hold more than it has) it is red. */
  const freeText = (account: Account) => moneyOf(account)(freeOf(account))
  const freeTone = (account: Account) => (overBooked(freeOf(account)) ? 'is-over' : '')

  // A desktop row: grip, name, type, this month, set aside, free, balance, then edit and archive (or delete, for an empty one); an archived
  // account has restore in their place. Every cell is one line. The grip says that the row can be held and dragged. The name is the link for
  // the keyboard (j and k stop on it).
  function tableRow(account: Account, hintId?: string) {
    const way = removal(account)
    return (
      <Row
        key={account.id}
        data-reorder-id={account.archived ? undefined : account.id}
        data-archived={account.archived || undefined}
        onClick={(event) => open(event, account)}
        actions={
          account.archived ? (
            <>
              <IconAction icon="export" label={t('accounts.restore')} onClick={() => void run(() => accountsApi.unarchive(account.id))} />
              {way === 'delete' ? <IconAction icon="trash" label={t('common.delete')} danger onClick={() => setDialog({ kind: 'delete', account })} /> : <span className="icon-slot" />}
            </>
          ) : (
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
          )
        }
      >
        <div className="cell-name">
          {account.archived ? null : <Icon name="grip" className="row-grip" />}
          <span className="acct-name">
            <Link className="card-name" to={link(account)} title={nameOf(account)} data-row-focus aria-describedby={hintId}>
              {nameOf(account)}
            </Link>
            {mark(account)}
          </span>
        </div>
        <span className="cell-sub acct-type">{kindText(account)}</span>
        {monthCell(account, 'cell-amount acct-month')}
        <span className="cell-sub-end acct-aside">{isSome(account.setAside) ? moneyOf(account)(account.setAside) : <span className="cell-none">—</span>}</span>
        <span className={cn('cell-sub-end acct-free', freeTone(account))}>{freeText(account)}</span>
        <span className="cell-amount acct-balance">{moneyOf(account)(account.balance)}</span>
      </Row>
    )
  }

  /** What an account's open row on a phone shows, one fact to a line; a fact it has nothing to say for is left out. */
  function accountFacts(account: Account): FactItem[] {
    const view = signedView(account.changeThisMonth, moneyOf(account))
    const free = freeOf(account)
    const facts: (FactItem | null)[] = [
      { label: t('common.type'), value: kindText(account) },
      { label: t('accounts.number'), value: account.number },
      account.archived ? null : { label: t('accounts.thisMonth'), value: view.text, tone: view.tone === 'is-income' ? 'positive' : view.tone === 'is-expense' ? 'negative' : undefined },
      { label: t('accounts.setAsideHead'), value: isSome(account.setAside) ? moneyOf(account)(account.setAside) : null },
      // the same as the balance when nothing is set aside and there is no limit: it says nothing more then
      free === account.balance ? null : { label: t('accounts.free'), value: moneyOf(account)(free), tone: overBooked(free) ? 'negative' : undefined },
      { label: t('accounts.lastActivity'), value: lastText(account) },
      { label: t('accounts.rate'), value: rateWords(account, t) },
      { label: t('accounts.returns'), value: account.returnPerMonth !== null && isSome(account.returnPerMonth) ? t('accounts.returnPerMonth', { amount: moneyOf(account)(account.returnPerMonth) }) : null },
      showAvailable(account) ? { label: t('accounts.virtualBalance'), value: moneyOf(account)(account.virtualBalance) } : null,
      showAvailable(account) ? { label: t('accounts.available'), value: moneyOf(account)(account.available) } : null,
      account.foreign ? { label: t('accounts.worthIn', { code: settings.currency.toUpperCase() }), value: money(account.balanceInMain) } : null,
      account.includeInNetWorth ? null : { label: t('accounts.netWorth'), value: t('accounts.offNetWorth'), tone: 'muted' },
    ]
    return facts.filter((fact) => fact !== null)
  }

  // A phone row: the name (with the mark) and the balance, the type under the name. It opens to the facts and a button for everything that can be done.
  function tapRow(account: Account) {
    const way = removal(account)
    const actions: (PanelAction | false)[] = [
      { label: t('accounts.open'), icon: 'play', to: link(account) },
      account.archived
        ? { label: t('accounts.restore'), icon: 'export', onClick: () => void run(() => accountsApi.unarchive(account.id)) }
        : { label: t('common.edit'), icon: 'pencil', onClick: () => setDialog({ kind: 'edit', account }) },
      !account.archived && canTransfer && { label: t('transfers.add'), icon: 'sync', onClick: () => setDialog({ kind: 'transfer', transfer: null, fromId: account.id }) },
      way === 'archive' && !account.archived && { label: t('accounts.archive'), icon: 'archive', onClick: () => setDialog({ kind: 'archive', account }) },
      way === 'delete' && { label: t('common.delete'), icon: 'trash', danger: true, onClick: () => setDialog({ kind: 'delete', account }) },
    ]
    return (
      <TapRow
        key={account.id}
        id={rows.panelId(account.id)}
        data-archived={account.archived || undefined}
        open={rows.isOpen(account.id)}
        onToggle={() => rows.toggle(account.id)}
        name={<NameMark mark={mark(account)}>{nameOf(account)}</NameMark>}
        amount={<span className="cell-amount">{moneyOf(account)(account.balance)}</span>}
        meta={kindText(account)}
        facts={accountFacts(account)}
      >
        <PanelActions actions={actions} />
      </TapRow>
    )
  }

  /** What a group's accounts hold together, in the app's currency (each at its rate). */
  const groupSum = (group: AccountGroup) => money(sumAmounts(group.accounts.map((account) => account.balanceInMain)))
  /** The row under a group's accounts on the desktop: that sum. */
  const groupTotal = (group: AccountGroup): ReactNode => (
    <li className="card-item acct-subtotal">
      <span className="cell-name">{t('accounts.groupTotal')}</span>
      <span className="cell-amount">{groupSum(group)}</span>
    </li>
  )
  /** The heading of a group, when there are groups: the accounts that have none come last, under "no group". */
  const groupName = (group: AccountGroup): string => group.name ?? t('accounts.noGroup')
  const groupTitle = (group: AccountGroup): ReactNode => (grouped ? <h2 className="acct-group" title={groupName(group)}>{groupName(group)}</h2> : null)
  const listHead = (
    <li className="list-head" aria-hidden="true">
      <span>{t('common.name')}</span>
      <span className="acct-type">{t('common.type')}</span>
      <span className="is-end acct-month">{t('accounts.thisMonth')}</span>
      <span className="is-end acct-aside">{t('accounts.setAsideHead')}</span>
      <span className="is-end acct-free">{t('accounts.free')}</span>
      <span className="is-end">{t('accounts.balance')}</span>
      <span />
    </li>
  )
  /** The top of the list: add an account and transfer, a switch for the archived ones, and what all accounts hold. */
  const listHeader = () => ({
    actions: [
      { label: t('accounts.add'), icon: 'add', primary: true, shortcut: 'new' as const, onClick: () => setDialog({ kind: 'add' }) },
      canTransfer && { label: t('transfers.add'), icon: 'sync', onClick: () => setDialog({ kind: 'transfer', transfer: null }) },
    ],
    more: [
      archived.length > 0 && {
        kind: 'toggle' as const,
        label: t('accounts.showArchived', { count: archived.length }),
        checked: showArchived,
        onChange: (checked: boolean) => {
          saveChoice('accounts.archived', checked)
          setShowArchived(checked)
        },
      },
    ],
    aside: summary ? (
      <>
        {t('accounts.inAccounts')} <b>{money(summary.inAccounts)}</b>
      </>
    ) : null,
  })

  /**
   * The top of an account's page: back, the balance (what is owed, for a debt), what can be added to it (an archived account takes nothing, and can
   * be restored), and edit, transfer and the interest in the "more" menu. The main account says what it is behind a help button.
   */
  const detailHeader = () => {
    const back = chosen && isLiability(chosen.kind) ? '/app/debts?tab=debts' : '/app/accounts'
    if (!chosen) return { back }
    const owing = chosen.liability !== null
    const balance = (
      <>
        {t(owing ? 'accounts.owed' : 'accounts.balance')} <b className={owing ? (chosen.liability?.state === 'OWING' ? 'debt-owe' : undefined) : chosen.balance.startsWith('-') ? 'debt-owe' : undefined}>{moneyOf(chosen)(chosen.liability ? chosen.liability.owed : chosen.balance)}</b>
      </>
    )
    const help = chosen.isMain ? <p>{t('accounts.mainHint')}</p> : undefined
    if (chosen.archived) {
      return { back, aside: balance, help, actions: [{ label: t('accounts.restore'), icon: 'export', primary: true, onClick: () => void run(() => accountsApi.unarchive(chosen.id)) }] }
    }
    if (owing) {
      return {
        back,
        aside: balance,
        help,
        actions: [{ label: t('loans.pay'), icon: 'check', primary: true, shortcut: 'new' as const, onClick: () => setDialog({ kind: 'pay', liability: chosen }) }],
        more: [{ label: t('common.edit'), icon: 'pencil', onSelect: () => setDialog({ kind: 'edit', account: chosen }) }],
      }
    }
    return {
      back,
      aside: balance,
      help,
      actions: [
        { label: t('accounts.addExpense'), icon: 'add', primary: true, shortcut: 'new' as const, onClick: () => addEntry('EXPENSE', chosen) },
        { label: t('accounts.addIncome'), icon: 'download', onClick: () => addEntry('INCOME', chosen) },
      ],
      more: [
        { label: t('common.edit'), icon: 'pencil', onSelect: () => setDialog({ kind: 'edit', account: chosen }) },
        canTransfer && { label: t('transfers.add'), icon: 'sync', onSelect: () => addEntry('TRANSFER', chosen) },
        chosen.returnPerMonth !== null && isSome(chosen.returnPerMonth) && { label: t('accounts.bookInterest'), icon: 'recurring', onSelect: () => bookInterest(chosen) },
      ],
    }
  }
  const transferDialog = dialog?.kind === 'transfer' ? dialog : null
  const editedTransfer = transferDialog?.transfer ?? null

  return (
    <section className="tx-page accounts-page">
      <PageHeader
        title={chosen ? accountLabel(chosen, t) : t('accounts.title')}
        {...(accountId ? detailHeader() : listHeader())}
      />
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
          initial={{ tab: dialog.tab, accountId: dialog.accountId, amount: dialog.amount, name: dialog.name }}
          today={now.date}
          onClose={() => setDialog(null)}
          onSaved={refresh}
        />
      ) : null}
      {dialog?.kind === 'pay' && accounts ? (
        <LoanPayModal
          key={`pay:${dialog.liability.id}`}
          liability={dialog.liability}
          accounts={accounts}
          locale={settings.locale}
          today={now.date}
          onClose={() => setDialog(null)}
          onSaved={() => {
            setDialog((current) => (current === dialog ? null : current))
            refresh()
          }}
        />
      ) : null}
      {dialog?.kind === 'stash' && accounts ? (
        <StashModal
          key={`stash:new:${dialog.accountId}`}
          stash={null}
          accounts={accounts}
          accountId={dialog.accountId}
          today={now.date}
          onClose={() => setDialog(null)}
          onSaved={() => {
            setDialog((current) => (current === dialog ? null : current))
            refresh()
          }}
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
      {accounts ? (
        // its own pane for each page, so that an account opens at its top whatever the list was scrolled to
        <div key={`pane:${accountId ?? 'list'}`} className="tx-pane wide pane-stack">
          {accountId ? (
            chosen ? (
              <AccountPage
                key={chosen.id}
                account={chosen}
                settings={settings}
                now={now}
                reload={reload}
                onEditTransfer={(id) => void editTransfer(id)}
                onDeleteTransfer={(id, name) => setDialog({ kind: 'deleteTransfer', id, name })}
                onNewStash={() => setDialog({ kind: 'stash', accountId: chosen.id })}
              />
            ) : (
              <p className="notice">{errorText(new ApiError('not_found'))}</p>
            )
          ) : (
            <>
              {active.length > 0 && phone
                ? groups.map((group) => (
                    <ListGroup key={group.name ?? ''} heading={grouped ? groupName(group) : undefined} total={grouped && group.accounts.length > 1 ? groupSum(group) : undefined}>
                      {group.accounts.map(tapRow)}
                    </ListGroup>
                  ))
                : null}
              {active.length > 0 && !phone
                ? // Written out, not a `Table`: each list is what its reordering listens on, so each group has its own, and the head is over the first only.
                  groups.map((group, at) => (
                    <GroupList
                      key={group.name ?? ''}
                      group={group}
                      title={groupTitle(group)}
                      head={at === 0 ? listHead : null}
                      total={grouped && group.accounts.length > 1 ? groupTotal(group) : null}
                      onReorder={onReorderGroup}
                    >
                      {tableRow}
                    </GroupList>
                  ))
                : null}
              {/* the archived ones, when the switch in the "more" menu is on: a group of their own, in the same columns */}
              {showArchived && archived.length > 0 && phone ? <ListGroup heading={t('accounts.archived')}>{archived.map(tapRow)}</ListGroup> : null}
              {showArchived && archived.length > 0 && !phone ? (
                <>
                  <h2 className="acct-group">{t('accounts.archived')}</h2>
                  <ul className="card-list is-table is-tidy acct-list is-archived">{archived.map((account) => tableRow(account))}</ul>
                </>
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
  total,
  onReorder,
  children,
}: {
  group: AccountGroup
  title: ReactNode
  /** The row of column names, over the first list only. */
  head: ReactNode
  /** The row of what the group holds together, under its accounts; none for a group of one. */
  total: ReactNode
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
      <ul className="card-list is-table is-tidy acct-list" ref={listRef}>
        {head}
        {group.accounts.map((account) => children(account, hintId))}
        {total}
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
 * One account's page, under the header (its back button, name, balance and buttons are in the page's `PageHeader`): its facts; the balance over a year;
 * then a month to step through, with what moved the account in it and the balance after each line. An archived account can only be looked at and
 * restored.
 */
function AccountPage({
  account,
  settings,
  now,
  reload,
  onEditTransfer,
  onDeleteTransfer,
  onNewStash,
}: {
  account: Account
  settings: MoneySettings
  now: Today
  /** Read again when this changes. */
  reload: number
  onEditTransfer: (id: string) => void
  onDeleteTransfer: (id: string, name: string) => void
  /** Opens the dialog that makes a stash, in this account. */
  onNewStash: () => void
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
  // the stashes in this account, and read again with the rest
  const [stashList, setStashList] = useState<Stash[] | null>(null)
  // The page of an account is in the account's currency: its title, balance and what is available, the statement (its lines and the balance after
  // each) and the two lists by category. The value in the app's currency is the one thing in the app's own.
  const appMoney = (amount: string) => formatMoney(amount, settings.currency, settings.locale)
  const money = account.foreign ? moneyIn(account.currency, settings.locale) : appMoney
  const statementMoney = (shown: { currency: string }) => (account.foreign ? moneyIn(shown.currency, settings.locale) : appMoney)

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

  useEffect(() => {
    let cancelled = false
    void stashesApi.list(account.id).then(
      (loaded) => {
        if (!cancelled) setStashList(loaded)
      },
      (failure) => {
        if (!cancelled) say(errorText(failure), 'error')
      },
    )
    return () => {
      cancelled = true
    }
  }, [account.id, reload, say])

  // Keyboard: [ and ] step through months, t jumps to this month.
  useShortcut('prev', () => setMonth((current) => shiftMonth(current, -1)))
  useShortcut('next', () => setMonth((current) => shiftMonth(current, 1)))
  useShortcut('today', () => setMonth(now.month))

  // Newest first: the balance after each line still reads the same. The lines of another month are not shown under this one's name.
  // the same for the two lists by category: a month that was asked for earlier is not shown under this one's name
  const shown = breakdown && breakdown.from.slice(0, 7) === month ? breakdown : null
  // the lines and the two lists say which currency they are in (the account's)
  const lineMoney = (amount: string) => (statement ? statementMoney(statement) : money)(amount)
  const lines = useMemo(() => (statement && statement.from.slice(0, 7) === month ? [...statement.lines].reverse() : null), [statement, month])
  const keyOf = (line: AccountLine) => `${line.source}-${line.id}`
  const isTransfer = (line: AccountLine) => line.source === 'TRANSFER_IN' || line.source === 'TRANSFER_OUT'
  const amountCell = (line: AccountLine) => {
    const view = signedView(line.amount, lineMoney)
    return <span className={`cell-amount ${view.tone}`}>{view.text}</span>
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
        <span className="cell-sub-end">{lineMoney(line.balance)}</span>
      </Row>
    )
  }

  // A phone row: what it was and what it did to the account, the other side under the name. It opens to the day and the balance after it, and a
  // button for where it leads; the opening balance leads nowhere.
  function tapRow(line: AccountLine) {
    const words = lineWords(line, t)
    const key = keyOf(line)
    const actions: (PanelAction | false)[] = [
      line.source === 'TRANSACTION' && {
        label: t('debts.goToTransaction'),
        icon: 'transactions',
        onClick: () => goToTransaction({ transactionId: line.id, transactionDate: line.date, date: line.date }),
      },
      line.source === 'LEDGER' && { label: `${t('nav.debts')}: ${line.other ?? ''}`, icon: 'debts', onClick: () => void goToFriend(line) },
      isTransfer(line) && { label: t('common.edit'), icon: 'pencil', onClick: () => onEditTransfer(line.id) },
      isTransfer(line) && { label: t('common.delete'), icon: 'trash', danger: true, onClick: () => onDeleteTransfer(line.id, words.what) },
    ]
    return (
      <TapRow
        key={key}
        id={rows.panelId(key)}
        open={rows.isOpen(key)}
        onToggle={() => rows.toggle(key)}
        name={words.what}
        amount={amountCell(line)}
        meta={words.other || undefined}
        facts={[
          { label: t('report.date'), value: formatDate(line.date, settings.locale) },
          { label: t('accounts.balanceAfter'), value: lineMoney(line.balance) },
        ]}
      >
        {line.source === 'OPENING' ? null : <PanelActions actions={actions} />}
      </TapRow>
    )
  }

  const owing = account.liability !== null
  // The facts of the account, one to a line. The balance is in the header; what is owed, too (the first of a debt's facts).
  const facts: (FactItem | null)[] = [
    { label: t('common.type'), value: t(`accounts.kind.${account.kind}`) },
    { label: t('accounts.number'), value: account.number },
    { label: t('accounts.bic'), value: account.bic },
    { label: t('accounts.group'), value: account.group },
    ...(owing
      ? loanFacts(account, money, settings.locale, t).filter((fact) => fact.label !== t('accounts.owed'))
      : [
          { label: t('accounts.rate'), value: rateWords(account, t) },
          { label: t('accounts.returns'), value: account.returnPerMonth !== null && isSome(account.returnPerMonth) ? t('accounts.returnPerMonth', { amount: money(account.returnPerMonth) }) : null },
        ]),
    account.foreign && account.rate !== null ? { label: t('accounts.currency'), value: t('accounts.rateLine', { main: settings.currency.toUpperCase(), rate: account.rate, code: account.currency.toUpperCase() }) } : null,
    account.foreign ? { label: t('accounts.worthIn', { code: settings.currency.toUpperCase() }), value: appMoney(account.balanceInMain) } : null,
    showAvailable(account) ? { label: t('accounts.virtualBalance'), value: money(account.virtualBalance) } : null,
    showAvailable(account) ? { label: t('accounts.available'), value: money(account.available) } : null,
    owing ? null : { label: t('accounts.setAsideHead'), value: isSome(account.setAside) ? money(account.setAside) : null },
    owing || freeOf(account) === account.balance ? null : { label: t('accounts.free'), value: money(freeOf(account)), tone: overBooked(freeOf(account)) ? 'negative' : undefined },
    { label: t('accounts.lastActivity'), value: account.lastActivity ? formatDate(account.lastActivity, settings.locale) : null },
    account.includeInNetWorth ? null : { label: t('accounts.netWorth'), value: t('accounts.offNetWorth'), tone: 'muted' },
  ]
  return (
    <>
      <Facts items={facts} className="acct-facts" />
      {account.notes ? <p className="acct-notes">{account.notes}</p> : null}
      <Card className="acct-chart">
        <CardHeader>
          <CardAction>
            <YearNav month={month} thisMonth={now.month} onChange={setMonth} />
          </CardAction>
        </CardHeader>
        <BalanceChart accountId={account.id} currency={account.foreign ? account.currency : settings.currency} month={month} reload={reload} onPick={setMonth} settings={settings} />
      </Card>
      <MonthNav month={month} thisMonth={now.month} onChange={setMonth} />
      {shown && !owing ? (
        <div className="acct-breakdown">
          <ShareList title={t('accounts.spentByCategory')} shares={shown.expenses} total={shown.expenseTotal} tone="expense" money={statementMoney(shown)} />
          <ShareList title={t('accounts.cameInByCategory')} shares={shown.income} total={shown.incomeTotal} tone="income" money={statementMoney(shown)} />
        </div>
      ) : null}
      {/* What is set aside in this account: its stashes as small cards that open them, what is set aside and what is free, and a new stash. */}
      {stashList && !owing && (stashList.length > 0 || !account.archived) ? (
        <section className="acct-stashes" aria-label={t('nav.stashes')}>
          <h2 className="debts-heading">
            <Link to="/app/stashes">{t('nav.stashes')}</Link>
          </h2>
          {stashList.length > 0 ? (
            <div className="stash-mini-list">
              {activeStashes(stashList).map((stash) => (
                <StashMini key={stash.id} stash={stash} locale={settings.locale} />
              ))}
            </div>
          ) : (
            <p className="hint">{t('stashes.noStashes')}</p>
          )}
          <div className="stash-part-foot">
            {/* what is set aside and what is free are in the facts above */}
            {overBooked(account.free) ? <p className="stash-warn">{t('stashes.overBooked')}</p> : <span />}
            {account.archived ? null : (
              <Button variant="outline" size="sm" onClick={onNewStash}>
                {t('stashes.add')}
              </Button>
            )}
          </div>
        </section>
      ) : null}
      {lines && lines.length === 0 ? <p className="hint">{t('accounts.empty')}</p> : null}
      {lines && lines.length > 0 && phone ? (
        <DayGroups rows={lines} dateOf={(line) => line.date} locale={settings.locale} today={now.date}>
          {tapRow}
        </DayGroups>
      ) : null}
      {lines && lines.length > 0 && !phone ? (
        <Table
          tidy
          className="acct-lines"
          heads={[t('report.date'), t('debts.what'), { label: '', className: 'line-other' }, { label: t('tx.amount'), className: 'is-end' }, { label: t('accounts.balanceAfter'), className: 'is-end' }, '']}
        >
          {lines.map(tableRow)}
        </Table>
      ) : null}
    </>
  )
}
