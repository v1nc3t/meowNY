import { Link } from 'react-router-dom'
import type { Due, DuePayment } from '../api/debts'
import { useT } from '../i18n/context'
import { formatDate, formatMoney } from '../lib/money'
import Button from './ui/Button'
import { Card, CardContent } from './ui/Card'

/**
 * The recurring payments that are overdue and not paid or skipped, one row for each occurrence that was missed, the oldest first. A
 * payment is a debt only once its day has gone by: one that falls due today is not here yet. Each row can be paid or skipped on its own,
 * and every name leads to the item in the list.
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
  const money = (amount: string) => formatMoney(amount, currency, locale)
  return (
    <>
      <Card>
        <CardContent className="debts-stats">
          <div className="debts-stat">
            <span className="debts-stat-label">{t('debts.dueTotal')}</span>
            <span className={`debts-stat-value ${Number(due.total) > 0 ? 'debt-owe' : 'debt-even'}`}>{money(due.total)}</span>
          </div>
          {due.unknown > 0 ? (
            <div className="debts-stat">
              <span className="debts-stat-label">{t('debts.dueUnknown', { count: due.unknown })}</span>
            </div>
          ) : null}
        </CardContent>
      </Card>
      {due.payments.length === 0 ? <p className="hint">{t('debts.dueEmpty')}</p> : null}
      <ul className="card-list">
        {due.payments.map((payment) => (
          <li key={`${payment.recurringId}-${payment.dueDate}`} className="card-item">
            <div className="card-body">
              <p className="card-name">
                <Link to={{ search: '' }} state={{ focus: { kind: 'recurring', id: payment.recurringId, date: payment.dueDate } }}>
                  {payment.name}
                </Link>
                <span className="muted">{` · ${payment.category}`}</span>
              </p>
              <p className="card-meta">
                <span className="card-amount debt-owe">{payment.amount === null ? t('rec.varies') : money(payment.amount)}</span>
                {` · ${t('debts.dueOn', { date: formatDate(payment.dueDate, locale) })} · ${t('debts.daysLate', { count: payment.daysOverdue })}`}
              </p>
            </div>
            <div className="card-actions">
              <Button variant="ghost" size="sm" onClick={() => onSkip(payment)}>
                {t('rec.skip')}
              </Button>
              <Button variant="outline" size="sm" data-row-focus onClick={() => onPay(payment)}>
                {t('rec.pay')}
              </Button>
            </div>
          </li>
        ))}
      </ul>
    </>
  )
}
