import { useEffect, useId, useMemo, useState } from 'react'
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
import Icon from '../components/NavIcon'
import { IconAction, TapRow } from '../components/RowParts'
import SettleModal from '../components/SettleModal'
import Button from '../components/ui/Button'
import { buttonClass } from '../components/ui/cn'
import { useClipCounts } from '../hooks/useClipCounts'
import { useFocusRequest, useJumpTo } from '../hooks/useFocusRequest'
import { useMoneySettings, type MoneySettings } from '../hooks/useMoneySettings'
import { useToast } from '../hooks/toast'
import { useProfile } from '../hooks/profile'
import { useT } from '../i18n/context'
import { useShortcut } from '../keys/registry'
import { useRowNavigation } from '../keys/useRowNavigation'
import { balanceView, entryOfLine } from '../lib/balance'
import { byDay, dayHeading } from '../lib/dayHeading'
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
  const panelId = useId()
  const [openKey, setOpenKey] = useState<string | null>(null)
  const [statement, setStatement] = useState<Statement | null>(null)
  const [missing, setMissing] = useState(false)
  const [reload, setReload] = useState(0)
  const [dialog, setDialog] = useState<Dialog>(null)
  const [pending, setPending] = useState(false)
  const say = useToast()
  // Without the statement the page has nothing to show, so this failure stays in its place; it is not a pop-up.
  const [loadError, setLoadError] = useState('')
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
        else setLoadError(errorText(failure))
      },
    )
    return () => {
      cancelled = true
    }
  }, [personId, reload])

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

  function done() {
    setDialog(null)
    setReload((value) => value + 1)
    // a balance or a reminder may have changed: the phone schedules again
    remindersChanged()
  }

  async function run(action: () => Promise<unknown>) {
    setPending(true)
    try {
      await action()
    } catch (failure) {
      say(errorText(failure), 'error')
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
  if (!statement) return loadError ? <p className="notice">{loadError}</p> : null

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
  const lineKey = (line: Line, at: number) => `${line.entryId ?? 'bill'}-${line.transactionId ?? ''}-${at}`
  const lineItem = (line: Line) => (line.entryId ? `ledger_entry:${line.entryId}` : undefined)
  /** What a line changed: what the friend owes you more (green) or less (red). */
  const change = (line: Line) => <span className={`cell-amount debt-${line.effect.startsWith('-') ? 'owe' : 'owed'}`}>{effectText(line)}</span>
  const showCode = (line: Line) => setDialog({ kind: 'code', transactionId: line.transactionId ?? '', amount: line.amount })

  // A desktop row: what it was, when, how much it changed, the balance after it, then its buttons (show code for a bill; edit and delete
  // for an entry of the person's own).
  function tableRow(line: Line, at: number) {
    const entry = entryOfLine(line, person.id)
    const count = line.entryId ? (clips[line.entryId] ?? 0) : 0
    return (
      <li key={lineKey(line, at)} className="card-item" data-item={lineItem(line)}>
        <div className="cell-name">
          <span className="card-name" title={lineLabel(line)}>
            {lineLabel(line)}
          </span>
          {count && line.entryId ? <ClipPreview inline target={{ kind: 'ledger_entry', id: line.entryId }} name={lineLabel(line)} count={count} /> : null}
        </div>
        <span className="cell-sub">{formatDate(line.date, settings.locale)}</span>
        {change(line)}
        <span className="cell-sub-end">{money(line.balance)}</span>
        <div className="cell-actions">
          {line.kind === 'BILL' && line.transactionId ? <IconAction icon="qr" label={t('code.show')} data-row-focus onClick={() => showCode(line)} /> : null}
          {entry ? (
            <>
              <IconAction icon="pencil" label={t('common.edit')} data-row-focus onClick={() => setDialog({ kind: 'entry', entry })} />
              <IconAction icon="trash" label={t('common.delete')} danger onClick={() => setDialog({ kind: 'delete', entry })} />
            </>
          ) : null}
        </div>
      </li>
    )
  }

  // A phone row: what it was and how much it changed, the balance under them. It opens to its documents and a labelled button for
  // each thing that can be done; a line with neither is plain.
  function tapRow(line: Line, at: number) {
    const entry = entryOfLine(line, person.id)
    const count = line.entryId ? (clips[line.entryId] ?? 0) : 0
    const bill = line.kind === 'BILL' && line.transactionId
    const key = lineKey(line, at)
    return (
      <TapRow
        key={key}
        id={`${panelId}-${key}`}
        item={lineItem(line)}
        open={openKey === key}
        onToggle={() => setOpenKey(openKey === key ? null : key)}
        name={lineLabel(line)}
        amount={change(line)}
        sub={
          <>
            <span>{`${t('debts.balance')} ${money(line.balance)}`}</span>
            {count ? (
              <span className="row-mark-icon">
                <Icon name="clip" className="mark-icon" />
                <span aria-hidden="true">{count}</span>
                <span className="sr-only">{t('docs.clip', { count })}</span>
              </span>
            ) : null}
          </>
        }
      >
        {bill || entry || count ? (
          <>
            {count && line.entryId ? (
              <section className="detail-section">
                <p className="detail-title">{t('docs.title')}</p>
                <ClipPreview target={{ kind: 'ledger_entry', id: line.entryId }} name={lineLabel(line)} count={count} />
              </section>
            ) : null}
            <div className="row-panel-actions">
              {bill ? (
                <Button variant="outline" onClick={() => showCode(line)}>
                  <Icon name="qr" className="btn-icon" />
                  {t('code.show')}
                </Button>
              ) : null}
              {entry ? (
                <>
                  <Button variant="outline" onClick={() => setDialog({ kind: 'entry', entry })}>
                    <Icon name="pencil" className="btn-icon" />
                    {t('common.edit')}
                  </Button>
                  <Button variant="outline" className="row-danger" onClick={() => setDialog({ kind: 'delete', entry })}>
                    <Icon name="trash" className="btn-icon" />
                    {t('common.delete')}
                  </Button>
                </>
              ) : null}
            </div>
          </>
        ) : null}
      </TapRow>
    )
  }

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
      {dialog?.kind === 'entry' ? (
        <EntryModal
          entry={dialog.entry}
          {...(dialog.as ? { kind: dialog.as } : {})}
          personId={person.id}
          today={now.date}
          onClose={() => setDialog(null)}
          onSaved={(notAttached) => {
            done()
            if (notAttached.length > 0) say(t('docs.attachFailed', { names: notAttached.join(', ') }), 'warning')
          }}
        />
      ) : null}
      {dialog?.kind === 'code' ? (
        <BillCodeModal target={{ kind: 'transaction', id: dialog.transactionId }} person={{ id: person.id, name: person.name }} part={dialog.amount} onClose={() => setDialog(null)} />
      ) : null}
      {dialog?.kind === 'settle' ? <SettleModal person={person} today={now.date} onClose={() => setDialog(null)} onDone={() => {
            done()
            say(t('debts.settled'), 'success')
          }} /> : null}
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
        <div className="debts-summary">
          <p className={`debt-total debt-${tone}`}>{summaryText}</p>
          <div className="card-actions">
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
          </div>
        </div>
        <h2 className="debts-heading">{t('debts.ledger')}</h2>
        {lines.length === 0 ? <p className="hint">{t('debts.noLines')}</p> : null}
        {lines.length > 0 && phone
          ? byDay(lines, (line) => line.date).map((day) => (
              <div key={day.date} className="list-day">
                <h2>{dayHeading(day.date, settings.locale, now.date.slice(0, 4))}</h2>
                <ul className="card-list">{day.rows.map((line) => tapRow(line, lines.indexOf(line)))}</ul>
              </div>
            ))
          : null}
        {lines.length > 0 && !phone ? (
          <ul className="card-list is-table ledger-table">
            <li className="list-head" aria-hidden="true">
              <span>{t('debts.what')}</span>
              <span>{t('sync.field.date')}</span>
              <span className="is-end">{t('debts.change')}</span>
              <span className="is-end">{t('debts.balance')}</span>
              <span />
            </li>
            {lines.map(tableRow)}
          </ul>
        ) : null}
        {phone ? null : <DebtsFiles personId={person.id} onSaved={() => say(t('export.saved'), 'success')} onFailed={(text) => say(text, 'error')} />}
      </div>
    </section>
  )
}
