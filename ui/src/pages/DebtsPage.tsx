import { useEffect, useState, type MouseEvent, type ReactNode } from 'react'
import { Link, useLocation, useNavigate, useParams, useSearchParams } from 'react-router-dom'
import { debts, type EntryKind, type History, type Person, type Reminders } from '../api/debts'
import { errorText } from '../api/invoke'
import { scanQr, stopScan } from '../api/scanner'
import { useToday, type Today } from '../api/today'
import PageHeader from '../components/PageHeader'
import BillAcceptModal from '../components/BillAcceptModal'
import ConfirmModal from '../components/ConfirmModal'
import DebtNotificationsModal from '../components/DebtNotificationsModal'
import DebtsFiles from '../components/DebtsFiles'
import DebtsLoans from '../components/DebtsLoans'
import DebtsHistory from '../components/DebtsHistory'
import EntryModal from '../components/EntryModal'
import FriendLedger from '../components/FriendLedger'
import FriendNameModal from '../components/FriendNameModal'
import { type FactItem } from '../components/Facts'
import { IconAction, ListGroup, NameMark, PanelActions, Row, RowMore, Table, TapRow, type PanelAction } from '../components/RowParts'
import SectionTabs from '../components/SectionTabs'
import SettleModal from '../components/SettleModal'
import { Badge } from '../components/ui/badge'
import { Button } from '../components/ui/button'
import { EmptyState } from '../components/ui/Pixel'
import { useMoneySettings, type MoneySettings } from '../hooks/useMoneySettings'
import { useOpenRow } from '../hooks/useOpenRow'
import { useToast } from '../hooks/toast'
import { useProfile } from '../hooks/profile'
import { useT } from '../i18n/context'
import { useShortcut } from '../keys/registry'
import { useRowNavigation } from '../keys/useRowNavigation'
import { balanceView } from '../lib/balance'
import { debtsTabOf, debtsTabQuery, type DebtsTab } from '../lib/loansView'
import { formatDate, formatMoney } from '../lib/money'
import { dismissedAfter, dismissedDays, notDismissed, reminderEvery, remindersChanged, saveDismissedDays } from '../lib/reminders'
import { saveChoice, storedChoice } from '../lib/sticky'
import { tabPanelProps } from '../lib/tabs'

/**
 * The debts page has two tabs, in the address as `?tab=debts` (docs/loans-deposits-design.md, section 10): the friends, which is the page as it was,
 * and the debts, the accounts you owe on (`DebtsLoans`). A friend's page (`/app/debts/:personId`) is on the friends tab.
 */
export default function DebtsPage() {
  const { settings, error } = useMoneySettings()
  const now = useToday()
  const navigate = useNavigate()
  const { personId } = useParams()
  const [params] = useSearchParams()
  const phone = useProfile() === 'phone'
  const t = useT()
  if (error) return <p className="notice">{error}</p>
  if (!settings || !now) return null
  const tab = personId ? 'friends' : debtsTabOf(params)
  const choose = (next: DebtsTab) => navigate({ pathname: '/app/debts', search: new URLSearchParams(debtsTabQuery(next)).toString() }, { replace: true })
  // a friend's page on a phone is a page of its own, with its way back
  const tabs =
    phone && personId ? null : (
      <SectionTabs
        label={t('debts.title')}
        value={tab}
        onChange={choose}
        tabs={[
          { id: 'friends', label: t('debts.tabFriends') },
          { id: 'debts', label: t('debts.tabDebts') },
        ]}
      />
    )
  return tab === 'debts' ? <DebtsLoans settings={settings} now={now} tabs={tabs} /> : <DebtsView settings={settings} now={now} tabs={tabs} />
}

function DebtsView({ settings, now, tabs }: { settings: MoneySettings; now: Today; tabs: ReactNode }) {
  const t = useT()
  const phone = useProfile() === 'phone'
  const navigate = useNavigate()
  const location = useLocation()
  // the friend chosen in the address, whose history is on the right
  const { personId } = useParams()
  const rows = useOpenRow()
  const [people, setPeople] = useState<Person[] | null>(null)
  const [reminders, setReminders] = useState<Reminders | null>(null)
  // the reminders cleared on this device, friend to day
  const [dismissed, setDismissed] = useState(dismissedDays)
  const [history, setHistory] = useState<History | null>(null)
  const [historyOffset, setHistoryOffset] = useState(0)
  // a code that was scanned or is being pasted, and the camera while it looks
  const [taking, setTaking] = useState<string | null>(null)
  const [scanning, setScanning] = useState(false)
  const [showRemoved, setShowRemoved] = useState(() => storedChoice('debts.removed', [false, true], false))
  const [reload, setReload] = useState(0)
  const refresh = () => setReload((value) => value + 1)
  // the chosen friend as the right side has read them, for the buttons above
  const [selected, setSelected] = useState<Person | null>(null)
  const say = useToast()
  const [addingFriend, setAddingFriend] = useState(false)
  const [notifying, setNotifying] = useState(false)
  const [entryKind, setEntryKind] = useState<EntryKind | null>(null)
  const [settling, setSettling] = useState<Person | null>(null)
  // the friend whose name is being changed, and the one being taken off the list
  const [editing, setEditing] = useState<Person | null>(null)
  const [deleting, setDeleting] = useState<Person | null>(null)
  const [pending, setPending] = useState(false)
  const money = (amount: string) => formatMoney(amount, settings.currency, settings.locale)

  useEffect(() => {
    let cancelled = false
    void Promise.all([debts.people(showRemoved), debts.reminders(reminderEvery())]).then(
      ([list, reminded]) => {
        if (cancelled) return
        setPeople(list)
        setReminders(reminded)
      },
      (failure) => {
        if (!cancelled) say(errorText(failure), 'error')
      },
    )
    return () => {
      cancelled = true
    }
  }, [showRemoved, reload, say])

  // the debts of all friends are read a page at a time, only while no friend is chosen
  useEffect(() => {
    if (personId) return
    let cancelled = false
    void debts.history({ only: 'friends', offset: historyOffset }).then(
      (loaded) => {
        if (!cancelled) setHistory(loaded)
      },
      (failure) => {
        if (!cancelled) say(errorText(failure), 'error')
      },
    )
    return () => {
      cancelled = true
    }
  }, [personId, historyOffset, reload, say])

  // A friend is taken off the list, never erased: the balance and the history stay, and "show removed friends" brings them back.
  async function onDelete(person: Person) {
    setPending(true)
    try {
      await debts.archive(person.id)
    } catch (failure) {
      say(errorText(failure), 'error')
      return
    } finally {
      setPending(false)
      setDeleting(null)
    }
    say(t('debts.deleted', { name: person.name }), 'success')
    refresh()
    remindersChanged()
  }

  async function bringBack(person: Person) {
    try {
      await debts.unarchive(person.id)
    } catch (failure) {
      say(errorText(failure), 'error')
      return
    }
    refresh()
    remindersChanged()
  }

  // The camera shows behind the page while it scans, so the page must not paint over it.
  useEffect(() => {
    document.documentElement.classList.toggle('is-scanning', scanning)
    return () => document.documentElement.classList.remove('is-scanning')
  }, [scanning])

  async function onScan() {
    setScanning(true)
    const result = await scanQr()
    setScanning(false)
    if (result.kind === 'text') setTaking(result.text)
    else if (result.kind === 'denied') say(t('phone.cameraDenied'), 'warning')
    else if (result.kind === 'unavailable') say(t('phone.noScanner'), 'warning')
  }

  // The phone's round button sends the person here to scan a bill, and the first steps on the dashboard to add a friend: the camera or the form opens at once, once.
  useEffect(() => {
    const asked = location.state as { scan?: boolean; addFriend?: boolean } | null
    if (!asked?.scan && !asked?.addFriend) return
    navigate(location.pathname, { replace: true, state: null })
    // the camera is the phone's; the form opens on both
    if (asked.scan) {
      if (phone) void onScan()
    } else setAddingFriend(true)
    // each time the page is asked, when it opens and when it is open already; the request is taken out of the location, so once
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [location.state])

  // The reminders that have come and were not cleared: what the badges, the count and the list of notifications show.
  const due = notDismissed(reminders?.due ?? [], dismissed)
  const dueIds = new Set(due.map((friend) => friend.personId))
  const dueOn = new Map(due.map((friend) => [friend.personId, friend.dueOn]))

  /** Clears reminders from the list on this device only: nothing rings differently, and the friend's next reminder comes back. */
  function dismiss(cleared: Reminders['due']) {
    const days = dismissedAfter(reminders?.due ?? [], dismissed, cleared)
    saveDismissedDays(days)
    setDismissed(days)
  }

  // The chosen friend, once the right side has read them (it is the one of the address; the one read before may be another).
  const friend = selected?.id === personId ? selected : null

  // Keyboard: n adds money lent (to the chosen friend), j and k move between rows.
  useShortcut('new', () => setEntryKind('LENT'))
  useRowNavigation()

  /** Who owes whom, in words: "owes you", "you owe", "all settled". */
  const statusText = (tone: 'owed' | 'owe' | 'even') => t(tone === 'owed' ? 'debts.labelOwesYou' : tone === 'owe' ? 'debts.labelYouOwe' : 'debts.settledUp')
  const friendLink = (person: Person) => `/app/debts/${person.id}`

  /** Where a friend's row leads: their history, or for the friend already chosen, back to the debts of all of them. */
  const rowLink = (person: Person) => (person.id === personId ? '/app/debts' : friendLink(person))

  /** A click anywhere on a desktop row chooses the friend (or lets go of them), as the name does. Not one on a link (that is the name), or one that ends a text selection. */
  function choose(event: MouseEvent<HTMLElement>, person: Person) {
    if ((event.target as Element).closest('a, button, input, label') || window.getSelection()?.isCollapsed === false) return
    navigate(rowLink(person))
  }

  /** The day the reminder of a friend came, when it did and was not cleared; none for a friend who owes nothing. */
  const reminderOf = (person: Person, tone: 'owed' | 'owe' | 'even') => (dueIds.has(person.id) && tone !== 'even' ? formatDate(dueOn.get(person.id) ?? '', settings.locale) : '')

  /** What can be done with a friend, in a desktop row's "more" menu and the open row of a phone: mark as paid (or bring back), the history, rename, and take off the list. */
  const actionsOf = (person: Person, tone: 'owed' | 'owe' | 'even'): PanelAction[] => [
    ...(person.archived
      ? [{ label: t('debts.unarchive'), icon: 'archive', onClick: () => void bringBack(person) }]
      : tone !== 'even'
        ? [{ label: t('debts.settle'), icon: 'check', onClick: () => setSettling(person) }]
        : []),
    { label: t('debts.ledger'), icon: 'play', to: friendLink(person) },
    { label: t('common.rename'), icon: 'pencil', onClick: () => setEditing(person) },
    ...(person.archived ? [] : [{ label: t('debts.archive'), icon: 'trash', danger: true, onClick: () => setDeleting(person) }]),
  ]

  // A desktop row: name, who owes whom, the day of the reminder, the amount, then the one quick button, mark as paid (or bring back one that was removed; an
  // empty place for a friend who owes nothing, so the "more" buttons line up), and the "more" menu with the rest. The friend whose history is on the
  // right is marked. The name is the link for the keyboard (j and k stop on it).
  function tableRow(person: Person) {
    const { tone, abs } = balanceView(person.balance)
    const chosen = person.id === personId
    return (
      <Row
        key={person.id}
        className={chosen ? 'is-selected' : undefined}
        data-archived={person.archived || undefined}
        onClick={(event) => choose(event, person)}
        actions={
          <>
            {/* the one thing to do with a friend, as an icon button: mark the debt as paid, or bring back one that was removed */}
            {person.archived ? (
              <IconAction icon="archive" label={`${t('debts.unarchive')}: ${person.name}`} onClick={() => void bringBack(person)} />
            ) : tone !== 'even' ? (
              <IconAction icon="check" label={`${t('debts.settle')}: ${person.name}`} className="icon-btn debt-pay" onClick={() => setSettling(person)} />
            ) : (
              <span className="icon-slot" />
            )}
            <RowMore name={person.name} actions={actionsOf(person, tone)} />
          </>
        }
      >
        <div className="cell-name">
          <Link className="card-name" to={rowLink(person)} title={person.name} aria-current={chosen ? 'true' : undefined} data-row-focus>
            {person.name}
          </Link>
          {person.archived ? <span className="row-mark">{t('debts.removed')}</span> : null}
        </div>
        <span className={`cell-sub friend-status debt-${tone}`}>{statusText(tone)}</span>
        <span className="cell-sub-end friend-reminder">{reminderOf(person, tone)}</span>
        <span className={`cell-amount debt-${tone}`}>{tone === 'even' ? '' : money(abs)}</span>
      </Row>
    )
  }

  // A phone row: the name (a removed friend says so) and what is owed; who owes whom, in its colour, under the name. It opens to the facts and
  // a button for everything that can be done.
  function tapRow(person: Person) {
    const { tone, abs } = balanceView(person.balance)
    const reminder = reminderOf(person, tone)
    const factTone = tone === 'owed' ? 'positive' : tone === 'owe' ? 'negative' : undefined
    const facts: FactItem[] = [
      { label: t('debts.colStatus'), value: statusText(tone), tone: factTone },
      { label: t('debts.balance'), value: money(abs), tone: factTone },
      { label: t('sync.field.reminder'), value: reminder },
    ]
    const actions = actionsOf(person, tone)
    return (
      <TapRow
        key={person.id}
        id={rows.panelId(person.id)}
        data-archived={person.archived || undefined}
        open={rows.isOpen(person.id)}
        onToggle={() => rows.toggle(person.id)}
        name={<NameMark mark={person.archived ? <Badge variant="secondary">{t('debts.removed')}</Badge> : null}>{person.name}</NameMark>}
        amount={<span className={`cell-amount debt-${tone}`}>{tone === 'even' ? '' : money(abs)}</span>}
        meta={<span className={`debt-${tone}`}>{statusText(tone)}</span>}
        facts={facts}
      >
        <PanelActions actions={actions} />
      </TapRow>
    )
  }

  // What the top of the page says. A friend's page on a phone is a page of its own: the friend's name, where it stands, and a way back.
  const friendPage = phone && personId !== undefined
  const standing = friend ? balanceView(friend.balance) : null
  const header = {
    title: friendPage && friend ? friend.name : t('debts.title'),
    back: friendPage ? '/app/debts' : undefined,
    aside:
      friendPage && standing ? (
        standing.tone === 'even' ? (
          <b>{t('debts.settledUp')}</b>
        ) : (
          <>
            {statusText(standing.tone)} <b>{money(standing.abs)}</b>
          </>
        )
      ) : undefined,
    actions: [
      { label: t('debts.lent'), icon: 'add', primary: true, shortcut: 'new' as const, onClick: () => setEntryKind('LENT') },
      { label: t('debts.borrowed'), icon: 'download', onClick: () => setEntryKind('BORROWED') },
    ],
    more: [
      // on a friend's page the one thing to do with the friend, as the list has it on their row
      friendPage && friend && !friend.archived && standing?.tone !== 'even' && { label: t('debts.settle'), icon: 'check', onSelect: () => setSettling(friend) },
      // A bill is split from a new expense: the phone scans a code, the desktop starts the split.
      !phone && { label: t('split.add'), icon: 'transactions', onSelect: () => navigate('/app/transactions', { state: { split: true, ...(friend ? { friendId: friend.id, friendName: friend.name } : {}) } }) },
      phone && { label: t('code.scanShort'), icon: 'qr', disabled: scanning, onSelect: () => void onScan() },
      { label: t('code.paste'), icon: 'qr', onSelect: () => setTaking('') },
      !friendPage && { label: t('debts.addFriend'), icon: 'account', onSelect: () => setAddingFriend(true) },
      !friendPage && { label: due.length > 0 ? `${t('debts.notifications')} (${due.length})` : t('debts.notifications'), icon: 'bell', onSelect: () => setNotifying(true) },
      !friendPage && {
        kind: 'toggle' as const,
        label: t('debts.showRemoved'),
        checked: showRemoved,
        onChange: (checked: boolean) => {
          saveChoice('debts.removed', checked)
          setShowRemoved(checked)
        },
      },
    ],
  }

  return (
    <section className="tx-page debts-page">
      <PageHeader {...header} />
      {tabs}
      {scanning ? (
        <div className="scan-overlay" role="dialog" aria-label={t('code.scan')}>
          <Button variant="outline" onClick={() => void stopScan()}>
            {t('common.cancel')}
          </Button>
        </div>
      ) : null}
      {taking !== null ? (
        <BillAcceptModal
          initialText={taking}
          onClose={() => setTaking(null)}
          onAccepted={() => {
            setTaking(null)
            say(t('code.accepted'), 'success')
            refresh()
            remindersChanged()
          }}
        />
      ) : null}
      {notifying ? (
        <DebtNotificationsModal
          due={due}
          locale={settings.locale}
          onDismiss={dismiss}
          onClose={() => {
            setNotifying(false)
            // the period may have changed in the settings tab
            refresh()
          }}
        />
      ) : null}
      {addingFriend ? (
        <FriendNameModal
          onClose={() => setAddingFriend(false)}
          onSaved={(person) => {
            setAddingFriend(false)
            refresh()
            navigate(`/app/debts/${person.id}`)
          }}
        />
      ) : null}
      {entryKind ? (
        <EntryModal
          entry={null}
          kind={entryKind}
          {...(friend ? { personId: friend.id } : {})}
          today={now.date}
          onClose={() => setEntryKind(null)}
          onSaved={(notAttached) => {
            setEntryKind(null)
            if (notAttached.length > 0) say(t('docs.attachFailed', { names: notAttached.join(', ') }), 'warning')
            refresh()
            remindersChanged()
          }}
        />
      ) : null}
      {settling ? (
        <SettleModal
          person={settling}
          today={now.date}
          onClose={() => setSettling(null)}
          onDone={() => {
            setSettling(null)
            say(t('debts.settled'), 'success')
            refresh()
            remindersChanged()
          }}
        />
      ) : null}
      {editing ? (
        <FriendNameModal
          person={editing}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null)
            refresh()
          }}
        />
      ) : null}
      {deleting ? (
        <ConfirmModal
          title={t('rec.confirmTitle', { action: t('debts.archive'), name: deleting.name })}
          pending={pending}
          onConfirm={() => void onDelete(deleting)}
          onClose={() => setDeleting(null)}
        />
      ) : null}
      {people ? (
        <>
          <div className="tx-pane wide debts-view" {...tabPanelProps('friends')}>
            <div className="debts-split">
              {/* The phone shows a friend's history on its own, with a way back. */}
              {phone && personId ? null : (
                <div className="debts-half">
                  {people.length === 0 ? <EmptyState text={t('debts.empty')} action={{ label: t('debts.addFriend'), onClick: () => setAddingFriend(true) }} /> : null}
                  {people.length > 0 && phone ? <ListGroup>{people.map(tapRow)}</ListGroup> : null}
                  {people.length > 0 && !phone ? (
                    <Table
                      className="friends-table"
                      tidy
                      heads={[
                        t('common.name'),
                        { label: t('debts.colStatus'), className: 'head-status' },
                        { label: t('sync.field.reminder'), className: 'head-reminder' },
                        { label: t('debts.balance'), className: 'is-end' },
                        '',
                      ]}
                    >
                      {people.map(tableRow)}
                    </Table>
                  ) : null}
                </div>
              )}
              {/* One friend, or the debts of all of them. The friend is a new one each time (key), so nothing of the last is shown while it is read. */}
              {personId ? (
                <FriendLedger key={personId} personId={personId} settings={settings} now={now} reload={reload} onChanged={refresh} onPerson={setSelected} />
              ) : (
                <div className="debts-half">
                  <h2 className="debts-heading">{t('debts.ledger')}</h2>
                  {history ? <DebtsHistory history={history} currency={settings.currency} locale={settings.locale} today={now.date} onPage={setHistoryOffset} /> : null}
                </div>
              )}
            </div>
          </div>
          {/* The statement stays at the window's bottom while the history scrolls above it, as the pager does on the transactions page. */}
          {friend && !phone ? (
            <div className="debts-foot wide">
              <DebtsFiles personId={friend.id} onSaved={() => say(t('export.saved'), 'success')} onFailed={(text) => say(text, 'error')} />
            </div>
          ) : null}
        </>
      ) : null}
    </section>
  )
}
