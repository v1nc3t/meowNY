import { useEffect, useState, type MouseEvent } from 'react'
import { Link, useLocation, useNavigate, useParams } from 'react-router-dom'
import { debts, type EntryKind, type History, type Person, type Reminders } from '../api/debts'
import { errorText } from '../api/invoke'
import { scanQr, stopScan } from '../api/scanner'
import { useToday, type Today } from '../api/today'
import BillAcceptModal from '../components/BillAcceptModal'
import ConfirmModal from '../components/ConfirmModal'
import DebtNotificationsModal from '../components/DebtNotificationsModal'
import DebtsFiles from '../components/DebtsFiles'
import DebtsHistory from '../components/DebtsHistory'
import EntryModal from '../components/EntryModal'
import FriendLedger from '../components/FriendLedger'
import FriendNameModal from '../components/FriendNameModal'
import Icon from '../components/NavIcon'
import { IconAction, Row, Table, TapRow } from '../components/RowParts'
import SettleModal from '../components/SettleModal'
import Button from '../components/ui/Button'
import { buttonClass } from '../components/ui/cn'
import { EmptyState } from '../components/ui/Pixel'
import { useMoneySettings, type MoneySettings } from '../hooks/useMoneySettings'
import { useOpenRow } from '../hooks/useOpenRow'
import { useToast } from '../hooks/toast'
import { useProfile } from '../hooks/profile'
import { useT } from '../i18n/context'
import { useShortcut } from '../keys/registry'
import { tip } from '../keys/tip'
import { useRowNavigation } from '../keys/useRowNavigation'
import { balanceView } from '../lib/balance'
import { formatDate, formatMoney } from '../lib/money'
import { dismissedAfter, dismissedDays, notDismissed, reminderEvery, remindersChanged, saveDismissedDays } from '../lib/reminders'
import { saveChoice, storedChoice } from '../lib/sticky'

export default function DebtsPage() {
  const { settings, error } = useMoneySettings()
  const now = useToday()
  if (error) return <p className="notice">{error}</p>
  if (!settings || !now) return null
  return <DebtsView settings={settings} now={now} />
}

function DebtsView({ settings, now }: { settings: MoneySettings; now: Today }) {
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

  // A desktop row: name, who owes whom, the reminder, the amount, then what can be done: mark as paid (or bring back), rename, and take off
  // the list (not for one that is removed already, whose place stays empty so the icons line up). The friend whose history is on the
  // right is marked. The name is the link for the keyboard (j and k stop on it).
  function tableRow(person: Person) {
    const { tone, abs } = balanceView(person.balance)
    const reminded = dueIds.has(person.id) && tone !== 'even'
    const chosen = person.id === personId
    return (
      <Row
        key={person.id}
        className={chosen ? 'is-selected' : undefined}
        data-archived={person.archived || undefined}
        onClick={(event) => choose(event, person)}
        actions={
          <>
            {person.archived ? (
              <Button variant="outline" size="sm" className="row-primary" onClick={() => void bringBack(person)}>
                {t('debts.unarchive')}
              </Button>
            ) : tone !== 'even' ? (
              <Button variant="outline" size="sm" className="row-primary" onClick={() => setSettling(person)}>
                {t('debts.settle')}
              </Button>
            ) : null}
            <IconAction icon="pencil" label={t('common.rename')} onClick={() => setEditing(person)} />
            {person.archived ? <span className="icon-slot" /> : <IconAction icon="trash" label={t('debts.archive')} danger onClick={() => setDeleting(person)} />}
          </>
        }
      >
        <div className="cell-name">
          <Link className="card-name" to={rowLink(person)} title={person.name} aria-current={chosen ? 'true' : undefined} data-row-focus>
            {person.name}
          </Link>
          {person.archived ? <span className="row-mark">{t('debts.removed')}</span> : null}
        </div>
        <span className={`cell-sub debt-${tone}`}>{statusText(tone)}</span>
        <span className="cell-sub-end">{reminded ? t('debts.dueBadge', { date: formatDate(dueOn.get(person.id) ?? '', settings.locale) }) : ''}</span>
        <span className={`cell-amount debt-${tone}`}>{tone === 'even' ? '' : money(abs)}</span>
      </Row>
    )
  }

  // A phone row: the name and what is owed, who owes whom under them. It opens to a labelled button for everything that can be done.
  function tapRow(person: Person) {
    const { tone, abs } = balanceView(person.balance)
    const reminded = dueIds.has(person.id) && tone !== 'even'
    return (
      <TapRow
        key={person.id}
        id={rows.panelId(person.id)}
        data-archived={person.archived || undefined}
        open={rows.isOpen(person.id)}
        onToggle={() => rows.toggle(person.id)}
        name={
          <>
            {person.name}
            {person.archived ? <span className="row-mark">{t('debts.removed')}</span> : null}
          </>
        }
        amount={<span className={`cell-amount debt-${tone}`}>{tone === 'even' ? '' : money(abs)}</span>}
        sub={
          <>
            <span className={`debt-${tone}`}>{statusText(tone)}</span>
            {reminded ? <span className="is-line">{t('debts.dueBadge', { date: formatDate(dueOn.get(person.id) ?? '', settings.locale) })}</span> : null}
          </>
        }
      >
        <div className="row-panel-actions">
          {person.archived ? (
            <Button variant="outline" onClick={() => void bringBack(person)}>
              {t('debts.unarchive')}
            </Button>
          ) : tone !== 'even' ? (
            <Button variant="outline" onClick={() => setSettling(person)}>
              {t('debts.settle')}
            </Button>
          ) : null}
          <Link to={friendLink(person)} className={buttonClass('outline')}>
            {t('debts.ledger')}
          </Link>
          <Button variant="outline" onClick={() => setEditing(person)}>
            <Icon name="pencil" className="btn-icon" />
            {t('common.rename')}
          </Button>
          {person.archived ? null : (
            <Button variant="outline" className="row-danger" onClick={() => setDeleting(person)}>
              <Icon name="trash" className="btn-icon" />
              {t('debts.archive')}
            </Button>
          )}
        </div>
      </TapRow>
    )
  }

  return (
    <section className="tx-page debts-page">
      <div className="tx-title">
        <h1>{t('debts.title')}</h1>
        <div className="tx-actions debts-actions">
          <Button onClick={() => setEntryKind('LENT')} {...tip('new', t('debts.lent'))}>
            {t('debts.lent')}
          </Button>
          <Button variant="outline" onClick={() => setEntryKind('BORROWED')}>
            {t('debts.borrowed')}
          </Button>
          {/* The phone has scan in its place: four buttons on both. A bill is still split from a new expense. */}
          {phone ? (
            <Button variant="outline" disabled={scanning} onClick={() => void onScan()}>
              {t('code.scanShort')}
            </Button>
          ) : (
            <Button
              variant="outline"
              onClick={() => navigate('/app/transactions', { state: { split: true, ...(friend ? { friendId: friend.id, friendName: friend.name } : {}) } })}
            >
              {t('split.add')}
            </Button>
          )}
          <Button variant="outline" onClick={() => setTaking('')}>
            {t('code.paste')}
          </Button>
        </div>
      </div>
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
          <div className="tx-pane wide debts-view">
            <div className="debts-split">
              {/* The phone shows a friend's history on its own, with a way back. */}
              {phone && personId ? null : (
                <div className="debts-half">
                  <div className="debts-bar">
                    <Button variant="outline" onClick={() => setAddingFriend(true)}>
                      {t('debts.addFriend')}
                    </Button>
                    <Button variant="outline" onClick={() => setNotifying(true)}>
                      <Icon name="bell" className="btn-icon" />
                      {due.length > 0 ? `${t('debts.notifications')} (${due.length})` : t('debts.notifications')}
                    </Button>
                    <label>
                      <input
                        type="checkbox"
                        checked={showRemoved}
                        onChange={(event) => {
                          saveChoice('debts.removed', event.target.checked)
                          setShowRemoved(event.target.checked)
                        }}
                      />
                      {t('debts.showRemoved')}
                    </label>
                  </div>
                  {people.length === 0 ? <EmptyState text={t('debts.empty')} action={{ label: t('debts.addFriend'), onClick: () => setAddingFriend(true) }} /> : null}
                  {people.length > 0 && phone ? <ul className="card-list">{people.map(tapRow)}</ul> : null}
                  {people.length > 0 && !phone ? (
                    <Table className="friends-table" heads={[t('common.name'), { label: t('debts.colStatus'), className: 'span-2' }, { label: t('debts.balance'), className: 'is-end' }, '']}>
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
