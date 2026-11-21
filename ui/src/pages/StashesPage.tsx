import { useEffect, useMemo, useRef, useState, type MouseEvent } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import { ApiError, errorText } from '../api/invoke'
import { stashes as stashesApi, type Detail, type Entry, type EntryKind, type Stash, type Summary } from '../api/stashes'
import { useToday, type Today } from '../api/today'
import { transfers as transfersApi, type Transfer } from '../api/transfers'
import ConfirmModal from '../components/ConfirmModal'
import Icon from '../components/NavIcon'
import Pick, { PickOption } from '../components/Pick'
import StashDetail from '../components/StashDetail'
import StashModal from '../components/StashModal'
import StashMoneyModal from '../components/StashMoneyModal'
import { IconAction, Row, Table, TapRow } from '../components/RowParts'
import { StashBar } from '../components/StashParts'
import TransferModal from '../components/TransferModal'
import Button from '../components/ui/Button'
import { buttonClass, cn } from '../components/ui/cn'
import { EmptyState } from '../components/ui/Pixel'
import { useAccounts } from '../hooks/useAccounts'
import { useMoneySettings, type MoneySettings } from '../hooks/useMoneySettings'
import { useOpenRow } from '../hooks/useOpenRow'
import { useProfile } from '../hooks/profile'
import { useReorder } from '../hooks/useReorder'
import { useToast } from '../hooks/toast'
import { useI18n } from '../i18n/context'
import { useShortcut } from '../keys/registry'
import { tip } from '../keys/tip'
import { useRowNavigation } from '../keys/useRowNavigation'
import { accountById, accountLabel, assetAccounts, several } from '../lib/accountsView'
import { formatMoney, fromMinor } from '../lib/money'
import { applyOrder } from '../lib/reorder'
import { saveChoice, storedChoice, storedId } from '../lib/sticky'
import { activeStashes, archivedStashes, canAdd, canTake, held, stashMoney, targetLine } from '../lib/stashView'

/** The dialog that is open. A transfer being looked at is read first, so the dialog opens with its row. */
type Dialog =
  | { kind: 'new'; accountId?: string }
  | { kind: 'edit'; stash: Stash }
  | { kind: 'money'; stash: Stash; tab: EntryKind }
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
            if (answer.stash.id === stashId) setDetail(answer)
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

  return (
    <section className="tx-page stashes-page">
      <div className="tx-title one-line">
        <h1>{t('stashes.title')}</h1>
        {stashId ? (
          shown && canAdd(shown.stash) ? (
            <div className="tx-actions">
              <Button onClick={() => setDialog({ kind: 'money', stash: shown.stash, tab: 'ADD' })} {...tip('new', t('stashes.addMoney'))}>
                {t('stashes.addMoney')}
              </Button>
              {canTake(shown.stash) ? (
                <Button variant="outline" onClick={() => setDialog({ kind: 'money', stash: shown.stash, tab: 'TAKE' })}>
                  {t('stashes.takeOut')}
                </Button>
              ) : null}
            </div>
          ) : shown && canTake(shown.stash) ? (
            <div className="tx-actions">
              <Button variant="outline" onClick={() => setDialog({ kind: 'money', stash: shown.stash, tab: 'TAKE' })}>
                {t('stashes.takeOut')}
              </Button>
            </div>
          ) : null
        ) : (
          <div className="tx-actions">
            <Button onClick={() => setDialog({ kind: 'new' })} {...tip('new', t('stashes.add'))}>
              {t('stashes.add')}
            </Button>
          </div>
        )}
      </div>
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
                onEdit={() => setDialog({ kind: 'edit', stash: shown.stash })}
                onArchive={() => setDialog({ kind: 'archive', stash: shown.stash })}
                onRestore={() => void run(() => stashesApi.unarchive(shown.stash.id))}
                onDelete={() => setDialog({ kind: 'delete', stash: shown.stash })}
                onEntry={(entry) => setDialog({ kind: 'entry', stash: shown.stash, entry })}
                onTransfer={(id) => void openTransfer(id)}
              />
            ) : missing ? (
              <>
                <div className="debts-name">
                  <Link to="/app/stashes" className={buttonClass('outline', 'sm')}>
                    {t('common.back')}
                  </Link>
                </div>
                <p className="notice">{errorText(new ApiError('not_found'))}</p>
              </>
            ) : null
          ) : (
            <StashList
              list={list}
              summary={summary}
              accounts={accounts}
              settings={settings}
              onNew={() => setDialog({ kind: 'new' })}
              onMoney={(stash, tab) => setDialog({ kind: 'money', stash, tab })}
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
 * The list: the total saved over it, the account to look at (with more than one), the stashes in the person's order as a table (held and dragged, or
 * Alt and the arrows, when no account is chosen: with a filter the order of the list is not the order of what is shown), then the archived ones
 * apart and greyed. On the phone the rows are `TapRow`s. A click on a row opens the stash; the + and - of a row add and take out without leaving the
 * list. docs/loans-deposits-design.md, section 10.
 */
function StashList({
  list,
  summary,
  accounts,
  settings,
  onNew,
  onMoney,
  onRestore,
  onDelete,
  onList,
  onFailed,
}: {
  list: Stash[]
  summary: Summary | null
  accounts: NonNullable<ReturnType<typeof useAccounts>>
  settings: MoneySettings
  onNew: () => void
  onMoney: (stash: Stash, tab: EntryKind) => void
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
  const [showArchived, setShowArchived] = useState(() => storedChoice('stashes.archived', [false, true], false))
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
  const inView = (stash: Stash) => filter === '' || stash.accountId === filter
  const shownActive = active.filter(inView)
  const shownArchived = archived.filter(inView)
  const reorderable = !phone && filter === ''

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
  const targetOf = (stash: Stash) => targetLine(stash, stashMoney(stash, settings.locale), settings.locale)

  /** What a stash holds, with a thin bar under it (the room of the bar is kept when there is no target, so the rows are as tall as each other). */
  const savedCell = (stash: Stash, className: string) => (
    <span className={cn(className, 'stash-saved', stash.saved.startsWith('-') && 'is-below')}>
      {stashMoney(stash, settings.locale)(stash.saved)}
      <span className="stash-bar-slot">
        <StashBar saved={stash.saved} target={stash.target} />
      </span>
    </span>
  )

  /** The + and the - of a stash: each where it is allowed, an empty place where it is not, so the buttons of all rows line up. A click on one does not open the row. */
  const steps = (stash: Stash) => {
    const press = (tab: EntryKind) => (event: MouseEvent<HTMLElement>) => {
      event.stopPropagation()
      onMoney(stash, tab)
    }
    return (
      <>
        {canAdd(stash) ? (
          <Button size="sm" className="stash-step" aria-label={`${t('stashes.addMoney')}: ${stash.name}`} onClick={press('ADD')}>
            +
          </Button>
        ) : (
          <span className="icon-slot" />
        )}
        {canTake(stash) ? (
          <Button variant="outline" size="sm" className="stash-step" aria-label={`${t('stashes.takeOut')}: ${stash.name}`} onClick={press('TAKE')}>
            &minus;
          </Button>
        ) : (
          <span className="icon-slot" />
        )}
      </>
    )
  }

  // A desktop row: name (with the grip that says the row can be held and dragged), account, what is saved, the target and its date, then + and -
  // (restore and delete for an archived one). The name is the link for the keyboard (j and k stop on it).
  function tableRow(stash: Stash, hintId?: string) {
    const drag = reorderable && !stash.archived
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
              <Button variant="outline" size="sm" className="row-primary" aria-label={`${t('stashes.restore')}: ${stash.name}`} onClick={() => onRestore(stash)}>
                {t('stashes.restore')}
              </Button>
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
        <span className="cell-sub" title={accountOf(stash)}>
          {accountOf(stash)}
        </span>
        {savedCell(stash, 'cell-amount')}
        <span className={cn('cell-sub-end stash-target', stash.overdue && 'is-late')}>{targetOf(stash)}</span>
      </Row>
    )
  }

  // A phone row: the name and what is saved, the account and the target under them. A tap anywhere on it opens the stash, and + and - are
  // on its right. An archived one opens to labelled buttons, as the archived accounts do.
  function tapRow(stash: Stash) {
    const sub = (
      <>
        <span>{accountOf(stash)}</span>
        {stash.target !== null || stash.targetDate !== null ? <span className={cn('stash-target', stash.overdue && 'is-late')}>{targetOf(stash)}</span> : null}
      </>
    )
    if (stash.archived) {
      return (
        <TapRow
          key={stash.id}
          id={rows.panelId(stash.id)}
          className="is-archived"
          data-archived
          open={rows.isOpen(stash.id)}
          onToggle={() => rows.toggle(stash.id)}
          name={stash.name}
          amount={savedCell(stash, 'cell-amount')}
          sub={sub}
        >
          <div className="row-panel-actions">
            <Link to={link(stash)} className={buttonClass('outline')}>
              {t('accounts.open')}
            </Link>
            <Button variant="outline" onClick={() => onRestore(stash)}>
              {t('stashes.restore')}
            </Button>
            <Button variant="outline" className="row-danger" onClick={() => onDelete(stash)}>
              <Icon name="trash" className="btn-icon" />
              {t('common.delete')}
            </Button>
          </div>
        </TapRow>
      )
    }
    return (
      <TapRow
        key={stash.id}
        id={rows.panelId(stash.id)}
        onClick={(event) => open(event, stash)}
        name={<Link to={link(stash)}>{stash.name}</Link>}
        amount={savedCell(stash, 'cell-amount')}
        sub={sub}
        action={<span className="cell-actions stash-steps">{steps(stash)}</span>}
      />
    )
  }

  const heads = [t('common.name'), t('stashes.account'), { label: t('stashes.savedLabel'), className: 'is-end' }, t('stashes.target'), '']

  if (list.length === 0) {
    return <EmptyState title={t('stashes.emptyTitle')} text={t('stashes.emptyText')} action={{ label: t('stashes.add'), onClick: onNew }} />
  }

  return (
    <>
      {summary ? (
        <p className="acct-total">
          <span className="dash-stat-label">{t('stashes.total')}</span> <b>{formatMoney(summary.saved, settings.currency, settings.locale)}</b>
        </p>
      ) : null}
      {multi ? (
        <div className="toolbar wide tx-bar filter-bar">
          <Pick
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
        </div>
      ) : null}
      {shownActive.length > 0 && phone ? <ul className="card-list stash-phone">{shownActive.map(tapRow)}</ul> : null}
      {shownActive.length > 0 && !phone ? (
        <Table className={cn('stash-list', reorderable && 'can-reorder')} heads={heads} listRef={listRef}>
          {shownActive.map((stash) => tableRow(stash, reorderable ? hintId : undefined))}
        </Table>
      ) : null}
      {shownActive.length === 0 && (active.length === 0 || filter !== '') ? <p className="hint">{t('stashes.noStashes')}</p> : null}
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
      {archived.length > 0 ? (
        <div className="debts-bar">
          <label>
            <input
              type="checkbox"
              checked={showArchived}
              onChange={(event) => {
                saveChoice('stashes.archived', event.target.checked)
                setShowArchived(event.target.checked)
              }}
            />
            {t('stashes.archivedCount', { count: archived.length })}
          </label>
        </div>
      ) : null}
      {showArchived && shownArchived.length > 0 && phone ? <ul className="card-list stash-phone">{shownArchived.map(tapRow)}</ul> : null}
      {showArchived && shownArchived.length > 0 && !phone ? (
        <Table className="stash-list stash-archived" heads={heads}>
          {shownArchived.map((stash) => tableRow(stash))}
        </Table>
      ) : null}
    </>
  )
}
