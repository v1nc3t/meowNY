import { Link } from 'react-router-dom'
import type { Due, DuePayment } from '../api/debts'
import { useProfile } from '../hooks/profile'
import { useOpenRow } from '../hooks/useOpenRow'
import { useT } from '../i18n/context'
import { formatDate, formatMoney } from '../lib/money'
import { Row, Table, TapRow } from './RowParts'
import Button from './ui/Button'
import { buttonClass } from './ui/cn'

/**
 * The recurring payments that are overdue and not paid or skipped, one row for each occurrence that was missed, the oldest first; an
 * income that was not received is listed too, in green. A payment is a debt only once its day has gone by: one that falls due today is
 * not here yet. Each row can be paid or skipped on its own, and every name leads to the item in the list.
 */
export default function DuePayments({
  due,
  currency,
  locale,
  onPay,
  onSkip,
}: {
  due: Due
  currency: string
  locale: string
  onPay: (payment: DuePayment) => void
  onSkip: (payment: DuePayment) => void
}) {
  const t = useT()
  const phone = useProfile() === 'phone'
  const rows = useOpenRow()
  const money = (amount: string) => formatMoney(amount, currency, locale)
  const keyOf = (payment: DuePayment) => `${payment.recurringId}-${payment.dueDate}`
  const focus = (payment: DuePayment) => ({ focus: { kind: 'recurring', id: payment.recurringId, date: payment.dueDate } })
  /** What is owed in red, an income that did not come in green. */
  const amount = (payment: DuePayment) => {
    if (payment.amount === null) return <span className="cell-amount is-varies">{t('rec.varies')}</span>
    return payment.income ? <span className="cell-amount is-income">{`+${money(payment.amount)}`}</span> : <span className="cell-amount debt-owe">{money(payment.amount)}</span>
  }

  const late = (payment: DuePayment) => t('debts.daysLate', { count: payment.daysOverdue })

  function tapRow(payment: DuePayment) {
    const key = keyOf(payment)
    return (
      <TapRow
        key={key}
        id={rows.panelId(key)}
        open={rows.isOpen(key)}
        onToggle={() => rows.toggle(key)}
        name={payment.name}
        amount={amount(payment)}
        sub={
          <>
            <span>{payment.category}</span>
            <span>{formatDate(payment.dueDate, locale)}</span>
            <span className="due-late">{late(payment)}</span>
          </>
        }
      >
        <div className="row-panel-actions">
          <Button variant="outline" onClick={() => onPay(payment)}>
            {t('rec.pay')}
          </Button>
          <Button variant="outline" onClick={() => onSkip(payment)}>
            {t('rec.skip')}
          </Button>
          <Link to={{ search: '' }} state={focus(payment)} className={buttonClass('outline')}>
            {t('common.details')}
          </Link>
        </div>
      </TapRow>
    )
  }

  function tableRow(payment: DuePayment) {
    return (
      <Row
        key={keyOf(payment)}
        actions={
          <>
            <Button variant="ghost" size="sm" onClick={() => onSkip(payment)}>
              {t('rec.skip')}
            </Button>
            <Button variant="outline" size="sm" className="row-primary" data-row-focus onClick={() => onPay(payment)}>
              {t('rec.pay')}
            </Button>
          </>
        }
      >
        <div className="cell-name">
          <Link className="card-name" to={{ search: '' }} state={focus(payment)} title={payment.name}>
            {payment.name}
          </Link>
          {payment.income ? <span className="row-mark">{t('common.income')}</span> : null}
        </div>
        <span className="cell-sub">{payment.category}</span>
        <span className="cell-sub-end due-date">
          {formatDate(payment.dueDate, locale)}
          <span className="due-late">{late(payment)}</span>
        </span>
        {amount(payment)}
      </Row>
    )
  }

  return (
    <>
      <div className="debts-stats">
        <div className="debts-stat">
          <span className="debts-stat-label">{t('debts.dueTotal')}</span>
          <span className={`debts-stat-value ${Number(due.total) > 0 ? 'debt-owe' : 'debt-even'}`}>{money(due.total)}</span>
        </div>
        {due.payments.some((payment) => payment.income) ? (
          <div className="debts-stat">
            <span className="debts-stat-label">{t('debts.dueIncome')}</span>
            <span className="debts-stat-value debt-owed">{money(due.incomeTotal)}</span>
          </div>
        ) : null}
        {due.unknown > 0 ? (
          <div className="debts-stat">
            <span className="debts-stat-label">{t('debts.dueUnknown', { count: due.unknown })}</span>
          </div>
        ) : null}
      </div>
      {due.payments.length === 0 ? <p className="hint">{t('debts.dueEmpty')}</p> : null}
      {due.payments.length > 0 ? (
        phone ? (
          <ul className="card-list">{due.payments.map(tapRow)}</ul>
        ) : (
          <Table className="has-action due-table" heads={[t('common.name'), t('common.category'), t('debts.colDue'), { label: t('tx.amount'), className: 'is-end' }, '']}>
            {due.payments.map(tableRow)}
          </Table>
        )
      ) : null}
    </>
  )
}
