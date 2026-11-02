import { useEffect, useId, useState } from 'react'
import { Link } from 'react-router-dom'
import { debts, type History, type HistoryLine } from '../api/debts'
import { errorText } from '../api/invoke'
import { useProfile } from '../hooks/profile'
import { useToast } from '../hooks/toast'
import { useT } from '../i18n/context'
import { formatDate, formatMoney } from '../lib/money'
import Pager from './Pager'
import { TapRow } from './RowParts'

/**
 * The recurring payments that were paid after their day, newest first, a page at a time: the day each fell due and the day it was paid.
 * A payment paid on time was never a debt, so it is not here, and the ones overdue now are in the list above. Each name leads to the
 * item in the list. `reload` changes when a payment was paid or skipped, which may add a line here.
 */
export default function PaidLatePayments({ currency, locale, reload }: { currency: string; locale: string; reload: number }) {
  const t = useT()
  const phone = useProfile() === 'phone'
  const panelId = useId()
  const say = useToast()
  const [history, setHistory] = useState<History | null>(null)
  const [offset, setOffset] = useState(0)
  const money = (amount: string) => formatMoney(amount, currency, locale)
  const keyOf = (line: HistoryLine) => line.transactionId ?? `${line.recurringId}-${line.date}`

  useEffect(() => {
    let cancelled = false
    void debts.history({ only: 'paidLate', offset }).then(
      (reply) => {
        if (!cancelled) setHistory(reply)
      },
      (failure) => {
        if (!cancelled) say(errorText(failure), 'error')
      },
    )
    return () => {
      cancelled = true
    }
  }, [offset, reload, say])

  // nothing is drawn until the first answer, so an empty hint does not flash before the lines
  if (!history) return null
  const { lines, total, limit, offset: at } = history

  // every such line has its item; a line without one is plain text
  const name = (line: HistoryLine) =>
    line.recurringId === null ? (
      <span className="card-name">{line.what}</span>
    ) : (
      <Link className="card-name" to={{ search: '' }} state={{ focus: { kind: 'recurring', id: line.recurringId, date: line.dueDate ?? line.date } }} title={line.what}>
        {line.what}
      </Link>
    )

  return (
    <>
      <h2 className="debts-heading">{t('rec.paidLateTitle')}</h2>
      {lines.length === 0 ? <p className="hint">{t('debts.noLines')}</p> : null}
      {lines.length > 0 ? (
        <ul className={phone ? 'card-list' : 'card-list is-table late-table'}>
          {phone ? null : (
            <li className="list-head" aria-hidden="true">
              <span>{t('common.name')}</span>
              <span>{t('debts.colDue')}</span>
              <span>{t('rec.paidOn')}</span>
              <span className="is-end">{t('tx.amount')}</span>
            </li>
          )}
          {lines.map((line) =>
            phone ? (
              <TapRow
                key={keyOf(line)}
                id={`${panelId}-${keyOf(line)}`}
                name={line.what}
                amount={<span className="cell-amount">{money(line.amount)}</span>}
                sub={
                  <>
                    {line.dueDate ? <span>{t('debts.dueOn', { date: formatDate(line.dueDate, locale) })}</span> : null}
                    <span>{`${t('rec.paidOn')} ${formatDate(line.date, locale)}`}</span>
                  </>
                }
              />
            ) : (
              <li key={keyOf(line)} className="card-item">
                <div className="cell-name">{name(line)}</div>
                <span className="cell-sub">{line.dueDate ? formatDate(line.dueDate, locale) : ''}</span>
                <span className="cell-sub-end">{formatDate(line.date, locale)}</span>
                <span className="cell-amount">{money(line.amount)}</span>
              </li>
            ),
          )}
        </ul>
      ) : null}
      <Pager page={Math.floor(at / limit) + 1} count={Math.ceil(total / limit)} onPage={(number) => setOffset((number - 1) * limit)} />
    </>
  )
}
