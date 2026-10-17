import { useEffect, useState } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { debts, type EntryKind, type Person, type Summary } from '../api/debts'
import { errorText } from '../api/invoke'
import { useToday, type Today } from '../api/today'
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
  const [showRemoved, setShowRemoved] = useState(false)
  const [reload, setReload] = useState(0)
  const [message, setMessage] = useNotice()
  const [addingFriend, setAddingFriend] = useState(false)
  const [entryKind, setEntryKind] = useState<EntryKind | null>(null)
  const [settling, setSettling] = useState<Person | null>(null)
  const money = (amount: string) => formatMoney(amount, settings.currency, settings.locale)

  useEffect(() => {
    let cancelled = false
    void Promise.all([debts.people(showRemoved), debts.summary()]).then(
      ([list, totals]) => {
        if (cancelled) return
        setPeople(list)
        setSummary(totals)
      },
      (failure) => {
        if (!cancelled) setMessage(errorText(failure))
      },
    )
    return () => {
      cancelled = true
    }
  }, [showRemoved, reload, setMessage])

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
        </div>
      </div>
      <p className="notice">{message}</p>
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
                      {person.remindOn && tone !== 'even' ? ` · ${t('debts.reminderBadge', { date: formatDate(person.remindOn, settings.locale) })}` : ''}
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
