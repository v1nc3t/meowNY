import { useEffect, useState } from 'react'
import { Link, useLocation, useNavigate } from 'react-router-dom'
import { debts, type EntryKind, type History, type Person, type Reminders } from '../api/debts'
import { errorText } from '../api/invoke'
import { scanQr, stopScan } from '../api/scanner'
import { useToday, type Today } from '../api/today'
import BillAcceptModal from '../components/BillAcceptModal'
import ConfirmModal from '../components/ConfirmModal'
import DebtsHistory from '../components/DebtsHistory'
import EntryModal from '../components/EntryModal'
import FriendNameModal from '../components/FriendNameModal'
import SectionTabs from '../components/SectionTabs'
import SettleModal from '../components/SettleModal'
import Button from '../components/ui/Button'
import { EmptyState } from '../components/ui/Pixel'
import { useMoneySettings, type MoneySettings } from '../hooks/useMoneySettings'
import { useToast } from '../hooks/toast'
import { useProfile } from '../hooks/profile'
import { useT } from '../i18n/context'
import { useShortcut } from '../keys/registry'
import { tip } from '../keys/tip'
import { useRowNavigation } from '../keys/useRowNavigation'
import { balanceView } from '../lib/balance'
import { formatDate, formatMoney } from '../lib/money'
import { reminderEvery, remindersChanged } from '../lib/reminders'

export default function DebtsPage() {
  const { settings, error } = useMoneySettings()
  const now = useToday()
  if (error) return <p className="notice">{error}</p>
  if (!settings || !now) return null
  return <DebtsView settings={settings} now={now} />
}

type Tab = 'friends' | 'history'

function DebtsView({ settings, now }: { settings: MoneySettings; now: Today }) {
  const t = useT()
  const phone = useProfile() === 'phone'
  const navigate = useNavigate()
  const location = useLocation()
  const [people, setPeople] = useState<Person[] | null>(null)
  const [reminders, setReminders] = useState<Reminders | null>(null)
  const [tab, setTab] = useState<Tab>('friends')
  const [history, setHistory] = useState<History | null>(null)
  const [historyOffset, setHistoryOffset] = useState(0)
  // a code that was scanned or is being pasted, and the camera while it looks
  const [taking, setTaking] = useState<string | null>(null)
  const [scanning, setScanning] = useState(false)
  const [showRemoved, setShowRemoved] = useState(false)
  const [reload, setReload] = useState(0)
  const say = useToast()
  const [addingFriend, setAddingFriend] = useState(false)
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

  // the history is read a page at a time, only while it is shown
  useEffect(() => {
    if (tab !== 'history') return
    let cancelled = false
    void debts.history({ offset: historyOffset }).then(
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
  }, [tab, historyOffset, reload, say])

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
    setReload((value) => value + 1)
    remindersChanged()
  }

  async function bringBack(person: Person) {
    try {
      await debts.unarchive(person.id)
    } catch (failure) {
      say(errorText(failure), 'error')
      return
    }
    setReload((value) => value + 1)
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

  // The phone's round button sends the person here to scan a bill or add a friend: the camera or the form opens at once, once.
  useEffect(() => {
    const asked = location.state as { scan?: boolean; addFriend?: boolean } | null
    if (!phone || (!asked?.scan && !asked?.addFriend)) return
    navigate(location.pathname, { replace: true, state: null })
    if (asked.scan) void onScan()
    else setAddingFriend(true)
    // each time the page is asked, when it opens and when it is open already; the request is taken out of the location, so once
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [location.state])

  const dueIds = new Set(reminders?.due.map((friend) => friend.personId))
  const dueOn = new Map(reminders?.due.map((friend) => [friend.personId, friend.dueOn]))

  // Keyboard: n adds money lent, j and k move between rows.
  useShortcut('new', () => setEntryKind('LENT'))
  useRowNavigation()

  const balanceText = (person: Person) => {
    const { tone, abs } = balanceView(person.balance)
    if (tone === 'even') return t('debts.settledUp')
    return t(tone === 'owed' ? 'debts.owesYou' : 'debts.youOweThem', { name: person.name, amount: money(abs) })
  }

  return (
    <section className="tx-page">
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
            <Button variant="outline" onClick={() => navigate('/app/transactions', { state: { split: true } })}>
              {t('split.add')}
            </Button>
          )}
          <Button variant="outline" onClick={() => setTaking('')}>
            {t('code.paste')}
          </Button>
        </div>
      </div>
      {reminders && reminders.due.length > 0 ? (
        <p className="notice">{t('debts.due', { names: reminders.due.map((friend) => friend.name).join(', ') })}</p>
      ) : null}
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
            setReload((value) => value + 1)
            remindersChanged()
          }}
        />
      ) : null}
      {addingFriend ? (
        <FriendNameModal
          onClose={() => setAddingFriend(false)}
          onSaved={(person) => {
            setAddingFriend(false)
            setReload((value) => value + 1)
            navigate(`/app/debts/${person.id}`)
          }}
        />
      ) : null}
      {entryKind ? (
        <EntryModal
          entry={null}
          kind={entryKind}
          today={now.date}
          onClose={() => setEntryKind(null)}
          onSaved={(notAttached) => {
            setEntryKind(null)
            if (notAttached.length > 0) say(t('docs.attachFailed', { names: notAttached.join(', ') }), 'warning')
            setReload((value) => value + 1)
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
            setReload((value) => value + 1)
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
            setReload((value) => value + 1)
          }}
        />
      ) : null}
      {deleting ? (
        <ConfirmModal
          title={t('rec.confirmTitle', { action: t('common.delete'), name: deleting.name })}
          pending={pending}
          onConfirm={() => void onDelete(deleting)}
          onClose={() => setDeleting(null)}
        />
      ) : null}
      <SectionTabs
        label={t('debts.title')}
        value={tab}
        onChange={setTab}
        tabs={[
          { id: 'friends', label: t('debts.tabFriends') },
          { id: 'history', label: t('debts.historyTab') },
        ]}
      />
      {tab === 'history' && history ? (
        <div className="tx-pane wide pane-stack" role="tabpanel">
          <DebtsHistory history={history} currency={settings.currency} locale={settings.locale} onPage={setHistoryOffset} />
        </div>
      ) : null}
      {tab === 'friends' && people ? (
        <div className="tx-pane wide pane-stack" role="tabpanel">
          <div className="debts-bar">
            <Button variant="outline" onClick={() => setAddingFriend(true)}>
              {t('debts.addFriend')}
            </Button>
            <label>
              <input type="checkbox" checked={showRemoved} onChange={(event) => setShowRemoved(event.target.checked)} />
              {t('debts.showRemoved')}
            </label>
          </div>
          {people.length === 0 ? <EmptyState text={t('debts.empty')} /> : null}
          <ul className="card-list">
            {people.map((person) => {
              const { tone } = balanceView(person.balance)
              return (
                <li key={person.id} className="card-item" data-archived={person.archived || undefined}>
                  <div className="card-body">
                    <p className="card-name">
                      <Link to={`/app/debts/${person.id}`}>{person.name}</Link>
                      {person.archived ? <span className="muted">{` · ${t('debts.removed')}`}</span> : null}
                    </p>
                    <p className="card-meta">
                      <span className={`card-amount debt-${tone}`}>{balanceText(person)}</span>
                      {dueIds.has(person.id) && tone !== 'even' ? ` · ${t('debts.dueBadge', { date: formatDate(dueOn.get(person.id) ?? '', settings.locale) })}` : ''}
                    </p>
                  </div>
                  <div className="card-actions">
                    <Button variant="ghost" size="sm" data-row-focus onClick={() => setEditing(person)}>
                      {t('common.edit')}
                    </Button>
                    {person.archived ? (
                      <Button variant="ghost" size="sm" onClick={() => void bringBack(person)}>
                        {t('debts.unarchive')}
                      </Button>
                    ) : (
                      <Button variant="ghost" size="sm" className="row-danger" onClick={() => setDeleting(person)}>
                        {t('common.delete')}
                      </Button>
                    )}
                    {tone !== 'even' ? (
                      <Button variant="outline" size="sm" onClick={() => setSettling(person)}>
                        {t('debts.settle')}
                      </Button>
                    ) : null}
                  </div>
                </li>
              )
            })}
          </ul>
        </div>
      ) : null}
    </section>
  )
}
