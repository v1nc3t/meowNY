import { useEffect, useState } from 'react'
import { Link } from 'react-router-dom'
import { debts, type History, type HistoryLine } from '../api/debts'
import { errorText } from '../api/invoke'
import { useProfile } from '../hooks/profile'
import { useOpenRow } from '../hooks/useOpenRow'
import { useToast } from '../hooks/toast'
import { useT } from '../i18n/context'
import { formatDate, formatMoney } from '../lib/money'
import Pager from './Pager'
import { ListGroup, Table, TapRow } from './RowParts'

/**
 * The recurring payments that were paid after their day, newest first, a page at a time: the day each fell due and the day it was paid.
 * A payment paid on time was never a debt, so it is not here, and the ones overdue now are in the list above. Each name leads to the
 * item in the list. `reload` changes when a payment was paid or skipped, which may add a line here.
 */
export default function PaidLatePayments({ currency, locale, reload }: { currency: string; locale: string; reload: number }) {
  const t = useT()
  const phone = useProfile() === 'phone'
  const say = useToast()
  const [history, setHistory] = useState<History | null>(null)
  const [offset, setOffset] = useState(0)
  const rows = useOpenRow()
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

  // A phone row: the name and the amount, the day it was paid under the name; it opens to the day it fell due.
  function tapRow(line: HistoryLine) {
    const key = keyOf(line)
    return (
      <TapRow
        key={key}
        id={rows.panelId(key)}
        open={rows.isOpen(key)}
        onToggle={() => rows.toggle(key)}
        name={line.what}
        amount={<span className="cell-amount">{money(line.amount)}</span>}
        meta={`${t('rec.paidOn')} ${formatDate(line.date, locale)}`}
        facts={[{ label: t('debts.colDue'), value: line.dueDate ? formatDate(line.dueDate, locale) : null }]}
      />
    )
  }

  // not a `Row`: it would add an empty `cell-actions` to a row that has no buttons, and a cell too many to the table's columns
  function tableRow(line: HistoryLine) {
    return (
      <li key={keyOf(line)} className="card-item">
        <div className="cell-name">{name(line)}</div>
        <span className="cell-sub">{line.dueDate ? formatDate(line.dueDate, locale) : ''}</span>
        <span className="cell-sub-end">{formatDate(line.date, locale)}</span>
        <span className="cell-amount">{money(line.amount)}</span>
      </li>
    )
  }

  return (
    <>
      {phone && lines.length > 0 ? null : <h2 className="debts-heading">{t('rec.paidLateTitle')}</h2>}
      {lines.length === 0 ? <p className="hint">{t('debts.noLines')}</p> : null}
      {lines.length > 0 ? (
        phone ? (
          <ListGroup heading={t('rec.paidLateTitle')}>{lines.map(tapRow)}</ListGroup>
        ) : (
          <Table className="late-table" tidy heads={[t('common.name'), t('debts.colDue'), t('rec.paidOn'), { label: t('tx.amount'), className: 'is-end' }]}>
            {lines.map(tableRow)}
          </Table>
        )
      ) : null}
      <Pager page={Math.floor(at / limit) + 1} count={Math.ceil(total / limit)} onPage={(number) => setOffset((number - 1) * limit)} />
    </>
  )
}
