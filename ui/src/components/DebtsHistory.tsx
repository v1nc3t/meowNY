import { useId } from 'react'
import { Link } from 'react-router-dom'
import type { History, HistoryLine } from '../api/debts'
import { useProfile } from '../hooks/profile'
import { useT } from '../i18n/context'
import { byDay, dayHeading } from '../lib/dayHeading'
import { formatDate, formatMoney } from '../lib/money'
import { TapRow } from './RowParts'
import Button from './ui/Button'

/**
 * The debts of all friends, newest first, a page at a time: what changed with each friend. A friend's name leads to the friend's history.
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
  const panelId = useId()
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

  /** What kind of change it was, and what it was for. */
  const what = (line: HistoryLine) => (line.what ? `${kindText(line)}: ${line.what}` : kindText(line))

  // A desktop row: who, what kind of change, the day, the change and the balance after it.
  function tableRow(line: HistoryLine, at: number) {
    return (
      <li key={keyOf(line, at)} className="card-item">
        <div className="cell-name">
          <span className="card-name">{subject(line)}</span>
        </div>
        <span className="cell-sub" title={what(line)}>
          {what(line)}
        </span>
        <span className="cell-sub-end">{formatDate(line.date, locale)}</span>
        {amount(line)}
        <span className="cell-bal">{line.balance !== null ? money(line.balance) : ''}</span>
      </li>
    )
  }

  // A phone row: who and the change, what it was and its balance under them. There is nothing to open.
  function tapRow(line: HistoryLine, at: number) {
    const key = keyOf(line, at)
    return (
      <TapRow
        key={key}
        id={`${panelId}-${key}`}
        name={subject(line)}
        amount={amount(line)}
        sub={
          <>
            <span className="is-line">{what(line)}</span>
            {line.balance !== null ? <span>{`${t('debts.balance')} ${money(line.balance)}`}</span> : null}
          </>
        }
      />
    )
  }

  return (
    <>
      {lines.length === 0 ? <p className="hint">{t('debts.noLines')}</p> : null}
      {lines.length > 0 && phone
        ? byDay(lines, (line) => line.date).map((day) => (
            <div key={day.date} className="list-day">
              <h2>{dayHeading(day.date, locale, today.slice(0, 4))}</h2>
              <ul className="card-list">{day.rows.map((line) => tapRow(line, lines.indexOf(line)))}</ul>
            </div>
          ))
        : null}
      {lines.length > 0 && !phone ? (
        <ul className="card-list is-table history-table">
          <li className="list-head" aria-hidden="true">
            <span>{t('common.name')}</span>
            <span>{t('debts.kind')}</span>
            <span>{t('sync.field.date')}</span>
            <span className="is-end">{t('debts.change')}</span>
            <span className="is-end">{t('debts.balance')}</span>
          </li>
          {lines.map(tableRow)}
        </ul>
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
