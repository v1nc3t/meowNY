import { useEffect, useMemo, useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import { debts, type Entry, type EntryKind, type Line, type Statement } from '../api/debts'
import { ApiError, errorText } from '../api/invoke'
import { useToday, type Today } from '../api/today'
import BillCodeModal from '../components/BillCodeModal'
import ClipMark from '../components/ClipMark'
import ConfirmModal from '../components/ConfirmModal'
import DebtsFiles from '../components/DebtsFiles'
import EntryModal from '../components/EntryModal'
import FriendNameModal from '../components/FriendNameModal'
import ReminderBlock from '../components/ReminderBlock'
import SettleModal from '../components/SettleModal'
import { useClipCounts } from '../hooks/useClipCounts'
import { useMoneySettings, type MoneySettings } from '../hooks/useMoneySettings'
import { useNotice } from '../hooks/useNotice'
import { useProfile } from '../hooks/profile'
import { useT } from '../i18n/context'
import { useShortcut } from '../keys/registry'
import { useRowNavigation } from '../keys/useRowNavigation'
import { balanceView, entryOfLine } from '../lib/balance'
import { formatDate, formatMoney } from '../lib/money'
import { remindersChanged } from '../lib/reminders'

export default function FriendPage() {
  const { settings, error } = useMoneySettings()
  const now = useToday()
  if (error) return <p className="notice">{error}</p>
  if (!settings || !now) return null
  return <FriendView settings={settings} now={now} />
}

type Dialog =
  | { kind: 'entry'; entry: Entry | null; as?: EntryKind }
  | { kind: 'settle' }
  | { kind: 'rename' }
  | { kind: 'delete'; entry: Entry }
  | { kind: 'code'; transactionId: string; amount: string }
  | null

function FriendView({ settings, now }: { settings: MoneySettings; now: Today }) {
  const t = useT()
  const phone = useProfile() === 'phone'
  const { personId = '' } = useParams()
  const navigate = useNavigate()
  const [statement, setStatement] = useState<Statement | null>(null)
  const [missing, setMissing] = useState(false)
  const [reload, setReload] = useState(0)
  const [dialog, setDialog] = useState<Dialog>(null)
  const [pending, setPending] = useState(false)
  const [message, setMessage] = useNotice()
  const money = (amount: string) => formatMoney(amount, settings.currency, settings.locale)

  useEffect(() => {
    let cancelled = false
    void debts.statement(personId).then(
      (loaded) => {
        if (!cancelled) setStatement(loaded)
      },
      (failure) => {
        if (cancelled) return
        if (failure instanceof ApiError && (failure.code === 'not_found' || failure.code === 'invalid_input')) setMissing(true)
        else setMessage(errorText(failure))
      },
    )
    return () => {
      cancelled = true
    }
  }, [personId, reload, setMessage])

  // Newest first: the balance after each line still reads the same.
  const lines = useMemo(() => [...(statement?.lines ?? [])].reverse(), [statement])
  const entryIds = useMemo(() => lines.flatMap((line) => (line.entryId ? [line.entryId] : [])), [lines])
  const clips = useClipCounts('ledger_entry', entryIds, reload)

  // Keyboard: n adds money lent, j and k move between rows.
  useShortcut('new', () => setDialog({ kind: 'entry', entry: null, as: 'LENT' }))
  useRowNavigation()

  function done(text = '') {
    setDialog(null)
    setMessage(text)
    setReload((value) => value + 1)
    // a balance or a reminder may have changed: the phone schedules again
    remindersChanged()
  }

  async function run(action: () => Promise<unknown>) {
    setPending(true)
    try {
      await action()
    } catch (failure) {
      setMessage(errorText(failure))
      return
    } finally {
      setPending(false)
    }
    done()
  }

  if (missing) {
    return (
      <section className="tx-page">
        <p>
          <Link to="/app/debts">{t('debts.back')}</Link>
        </p>
        <p className="notice">{errorText(new ApiError('not_found'))}</p>
      </section>
    )
  }
  if (!statement) return <p className="notice">{message}</p>

  const { person } = statement
  const { tone, abs } = balanceView(person.balance)
  const summaryText =
    tone === 'even'
      ? t('debts.settledUp')
      : t(tone === 'owed' ? 'debts.owesYou' : 'debts.youOweThem', { name: person.name, amount: money(abs) })
  const lineLabel = (line: Line) => {
    const kind = t(`debts.kind.${line.kind}`)
    return line.what ? `${kind}: ${line.what}` : kind
  }
  const effectText = (line: Line) => `${line.effect.startsWith('-') ? '-' : '+'}${money(line.effect.replace('-', ''))}`

  return (
    <section className="tx-page">
      <div className="tx-title">
        <h1>{person.name}</h1>
        <div className="tx-actions">
          <button type="button" onClick={() => setDialog({ kind: 'entry', entry: null, as: 'LENT' })}>
            {t('debts.lent')}
          </button>
          <button type="button" onClick={() => setDialog({ kind: 'entry', entry: null, as: 'BORROWED' })}>
            {t('debts.borrowed')}
          </button>
          <button type="button" onClick={() => navigate('/app/transactions', { state: { split: true, friendId: person.id, friendName: person.name } })}>
            {t('split.add')}
          </button>
          {tone !== 'even' ? (
            <button type="button" onClick={() => setDialog({ kind: 'settle' })}>
              {t('debts.settle')}
            </button>
          ) : null}
        </div>
      </div>
      <p className="notice">{message}</p>
      {dialog?.kind === 'entry' ? (
        <EntryModal
          entry={dialog.entry}
          {...(dialog.as ? { kind: dialog.as } : {})}
          personId={person.id}
          today={now.date}
          onClose={() => setDialog(null)}
          onSaved={(notAttached) => done(notAttached.length > 0 ? t('docs.attachFailed', { names: notAttached.join(', ') }) : '')}
        />
      ) : null}
      {dialog?.kind === 'code' ? (
        <BillCodeModal transactionId={dialog.transactionId} person={{ id: person.id, name: person.name }} part={dialog.amount} onClose={() => setDialog(null)} />
      ) : null}
      {dialog?.kind === 'settle' ? <SettleModal person={person} today={now.date} onClose={() => setDialog(null)} onDone={() => done(t('debts.settled'))} /> : null}
      {dialog?.kind === 'rename' ? <FriendNameModal person={person} onClose={() => setDialog(null)} onSaved={() => done()} /> : null}
      {dialog?.kind === 'delete' ? (
        <ConfirmModal
          title={t('rec.confirmTitle', {
            action: t('common.delete'),
            name: `${t(`debts.kind.${dialog.entry.kind}`)}${dialog.entry.note ? `: ${dialog.entry.note}` : ''}`,
          })}
          pending={pending}
          onConfirm={() => void run(() => debts.removeEntry(dialog.entry.id))}
          onClose={() => setDialog(null)}
        />
      ) : null}
      <div className="tx-pane wide">
        <div className="dash-card">
          <p className={`debt-total debt-${tone}`}>{summaryText}</p>
          {person.remindOn && tone !== 'even' ? (
            <p className="hint">{t('debts.reminderBadge', { date: formatDate(person.remindOn, settings.locale) })}</p>
          ) : null}
          <div className="card-actions">
            <Link to="/app/debts">{t('debts.back')}</Link>
            <button type="button" onClick={() => setDialog({ kind: 'rename' })}>
              {t('common.rename')}
            </button>
            <button type="button" disabled={pending} onClick={() => void run(() => (person.archived ? debts.unarchive(person.id) : debts.archive(person.id)))}>
              {t(person.archived ? 'debts.unarchive' : 'debts.archive')}
            </button>
          </div>
        </div>
        <ReminderBlock key={`${person.remindOn}-${person.remindEveryDays}`} person={person} today={now.date} onDone={done} />
        <h2>{t('debts.ledger')}</h2>
        {lines.length === 0 ? <p>{t('debts.noLines')}</p> : null}
        <ul className="card-list">
          {lines.map((line, at) => {
            const entry = entryOfLine(line, person.id)
            return (
              <li key={`${line.entryId ?? 'bill'}-${line.transactionId ?? ''}-${at}`} className="card-item">
                <div className="card-body">
                  <p className="card-name">{lineLabel(line)}</p>
                  <p className="card-meta">
                    <span className={`card-amount debt-${line.effect.startsWith('-') ? 'owe' : 'owed'}`}>{effectText(line)}</span>
                    {` · ${formatDate(line.date, settings.locale)} · ${t('debts.balance')} ${money(line.balance)}`}
                  </p>
                  {line.entryId && clips[line.entryId] && entry ? (
                    <ClipMark count={clips[line.entryId] ?? 0} onClick={() => setDialog({ kind: 'entry', entry })} />
                  ) : null}
                </div>
                {line.kind === 'BILL' && line.transactionId ? (
                  <div className="card-actions">
                    <button type="button" data-row-focus onClick={() => setDialog({ kind: 'code', transactionId: line.transactionId ?? '', amount: line.amount })}>
                      {t('code.show')}
                    </button>
                  </div>
                ) : null}
                {entry ? (
                  <div className="card-actions">
                    <button type="button" data-row-focus onClick={() => setDialog({ kind: 'entry', entry })}>
                      {t('common.edit')}
                    </button>
                    <button type="button" onClick={() => setDialog({ kind: 'delete', entry })}>
                      {t('common.delete')}
                    </button>
                  </div>
                ) : null}
              </li>
            )
          })}
        </ul>
        {phone ? null : <DebtsFiles personId={person.id} onSaved={() => setMessage(t('export.saved'))} onFailed={setMessage} />}
      </div>
    </section>
  )
}
