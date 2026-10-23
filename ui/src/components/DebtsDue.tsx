import { Link, useNavigate } from 'react-router-dom'
import type { Due } from '../api/debts'
import { useT } from '../i18n/context'
import { formatDate, formatMoney } from '../lib/money'
import Button from './ui/Button'
import { Card, CardContent } from './ui/Card'

/**
 * The recurring payments that are overdue and not paid or skipped, one row for each occurrence that was missed, the oldest first. A
 * payment is a debt only once its day has gone by: one that falls due today is not here yet. Paying is done on the recurring screen (it
 * opens the dialog of the item): the oldest occurrence of an item is the one that dialog pays, so only that row has the button, and every
 * name leads to the item.
 */
export default function DebtsDue({ due, currency, locale }: { due: Due; currency: string; locale: string }) {
  const t = useT()
  const navigate = useNavigate()
  const money = (amount: string) => formatMoney(amount, currency, locale)
  const first = new Set<string>()
  const payable = due.payments.map((payment) => {
    const isFirst = !first.has(payment.recurringId)
    first.add(payment.recurringId)
    return isFirst
  })
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
        {due.payments.map((payment, at) => (
          <li key={`${payment.recurringId}-${payment.dueDate}`} className="card-item">
            <div className="card-body">
              <p className="card-name">
                <Link to="/app/recurring" state={{ focus: { kind: 'recurring', id: payment.recurringId, date: payment.dueDate } }}>
                  {payment.name}
                </Link>
                <span className="muted">{` · ${payment.category}`}</span>
              </p>
              <p className="card-meta">
                <span className="card-amount debt-owe">{payment.amount === null ? t('rec.varies') : money(payment.amount)}</span>
                {` · ${t('debts.dueOn', { date: formatDate(payment.dueDate, locale) })} · ${t('debts.daysLate', { count: payment.daysOverdue })}`}
              </p>
            </div>
            {payable[at] ? (
              <div className="card-actions">
                <Button
                  variant="outline"
                  size="sm"
                  data-row-focus
                  onClick={() => navigate('/app/recurring', { state: { focus: { kind: 'recurring', id: payment.recurringId, date: payment.dueDate }, pay: true } })}
                >
                  {t('rec.pay')}
                </Button>
              </div>
            ) : null}
          </li>
        ))}
      </ul>
    </>
  )
}
