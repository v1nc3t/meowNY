import { useEffect, useState } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { debts, type EntryKind, type Person, type Reminders, type Summary } from '../api/debts'
import { errorText } from '../api/invoke'
import { scanQr, stopScan } from '../api/scanner'
import { useToday, type Today } from '../api/today'
import BillAcceptModal from '../components/BillAcceptModal'
import DebtsFiles from '../components/DebtsFiles'
import EntryModal from '../components/EntryModal'
import FriendNameModal from '../components/FriendNameModal'
import SettleModal from '../components/SettleModal'
import { useMoneySettings, type MoneySettings } from '../hooks/useMoneySettings'
import { useNotice } from '../hooks/useNotice'
import { useProfile } from '../hooks/profile'
import { useT } from '../i18n/context'
import { useShortcut } from '../keys/registry'
import { tip } from '../keys/tip'
import { useRowNavigation } from '../keys/useRowNavigation'
import { balanceView } from '../lib/balance'
import { formatDate, formatMoney } from '../lib/money'
import { remindersChanged } from '../lib/reminders'

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
  const [people, setPeople] = useState<Person[] | null>(null)
  const [summary, setSummary] = useState<Summary | null>(null)
  const [reminders, setReminders] = useState<Reminders | null>(null)
  // a code that was scanned or is being pasted, and the camera while it looks
  const [taking, setTaking] = useState<string | null>(null)
  const [scanning, setScanning] = useState(false)
  const [showRemoved, setShowRemoved] = useState(false)
  const [reload, setReload] = useState(0)
  const [message, setMessage] = useNotice()
  const [addingFriend, setAddingFriend] = useState(false)
  const [entryKind, setEntryKind] = useState<EntryKind | null>(null)
  const [settling, setSettling] = useState<Person | null>(null)
  const money = (amount: string) => formatMoney(amount, settings.currency, settings.locale)

  useEffect(() => {
    let cancelled = false
    void Promise.all([debts.people(showRemoved), debts.summary(), debts.reminders()]).then(
      ([list, totals, due]) => {
        if (cancelled) return
        setPeople(list)
        setSummary(totals)
        setReminders(due)
      },
      (failure) => {
        if (!cancelled) setMessage(errorText(failure))
      },
    )
    return () => {
      cancelled = true
    }
  }, [showRemoved, reload, setMessage])

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
    else if (result.kind === 'denied') setMessage(t('phone.cameraDenied'))
    else if (result.kind === 'unavailable') setMessage(t('phone.noScanner'))
  }

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
        <div className="tx-actions">
          <button type="button" onClick={() => setEntryKind('LENT')} {...tip('new', t('debts.lent'))}>
            {t('debts.lent')}
          </button>
          <button type="button" onClick={() => setEntryKind('BORROWED')}>
            {t('debts.borrowed')}
          </button>
          <button type="button" onClick={() => setAddingFriend(true)}>
            {t('debts.addFriend')}
          </button>
          <button type="button" onClick={() => navigate('/app/transactions', { state: { split: true } })}>
            {t('split.add')}
          </button>
          {phone ? (
            <button type="button" disabled={scanning} onClick={() => void onScan()}>
              {t('code.scan')}
            </button>
          ) : null}
          <button type="button" onClick={() => setTaking('')}>
            {t('code.paste')}
          </button>
        </div>
      </div>
      <p className="notice">{message}</p>
      {reminders && reminders.due.length > 0 ? (
        <p className="notice">{t('debts.due', { names: reminders.due.map((friend) => friend.name).join(', ') })}</p>
      ) : null}
      {scanning ? (
        <div className="scan-overlay" role="dialog" aria-label={t('code.scan')}>
          <p>{t('code.scanning')}</p>
          <button type="button" onClick={() => void stopScan()}>
            {t('common.cancel')}
          </button>
        </div>
      ) : null}
      {taking !== null ? (
        <BillAcceptModal
          initialText={taking}
          onClose={() => setTaking(null)}
          onAccepted={() => {
            setTaking(null)
            setMessage(t('code.accepted'))
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
            setMessage(notAttached.length > 0 ? t('docs.attachFailed', { names: notAttached.join(', ') }) : '')
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
            setMessage(t('debts.settled'))
            setReload((value) => value + 1)
            remindersChanged()
          }}
        />
      ) : null}
      {summary && people ? (
        <div className="tx-pane wide">
          <div className="dash-card">
            <div className="dash-stats">
              <div className="dash-stat">
                <span className="dash-stat-label">{t('debts.owedToYou')}</span>
                <span className="dash-stat-value">{money(summary.owedToYou)}</span>
              </div>
              <div className="dash-stat">
                <span className="dash-stat-label">{t('debts.youOwe')}</span>
                <span className="dash-stat-value">{money(summary.youOwe)}</span>
              </div>
              <div className="dash-stat">
                <span className="dash-stat-label">{t('debts.net')}</span>
                <span className="dash-stat-value">{money(summary.net)}</span>
              </div>
            </div>
          </div>
          {people.length === 0 ? <p>{t('debts.empty')}</p> : null}
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
                      {person.remindOn && tone !== 'even'
                        ? ` · ${dueIds.has(person.id) ? t('debts.dueBadge', { date: formatDate(dueOn.get(person.id) ?? person.remindOn, settings.locale) }) : t('debts.reminderBadge', { date: formatDate(person.remindOn, settings.locale) })}`
                        : ''}
                    </p>
                  </div>
                  <div className="card-actions">
                    <button type="button" data-row-focus onClick={() => navigate(`/app/debts/${person.id}`)}>
                      {t('debts.statement')}
                    </button>
                    {tone !== 'even' ? (
                      <button type="button" onClick={() => setSettling(person)}>
                        {t('debts.settle')}
                      </button>
                    ) : null}
                  </div>
                </li>
              )
            })}
          </ul>
          <label>
            <input type="checkbox" checked={showRemoved} onChange={(event) => setShowRemoved(event.target.checked)} />
            {t('debts.showRemoved')}
          </label>
          {phone ? null : (
            <DebtsFiles
              onSaved={() => setMessage(t('export.saved'))}
              onFailed={setMessage}
            />
          )}
        </div>
      ) : null}
    </section>
  )
}
