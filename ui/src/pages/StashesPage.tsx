import { useEffect, useMemo, useRef, useState, type MouseEvent } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import { ApiError, errorText } from '../api/invoke'
import { stashes as stashesApi, type Detail, type Entry, type Stash, type Summary } from '../api/stashes'
import { useToday, type Today } from '../api/today'
import { transfers as transfersApi, type Transfer } from '../api/transfers'
import PageHeader from '../components/PageHeader'
import ConfirmModal from '../components/ConfirmModal'
import Icon from '../components/NavIcon'
import Pick, { PickOption } from '../components/Pick'
import StashDetail from '../components/StashDetail'
import StashModal from '../components/StashModal'
import StashMoneyModal from '../components/StashMoneyModal'
import { IconAction, ListGroup, NameMark, PanelActions, Row, Table, TapRow, type PanelAction } from '../components/RowParts'
import { MeterBar } from '../components/StashParts'
import TransferModal from '../components/TransferModal'
import Badge from '../components/ui/Badge'
import Button from '../components/ui/Button'
import { cn } from '../components/ui/cn'
import { EmptyState } from '../components/ui/Pixel'
import { useAccounts } from '../hooks/useAccounts'
import { useMoneySettings, type MoneySettings } from '../hooks/useMoneySettings'
import { useOpenRow } from '../hooks/useOpenRow'
import { useProfile } from '../hooks/profile'
import { useReorder } from '../hooks/useReorder'
import { useToast } from '../hooks/toast'
import { useI18n } from '../i18n/context'
import { useShortcut } from '../keys/registry'
import { useRowNavigation } from '../keys/useRowNavigation'
import { accountById, accountLabel, assetAccounts, several } from '../lib/accountsView'
import { formatDate, formatMoney, fromMinor } from '../lib/money'
import { applyOrder } from '../lib/reorder'
import { saveChoice, storedChoice, storedId } from '../lib/sticky'
import { activeStashes, archivedStashes, canAdd, canBookReturn, canClose, canTake, held, isBreak, KIND_FILTERS, listTotal, rowMark, rowMeter, rowStatus, stashFacts, stashMoney, type KindFilter, type MoneyMode, type RowMeter } from '../lib/stashView'

/** The dialog that is open. A transfer being looked at is read first, so the dialog opens with its row. */
type Dialog =
  | { kind: 'new'; accountId?: string }
  | { kind: 'edit'; stash: Stash }
  | { kind: 'money'; stash: Stash; tab: MoneyMode }
  | { kind: 'entry'; stash: Stash; entry: Entry }
  | { kind: 'transfer'; transfer: Transfer }
  | { kind: 'archive'; stash: Stash }
  | { kind: 'delete'; stash: Stash }
  | null

export default function StashesPage() {
  const { settings, error } = useMoneySettings()
  const now = useToday()
  if (error) return <p className="notice">{error}</p>
  if (!settings || !now) return null
  return <StashesView settings={settings} now={now} />
}

/**
 * The stashes: money set aside inside an account for a goal. The list as rows (in the person's order, the total saved over it), the archived ones
 * apart, and, with a stash in the address, that stash's page: how far it is, its chart and its history of adds and takes. docs/stash-design.md, section 13.
 */
function StashesView({ settings, now }: { settings: MoneySettings; now: Today }) {
  const { t } = useI18n()
  const navigate = useNavigate()
  const say = useToast()
  const { stashId } = useParams()
  const [list, setList] = useState<Stash[] | null>(null)
  const [summary, setSummary] = useState<Summary | null>(null)
  const [detail, setDetail] = useState<Detail | null>(null)
  const [missing, setMissing] = useState(false)
  const [reload, setReload] = useState(0)
  const refresh = () => setReload((value) => value + 1)
  const accounts = useAccounts(reload)
  const [dialog, setDialog] = useState<Dialog>(null)
  const [pending, setPending] = useState(false)
  // the switch is in the "more" menu of the header, the archived ones are in the list: the page holds it for both
  const [showArchived, setShowArchived] = useState(() => storedChoice('stashes.archived', [false, true], false))

  useEffect(() => {
    let cancelled = false
    void Promise.all([stashesApi.list(), stashesApi.summary()]).then(
      ([stashes, sum]) => {
        if (cancelled) return
        setList(stashes)
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

  // the page of one stash: read again when the list is (something was saved), and for another stash
  useEffect(() => {
    if (!stashId) return
    let cancelled = false
    void stashesApi.get(stashId).then(
      (loaded) => {
        if (cancelled) return
        setDetail(loaded)
        setMissing(false)
      },
      (failure) => {
        if (cancelled) return
        if (failure instanceof ApiError && failure.code === 'not_found') setMissing(true)
        else say(errorText(failure), 'error')
      },
    )
    return () => {
      cancelled = true
    }
  }, [stashId, reload, say])

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

  /** A transfer that came with an entry, read so that the dialog opens with it. */
  async function openTransfer(id: string) {
    try {
      setDialog({ kind: 'transfer', transfer: await transfersApi.get(id) })
    } catch (failure) {
      say(errorText(failure), 'error')
      refresh()
    }
  }

  // The page of a stash that is not the one in the address is not shown while its own is read.
  const shown = stashId && detail && detail.stash.id === stashId && !missing ? detail : null

  // Keyboard: n makes a stash, or (on a stash's page) adds money to it; j and k move between the rows.
  useShortcut('new', () => {
    if (stashId) {
      if (shown && canAdd(shown.stash)) setDialog({ kind: 'money', stash: shown.stash, tab: 'ADD' })
      return
    }
    setDialog({ kind: 'new' })
  })
  useRowNavigation()

  /** The dialog may have been closed while it saved, and another opened since: only its own is closed. The lists are read again either way. */
  const saved = (opened: Dialog) => {
    setDialog((current) => (current === opened ? null : current))
    refresh()
  }

  const dialogs = accounts ? (
    <>
      {dialog?.kind === 'new' || dialog?.kind === 'edit' ? (
        <StashModal
          key={dialog.kind === 'edit' ? `stash:${dialog.stash.id}` : 'stash:new'}
          stash={dialog.kind === 'edit' ? dialog.stash : null}
          accounts={accounts}
          accountId={dialog.kind === 'new' ? dialog.accountId : undefined}
          today={now.date}
          onClose={() => setDialog(null)}
          onSaved={() => saved(dialog)}
        />
      ) : null}
      {dialog?.kind === 'money' || dialog?.kind === 'entry' ? (
        <StashMoneyModal
          key={dialog.kind === 'entry' ? `entry:${dialog.entry.id}` : `money:${dialog.stash.id}:${dialog.tab}`}
          stash={dialog.stash}
          tab={dialog.kind === 'money' ? dialog.tab : dialog.entry.kind}
          entry={dialog.kind === 'entry' ? dialog.entry : null}
          accounts={accounts}
          settings={settings}
          today={now.date}
          onClose={() => setDialog(null)}
          onSaved={(answer) => {
            // the answer is the whole stash: the page shows it at once, and the lists are read again
            if (answer && answer.stash.id === stashId) setDetail(answer)
            saved(dialog)
          }}
        />
      ) : null}
      {dialog?.kind === 'transfer' ? (
        <TransferModal
          key={`transfer:${dialog.transfer.id}`}
          transfer={dialog.transfer}
          accounts={accounts}
          today={now.date}
          onClose={() => setDialog(null)}
          onSaved={() => saved(dialog)}
        />
      ) : null}
    </>
  ) : null

  const confirms = (
    <>
      {dialog?.kind === 'archive' ? (
        <ConfirmModal
          title={t('rec.confirmTitle', { action: t('stashes.archive'), name: dialog.stash.name })}
          text={t('stashes.archiveText')}
          danger={false}
          pending={pending}
          onConfirm={() => void run(() => stashesApi.archive(dialog.stash.id))}
          onClose={() => setDialog(null)}
        />
      ) : null}
      {dialog?.kind === 'delete' ? (
        <ConfirmModal
          title={t('rec.confirmTitle', { action: t('common.delete'), name: dialog.stash.name })}
          text={t('stashes.deleteText', {
            account: dialog.stash.account,
            amount: formatMoney(fromMinor(held(dialog.stash)), dialog.stash.currency, settings.locale),
          })}
          pending={pending}
          // the page of a stash that is gone has nothing to show
          onConfirm={() => void run(() => stashesApi.remove(dialog.stash.id), () => (stashId === dialog.stash.id ? navigate('/app/stashes') : undefined))}
          onClose={() => setDialog(null)}
        />
      ) : null}
    </>
  )

  const archivedCount = list ? archivedStashes(list).length : 0
  const total = summary && summary.stashes > 0 ? formatMoney(summary.saved, settings.currency, settings.locale) : null

  /** The top of the list: a new stash, a switch for the archived ones, and what the stashes hold together. */
  const listHeader = () => ({
    actions: [{ label: t('stashes.add'), icon: 'add', primary: true, shortcut: 'new' as const, onClick: () => setDialog({ kind: 'new' }) }],
    more: [
      archivedCount > 0 && {
        kind: 'toggle' as const,
        label: t('accounts.showArchived', { count: archivedCount }),
        checked: showArchived,
        onChange: (checked: boolean) => {
          saveChoice('stashes.archived', checked)
          setShowArchived(checked)
        },
      },
    ],
    aside: total ? (
      <>
        {t('stashes.total')} <b>{total}</b>
      </>
    ) : null,
  })

  /**
   * The top of a stash's page: back, what it holds, what can be done with it (add, take out or break, book the return, close: two at most, the main
   * one first) and edit, archive and delete in the "more" menu. An archived stash is restored from here. What stops a stash from taking money is
   * behind a help button.
   */
  const detailHeader = () => {
    if (!shown) return { back: '/app/stashes' }
    const { stash } = shown
    const aside = (
      <>
        {t('stashes.savedLabel')} <b className={stash.saved.startsWith('-') ? 'debt-owe' : undefined}>{stashMoney(stash, settings.locale)(stash.saved)}</b>
      </>
    )
    const hint = stash.accountArchived && !stash.archived ? t('stashes.accountArchivedHint') : stash.archived && !stash.accountArchived && !canAdd(stash) ? t('stashes.archivedHint') : null
    const common = { back: '/app/stashes', aside, help: hint ? <p>{hint}</p> : undefined }
    const remove = { label: t('common.delete'), icon: 'trash', danger: true, onSelect: () => setDialog({ kind: 'delete', stash }) }
    if (stash.archived) {
      return { ...common, actions: [{ label: t('stashes.restore'), icon: 'export', primary: true, onClick: () => void run(() => stashesApi.unarchive(stash.id)) }], more: [remove] }
    }
    return {
      ...common,
      actions: [
        canAdd(stash) && { label: t('stashes.addMoney'), icon: 'add', primary: true, shortcut: 'new' as const, onClick: () => setDialog({ kind: 'money', stash, tab: 'ADD' }) },
        canBookReturn(stash) && { label: t('stashes.bookReturn'), icon: 'check', primary: true, onClick: () => setDialog({ kind: 'money', stash, tab: 'RETURN' }) },
        canTake(stash) && { label: t(isBreak(stash) ? 'stashes.breakDeposit' : 'stashes.takeOut'), icon: 'export', onClick: () => setDialog({ kind: 'money', stash, tab: 'TAKE' }) },
        // a deposit that is booked or broken and empty has nothing left to do: closing it is archiving it
        canClose(stash) && { label: t('stashes.archive'), icon: 'archive', primary: true, onClick: () => setDialog({ kind: 'archive', stash }) },
      ],
      more: [
        { label: t('common.edit'), icon: 'pencil', onSelect: () => setDialog({ kind: 'edit', stash }) },
        !canClose(stash) && { label: t('stashes.archive'), icon: 'archive', onSelect: () => setDialog({ kind: 'archive', stash }) },
        remove,
      ],
    }
  }

  return (
    <section className="tx-page stashes-page">
      <PageHeader
        // a stash's page has the stash's name, and a way back to the list
        title={stashId && shown ? shown.stash.name : t('stashes.title')}
        {...(stashId ? detailHeader() : listHeader())}
      />
      {/* the dialogs and the pane are siblings: each key says what it is */}
      {dialogs}
      {confirms}
      {list && accounts ? (
        <div key={`pane:${stashId ?? 'list'}`} className="tx-pane wide pane-stack">
          {stashId ? (
            shown ? (
              <StashDetail
                key={shown.stash.id}
                detail={shown}
                accounts={accounts}
                settings={settings}
                now={now}
                reload={reload}
                onEntry={(entry) => setDialog({ kind: 'entry', stash: shown.stash, entry })}
                onTransfer={(id) => void openTransfer(id)}
              />
            ) : missing ? (
              <p className="notice">{errorText(new ApiError('not_found'))}</p>
            ) : null
          ) : (
            <StashList
              list={list}
              accounts={accounts}
              settings={settings}
              showArchived={showArchived}
              onNew={() => setDialog({ kind: 'new' })}
              onMoney={(stash, tab) => setDialog({ kind: 'money', stash, tab })}
              onEdit={(stash) => setDialog({ kind: 'edit', stash })}
              onArchive={(stash) => setDialog({ kind: 'archive', stash })}
              onRestore={(stash) => void run(() => stashesApi.unarchive(stash.id))}
              onDelete={(stash) => setDialog({ kind: 'delete', stash })}
              onList={setList}
              onFailed={refresh}
            />
          )}
        </div>
      ) : null}
    </section>
  )
}

/**
 * The list: the account and the kind to look at (a compact line, with more than one of either), the stashes in the person's order as a table (held and
 * dragged, or Alt and the arrows, when no filter is chosen: with a filter the order of the list is not the order of what is shown), then the archived
 * ones apart and greyed, when the switch in the header's "more" menu is on. On the phone the rows are `TapRow`s that open to the facts and the buttons
 * (add, take out, return, open, edit, archive). A click on a desktop row opens the stash; the + and - of a row add and take out without leaving the
 * list. docs/loans-deposits-design.md, section 10.
 */
function StashList({
  list,
  accounts,
  settings,
  showArchived,
  onNew,
  onMoney,
  onEdit,
  onArchive,
  onRestore,
  onDelete,
  onList,
  onFailed,
}: {
  list: Stash[]
  accounts: NonNullable<ReturnType<typeof useAccounts>>
  settings: MoneySettings
  /** The archived ones are shown too (the switch is in the page's header). */
  showArchived: boolean
  onNew: () => void
  onMoney: (stash: Stash, tab: MoneyMode) => void
  onEdit: (stash: Stash) => void
  onArchive: (stash: Stash) => void
  onRestore: (stash: Stash) => void
  onDelete: (stash: Stash) => void
  /** The list in the new order, shown at once. */
  onList: (list: Stash[]) => void
  /** The new order could not be saved: the saved one is read again. */
  onFailed: () => void
}) {
  const { t } = useI18n()
  const phone = useProfile() === 'phone'
  const navigate = useNavigate()
  const say = useToast()
  const rows = useOpenRow()
  const [menu, setMenu] = useState(false)
  const [accountId, setAccountId] = useState(() => storedId('stashes.account'))
  const [kindMenu, setKindMenu] = useState(false)
  const [kind, setKind] = useState<KindFilter>(() => storedChoice('stashes.kind', KIND_FILTERS, ''))
  // a row was just dropped in a new place: the click that ends the drag must not open the stash
  const dropped = useRef(false)
  const multi = several(accounts)

  // a remembered account that is gone, archived or the only one left is no longer a filter
  const known = accountId === '' || (multi && assetAccounts(accounts).some((account) => account.id === accountId))
  if (!known) setAccountId('')
  useEffect(() => {
    if (!known) saveChoice('stashes.account', '')
  }, [known])
  const filter = known ? accountId : ''

  const active = useMemo(() => activeStashes(list), [list])
  const archived = useMemo(() => archivedStashes(list), [list])
  // the kinds are a filter only while there are goals and deposits both
  const mixed = list.some((stash) => stash.kind === 'GOAL') && list.some((stash) => stash.kind === 'DEPOSIT')
  const kindFilter = mixed ? kind : ''
  const inView = (stash: Stash) => (filter === '' || stash.accountId === filter) && (kindFilter === '' || stash.kind === kindFilter)
  const shownActive = active.filter(inView)
  const shownArchived = archived.filter(inView)
  const total = listTotal(shownActive)
  // with a filter the order of the list is not the order of what is shown, so it cannot be changed
  const reorderable = !phone && filter === '' && kindFilter === ''

  async function onReorder(ids: string[]) {
    dropped.current = true
    window.setTimeout(() => (dropped.current = false), 500)
    // the new order shows at once; if it cannot be saved, the saved one comes back
    onList([...applyOrder(active, ids), ...archived])
    try {
      await stashesApi.reorder(ids)
    } catch (failure) {
      say(errorText(failure), 'error')
      onFailed()
    }
  }
  const items = useMemo(() => (reorderable ? active.map((stash) => ({ id: stash.id, name: stash.name, deleted: false })) : null), [reorderable, active])
  const { listRef, hintId, live } = useReorder(items, (ids) => void onReorder(ids), 'stashes.reordered')

  const pick = (id: string) => {
    saveChoice('stashes.account', id)
    setAccountId(id)
    setMenu(false)
  }
  const pickKind = (value: KindFilter) => {
    saveChoice('stashes.kind', value)
    setKind(value)
    setKindMenu(false)
  }
  const kindWords: Record<KindFilter, string> = { '': t('stashes.allKinds'), GOAL: t('stashes.goals'), DEPOSIT: t('stashes.deposits') }
  const accountText = filter ? ((accountById(accounts, filter) && accountLabel(accountById(accounts, filter)!, t)) ?? '') : t('accounts.allAccounts')

  const link = (stash: Stash) => `/app/stashes/${stash.id}`

  /** A click anywhere on a row opens the stash, as its name does. Not one on a link, a button or a field, one that ends a text selection, or the one that drops a dragged row. */
  function open(event: MouseEvent<HTMLElement>, stash: Stash) {
    if ((event.target as Element).closest('a, button, input, label') || window.getSelection()?.isCollapsed === false) return
    if (dropped.current) return
    navigate(link(stash))
  }

  const accountOf = (stash: Stash) => {
    const account = accountById(accounts, stash.accountId)
    return account ? accountLabel(account, t) : stash.account
  }
  const money = (stash: Stash) => stashMoney(stash, settings.locale)
  const dateOf = (stash: Stash) => (stash.targetDate === null ? null : formatDate(stash.targetDate, settings.locale))
  // the account column says something only while the stashes of more than one account are shown
  const showAccount = multi && filter === ''

  /** What a stash holds, in red below 0. */
  const savedCell = (stash: Stash) => <span className={cn('cell-amount stash-saved', stash.saved.startsWith('-') && 'is-below')}>{money(stash)(stash.saved)}</span>

  /** The progress column: the bar and its figure, or a quiet dash for a stash with no bar. A running deposit's figure is its days left, said in full on hover. */
  const meterCell = (stash: Stash, meter: RowMeter | null) => (
    <span className="stash-meter" title={stash.deposit?.state === 'RUNNING' ? `${t('stashes.daysLeft')}: ${stash.deposit.daysLeft}` : undefined}>
      {meter ? (
        <>
          <MeterBar fill={meter.fill} reached={meter.reached} />
          <span className="stash-meter-figure">{meter.label}</span>
        </>
      ) : (
        <span className="stash-none">—</span>
      )}
    </span>
  )

  /**
   * The + and the - of a stash: each where it is allowed, an empty place where it is not, so the buttons of all rows line up. A deposit that matured has
   * a "return" button where the + would be, and the - of one that runs is its break. A click on one does not open the row.
   */
  const steps = (stash: Stash) => {
    const press = (tab: MoneyMode) => (event: MouseEvent<HTMLElement>) => {
      event.stopPropagation()
      onMoney(stash, tab)
    }
    return (
      <>
        {canBookReturn(stash) ? (
          <Button size="sm" className="stash-return" aria-label={`${t('stashes.bookReturn')}: ${stash.name}`} onClick={press('RETURN')}>
            {t('stashes.returnButton')}
          </Button>
        ) : canAdd(stash) ? (
          <Button size="sm" className="stash-step" aria-label={`${t('stashes.addMoney')}: ${stash.name}`} onClick={press('ADD')}>
            +
          </Button>
        ) : (
          <span className="icon-slot" />
        )}
        {canTake(stash) ? (
          <Button variant="outline" size="sm" className="stash-step" aria-label={`${t(isBreak(stash) ? 'stashes.breakDeposit' : 'stashes.takeOut')}: ${stash.name}`} onClick={press('TAKE')}>
            &minus;
          </Button>
        ) : (
          <span className="icon-slot" />
        )}
      </>
    )
  }

  // A desktop row, one value to a cell: name (with the grip that says the row can be held and dragged), account, what is saved, its target, the bar, the
  // date (a goal's target date, a deposit's maturity), where it stands, then + and - (restore and delete for an archived one). With less room columns
  // go (stashes.css). The name is the link for the keyboard (j and k stop on it).
  function tableRow(stash: Stash, hintId?: string) {
    const drag = reorderable && !stash.archived
    const status = rowStatus(stash, money(stash), t)
    return (
      <Row
        key={stash.id}
        className={cn(stash.archived && 'is-archived')}
        data-reorder-id={drag ? stash.id : undefined}
        data-archived={stash.archived || undefined}
        onClick={(event) => open(event, stash)}
        actions={
          stash.archived ? (
            <>
              <IconAction icon="export" label={`${t('stashes.restore')}: ${stash.name}`} onClick={() => onRestore(stash)} />
              <IconAction icon="trash" label={`${t('common.delete')}: ${stash.name}`} danger onClick={() => onDelete(stash)} />
            </>
          ) : (
            steps(stash)
          )
        }
      >
        <div className="cell-name">
          {drag ? <Icon name="grip" className="row-grip" /> : null}
          <Link className="card-name" to={link(stash)} title={stash.name} data-row-focus aria-describedby={hintId}>
            {stash.name}
          </Link>
        </div>
        {showAccount ? (
          <span className="cell-sub stash-account" title={accountOf(stash)}>
            {accountOf(stash)}
          </span>
        ) : null}
        {savedCell(stash)}
        <span className="cell-sub-end stash-goal">{stash.target === null ? <span className="stash-none">—</span> : money(stash)(stash.target)}</span>
        {meterCell(stash, rowMeter(stash, t))}
        <span className={cn('cell-sub stash-when', stash.overdue && 'is-late')}>{dateOf(stash) ?? <span className="stash-none">—</span>}</span>
        <span className={cn('cell-sub stash-status', `tone-${status.tone}`)}>{status.text}</span>
      </Row>
    )
  }

  // A phone row: the name (with a mark for what is news: reached, overdue, matured, broken) and what is saved, a thin bar under the name (a deposit says
  // its state in words instead), and a chevron. It opens to the facts and the buttons: add (or the return of a deposit that matured), take out, open,
  // edit, archive. An archived one opens to open, restore and delete.
  function tapRow(stash: Stash) {
    const status = rowStatus(stash, money(stash), t)
    const mark = rowMark(status)
    const meter = rowMeter(stash, t)
    const actions: (PanelAction | false)[] = stash.archived
      ? [
          { label: t('accounts.open'), icon: 'play', to: link(stash) },
          { label: t('stashes.restore'), icon: 'export', onClick: () => onRestore(stash) },
          { label: t('common.delete'), icon: 'trash', danger: true, onClick: () => onDelete(stash) },
        ]
      : [
          canBookReturn(stash)
            ? { label: t('stashes.bookReturn'), icon: 'check', onClick: () => onMoney(stash, 'RETURN') }
            : canAdd(stash) && { label: t('stashes.addMoney'), icon: 'add', onClick: () => onMoney(stash, 'ADD') },
          canTake(stash) && { label: t(isBreak(stash) ? 'stashes.breakDeposit' : 'stashes.takeOut'), icon: 'export', onClick: () => onMoney(stash, 'TAKE') },
          { label: t('accounts.open'), icon: 'play', to: link(stash) },
          { label: t('common.edit'), icon: 'pencil', onClick: () => onEdit(stash) },
          { label: t('stashes.archive'), icon: 'archive', onClick: () => onArchive(stash) },
        ]
    // under the name: the bar of a goal, the state of a deposit; the mark says it already when it is news, and an archived one says nothing
    const meta = stash.archived ? undefined : stash.deposit ? (
      mark ? undefined : <span className={`tone-${status.tone}`}>{status.text}</span>
    ) : meter ? (
      <MeterBar fill={meter.fill} reached={meter.reached} />
    ) : undefined
    return (
      <TapRow
        key={stash.id}
        id={rows.panelId(stash.id)}
        className={cn(stash.archived && 'is-archived')}
        data-archived={stash.archived || undefined}
        open={rows.isOpen(stash.id)}
        onToggle={() => rows.toggle(stash.id)}
        name={<NameMark mark={mark ? <Badge variant={mark.tone === 'good' ? 'success' : 'destructive'}>{mark.text}</Badge> : null}>{stash.name}</NameMark>}
        amount={savedCell(stash)}
        meta={meta}
        facts={stashFacts(stash, accountOf(stash), money(stash), settings.locale, t)}
      >
        <PanelActions actions={actions} />
      </TapRow>
    )
  }

  // The columns of the table, each with a class that its cells share: with less room they go one by one (stashes.css).
  const heads = [
    t('common.name'),
    ...(showAccount ? [{ label: t('stashes.account'), className: 'head-account' }] : []),
    { label: t('stashes.savedLabel'), className: 'is-end' },
    { label: t('stashes.target'), className: 'is-end head-target' },
    { label: t('stashes.progress'), className: 'head-meter' },
    { label: t('stashes.dateHead'), className: 'head-date' },
    { label: t('stashes.status'), className: 'head-status' },
    '',
  ]
  const tableClass = (more: string | false) => cn('stash-list', !showAccount && 'no-account', more)

  if (list.length === 0) {
    return <EmptyState title={t('stashes.emptyTitle')} text={t('stashes.emptyText')} action={{ label: t('stashes.add'), onClick: onNew }} />
  }

  return (
    <>
      {multi || mixed ? (
        <div className="toolbar wide tx-bar filter-bar">
          {multi ? (
            <Pick
              compact
              label={t('stashes.account')}
              className="scope"
              text={accountText}
              title={accountText}
              open={menu}
              onToggle={() => setMenu((open) => !open)}
              onClose={() => setMenu(false)}
            >
              <PickOption selected={filter === ''} onClick={() => pick('')}>
                {t('accounts.allAccounts')}
              </PickOption>
              {assetAccounts(accounts).map((account) => (
                <PickOption key={account.id} selected={filter === account.id} onClick={() => pick(account.id)}>
                  {accountLabel(account, t)}
                </PickOption>
              ))}
            </Pick>
          ) : null}
          {mixed ? (
            <Pick
              compact
              label={t('stashes.kindTabs')}
              className="scope"
              text={kindWords[kindFilter]}
              open={kindMenu}
              onToggle={() => setKindMenu((open) => !open)}
              onClose={() => setKindMenu(false)}
            >
              {KIND_FILTERS.map((value) => (
                <PickOption key={value || 'all'} selected={kindFilter === value} onClick={() => pickKind(value)}>
                  {kindWords[value]}
                </PickOption>
              ))}
            </Pick>
          ) : null}
        </div>
      ) : null}
      {shownActive.length > 0 && phone ? <ListGroup className="stash-phone">{shownActive.map(tapRow)}</ListGroup> : null}
      {shownActive.length > 0 && !phone ? (
        <Table tidy className={cn(tableClass(reorderable && 'can-reorder'), shownActive.some(canBookReturn) && 'has-return')} heads={heads} listRef={listRef}>
          {shownActive.map((stash) => tableRow(stash, reorderable ? hintId : undefined))}
          {total ? (
            <li className="card-item stash-total">
              <span className="cell-name">{t('stashes.totalRow')}</span>
              {showAccount ? <span className="stash-account" /> : null}
              <span className="cell-amount stash-saved">{stashMoney(total, settings.locale)(total.saved)}</span>
              <span className="cell-sub-end stash-goal">{total.target === null ? null : stashMoney(total, settings.locale)(total.target)}</span>
            </li>
          ) : null}
        </Table>
      ) : null}
      {shownActive.length === 0 && (active.length === 0 || filter !== '' || kindFilter !== '') ? <p className="hint">{t('stashes.noStashes')}</p> : null}
      {reorderable ? (
        <>
          <span id={hintId} className="sr-only">
            {t('stashes.reorderHint')}
          </span>
          <div className="sr-only" aria-live="polite">
            {live}
          </div>
        </>
      ) : null}
      {/* the archived ones, when the switch in the "more" menu is on: a group of their own, in the same columns */}
      {showArchived && shownArchived.length > 0 && phone ? <ListGroup heading={t('accounts.archived')} className="stash-phone">
          {shownArchived.map(tapRow)}
        </ListGroup> : null}
      {showArchived && shownArchived.length > 0 && !phone ? (
        <>
          <h2 className="acct-group">{t('accounts.archived')}</h2>
          <Table tidy className={tableClass('stash-archived')} heads={heads}>
            {shownArchived.map((stash) => tableRow(stash))}
          </Table>
        </>
      ) : null}
    </>
  )
}
