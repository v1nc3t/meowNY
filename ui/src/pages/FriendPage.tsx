import { useEffect, useMemo, useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import { debts, type Entry, type EntryKind, type Line, type Statement } from '../api/debts'
import { ApiError, errorText } from '../api/invoke'
import { useToday, type Today } from '../api/today'
import BillCodeModal from '../components/BillCodeModal'
import ClipPreview from '../components/ClipPreview'
import ConfirmModal from '../components/ConfirmModal'
import DebtsFiles from '../components/DebtsFiles'
import EntryModal from '../components/EntryModal'
import FriendNameModal from '../components/FriendNameModal'
import SettleModal from '../components/SettleModal'
import Button from '../components/ui/Button'
import { Card, CardContent, CardHeader, CardTitle } from '../components/ui/Card'
import { buttonClass } from '../components/ui/cn'
import { useClipCounts } from '../hooks/useClipCounts'
import { useFocusRequest, useJumpTo } from '../hooks/useFocusRequest'
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
  // a document can send the person here for one of its entries: the line is scrolled to and outlined
  const { request: focus, done: focusDone } = useFocusRequest('ledger_entry')
  useJumpTo(focus, focusDone, lines)

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
        <Link to="/app/debts" className={buttonClass('outline', 'sm')}>
          {t('debts.back')}
        </Link>
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
        <div className="tx-actions debts-actions">
          <Button onClick={() => setDialog({ kind: 'entry', entry: null, as: 'LENT' })}>{t('debts.lent')}</Button>
          <Button variant="outline" onClick={() => setDialog({ kind: 'entry', entry: null, as: 'BORROWED' })}>
            {t('debts.borrowed')}
          </Button>
          <Button variant="outline" onClick={() => navigate('/app/transactions', { state: { split: true, friendId: person.id, friendName: person.name } })}>
            {t('split.add')}
          </Button>
          {tone !== 'even' ? (
            <Button variant="outline" onClick={() => setDialog({ kind: 'settle' })}>
              {t('debts.settle')}
            </Button>
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
        <BillCodeModal target={{ kind: 'transaction', id: dialog.transactionId }} person={{ id: person.id, name: person.name }} part={dialog.amount} onClose={() => setDialog(null)} />
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
      <div className="tx-pane wide pane-stack">
        <Card>
          <CardHeader>
            <CardTitle className={`debt-total debt-${tone}`}>{summaryText}</CardTitle>
          </CardHeader>
          <CardContent className="card-actions">
            <Link to="/app/debts" className={buttonClass('outline', 'sm')}>
              {t('debts.back')}
            </Link>
            <Button variant="outline" size="sm" onClick={() => setDialog({ kind: 'rename' })}>
              {t('common.rename')}
            </Button>
            <Button
              variant="outline"
              size="sm"
              disabled={pending}
              onClick={() => void run(() => (person.archived ? debts.unarchive(person.id) : debts.archive(person.id)))}
            >
              {t(person.archived ? 'debts.unarchive' : 'debts.archive')}
            </Button>
          </CardContent>
        </Card>
        <h2 className="debts-heading">{t('debts.ledger')}</h2>
        {lines.length === 0 ? <p className="hint">{t('debts.noLines')}</p> : null}
        <ul className="card-list">
          {lines.map((line, at) => {
            const entry = entryOfLine(line, person.id)
            return (
              <li key={`${line.entryId ?? 'bill'}-${line.transactionId ?? ''}-${at}`} className="card-item" data-item={line.entryId ? `ledger_entry:${line.entryId}` : undefined}>
                <div className="card-body">
                  <p className="card-name">{lineLabel(line)}</p>
                  <p className="card-meta">
                    <span className={`card-amount debt-${line.effect.startsWith('-') ? 'owe' : 'owed'}`}>{effectText(line)}</span>
                    {` · ${formatDate(line.date, settings.locale)} · ${t('debts.balance')} ${money(line.balance)}`}
                  </p>
                  {line.entryId && clips[line.entryId] ? (
                    <ClipPreview target={{ kind: 'ledger_entry', id: line.entryId }} name={lineLabel(line)} count={clips[line.entryId] ?? 0} />
                  ) : null}
                </div>
                {line.kind === 'BILL' && line.transactionId ? (
                  <div className="card-actions">
                    <Button variant="ghost" size="sm" data-row-focus onClick={() => setDialog({ kind: 'code', transactionId: line.transactionId ?? '', amount: line.amount })}>
                      {t('code.show')}
                    </Button>
                  </div>
                ) : null}
                {entry ? (
                  <div className="card-actions">
                    <Button variant="ghost" size="sm" data-row-focus onClick={() => setDialog({ kind: 'entry', entry })}>
                      {t('common.edit')}
                    </Button>
                    <Button variant="ghost" size="sm" className="row-danger" onClick={() => setDialog({ kind: 'delete', entry })}>
                      {t('common.delete')}
                    </Button>
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
