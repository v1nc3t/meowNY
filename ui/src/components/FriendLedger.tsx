import { useEffect, useMemo, useState } from 'react'
import { Link } from 'react-router-dom'
import { debts, type Entry, type Line, type Person, type Statement } from '../api/debts'
import { ApiError, errorText } from '../api/invoke'
import type { Today } from '../api/today'
import { useClipCounts } from '../hooks/useClipCounts'
import { useFocusRequest, useGoToTransaction, useJumpTo } from '../hooks/useFocusRequest'
import type { MoneySettings } from '../hooks/useMoneySettings'
import { useOpenRow } from '../hooks/useOpenRow'
import { useToast } from '../hooks/toast'
import { useProfile } from '../hooks/profile'
import { useT } from '../i18n/context'
import { balanceView, entryOfLine, noteOf } from '../lib/balance'
import { formatDate, formatMoney } from '../lib/money'
import { remindersChanged } from '../lib/reminders'
import BillCodeModal from './BillCodeModal'
import ClipPreview from './ClipPreview'
import ConfirmModal from './ConfirmModal'
import EntryModal from './EntryModal'
import Icon from './NavIcon'
import { DayGroups, IconAction, NoteSection, Row, Table, TapRow } from './RowParts'
import SettleModal from './SettleModal'
import Button from './ui/Button'
import { buttonClass } from './ui/cn'

type Dialog =
  | { kind: 'entry'; entry: Entry }
  | { kind: 'settle' }
  | { kind: 'delete'; entry: Entry }
  | { kind: 'code'; transactionId: string; amount: string }
  | null

/**
 * One friend's history: back and mark as paid on one line (renaming, removing and bringing back a friend are on their row in the list),
 * then the name and the balance over the ledger with the buttons and documents of each line. It is the right half of the debts page, and
 * the whole page of a friend on the phone. The page says when to read again (`reload`) and is told when something changed (`onChanged`),
 * so the list of friends beside it stays right.
 */
export default function FriendLedger({
  personId,
  settings,
  now,
  reload,
  onChanged,
  onPerson,
}: {
  personId: string
  settings: MoneySettings
  now: Today
  reload: number
  onChanged: () => void
  /** Told who the friend is once the statement is read (none while it is not), for the page's own buttons. */
  onPerson: (person: Person | null) => void
}) {
  const t = useT()
  const phone = useProfile() === 'phone'
  const rows = useOpenRow()
  const goToTransaction = useGoToTransaction()
  const [statement, setStatement] = useState<Statement | null>(null)
  const [missing, setMissing] = useState(false)
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

  useEffect(() => onPerson(statement?.person ?? null), [statement, onPerson])

  // Newest first: the balance after each line still reads the same.
  const lines = useMemo(() => [...(statement?.lines ?? [])].reverse(), [statement])
  const entryIds = useMemo(() => lines.flatMap((line) => (line.entryId ? [line.entryId] : [])), [lines])
  const clips = useClipCounts('ledger_entry', entryIds, reload)
  // a document can send the person here for one of its entries: the line is scrolled to and outlined
  const { request: focus, done: focusDone } = useFocusRequest('ledger_entry')
  useJumpTo(focus, focusDone, lines)

  function done() {
    setDialog(null)
    onChanged()
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
      <div className="debts-half">
        <Link to="/app/debts" className={buttonClass('outline', 'sm')}>
          {t('common.back')}
        </Link>
        <p className="notice">{errorText(new ApiError('not_found'))}</p>
      </div>
    )
  }
  if (!statement) return loadError ? <p className="notice">{loadError}</p> : null

  const { person } = statement
  const { tone, abs } = balanceView(person.balance)
  // Beside the name, so the short words of the list of friends: "you owe €0.35", "owes you €0.35".
  const summaryText = tone === 'even' ? t('debts.settledUp') : `${t(tone === 'owed' ? 'debts.labelOwesYou' : 'debts.labelYouOwe')} ${money(abs)}`
  // A bill's name is in its label; an entry's note is not, it is in the panel of its row.
  const lineLabel = (line: Line) => {
    const kind = t(`debts.kind.${line.kind}`)
    return line.what && !noteOf(line) ? `${kind}: ${line.what}` : kind
  }
  const effectText = (line: Line) => `${line.effect.startsWith('-') ? '-' : '+'}${money(line.effect.replace('-', ''))}`
  const lineKey = (line: Line, at: number) => `${line.entryId ?? 'bill'}-${line.transactionId ?? ''}-${at}`
  const lineItem = (line: Line) => (line.entryId ? `ledger_entry:${line.entryId}` : undefined)
  /** What a line changed: what the friend owes you more (green) or less (red). */
  const change = (line: Line) => <span className={`cell-amount debt-${line.effect.startsWith('-') ? 'owe' : 'owed'}`}>{effectText(line)}</span>
  // a bill the friend paid has a negative change (the person owes them), and the app makes no code for the one who paid
  const hasCode = (line: Line) => line.kind === 'BILL' && line.transactionId !== null && !line.effect.startsWith('-')
  const showCode = (line: Line) => setDialog({ kind: 'code', transactionId: line.transactionId ?? '', amount: line.amount })

  // A desktop row: what it was, when, how much it changed, the balance after it, then its buttons (go to the transaction; show code for a
  // bill; edit and delete for an entry of the person's own) and the chevron of a note, which opens under the row.
  function tableRow(line: Line, at: number) {
    const entry = entryOfLine(line, person.id)
    const count = line.entryId ? (clips[line.entryId] ?? 0) : 0
    const note = noteOf(line)
    const key = lineKey(line, at)
    return (
      <Row
        key={key}
        rows={rows}
        id={key}
        item={lineItem(line)}
        actions={
          <>
            {line.transactionId ? (
              <IconAction icon="transactions" label={t('debts.goToTransaction')} data-row-focus={line.kind === 'BILL' && !hasCode(line) ? true : undefined} onClick={() => goToTransaction(line)} />
            ) : null}
            {line.kind === 'BILL' && line.transactionId ? (
              <>
                {hasCode(line) ? <IconAction icon="qr" label={t('code.show')} data-row-focus onClick={() => showCode(line)} /> : <span className="icon-slot" />}
                {/* where an entry has delete, so that the buttons of bills and entries line up */}
                <span className="icon-slot" />
              </>
            ) : null}
            {entry ? (
              <>
                <IconAction icon="pencil" label={t('common.edit')} data-row-focus onClick={() => setDialog({ kind: 'entry', entry })} />
                <IconAction icon="trash" label={t('common.delete')} danger onClick={() => setDialog({ kind: 'delete', entry })} />
              </>
            ) : null}
          </>
        }
        panel={note ? <NoteSection note={note} /> : null}
      >
        <div className="cell-name">
          <span className="card-name" title={lineLabel(line)}>
            {lineLabel(line)}
          </span>
          {count && line.entryId ? <ClipPreview inline target={{ kind: 'ledger_entry', id: line.entryId }} name={lineLabel(line)} count={count} /> : null}
        </div>
        <span className="cell-sub">{formatDate(line.date, settings.locale)}</span>
        {change(line)}
        <span className="cell-sub-end">{money(line.balance)}</span>
      </Row>
    )
  }

  // A phone row: what it was and how much it changed, the balance under them. It opens to its note, its documents and a labelled button
  // for each thing that can be done; a line with none of them is plain.
  function tapRow(line: Line) {
    const entry = entryOfLine(line, person.id)
    const count = line.entryId ? (clips[line.entryId] ?? 0) : 0
    const note = noteOf(line)
    const bill = hasCode(line)
    const key = lineKey(line, lines.indexOf(line))
    return (
      <TapRow
        key={key}
        id={rows.panelId(key)}
        item={lineItem(line)}
        open={rows.isOpen(key)}
        onToggle={() => rows.toggle(key)}
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
        {line.transactionId || entry || count || note ? (
          <>
            {note ? <NoteSection note={note} /> : null}
            {count && line.entryId ? (
              <section className="detail-section">
                <p className="detail-title">{t('docs.title')}</p>
                <ClipPreview target={{ kind: 'ledger_entry', id: line.entryId }} name={lineLabel(line)} count={count} />
              </section>
            ) : null}
            <div className="row-panel-actions">
              {line.transactionId ? (
                <Button variant="outline" onClick={() => goToTransaction(line)}>
                  <Icon name="transactions" className="btn-icon" />
                  {t('debts.goToTransaction')}
                </Button>
              ) : null}
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
    <div className="debts-half">
      <div className="debts-heading debts-name">
        <Link to="/app/debts" className={buttonClass('outline', 'sm')}>
          {t('common.back')}
        </Link>
        {tone !== 'even' ? (
          <Button variant="outline" size="sm" onClick={() => setDialog({ kind: 'settle' })}>
            {t('debts.settle')}
          </Button>
        ) : null}
      </div>
      {/* The title of the ledger is who it is with and where it stands. */}
      <h2 className="debts-heading debts-balance">
        {person.name}: <span className={`debt-${tone}`}>{summaryText}</span>
      </h2>
      {lines.length === 0 ? <p className="hint">{t('debts.noLines')}</p> : null}
      {lines.length > 0 && phone ? (
        <DayGroups rows={lines} dateOf={(line) => line.date} locale={settings.locale} today={now.date}>
          {tapRow}
        </DayGroups>
      ) : null}
      {lines.length > 0 && !phone ? (
        <Table
          className="ledger-table"
          heads={[t('debts.what'), t('sync.field.date'), { label: t('debts.change'), className: 'is-end' }, { label: t('debts.balance'), className: 'is-end' }, '']}
          opens
        >
          {lines.map(tableRow)}
        </Table>
      ) : null}
      {dialog?.kind === 'entry' ? (
        <EntryModal
          entry={dialog.entry}
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
      {dialog?.kind === 'settle' ? (
        <SettleModal
          person={person}
          today={now.date}
          onClose={() => setDialog(null)}
          onDone={() => {
            done()
            say(t('debts.settled'), 'success')
          }}
        />
      ) : null}
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
    </div>
  )
}
