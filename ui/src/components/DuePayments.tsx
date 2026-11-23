import { Link, useNavigate } from 'react-router-dom'
import type { Due, DuePayment } from '../api/debts'
import { useProfile } from '../hooks/profile'
import { useOpenRow } from '../hooks/useOpenRow'
import { useT } from '../i18n/context'
import { formatDate, formatMoney } from '../lib/money'
import Facts from './Facts'
import { IconAction, ListGroup, NameMark, PanelActions, Row, Table, TapRow } from './RowParts'
import Badge from './ui/Badge'

/**
 * The recurring payments that are overdue and not paid or skipped, one row for each occurrence that was missed, the oldest first; an
 * income that was not received is listed too, in green. A payment is a debt only once its day has gone by: one that falls due today is
 * not here yet. Each row can be paid or skipped on its own, and every name leads to the item in the list. What the payments come to is in
 * the page's header; the income that was not received and the bills with no amount are here, over the list, when there are any.
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
  const navigate = useNavigate()
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

  // A phone row: the name (an income says so) and the amount, how late under the name. It opens to its category and day, and the buttons.
  function tapRow(payment: DuePayment) {
    const key = keyOf(payment)
    return (
      <TapRow
        key={key}
        id={rows.panelId(key)}
        open={rows.isOpen(key)}
        onToggle={() => rows.toggle(key)}
        name={<NameMark mark={payment.income ? <Badge variant="success">{t('common.income')}</Badge> : null}>{payment.name}</NameMark>}
        amount={amount(payment)}
        meta={<span className="due-late">{late(payment)}</span>}
        facts={[
          { label: t('common.category'), value: payment.category },
          { label: t('debts.colDue'), value: formatDate(payment.dueDate, locale) },
        ]}
      >
        <PanelActions
          actions={[
            { label: t('rec.pay'), icon: 'check', onClick: () => onPay(payment) },
            { label: t('rec.skip'), icon: 'skip', onClick: () => onSkip(payment) },
            { label: t('common.details'), icon: 'eye', onClick: () => navigate({ search: '' }, { state: focus(payment) }) },
          ]}
        />
      </TapRow>
    )
  }

  // A desktop row: name, category, the day, how late it is, the amount, then skip and mark as paid as icon buttons.
  function tableRow(payment: DuePayment) {
    return (
      <Row
        key={keyOf(payment)}
        actions={
          <>
            <IconAction icon="skip" label={`${t('rec.skip')}: ${payment.name}`} onClick={() => onSkip(payment)} />
            <IconAction icon="check" label={`${t('rec.pay')}: ${payment.name}`} className="icon-btn rec-pay" data-row-focus onClick={() => onPay(payment)} />
          </>
        }
      >
        <div className="cell-name">
          <Link className="card-name" to={{ search: '' }} state={focus(payment)} title={payment.name}>
            {payment.name}
          </Link>
          {payment.income ? <span className="row-mark">{t('common.income')}</span> : null}
        </div>
        <span className="cell-sub due-cat">{payment.category}</span>
        <span className="cell-sub-end due-date">{formatDate(payment.dueDate, locale)}</span>
        <span className="cell-sub due-late">{late(payment)}</span>
        {amount(payment)}
      </Row>
    )
  }

  return (
    <>
      <Facts
        className="acct-facts"
        items={[
          due.payments.some((payment) => payment.income) ? { label: t('debts.dueIncome'), value: money(due.incomeTotal), tone: 'positive' } : null,
          due.unknown > 0 ? { label: t('debts.noFixedAmount'), value: due.unknown } : null,
        ]}
      />
      {due.payments.length === 0 ? <p className="hint">{t('debts.dueEmpty')}</p> : null}
      {due.payments.length > 0 ? (
        phone ? (
          <ListGroup>{due.payments.map(tapRow)}</ListGroup>
        ) : (
          <Table className="due-table" tidy heads={[t('common.name'), { label: t('common.category'), className: 'head-cat' }, t('debts.colDue'), '', { label: t('tx.amount'), className: 'is-end' }, '']}>
            {due.payments.map(tableRow)}
          </Table>
        )
      ) : null}
    </>
  )
}
