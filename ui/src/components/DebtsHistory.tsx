import { Link } from 'react-router-dom'
import type { History, HistoryLine } from '../api/debts'
import { useGoToTransaction } from '../hooks/useFocusRequest'
import { useOpenRow } from '../hooks/useOpenRow'
import { useProfile } from '../hooks/profile'
import { useT } from '../i18n/context'
import { noteOf } from '../lib/balance'
import { formatDate, formatMoney } from '../lib/money'
import Icon from './NavIcon'
import { DayGroups, IconAction, NoteSection, Row, Table, TapRow } from './RowParts'
import Button from './ui/Button'
import { buttonClass } from './ui/cn'

/**
 * The debts of all friends, newest first, a page at a time: what changed with each friend. A friend's name leads to the friend's history.
 * A line with a note opens to it, and one with a transaction (a bill, or a payment that was for one) has a way to go to it.
 */
export default function DebtsHistory({
  history,
  currency,
  locale,
  today,
  onPage,
}: {
  history: History
  currency: string
  locale: string
  /** Today's date, for the year of a day's heading on the phone. */
  today: string
  onPage: (offset: number) => void
}) {
  const t = useT()
  const phone = useProfile() === 'phone'
  const rows = useOpenRow()
  const goToTransaction = useGoToTransaction()
  const money = (amount: string) => formatMoney(amount, currency, locale)
  const { lines, total, limit, offset } = history

  const effectText = (effect: string) => `${effect.startsWith('-') ? '-' : '+'}${money(effect.replace('-', ''))}`
  const kindText = (line: HistoryLine) => t(`debts.kind.${line.kind}`)
  const keyOf = (line: HistoryLine, at: number) => `${line.date}-${line.kind}-${line.entryId ?? line.transactionId ?? ''}-${offset + at}`

  /** The friend, leading to their page. */
  const subject = (line: HistoryLine) => (line.personId ? <Link to={`/app/debts/${line.personId}`}>{line.friend}</Link> : line.friend)

  /** The change: what a friend owes you more is green and what you owe is red. */
  function amount(line: HistoryLine) {
    const effect = line.effect ?? line.amount
    return <span className={`cell-amount ${effect.startsWith('-') ? 'debt-owe' : 'debt-owed'}`}>{effectText(effect)}</span>
  }

  /** What kind of change it was, and what it was for if that is a name (of a bill); a note is not here, it is in the panel of the row. */
  const what = (line: HistoryLine) => (line.what && !noteOf(line) ? `${kindText(line)}: ${line.what}` : kindText(line))

  // A desktop row: who, what kind of change, the day, the change and the balance after it, then the button that goes to its transaction
  // and the chevron of a note, which opens under the row.
  function tableRow(line: HistoryLine, at: number) {
    const note = noteOf(line)
    const key = keyOf(line, at)
    return (
      <Row
        key={key}
        rows={rows}
        id={key}
        actions={line.transactionId ? <IconAction icon="transactions" label={t('debts.goToTransaction')} data-row-focus onClick={() => goToTransaction(line)} /> : null}
        panel={note ? <NoteSection note={note} /> : null}
      >
        <div className="cell-name">
          <span className="card-name">{subject(line)}</span>
        </div>
        <span className="cell-sub" title={what(line)}>
          {what(line)}
        </span>
        <span className="cell-sub-end">{formatDate(line.date, locale)}</span>
        {amount(line)}
        <span className="cell-bal">{line.balance !== null ? money(line.balance) : ''}</span>
      </Row>
    )
  }

  // A phone row: who and the change, what it was and its balance under them. It opens to its note and to a button for its transaction; a
  // line with neither is plain. The friend's name is a link only in a plain row, since a link cannot be inside the button of a row that
  // opens: there the friend is a button of the panel.
  function tapRow(line: HistoryLine) {
    const key = keyOf(line, lines.indexOf(line))
    const note = noteOf(line)
    const opens = Boolean(note || line.transactionId)
    return (
      <TapRow
        key={key}
        id={rows.panelId(key)}
        open={rows.isOpen(key)}
        onToggle={() => rows.toggle(key)}
        name={opens ? line.friend : subject(line)}
        amount={amount(line)}
        sub={
          <>
            <span className="is-line">{what(line)}</span>
            {line.balance !== null ? <span>{`${t('debts.balance')} ${money(line.balance)}`}</span> : null}
          </>
        }
      >
        {opens ? (
          <>
            {note ? <NoteSection note={note} /> : null}
            <div className="row-panel-actions">
              {line.transactionId ? (
                <Button variant="outline" onClick={() => goToTransaction(line)}>
                  <Icon name="transactions" className="btn-icon" />
                  {t('debts.goToTransaction')}
                </Button>
              ) : null}
              {line.personId ? (
                <Link to={`/app/debts/${line.personId}`} className={buttonClass('outline')}>
                  {line.friend}
                </Link>
              ) : null}
            </div>
          </>
        ) : null}
      </TapRow>
    )
  }

  return (
    <>
      {lines.length === 0 ? <p className="hint">{t('debts.noLines')}</p> : null}
      {lines.length > 0 && phone ? (
        <DayGroups rows={lines} dateOf={(line) => line.date} locale={locale} today={today}>
          {tapRow}
        </DayGroups>
      ) : null}
      {lines.length > 0 && !phone ? (
        <Table
          className="history-table"
          heads={[t('common.name'), t('debts.kind'), t('sync.field.date'), { label: t('debts.change'), className: 'is-end' }, { label: t('debts.balance'), className: 'is-end' }, '']}
          opens
        >
          {lines.map(tableRow)}
        </Table>
      ) : null}
      {total > limit ? (
        <div className="tx-actions debts-pager">
          <Button variant="outline" size="sm" disabled={offset + limit >= total} onClick={() => onPage(offset + limit)}>
            {t('debts.older')}
          </Button>
          <Button variant="outline" size="sm" disabled={offset === 0} onClick={() => onPage(Math.max(0, offset - limit))}>
            {t('debts.newer')}
          </Button>
          <span className="muted">{t('debts.historyPage', { from: offset + 1, to: Math.min(offset + limit, total), total })}</span>
        </div>
      ) : null}
    </>
  )
}
