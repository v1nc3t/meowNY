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
 * The history of debts, newest first, a page at a time: what changed with each friend, and the recurring payments that were paid or are
 * due. A friend's line leads to the friend's statement, a recurring payment's to the recurring screen.
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
  const keyOf = (line: HistoryLine, at: number) => `${line.date}-${line.kind}-${line.entryId ?? line.transactionId ?? line.recurringId ?? ''}-${offset + at}`

  /** Who or what it was about: a friend or a recurring payment, each leading to its page. */
  const subject = (line: HistoryLine) =>
    line.recurringId !== null ? (
      <Link to="/app/recurring" state={{ focus: { kind: 'recurring', id: line.recurringId, date: line.date } }}>
        {line.what}
      </Link>
    ) : line.personId ? (
      <Link to={`/app/debts/${line.personId}`}>{line.friend}</Link>
    ) : (
      line.friend
    )

  /** The change: what a friend owes you more is green and what you owe is red; a payment you made (late) is money paid, so it has no colour. */
  function amount(line: HistoryLine) {
    const tone = line.effect !== null ? (line.effect.startsWith('-') ? 'debt-owe' : 'debt-owed') : line.kind === 'PAYMENT_DUE' ? 'debt-owe' : ''
    const text =
      line.effect !== null ? effectText(line.effect) : line.kind === 'PAYMENT_DUE' && Number(line.amount) === 0 ? t('rec.varies') : money(line.amount)
    return <span className={`cell-amount ${tone}`}>{text}</span>
  }

  /** What kind of change it was, and for a friend's line what it was for. */
  const what = (line: HistoryLine) => (line.recurringId === null && line.what ? `${kindText(line)}: ${line.what}` : kindText(line))
  const dueText = (line: HistoryLine) => (line.dueDate && line.dueDate !== line.date ? t('debts.dueOn', { date: formatDate(line.dueDate, locale) }) : '')

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
        <span className="cell-sub-end">
          {formatDate(line.date, locale)}
          {dueText(line) ? <span className="cell-fine">{dueText(line)}</span> : null}
        </span>
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
            {dueText(line) ? <span>{dueText(line)}</span> : null}
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
