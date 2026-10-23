import { Link } from 'react-router-dom'
import type { Dues, Recurring } from '../api/recurring'
import { useT } from '../i18n/context'
import { formatDate, formatMoney } from '../lib/money'
import { sharedItems, sharedWith } from '../lib/recurringView'
import { Card, CardContent, CardHeader, CardTitle } from './ui/Card'

/** How many days ahead "due soon" looks. */
export const SOON_DAYS = 7

/**
 * Three cards for the dashboard: the recurring items that are overdue, those that fall due in the next days, and those whose payments
 * are split with friends. Each row leads to the item on the recurring page.
 */
export default function RecurringDashboard({ dues, items, today, currency, locale }: { dues: Dues; items: Recurring[]; today: string; currency: string; locale: string }) {
  const t = useT()
  const money = (item: Recurring) => (item.amount === null ? t('rec.varies') : formatMoney(item.amount, currency, locale))
  const link = (item: Recurring) => (
    <Link to="/app/recurring" state={{ focus: { kind: 'recurring', id: item.id, date: item.nextDueDate } }}>
      {item.name}
    </Link>
  )
  const shared = sharedItems(items)
  return (
    <div className="dash-grid">
      <Card>
        <CardHeader>
          <CardTitle>
            <Link to="/app/recurring">{t('dashboard.recOverdue')}</Link>
          </CardTitle>
        </CardHeader>
        <CardContent>
          {dues.overdue.length === 0 ? (
            <p className="dash-empty">{t('dashboard.recOverdueEmpty')}</p>
          ) : (
            <ul className="dash-legend">
              {dues.overdue.map((item) => (
                <li key={item.id}>
                  <span className="legend-name">{link(item)}</span>
                  <span className="legend-pct rec-over">{formatDate(item.nextDueDate, locale)}</span>
                  <span className="legend-amount">{money(item)}</span>
                </li>
              ))}
            </ul>
          )}
        </CardContent>
      </Card>
      <Card>
        <CardHeader>
          <CardTitle>
            <Link to="/app/recurring">{t('dashboard.recSoon', { days: SOON_DAYS })}</Link>
          </CardTitle>
        </CardHeader>
        <CardContent>
          {dues.upcoming.length === 0 ? (
            <p className="dash-empty">{t('dashboard.recSoonEmpty', { days: SOON_DAYS })}</p>
          ) : (
            <ul className="dash-legend">
              {dues.upcoming.map((item) => (
                <li key={item.id}>
                  <span className="legend-name">{link(item)}</span>
                  <span className="legend-pct">{item.nextDueDate === today ? t('rec.dueToday') : formatDate(item.nextDueDate, locale)}</span>
                  <span className="legend-amount">{money(item)}</span>
                </li>
              ))}
            </ul>
          )}
        </CardContent>
      </Card>
      <Card>
        <CardHeader>
          <CardTitle>
            <Link to="/app/recurring">{t('dashboard.recSplit')}</Link>
          </CardTitle>
        </CardHeader>
        <CardContent>
          {shared.length === 0 ? (
            <p className="dash-empty">{t('dashboard.recSplitEmpty')}</p>
          ) : (
            <ul className="dash-legend">
              {shared.map((item) => {
                const names = t('dashboard.recWith', { names: sharedWith(item).join(', ') })
                return (
                  <li key={item.id} title={`${item.name}: ${names}`}>
                    <span className="legend-name">
                      {link(item)}
                      <span className="muted">{` · ${names}`}</span>
                    </span>
                    <span className="legend-amount">{money(item)}</span>
                  </li>
                )
              })}
            </ul>
          )}
        </CardContent>
      </Card>
    </div>
  )
}
